//! Test notification handler

use crate::api::AppServicesState;
use crate::auth::AuthUser;
use crate::config::AppConfig;
use crate::extractors::AuthenticatedUser;
use crate::handlers::helpers::{reject_nostr_in_cloud_mode, reject_webhook_in_cloud_mode};
use crate::metadata::{Language, ProviderType};
use crate::models::{
    ErrorResponse, NostrSettingsResponse, TestNostrRequest, TestNostrResponse, TestNtfyRequest,
    TestNtfyResponse, TestWebhookRequest, TestWebhookResponse, UpdateNostrSettingsRequest,
};
use crate::nostr_provider::{
    ensure_nostr_sender_keys, get_nostr_dm_mode, nostr_test_error_code,
    parse_nostr_recipient_or_error, set_nostr_dm_mode, NostrDmMode, NostrProvider,
};
use crate::ntfy_provider::NtfyAuth;
use crate::test_notification::{
    format_generic_nostr_test_message, format_generic_test_notification,
    format_saved_nostr_test_message, format_saved_test_notification, load_saved_test_config,
    SavedTestConfigError, SavedTestRequestIds, TestNotificationConfig, TestNotificationCopy,
};
use crate::webhook_provider::{validate_webhook_url, WebhookPayload, WebhookProvider};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use std::sync::Arc;
use std::time::Instant;

/// Send a test notification to an ntfy topic (self-hosted mode only)
pub async fn send_test_ntfy_notification(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    Json(payload): Json<TestNtfyRequest>,
) -> Response {
    // Only available in self-hosted mode
    if !config.is_self_hosted_mode() {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new(
                "Test notifications are only available in self-hosted mode",
            )),
        )
            .into_response();
    }

    // Validate topic
    let topic = payload.topic.trim();
    if topic.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("ntfy topic cannot be empty")),
        )
            .into_response();
    }
    if topic.len() > 64 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(
                "ntfy topic must be at most 64 characters",
            )),
        )
            .into_response();
    }
    if !topic
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(
                "ntfy topic can only contain letters, numbers, dashes, and underscores",
            )),
        )
            .into_response();
    }

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
    let copy = match resolve_test_copy(
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
    .await
    {
        Ok(copy) => copy,
        Err(error) => return test_copy_error_response(error),
    };
    let title = copy.title;
    let message = copy.body;

    // Build ntfy URL
    let ntfy_url = format!("{}/{}", ntfy_server.trim_end_matches('/'), topic);

    // Use the same mode policy and client construction as regular delivery.
    let provider = config.ntfy_provider(
        ntfy_server,
        ntfy_auth.clone(),
        user_ntfy_server_url.as_deref(),
    );
    let client = match provider.client_for_url(&ntfy_url).await {
        Ok(client) => client,
        Err(_) => {
            return (
                StatusCode::OK,
                Json(TestNtfyResponse {
                    success: false,
                    error: Some("ntfy server URL is invalid or not allowed.".to_string()),
                }),
            )
                .into_response();
        }
    };
    let mut request = client
        .post(&ntfy_url)
        .header("Content-Type", "text/plain; charset=utf-8")
        .header("Title", title)
        .header("Priority", "urgent")
        .header("Tags", "rotating_light");

    // Add authentication header if configured
    match &ntfy_auth {
        NtfyAuth::None => {}
        NtfyAuth::AccessToken(token) => {
            request = request.header("Authorization", format!("Bearer {}", token));
        }
        NtfyAuth::BasicAuth { username, password } => {
            let credentials = format!("{}:{}", username, password);
            let encoded = BASE64.encode(credentials.as_bytes());
            request = request.header("Authorization", format!("Basic {}", encoded));
        }
    }

    match request.body(message).send().await {
        Ok(response) => {
            if response.status().is_success() {
                (
                    StatusCode::OK,
                    Json(TestNtfyResponse {
                        success: true,
                        error: None,
                    }),
                )
                    .into_response()
            } else {
                let status = response.status();
                let error = format!("HTTP {}", status.as_u16());
                (
                    StatusCode::OK,
                    Json(TestNtfyResponse {
                        success: false,
                        error: Some(error),
                    }),
                )
                    .into_response()
            }
        }
        Err(_) => (
            StatusCode::OK,
            Json(TestNtfyResponse {
                success: false,
                error: Some("Request to ntfy server failed".to_string()),
            }),
        )
            .into_response(),
    }
}

