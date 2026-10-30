//! Test notification handler

use super::error::{ApiError, ApiResult, OrInternal};
use crate::api::AppServicesState;
use crate::auth::AuthUser;
use crate::config::AppConfig;
use crate::extractors::{require_non_demo, AuthenticatedUser};
use crate::handlers::helpers::{
    reject_nostr_in_cloud_mode, reject_telegram_if_unconfigured, reject_telegram_in_cloud_mode,
    reject_webhook_in_cloud_mode,
};
use crate::metadata::{Language, ProviderType};
use crate::models::{
    NostrSettingsResponse, TelegramSettingsResponse, TestNostrRequest, TestNostrResponse,
    TestNtfyRequest, TestNtfyResponse, TestTelegramRequest, TestTelegramResponse,
    TestWebhookRequest, TestWebhookResponse, UpdateNostrSettingsRequest,
    UpdateTelegramSettingsRequest,
};
use crate::nostr_provider::{
    ensure_nostr_sender_keys, get_nostr_dm_mode, nostr_test_error_code,
    parse_nostr_recipient_or_error, set_nostr_dm_mode, NostrDmMode, NostrProvider, NostrSenderKeys,
};
use crate::ntfy_provider::NtfyAuth;
use crate::telegram_provider::{
    is_configured, set_bot_token, validate_telegram_chat_id, TelegramProvider,
};
use crate::test_notification::{
    format_generic_nostr_test_message, format_generic_telegram_test_notification,
    format_generic_test_notification, format_saved_nostr_test_message,
    format_saved_test_notification, load_saved_test_config, SavedTestConfigError,
    SavedTestRequestIds, TestNotificationConfig, TestNotificationCopy,
};
use crate::webhook_provider::{validate_webhook_url, WebhookPayload, WebhookProvider};
use axum::{extract::State, http::StatusCode, response::Json};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use std::sync::Arc;
use std::time::Instant;

/// Send a test notification to an ntfy topic (self-hosted mode only)
pub async fn send_test_ntfy_notification(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    Json(payload): Json<TestNtfyRequest>,
) -> ApiResult<Json<TestNtfyResponse>> {
    if !config.is_self_hosted_mode() {
        return Err(ApiError::uncoded(
            StatusCode::FORBIDDEN,
            "Test notifications are only available in self-hosted mode",
        ));
    }

    let topic = payload.topic.trim();
    validate_ntfy_topic(topic)?;

    // Look up user's ntfy server URL
    let user_ntfy_server_url = match app_services
        .metadata_db
        .get_user_ntfy_server_url(&user.user_id)
        .await
    {
        Ok(Some(url)) if !url.is_empty() => Some(url),
        _ => None,
    };
    let ntfy_server = user_ntfy_server_url
        .clone()
        .unwrap_or_else(|| config.ntfy_server_url());
    let ntfy_server = ntfy_server.trim().trim_end_matches('/').to_string();
    let should_use_ntfy_auth =
        config.should_use_ntfy_auth_for_url(&ntfy_server, user_ntfy_server_url.as_deref());

    // Look up user's ntfy auth credentials
    let ntfy_auth = if should_use_ntfy_auth {
        match app_services
            .metadata_db
            .get_user_ntfy_auth(&user.user_id)
            .await
        {
            Ok((Some(token), _, _)) => NtfyAuth::AccessToken(token),
            Ok((None, Some(username), Some(password))) => {
                NtfyAuth::BasicAuth { username, password }
            }
            _ => NtfyAuth::None,
        }
    } else {
        NtfyAuth::None
    };
    let ntfy_auth =
        config.with_managed_ntfy_auth(ntfy_auth, &ntfy_server, user_ntfy_server_url.as_deref());

    let language = user_preferred_language(&app_services, &user.user_id).await;
    let copy = resolve_test_copy(
        &app_services,
        &user,
        SavedTestRequestIds {
            wallet_checksum: payload.wallet_checksum,
            contact_id: payload.contact_id,
            method_id: payload.method_id,
        },
        ProviderType::Ntfy,
        topic,
        &language,
        GenericTestCopy::Ntfy,
    )
    .await?;

    let ntfy_url = format!("{}/{}", ntfy_server.trim_end_matches('/'), topic);

    // Use the same mode policy and client construction as regular delivery.
    let provider = config.ntfy_provider(
        &ntfy_server,
        ntfy_auth.clone(),
        user_ntfy_server_url.as_deref(),
    );
    let Ok(client) = provider.client_for_url(&ntfy_url).await else {
        return Ok(ntfy_test_result(Some(
            "ntfy server URL is invalid or not allowed.".to_string(),
        )));
    };
    let mut request = client
        .post(&ntfy_url)
        .header("Content-Type", "text/plain; charset=utf-8")
        .header("Title", copy.title)
        .header("Priority", "urgent")
        .header("Tags", "rotating_light");

    match &ntfy_auth {
        NtfyAuth::None => {}
        NtfyAuth::AccessToken(token) => {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        NtfyAuth::BasicAuth { username, password } => {
            let credentials = format!("{username}:{password}");
            let encoded = BASE64.encode(credentials.as_bytes());
            request = request.header("Authorization", format!("Basic {encoded}"));
        }
    }

    let error = match request.body(copy.body).send().await {
        Ok(response) if response.status().is_success() => None,
        Ok(response) => Some(format!("HTTP {}", response.status().as_u16())),
        Err(_) => Some("Request to ntfy server failed".to_string()),
    };
    Ok(ntfy_test_result(error))
}

fn validate_ntfy_topic(topic: &str) -> ApiResult<()> {
    let message = if topic.is_empty() {
        "ntfy topic cannot be empty"
    } else if topic.len() > 64 {
        "ntfy topic must be at most 64 characters"
    } else if !topic
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        "ntfy topic can only contain letters, numbers, dashes, and underscores"
    } else {
        return Ok(());
    };
    Err(ApiError::uncoded(StatusCode::BAD_REQUEST, message))
}

