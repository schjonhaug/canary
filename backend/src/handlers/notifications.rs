//! Test notification handler

use crate::api::AppServicesState;
use crate::config::AppConfig;
use crate::extractors::AuthenticatedUser;
use crate::models::{ErrorResponse, TestNtfyRequest, TestNtfyResponse};
use crate::ntfy_provider::{NtfyAuth, NtfyProvider};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rust_i18n::t;
use std::sync::Arc;

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
    let should_use_ntfy_auth =
        config.should_use_ntfy_auth_for_url(&ntfy_server, user_ntfy_server_url.as_deref());

    // Look up user's ntfy auth credentials
    let mut ntfy_auth = if should_use_ntfy_auth {
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
    if matches!(ntfy_auth, NtfyAuth::None) {
        if let Some(token) =
            config.managed_ntfy_access_token_for_url(&ntfy_server, user_ntfy_server_url.as_deref())
        {
            ntfy_auth = NtfyAuth::AccessToken(token);
        }
    }

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
    let client = NtfyProvider::default_client();
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
                let body = response.text().await.unwrap_or_default();
                // Try to extract "error" field from ntfy JSON responses
                let detail = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                    json["error"].as_str().unwrap_or(&body).to_string()
                } else if body.is_empty() {
                    status.canonical_reason().unwrap_or("Unknown").to_string()
                } else {
                    body
                };
                let error = format!("HTTP {}: {}", status.as_u16(), detail);
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
        Err(e) => (
            StatusCode::OK,
            Json(TestNtfyResponse {
                success: false,
                error: Some(format!("Request failed: {}", e)),
            }),
        )
            .into_response(),
    }
}