/// Send a versioned JSON test payload to a webhook (self-hosted mode only).
pub async fn send_test_webhook_notification(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    Json(payload): Json<TestWebhookRequest>,
) -> Response {
    if let Some(response) = reject_webhook_in_cloud_mode(config.as_ref()) {
        return response;
    }

    let url = match validate_webhook_url(&payload.url).await {
        Ok(url) => url,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::coded("invalid_webhook_url", error)),
            )
                .into_response();
        }
    };

    let language = user_preferred_language(&app_services, &user.user_id).await;
    let payload_body = match resolve_webhook_payload(
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
    .await
    {
        Ok(payload_body) => payload_body,
        Err(error) => return test_copy_error_response(error),
    };
    let result = WebhookProvider::new()
        .send_payload(&url, &payload_body)
        .await;

    (
        StatusCode::OK,
        Json(TestWebhookResponse {
            success: result.success,
            error: result.error_message,
        }),
    )
        .into_response()
}

/// Get the generated Canary Nostr sender public key (self-hosted mode only).
pub async fn get_nostr_settings(
    AuthenticatedUser(_user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
) -> Response {
    if let Some(response) = reject_nostr_in_cloud_mode(config.as_ref()) {
        return response;
    }

    let keys = match ensure_nostr_sender_keys(&app_services.metadata_db).await {
        Ok(keys) => keys,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to initialize Nostr sender key: {}",
                    e
                ))),
            )
                .into_response();
        }
    };

    let dm_mode = match get_nostr_dm_mode(&app_services.metadata_db).await {
        Ok(dm_mode) => dm_mode,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to load Nostr settings: {}",
                    e
                ))),
            )
                .into_response();
        }
    };

    (
        StatusCode::OK,
        Json(NostrSettingsResponse {
            sender_npub: keys.sender_npub,
            dm_mode,
        }),
    )
        .into_response()
}

/// Update Nostr notification settings (self-hosted mode only).
pub async fn update_nostr_settings(
    AuthenticatedUser(_user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    Json(payload): Json<UpdateNostrSettingsRequest>,
) -> Response {
    if let Some(response) = reject_nostr_in_cloud_mode(config.as_ref()) {
        return response;
    }

    match set_nostr_dm_mode(&app_services.metadata_db, payload.dm_mode).await {
        Ok(()) => (
            StatusCode::OK,
            Json(NostrSettingsResponse {
                sender_npub: match ensure_nostr_sender_keys(&app_services.metadata_db).await {
                    Ok(keys) => keys.sender_npub,
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse::new(format!(
                                "Failed to initialize Nostr sender key: {}",
                                e
                            ))),
                        )
                            .into_response();
                    }
                },
                dm_mode: payload.dm_mode,
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(format!(
                "Failed to update Nostr settings: {}",
                e
            ))),
        )
            .into_response(),
    }
}