/// Delivery failures are reported in the body with a 200 status.
fn ntfy_test_result(error: Option<String>) -> Json<TestNtfyResponse> {
    Json(TestNtfyResponse {
        success: error.is_none(),
        error,
    })
}

/// Send a test Telegram Bot message when a bot token is configured (self-hosted only).
/// Cloud mode never exposes Telegram.
pub async fn send_test_telegram_notification(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    Json(payload): Json<TestTelegramRequest>,
) -> ApiResult<Json<TestTelegramResponse>> {
    require_non_demo(&user)?;
    reject_telegram_in_cloud_mode(config.as_ref())?;
    reject_telegram_if_unconfigured(&app_services).await?;

    let chat_id = validate_telegram_chat_id(&payload.chat_id)
        .map_err(|error| ApiError::bad_request("invalid_telegram_chat_id", error))?;

    let provider = TelegramProvider::with_metadata_db(
        app_services.metadata_db.clone(),
        "https://api.telegram.org",
    );

    let language = user_preferred_language(&app_services, &user.user_id).await;
    let copy = resolve_test_copy(
        &app_services,
        &user,
        SavedTestRequestIds {
            wallet_checksum: payload.wallet_checksum,
            contact_id: payload.contact_id,
            method_id: payload.method_id,
        },
        ProviderType::Telegram,
        &chat_id,
        &language,
        GenericTestCopy::Telegram,
    )
    .await?;

    let result = provider.send_message(&chat_id, &copy.body).await;
    Ok(Json(TestTelegramResponse {
        success: result.success,
        error: result.error_message,
    }))
}

