//! Custom Axum extractors for authentication

use crate::auth::{authenticate_user, AuthUser};
use crate::config::AppConfig;
use crate::handlers::extract_token_from_cookies;
use crate::models::ErrorResponse;
use axum::{
    extract::{FromRef, FromRequestParts},
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Json, Response},
};
use std::sync::Arc;

/// Custom extractor that handles authentication in both cloud and self-hosted modes.
///
/// In self-hosted mode, returns a hardcoded admin user.
/// In cloud mode, validates the JWT token from HttpOnly cookie (preferred) or Authorization header.
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
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let config = <Arc<AppConfig> as FromRef<S>>::from_ref(state);

        if config.is_self_hosted_mode() {
            // Self-hosted mode: return hardcoded admin user
            Ok(AuthenticatedUser(AuthUser {
                user_id: "foss-user".to_string(),
                is_admin: true,
                is_demo: false,
            }))
        } else {
            // Cloud mode: authenticate using JWT from cookie (preferred) or Authorization header
            let jwt_secret = config.get_jwt_secret().map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Configuration error: {}", e),
                    }),
                )
                    .into_response()
            })?;

            // Extract token from cookie (HttpOnly, secure)
            let cookie_token = extract_token_from_cookies(&parts.headers);

            // Fall back to Authorization header for backwards compatibility
            let auth_header = parts
                .headers
                .get("authorization")
                .and_then(|h| h.to_str().ok());

            authenticate_user(auth_header, cookie_token.as_deref(), jwt_secret)
                .map(AuthenticatedUser)
                .map_err(|_| {
                    (
                        StatusCode::UNAUTHORIZED,
                        Json(ErrorResponse {
                            error: "Authentication required".to_string(),
                        }),
                    )
                        .into_response()
                })
        }
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
pub fn require_non_demo(user: &AuthUser) -> Result<(), Response> {
    if user.is_demo {
        Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "Demo account is read-only. Sign up to create your own wallet at https://canarybitcoin.com".to_string(),
            }),
        )
            .into_response())
    } else {
        Ok(())
    }
}
