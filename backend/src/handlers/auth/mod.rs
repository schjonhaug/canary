//! Authentication and user management handlers.
//!
//! - `register`: sign-up and email verification
//! - `login`: password login, cloud administrator MFA and demo login
//! - `password`: forgotten and reset passwords
//! - `session`: issuing sessions, logout and the signed-in user's profile
//! - `cookies`: the `HttpOnly` auth cookie
//! - `rate_limit`: per-address and per-email limits and response padding

mod cookies;
mod login;
mod password;
mod rate_limit;
mod register;
mod session;

pub use cookies::extract_token_from_cookies;
pub use login::{demo_login, login};
// Only the unit tests in `crate::tests::auth` call this from outside `password`.
#[cfg(test)]
pub(crate) use password::update_password_and_revoke_sessions;
pub use password::{forgot_password, reset_password};
pub(super) use rate_limit::{client_ip, enforce_ip_rate_limit};
pub use register::{register, verify_email};
pub use session::{logout, me, update_user};

use super::error::{ApiError, ApiResult};
use crate::api::AuthServiceState;
use crate::auth::AuthService;
use crate::config::AppConfig;
use axum::{http::StatusCode, response::Json};

const MIN_PASSWORD_LENGTH: usize = 12;

/// The configured JWT secret, or 503 when the instance has none.
fn jwt_secret(config: &AppConfig) -> ApiResult<&str> {
    config
        .get_jwt_secret()
        .map_err(|e| ApiError::uncoded(StatusCode::SERVICE_UNAVAILABLE, e))
}

/// The shared auth service, or the `JWT_SECRET` error it was not built for.
pub(crate) fn shared_auth_service<'a>(
    auth_service: &'a AuthServiceState,
    config: &AppConfig,
) -> ApiResult<&'a AuthService> {
    jwt_secret(config)?;
    auth_service.as_deref().ok_or_else(|| {
        ApiError::uncoded(
            StatusCode::SERVICE_UNAVAILABLE,
            "Authentication is not configured",
        )
    })
}

fn message_json(message: &str) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "message": message }))
}

fn validate_password_length(password: &str) -> ApiResult<()> {
    if password.chars().count() < MIN_PASSWORD_LENGTH {
        return Err(ApiError::bad_request(
            "password_too_short",
            "Password must be at least 12 characters long",
        ));
    }
    Ok(())
}
