//! Forgotten and reset passwords.

use super::rate_limit::{
    client_ip, enforce_email_rate_limit, enforce_ip_rate_limit, pad_enumeration_response,
    AUTH_IP_RATE_LIMIT_WINDOW_MINUTES, FORGOT_PASSWORD_EMAIL_RATE_LIMIT, MAX_AUTH_REQUESTS_PER_IP,
};
use super::{message_json, shared_auth_service, validate_password_length};
use crate::api::{AppServicesState, AuthServiceState};
use crate::auth::{AuthService, ForgotPasswordRequest, ResetPasswordRequest};
use crate::config::AppConfig;
use crate::handlers::error::{ApiError, ApiResult, OrInternal};
use crate::metadata::{MetadataDb, UserRecord};
use anyhow::Context;
use axum::{
    extract::{ConnectInfo, Extension, Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use std::{net::SocketAddr, sync::Arc, time::Instant};
use tracing::info;

const FORGOT_PASSWORD_SUCCESS_MESSAGE: &str =
    "If an account with that email exists, a password reset link has been sent.";

pub(crate) async fn update_password_and_revoke_sessions(
    metadata_db: &MetadataDb,
    user_id: &str,
    password_hash: &str,
) -> anyhow::Result<()> {
    metadata_db
        .update_user_password_and_revoke_sessions(user_id, password_hash)
        .await?;
    Ok(())
}

/// Forgot password endpoint
pub async fn forgot_password(
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    State(auth_service): State<AuthServiceState>,
    address: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(request): Json<ForgotPasswordRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if config.is_self_hosted_mode() {
        return Err(ApiError::uncoded(
            StatusCode::FORBIDDEN,
            "Password reset is unavailable in self-hosted mode",
        ));
    }

    let start_time = Instant::now();
    enforce_ip_rate_limit(
        &app_services,
        &config,
        "forgot_password",
        client_ip(&headers, address, &config),
        MAX_AUTH_REQUESTS_PER_IP,
        AUTH_IP_RATE_LIMIT_WINDOW_MINUTES,
        true,
    )
    .await?;
    enforce_email_rate_limit(
        &app_services,
        &FORGOT_PASSWORD_EMAIL_RATE_LIMIT,
        &request.email,
    )
    .await?;

    let user_record = app_services
        .metadata_db
        .get_user_by_email(&request.email)
        .await
        .or_internal("Failed to check user")?;

    // Every outcome from here on is padded, so response timing does not
    // reveal whether an account exists for the email.
    let Some(user_record) = user_record else {
        pad_enumeration_response(start_time).await;
        info!(
            "forgot_password (unknown email) completed in {:?}",
            start_time.elapsed()
        );
        return Ok(message_json(FORGOT_PASSWORD_SUCCESS_MESSAGE));
    };

    let sent = send_password_reset(&app_services, &config, &auth_service, user_record).await;
    pad_enumeration_response(start_time).await;
    sent?;
    info!("forgot_password completed in {:?}", start_time.elapsed());
    Ok(message_json(FORGOT_PASSWORD_SUCCESS_MESSAGE))
}

async fn send_password_reset(
    app_services: &AppServicesState,
    config: &AppConfig,
    auth_service: &AuthServiceState,
    user_record: UserRecord,
) -> ApiResult<()> {
    let auth_service = shared_auth_service(auth_service, config)?;
    let token = AuthService::generate_verification_token();
    let token_hash = AuthService::hash_token(&token);

    // Only the hash is stored, so a database leak does not expose usable tokens.
    app_services
        .metadata_db
        .create_password_reset_token(&user_record.id, &token_hash)
        .await
        .or_internal("Failed to create reset token")?;

    let user_language = user_record.preferred_language.as_deref().unwrap_or("en-US");
    auth_service
        .send_password_reset(
            &user_record.email,
            user_record.name.as_deref().unwrap_or("User"),
            &token,
            user_language,
        )
        .await
        .or_internal("Failed to send reset email")
}

/// Reset password endpoint
pub async fn reset_password(
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    Path(token): Path<String>,
    Json(request): Json<ResetPasswordRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if config.is_self_hosted_mode() {
        return Err(ApiError::uncoded(
            StatusCode::FORBIDDEN,
            "Password reset is unavailable in self-hosted mode",
        ));
    }

    let start_time = Instant::now();
    validate_password_length(&request.password)?;

    // Hash the incoming token for verification (tokens are stored hashed)
    let token_hash = AuthService::hash_token(&token);
    let user_id = app_services
        .metadata_db
        .verify_password_reset_token(&token_hash)
        .await
        .or_internal("Failed to verify token")?
        .ok_or_else(|| {
            ApiError::bad_request("invalid_reset_token", "Invalid or expired reset token")
        })?;

    let password_hash = AuthService::hash_password_async(&request.password)
        .await
        .or_internal("Failed to hash password")?;
    update_password_and_revoke_sessions(&app_services.metadata_db, &user_id, &password_hash)
        .await
        .with_context(|| format!("Failed to complete password reset for user {user_id}"))
        .or_internal("Failed to complete password reset")?;

    info!("reset_password completed in {:?}", start_time.elapsed());
    Ok(message_json(
        "Password reset successfully. You can now log in with your new password.",
    ))
}
