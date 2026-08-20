//! Custom Axum extractors for authentication

use crate::api::AppServicesState;
use crate::auth::{authenticate_user, AuthError, AuthUser};
use crate::config::AppConfig;
use crate::handlers::extract_token_from_cookies;
use crate::models::ErrorResponse;
use axum::{
    extract::{FromRef, FromRequestParts},
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Json, Response},
};
use std::sync::Arc;

/// Custom extractor that validates authentication for both cloud and self-hosted modes.
///
/// Validates the JWT token from HttpOnly cookie (preferred) or Authorization header.
///
/// # Usage
/// ```rust,ignore
/// async fn my_handler(
///     AuthenticatedUser(user): AuthenticatedUser,
///     State(app_services): State<AppServicesState>,
/// ) -> Response {
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
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let config = <Arc<AppConfig> as FromRef<S>>::from_ref(state);
        let app_services = <AppServicesState as FromRef<S>>::from_ref(state);

        let jwt_secret = config.get_jwt_secret().map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!("Configuration error: {}", e))),
            )
                .into_response()
        })?;

        let cookie_token = extract_token_from_cookies(&parts.headers);
        let auth_header = parts
            .headers
            .get("authorization")
            .and_then(|h| h.to_str().ok());
        authenticate_user(
            &app_services.metadata_db,
            auth_header,
            cookie_token.as_deref(),
            jwt_secret,
        )
        .await
        .map(AuthenticatedUser)
        .map_err(|err| match err {
            AuthError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::new("Authentication required")),
            )
                .into_response(),
            AuthError::Internal(inner) => {
                tracing::error!("Session validation failed: {}", inner);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("Authentication service unavailable")),
                )
                    .into_response()
            }
        })
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
/// ) -> Response {
///     require_non_demo(&user)?;
///     // Continue with the operation
/// }
/// ```
#[allow(clippy::result_large_err)]
pub fn require_non_demo(user: &AuthUser) -> Result<(), Response> {
    if user.is_demo {
        Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::coded("demo_read_only", "Demo account is read-only. Sign up to create your own wallet at https://canarybitcoin.com")),
        )
            .into_response())
    } else {
        Ok(())
    }
}