/// Send a test Nostr DM to a recipient public key (self-hosted mode only).
pub async fn send_test_nostr_notification(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    Json(payload): Json<TestNostrRequest>,
) -> Response {
    if let Some(response) = reject_nostr_in_cloud_mode(config.as_ref()) {
        return response;
    }

    let recipient = match parse_nostr_recipient_or_error(&payload.recipient) {
        Ok(recipient) => recipient,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::coded("invalid_nostr_recipient", e)),
            )
                .into_response();
        }
    };
    let dm_mode = match payload.dm_mode {
        Some(dm_mode) => dm_mode,
        None => match get_nostr_dm_mode(&app_services.metadata_db).await {
            Ok(dm_mode) => dm_mode,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(format!(
                        "Failed to load Nostr settings: {}",
                        e
                    ))),
                )
                    .into_response();
            }
        },
    };
    let language = user_preferred_language(&app_services, &user.user_id).await;
    let message = match resolve_nostr_test_message(
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
    .await
    {
        Ok(message) => message,
        Err(error) => return test_copy_error_response(error),
    };
    let start = Instant::now();
    tracing::info!(dm_mode = dm_mode.as_str(), "Sending test Nostr DM");

    let sender_keys = match ensure_nostr_sender_keys(&app_services.metadata_db).await {
        Ok(keys) => keys,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to initialize Nostr sender key: {}",
                    e
                ))),
            )
                .into_response();
        }
    };

    let provider = NostrProvider::new(sender_keys);
    let (result, dm_mode_used) = provider
        .send_test_message(recipient, dm_mode, message)
        .await;
    let error_code = nostr_test_error_code(result.error_message.as_deref()).map(str::to_string);

    tracing::info!(
        success = result.success,
        dm_mode = dm_mode.as_str(),
        dm_mode_used = dm_mode_used.map(|mode| mode.as_str()).unwrap_or("none"),
        error_code = error_code.as_deref().unwrap_or("none"),
        elapsed_ms = start.elapsed().as_millis(),
        "Test Nostr DM completed"
    );

    (
        StatusCode::OK,
        Json(TestNostrResponse {
            success: result.success,
            dm_mode_used,
            error_code,
            error: result.error_message,
        }),
    )
        .into_response()
}

enum GenericTestCopy {
    Ntfy,
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
        Some(config) => Ok(format_saved_test_notification(&config, language)),
        None => Ok(match generic {
            GenericTestCopy::Ntfy => format_generic_test_notification(language),
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
        Some(config) => Ok(WebhookPayload::saved_test(language, &config)),
        None => Ok(WebhookPayload::test(language)),
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
        Some(config) => Ok(format_saved_nostr_test_message(&config, language, dm_mode)),
        None => Ok(format_generic_nostr_test_message(language, dm_mode)),
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

fn test_copy_error_response(error: TestCopyError) -> Response {
    match error {
        TestCopyError::IncompleteIds => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::coded(
                "incomplete_saved_test_ids",
                "wallet_checksum, contact_id, and method_id are all required when testing a saved destination",
            )),
        )
            .into_response(),
        TestCopyError::Saved(error) => saved_test_config_error_response(error),
    }
}

fn saved_test_config_error_response(error: SavedTestConfigError) -> Response {
    let (status, code, message) = match error {
        SavedTestConfigError::WalletNotFound => (
            StatusCode::NOT_FOUND,
            "wallet_not_found",
            "Wallet not found".to_string(),
        ),
        SavedTestConfigError::AccessDenied => (
            StatusCode::FORBIDDEN,
            "access_denied",
            "Access denied".to_string(),
        ),
        SavedTestConfigError::ContactNotFound => (
            StatusCode::NOT_FOUND,
            "contact_not_found",
            "Contact not found".to_string(),
        ),
        SavedTestConfigError::MethodNotFound => (
            StatusCode::NOT_FOUND,
            "notification_method_not_found",
            "Notification method not found".to_string(),
        ),
        SavedTestConfigError::MethodDisabled => (
            StatusCode::BAD_REQUEST,
            "notification_method_disabled",
            "Notification method is disabled".to_string(),
        ),
        SavedTestConfigError::ProviderMismatch => (
            StatusCode::BAD_REQUEST,
            "notification_method_provider_mismatch",
            "Notification method does not match this test endpoint".to_string(),
        ),
        SavedTestConfigError::DestinationMismatch => (
            StatusCode::BAD_REQUEST,
            "notification_target_mismatch",
            "Destination does not match the saved notification method".to_string(),
        ),
        SavedTestConfigError::Database(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(error)),
            )
                .into_response();
        }
    };

    (status, Json(ErrorResponse::coded(code, message))).into_response()
}
