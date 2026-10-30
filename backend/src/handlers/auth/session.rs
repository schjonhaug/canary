//! Signed-in sessions: issuing a token and session at sign-in, logout, and
//! reading or updating the signed-in user's profile.

use super::cookies::{build_auth_cookie, build_clear_auth_cookie, extract_token_from_cookies};
use super::message_json;
use crate::api::AppServicesState;
use crate::auth::{
    AuthResponse, AuthService, AuthUserResponse, UpdateUserRequest, UpdateUserResponse,
};
use crate::config::{AppConfig, OperatingMode};
use crate::extractors::AuthenticatedUser;
use crate::handlers::error::{ApiError, ApiResult, OrInternal};
use crate::handlers::helpers::get_user_or_error;
use crate::metadata::UserRecord;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Json, Response},
};
use std::{sync::Arc, time::Instant};
use tracing::info;

pub(super) fn auth_user_response(user: UserRecord) -> AuthUserResponse {
    AuthUserResponse {
        id: user.id,
        email: user.email,
        name: user.name,
        is_admin: user.is_admin,
        is_demo: user.is_demo,
        email_verified: user.email_verified,
        subscription_tier: user.subscription_tier,
        created_at: user.created_at,
        preferred_fiat_currency: user.preferred_fiat_currency,
        preferred_language: user.preferred_language,
    }
}

pub(super) fn generate_token(auth_service: &AuthService, user: &UserRecord) -> ApiResult<String> {
    auth_service
        .generate_token(&user.id, &user.email, user.is_admin, user.is_demo)
        .or_internal("Failed to generate token")
}

/// Stores a seven-day session for `token` and returns the token hash.
pub(super) async fn create_session(
    app_services: &AppServicesState,
    user_id: &str,
    token: &str,
) -> ApiResult<String> {
    let token_hash = AuthService::hash_token(token);
    let expires_at = chrono::Utc::now() + chrono::Duration::days(7);
    app_services
        .metadata_db
        .create_session(user_id, &token_hash, expires_at)
        .await
        .or_internal("Failed to create session")?;
    Ok(token_hash)
}

/// Returns the session token both in an `HttpOnly` cookie and in the body.
///
/// Web browsers use the cookie, which scripts cannot read. CLI and mobile
/// clients use the token from the body with an Authorization header.
pub(super) fn signed_in_response(token: String, user: UserRecord, mode: OperatingMode) -> Response {
    let cookie = build_auth_cookie(&token, mode);
    let body = AuthResponse {
        token,
        user: auth_user_response(user),
        requires_name: None,
    };
    ([(header::SET_COOKIE, cookie)], Json(body)).into_response()
}

/// The session token from the auth cookie, or else from a Bearer
/// Authorization header for clients that predate the cookie.
fn session_token(headers: &HeaderMap) -> ApiResult<String> {
    if let Some(cookie_token) = extract_token_from_cookies(headers) {
        return Ok(cookie_token);
    }
    let auth_header = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| ApiError::uncoded(StatusCode::UNAUTHORIZED, "Authentication required"))?;
    auth_header
        .strip_prefix("Bearer ")
        .map(str::to_string)
        .ok_or_else(|| ApiError::uncoded(StatusCode::UNAUTHORIZED, "Invalid authorization header"))
}

/// Logout endpoint
pub async fn logout(
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let start_time = Instant::now();
    let token = session_token(&headers)?;

    // Hash the token to find it in the database
    let token_hash = AuthService::hash_token(&token);
    let deleted = app_services.metadata_db.delete_session(&token_hash).await;
    info!("logout completed in {:?}", start_time.elapsed());

    // Always clear the cookie, even if session deletion fails
    let clear_cookie = build_clear_auth_cookie(config.operating_mode);
    let body = deleted
        .map(|()| message_json("Logged out successfully"))
        .or_internal("Failed to delete session");
    Ok(([(header::SET_COOKIE, clear_cookie)], body).into_response())
}

/// Get current user info endpoint
pub async fn me(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
) -> ApiResult<Json<serde_json::Value>> {
    let start_time = Instant::now();

    let db_user = get_user_or_error(
        &app_services,
        &user.user_id,
        None,
        "User not found",
        "Failed to get user info",
    )
    .await?;
    let user_info = AuthUserResponse {
        is_admin: user.is_admin,
        is_demo: user.is_demo,
        ..auth_user_response(db_user)
    };

    info!("me completed in {:?}", start_time.elapsed());
    Ok(Json(serde_json::json!({ "user": user_info })))
}

/// Update user info endpoint
pub async fn update_user(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    Json(request): Json<UpdateUserRequest>,
) -> ApiResult<Json<UpdateUserResponse>> {
    let start_time = Instant::now();

    let name = request.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request(
            "name_required",
            "Name cannot be empty",
        ));
    }

    app_services
        .metadata_db
        .update_user_name(&user.user_id, name)
        .await
        .or_internal("Failed to update user")?;

    let db_user = get_user_or_error(
        &app_services,
        &user.user_id,
        None,
        "User not found",
        "Failed to get user",
    )
    .await?;
    let user_info = AuthUserResponse {
        is_admin: user.is_admin,
        is_demo: user.is_demo,
        ..auth_user_response(db_user)
    };

    info!("update_user completed in {:?}", start_time.elapsed());
    Ok(Json(UpdateUserResponse { user: user_info }))
}
