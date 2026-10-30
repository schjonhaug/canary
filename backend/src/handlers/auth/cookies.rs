//! The `HttpOnly` cookie that carries a browser's session token.

use crate::config::OperatingMode;
use axum::http::{header, HeaderMap};

/// Cookie name for storing the JWT token
const AUTH_COOKIE_NAME: &str = "auth_token";

/// Build an `HttpOnly`, Secure, SameSite=Lax cookie for authentication
/// Cloud cookies always require HTTPS. Self-hosted mode supports local HTTP.
/// The cookie expires in 7 days, matching the JWT token expiration
///
/// SameSite=Lax is used because:
/// - It allows cookies on same-site navigation (clicking links) while blocking cross-site POST
/// - Works for same-origin deployments (frontend and backend on same domain)
/// - For cross-origin setups, clients should use the Authorization header with the token from login response
pub(super) fn build_auth_cookie(token: &str, mode: OperatingMode) -> String {
    let secure = if mode == OperatingMode::Cloud {
        "; Secure"
    } else {
        ""
    };
    format!(
        "{}={}; HttpOnly; SameSite=Lax; Path=/; Max-Age={}{}",
        AUTH_COOKIE_NAME,
        token,
        7 * 24 * 60 * 60, // 7 days in seconds
        secure
    )
}

/// Build a cookie that clears the auth token (for logout)
pub(super) fn build_clear_auth_cookie(mode: OperatingMode) -> String {
    let secure = if mode == OperatingMode::Cloud {
        "; Secure"
    } else {
        ""
    };
    format!("{AUTH_COOKIE_NAME}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0{secure}")
}

/// Extract auth token from cookie header
pub fn extract_token_from_cookies(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|h| h.to_str().ok())
        .and_then(|cookie_str| {
            cookie_str.split(';').find_map(|cookie| {
                let cookie = cookie.trim();
                if let Some(value) = cookie.strip_prefix(&format!("{AUTH_COOKIE_NAME}=")) {
                    if !value.is_empty() {
                        return Some(value.to_string());
                    }
                }
                None
            })
        })
}

#[cfg(test)]
mod tests {
    use super::{build_auth_cookie, build_clear_auth_cookie};
    use crate::config::OperatingMode;

    #[test]
    fn cloud_authentication_and_logout_cookies_always_require_https() {
        for cookie in [
            build_auth_cookie("synthetic-token", OperatingMode::Cloud),
            build_clear_auth_cookie(OperatingMode::Cloud),
        ] {
            let attributes: Vec<_> = cookie.split("; ").collect();
            assert!(attributes.contains(&"Secure"));
            assert!(attributes.contains(&"HttpOnly"));
            assert!(attributes.contains(&"SameSite=Lax"));
            assert!(attributes.contains(&"Path=/"));
        }
        assert!(build_clear_auth_cookie(OperatingMode::Cloud).contains("Max-Age=0"));
    }

    #[test]
    fn self_hosted_authentication_and_logout_support_local_http() {
        for cookie in [
            build_auth_cookie("synthetic-token", OperatingMode::SelfHosted),
            build_clear_auth_cookie(OperatingMode::SelfHosted),
        ] {
            assert!(!cookie.split("; ").any(|attribute| attribute == "Secure"));
            assert!(cookie.contains("HttpOnly"));
        }
    }
}
