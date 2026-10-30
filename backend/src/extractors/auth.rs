//! Custom Axum extractors for authentication

use crate::api::AppServicesState;
use crate::auth::{authenticate_user_with_token, AuthError, AuthUser};
use crate::config::AppConfig;
use crate::handlers::{extract_token_from_cookies, ApiError, ApiResult};
use axum::{
    extract::{FromRef, FromRequestParts},
    http::{request::Parts, StatusCode},
};
use std::sync::Arc;

/// Custom extractor that validates authentication for both cloud and self-hosted modes.
///
/// Validates the JWT token from `HttpOnly` cookie (preferred) or Authorization header.
///
/// # Usage
/// ```rust,ignore
/// async fn my_handler(
///     AuthenticatedUser(user): AuthenticatedUser,
///     State(app_services): State<AppServicesState>,
/// ) -> ApiResult<Json<Wallet>> {
///     // user is already authenticated
/// }
/// ```
pub struct AuthenticatedUser(pub AuthUser);

impl<S> FromRequestParts<S> for AuthenticatedUser
where
    Arc<AppConfig>: FromRef<S>,
    AppServicesState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let config = <Arc<AppConfig> as FromRef<S>>::from_ref(state);
        let app_services = <AppServicesState as FromRef<S>>::from_ref(state);

        let jwt_secret = config
            .get_jwt_secret()
            .map_err(|e| ApiError::internal("Configuration error", anyhow::anyhow!(e)))?;

        let cookie_token = extract_token_from_cookies(&parts.headers);
        let auth_header = parts
            .headers
            .get("authorization")
            .and_then(|h| h.to_str().ok());
        let (user, token_hash) = authenticate_user_with_token(
            &app_services.metadata_db,
            auth_header,
            cookie_token.as_deref(),
            jwt_secret,
        )
        .await
        .map_err(|err| match err {
            AuthError::Unauthorized => {
                ApiError::uncoded(StatusCode::UNAUTHORIZED, "Authentication required")
            }
            AuthError::Internal(inner) => {
                ApiError::internal("Authentication service unavailable", inner)
            }
        })?;
        if config.is_cloud_mode()
            && user.is_admin
            && !crate::admin_mfa::session_is_recent(
                &app_services.metadata_db,
                config.admin_mfa_secrets_file(),
                &user.user_id,
                &token_hash,
            )
            .await
            .unwrap_or(false)
        {
            return Err(ApiError::unauthorized(
                "admin_reauthentication_required",
                "Sign in again with your password and authenticator code.",
            ));
        }
        Ok(AuthenticatedUser(user))
    }
}

/// Guard that rejects demo users from performing write operations.
///
/// Returns an error response if the user is a demo account.
///
/// # Usage
/// ```rust,ignore
/// async fn my_handler(
///     AuthenticatedUser(user): AuthenticatedUser,
/// ) -> ApiResult<StatusCode> {
///     require_non_demo(&user)?;
///     // Continue with the operation
/// }
/// ```
pub fn require_non_demo(user: &AuthUser) -> ApiResult<()> {
    if user.is_demo {
        Err(ApiError::forbidden(
            "demo_read_only",
            "Demo account is read-only. Sign up to create your own wallet at https://canarybitcoin.com",
        ))
    } else {
        Ok(())
    }
}