/// Telegram bot-token Settings (self-hosted only). Never echoes the token.
pub async fn get_telegram_settings(
    AuthenticatedUser(_user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
) -> ApiResult<Json<TelegramSettingsResponse>> {
    reject_telegram_in_cloud_mode(config.as_ref())?;

    Ok(Json(TelegramSettingsResponse {
        configured: is_configured(&app_services.metadata_db).await,
    }))
}

/// Save or clear the instance Telegram bot token (self-hosted only).
pub async fn update_telegram_settings(
    AuthenticatedUser(_user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    Json(payload): Json<UpdateTelegramSettingsRequest>,
) -> ApiResult<Json<TelegramSettingsResponse>> {
    reject_telegram_in_cloud_mode(config.as_ref())?;

    set_bot_token(&app_services.metadata_db, &payload.bot_token)
        .await
        .or_internal("Failed to save Telegram settings")?;
    Ok(Json(TelegramSettingsResponse {
        configured: is_configured(&app_services.metadata_db).await,
    }))
}

/// Send a versioned JSON test payload to a webhook (self-hosted mode only).
pub async fn send_test_webhook_notification(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    Json(payload): Json<TestWebhookRequest>,
) -> ApiResult<Json<TestWebhookResponse>> {
    reject_webhook_in_cloud_mode(config.as_ref())?;

    let url = validate_webhook_url(&payload.url)
        .await
        .map_err(|error| ApiError::bad_request("invalid_webhook_url", error))?;

    let language = user_preferred_language(&app_services, &user.user_id).await;
    let payload_body = resolve_webhook_payload(
        &app_services,
        &user,
        SavedTestRequestIds {
            wallet_checksum: payload.wallet_checksum,
            contact_id: payload.contact_id,
            method_id: payload.method_id,
        },
        &url,
        &language,
    )
    .await?;
    let result = WebhookProvider::new()
        .send_payload(&url, &payload_body)
        .await;

    Ok(Json(TestWebhookResponse {
        success: result.success,
        error: result.error_message,
    }))
}

/// Get the generated Canary Nostr sender public key (self-hosted mode only).
pub async fn get_nostr_settings(
    AuthenticatedUser(_user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
) -> ApiResult<Json<NostrSettingsResponse>> {
    reject_nostr_in_cloud_mode(config.as_ref())?;

    let keys = nostr_sender_keys(&app_services).await?;
    let dm_mode = nostr_dm_mode(&app_services).await?;
    Ok(Json(NostrSettingsResponse {
        sender_npub: keys.sender_npub,
        dm_mode,
    }))
}

/// Update Nostr notification settings (self-hosted mode only).
pub async fn update_nostr_settings(
    AuthenticatedUser(_user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    Json(payload): Json<UpdateNostrSettingsRequest>,
) -> ApiResult<Json<NostrSettingsResponse>> {
    reject_nostr_in_cloud_mode(config.as_ref())?;

    set_nostr_dm_mode(&app_services.metadata_db, payload.dm_mode)
        .await
        .or_internal("Failed to update Nostr settings")?;
    let keys = nostr_sender_keys(&app_services).await?;
    Ok(Json(NostrSettingsResponse {
        sender_npub: keys.sender_npub,
        dm_mode: payload.dm_mode,
    }))
}

/// Send a test Nostr DM to a recipient public key (self-hosted mode only).
pub async fn send_test_nostr_notification(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    Json(payload): Json<TestNostrRequest>,
) -> ApiResult<Json<TestNostrResponse>> {
    reject_nostr_in_cloud_mode(config.as_ref())?;

    let recipient = parse_nostr_recipient_or_error(&payload.recipient)
        .map_err(|error| ApiError::bad_request("invalid_nostr_recipient", error))?;
    let dm_mode = match payload.dm_mode {
        Some(dm_mode) => dm_mode,
        None => nostr_dm_mode(&app_services).await?,
    };
    let language = user_preferred_language(&app_services, &user.user_id).await;
    let message = resolve_nostr_test_message(
        &app_services,
        &user,
        SavedTestRequestIds {
            wallet_checksum: payload.wallet_checksum,
            contact_id: payload.contact_id,
            method_id: payload.method_id,
        },
        &payload.recipient,
        &language,
        dm_mode,
    )
    .await?;
    let start = Instant::now();
    tracing::info!(dm_mode = dm_mode.as_str(), "Sending test Nostr DM");

    let sender_keys = nostr_sender_keys(&app_services).await?;
    let provider = NostrProvider::new(sender_keys);
    let (result, dm_mode_used) = provider
        .send_test_message(recipient, dm_mode, message)
        .await;
    let error_code = nostr_test_error_code(result.error_message.as_deref()).map(str::to_string);

    tracing::info!(
        success = result.success,
        dm_mode = dm_mode.as_str(),
        dm_mode_used = dm_mode_used.map_or("none", NostrDmMode::as_str),
        error_code = error_code.as_deref().unwrap_or("none"),
        elapsed_ms = start.elapsed().as_millis(),
        "Test Nostr DM completed"
    );

    Ok(Json(TestNostrResponse {
        success: result.success,
        dm_mode_used,
        error_code,
        error: result.error_message,
    }))
}

async fn nostr_sender_keys(app_services: &AppServicesState) -> ApiResult<NostrSenderKeys> {
    ensure_nostr_sender_keys(&app_services.metadata_db)
        .await
        .or_internal("Failed to initialize Nostr sender key")
}

async fn nostr_dm_mode(app_services: &AppServicesState) -> ApiResult<NostrDmMode> {
    get_nostr_dm_mode(&app_services.metadata_db)
        .await
        .or_internal("Failed to load Nostr settings")
}

enum GenericTestCopy {
    Ntfy,
    Telegram,
}

async fn user_preferred_language(app_services: &AppServicesState, user_id: &str) -> Language {
    app_services
        .metadata_db
        .get_user_preferred_language(user_id)
        .await
        .unwrap_or(Language::English)
}

enum TestCopyError {
    IncompleteIds,
    Saved(SavedTestConfigError),
}

async fn resolve_test_copy(
    app_services: &AppServicesState,
    user: &AuthUser,
    ids: SavedTestRequestIds,
    provider: ProviderType,
    destination: &str,
    language: &Language,
    generic: GenericTestCopy,
) -> Result<TestNotificationCopy, TestCopyError> {
    match load_optional_saved_config(app_services, user, ids, provider, destination).await? {
        Some(config) => Ok(format_saved_test_notification(&config, *language)),
        None => Ok(match generic {
            GenericTestCopy::Ntfy => format_generic_test_notification(*language),
            GenericTestCopy::Telegram => format_generic_telegram_test_notification(*language),
        }),
    }
}

async fn resolve_webhook_payload(
    app_services: &AppServicesState,
    user: &AuthUser,
    ids: SavedTestRequestIds,
    destination: &str,
    language: &Language,
) -> Result<WebhookPayload, TestCopyError> {
    match load_optional_saved_config(app_services, user, ids, ProviderType::Webhook, destination)
        .await?
    {
        Some(config) => Ok(WebhookPayload::saved_test(*language, &config)),
        None => Ok(WebhookPayload::test(*language)),
    }
}

async fn resolve_nostr_test_message(
    app_services: &AppServicesState,
    user: &AuthUser,
    ids: SavedTestRequestIds,
    destination: &str,
    language: &Language,
    dm_mode: NostrDmMode,
) -> Result<String, TestCopyError> {
    match load_optional_saved_config(app_services, user, ids, ProviderType::Nostr, destination)
        .await?
    {
        Some(config) => Ok(format_saved_nostr_test_message(&config, *language, dm_mode)),
        None => Ok(format_generic_nostr_test_message(*language, dm_mode)),
    }
}

async fn load_optional_saved_config(
    app_services: &AppServicesState,
    user: &AuthUser,
    ids: SavedTestRequestIds,
    provider: ProviderType,
    destination: &str,
) -> Result<Option<TestNotificationConfig>, TestCopyError> {
    let ids = ids.parse().map_err(|_| TestCopyError::IncompleteIds)?;
    let Some(ids) = ids else {
        return Ok(None);
    };

    load_saved_test_config(
        &app_services.metadata_db,
        &user.user_id,
        user.is_admin,
        &ids,
        provider,
        destination,
    )
    .await
    .map(Some)
    .map_err(TestCopyError::Saved)
}

impl From<TestCopyError> for ApiError {
    fn from(error: TestCopyError) -> Self {
        match error {
            TestCopyError::IncompleteIds => ApiError::bad_request(
                "incomplete_saved_test_ids",
                "wallet_checksum, contact_id, and method_id are all required when testing a saved destination",
            ),
            TestCopyError::Saved(error) => saved_test_config_error(error),
        }
    }
}

fn saved_test_config_error(error: SavedTestConfigError) -> ApiError {
    match error {
        SavedTestConfigError::WalletNotFound => {
            ApiError::not_found("wallet_not_found", "Wallet not found")
        }
        SavedTestConfigError::AccessDenied => ApiError::forbidden("access_denied", "Access denied"),
        SavedTestConfigError::ContactNotFound => {
            ApiError::not_found("contact_not_found", "Contact not found")
        }
        SavedTestConfigError::MethodNotFound => ApiError::not_found(
            "notification_method_not_found",
            "Notification method not found",
        ),
        SavedTestConfigError::MethodDisabled => ApiError::bad_request(
            "notification_method_disabled",
            "Notification method is disabled",
        ),
        SavedTestConfigError::ProviderMismatch => ApiError::bad_request(
            "notification_method_provider_mismatch",
            "Notification method does not match this test endpoint",
        ),
        SavedTestConfigError::DestinationMismatch => ApiError::bad_request(
            "notification_target_mismatch",
            "Destination does not match the saved notification method",
        ),
        SavedTestConfigError::Database(error) => ApiError::internal(
            "Failed to load saved notification method",
            anyhow::Error::msg(error),
        ),
    }
}
