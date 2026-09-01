//! Test notification handler

use crate::api::AppServicesState;
use crate::config::AppConfig;
use crate::extractors::AuthenticatedUser;
use crate::handlers::helpers::{reject_nostr_in_cloud_mode, reject_webhook_in_cloud_mode};
use crate::models::{
    ErrorResponse, NostrSettingsResponse, TestNostrRequest, TestNostrResponse, TestNtfyRequest,
    TestNtfyResponse, TestWebhookRequest, TestWebhookResponse, UpdateNostrSettingsRequest,
};
use crate::nostr_provider::{
    ensure_nostr_sender_keys, get_nostr_dm_mode, nostr_test_error_code,
    parse_nostr_recipient_or_error, set_nostr_dm_mode, NostrProvider,
};
use crate::ntfy_provider::NtfyAuth;
use crate::outbound_target::{client_for_public_url, validate_public_url};
use crate::webhook_provider::{validate_webhook_url, WebhookPayload, WebhookProvider};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rust_i18n::t;
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

    // Look up user's preferred language
    let language = app_services
        .metadata_db
        .get_user_preferred_language(&user.user_id)
        .await
        .unwrap_or(crate::metadata::Language::English);
    let locale = language.as_str();

    // Build localized title and message
    let title = t!("test_notification.title", locale = locale).to_string();
    let message = t!("test_notification.message", locale = locale).to_string();

    // Build ntfy URL
    let ntfy_url = format!("{}/{}", ntfy_server.trim_end_matches('/'), topic);

    // Build and send the HTTP request
    let client =
        if config.should_trust_ntfy_server_url(&ntfy_server, user_ntfy_server_url.as_deref()) {
            reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("failed to build ntfy HTTP client")
        } else {
            match validate_public_url(&ntfy_url).await {
                Ok(url) => match client_for_public_url(&url).await {
                    Ok(client) => client,
                    Err(_) => {
                        return (
                            StatusCode::OK,
                            Json(TestNtfyResponse {
                                success: false,
                                error: Some("ntfy server is not publicly reachable".to_string()),
                            }),
                        )
                            .into_response()
                    }
                },
                Err(_) => {
                    return (
                        StatusCode::OK,
                        Json(TestNtfyResponse {
                            success: false,
                            error: Some("ntfy server is not publicly reachable".to_string()),
                        }),
                    )
                        .into_response()
                }
            }
        };
    let mut request = client
        .post(&ntfy_url)
        .header("Content-Type", "text/plain; charset=utf-8")
        .header("Title", title)
        .header("Priority", "default")
        .header("Tags", "bell");

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

    let language = app_services
        .metadata_db
        .get_user_preferred_language(&user.user_id)
        .await
        .unwrap_or(crate::metadata::Language::English);
    let result = WebhookProvider::new()
        .send_payload(&url, &WebhookPayload::test(&language))
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
    AuthenticatedUser(_user): AuthenticatedUser,
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
    let recipient_hex = recipient.to_hex();
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
    let start = Instant::now();
    tracing::info!(
        recipient = %recipient_hex,
        dm_mode = dm_mode.as_str(),
        "Sending test Nostr DM"
    );

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
    let (result, dm_mode_used) = provider.send_test_message(recipient, dm_mode).await;
    let error_code = nostr_test_error_code(result.error_message.as_deref()).map(str::to_string);

    tracing::info!(
        recipient = %recipient_hex,
        success = result.success,
        dm_mode = dm_mode.as_str(),
        dm_mode_used = dm_mode_used.map(|mode| mode.as_str()).unwrap_or("none"),
        error_code = error_code.as_deref().unwrap_or("none"),
        error = result.error_message.as_deref().unwrap_or("none"),
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
