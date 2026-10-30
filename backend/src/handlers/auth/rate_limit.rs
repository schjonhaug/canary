//! Rate limits for unauthenticated endpoints, keyed by client address or
//! email, and padding that keeps response timing from revealing accounts.

use super::jwt_secret;
use crate::api::AppServicesState;
use crate::config::AppConfig;
use crate::handlers::error::{ApiError, ApiResult, OrInternal};
use anyhow::Context;
use axum::{
    extract::{ConnectInfo, Extension},
    http::{HeaderMap, StatusCode},
};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use std::{
    collections::HashSet,
    net::{IpAddr, SocketAddr},
    time::{Duration, Instant},
};

const ENUMERATION_RESPONSE_FLOOR: Duration = Duration::from_millis(1500);
pub(super) const MAX_AUTH_REQUESTS_PER_IP: i64 = 10;
pub(super) const AUTH_IP_RATE_LIMIT_WINDOW_MINUTES: i64 = 15;
const MAX_FAILED_LOGIN_ATTEMPTS_PER_EMAIL: i64 = 50;
const FAILED_LOGIN_EMAIL_RATE_LIMIT_WINDOW_MINUTES: i64 = 60;

pub(super) fn rate_limited(
    code: &'static str,
    message: &'static str,
    retry_after_seconds: i64,
) -> ApiError {
    ApiError::too_many_requests(
        code,
        message,
        u64::try_from(retry_after_seconds).unwrap_or(0),
    )
}

/// A per-email limit on attempts at an unauthenticated endpoint.
pub(super) struct EmailRateLimit {
    scope: &'static str,
    max_attempts: i64,
    window_minutes: i64,
    code: &'static str,
    message: &'static str,
    error_message: &'static str,
}

pub(super) const REGISTRATION_EMAIL_RATE_LIMIT: EmailRateLimit = EmailRateLimit {
    scope: "register",
    max_attempts: 3,
    window_minutes: 60,
    code: "registration_rate_limit",
    message: "Too many registration attempts. Please try again later.",
    error_message: "Failed to validate registration rate limit",
};

pub(super) const FORGOT_PASSWORD_EMAIL_RATE_LIMIT: EmailRateLimit = EmailRateLimit {
    scope: "forgot_password",
    max_attempts: 3,
    window_minutes: 60,
    code: "forgot_password_rate_limit",
    message: "Too many password reset attempts. Please try again later.",
    error_message: "Failed to validate password reset rate limit",
};

pub(super) async fn enforce_email_rate_limit(
    app_services: &AppServicesState,
    limit: &EmailRateLimit,
    email: &str,
) -> ApiResult<()> {
    let allowed = app_services
        .metadata_db
        .check_auth_rate_limit(limit.scope, email, limit.max_attempts, limit.window_minutes)
        .await
        .or_internal(limit.error_message)?;
    if allowed {
        Ok(())
    } else {
        Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            limit.code,
            limit.message,
        ))
    }
}

pub(super) async fn pad_enumeration_response(start_time: Instant) {
    pad_response_to_duration(start_time, ENUMERATION_RESPONSE_FLOOR).await;
}

async fn pad_response_to_duration(start_time: Instant, target_duration: Duration) {
    let elapsed = start_time.elapsed();
    if elapsed < target_duration {
        tokio::time::sleep(target_duration.saturating_sub(elapsed)).await;
    }
}

pub(crate) async fn enforce_ip_rate_limit(
    app_services: &AppServicesState,
    config: &AppConfig,
    endpoint: &str,
    address: Option<SocketAddr>,
    max_attempts: i64,
    window_minutes: i64,
    record_attempt: bool,
) -> ApiResult<()> {
    let Some(address) = address else {
        return Ok(());
    };

    // Only an HMAC is stored. Raw client addresses cannot be recovered without the server secret.
    let secret = jwt_secret(config)?;
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(address.ip().to_string().as_bytes());
    let identifier = hex::encode(mac.finalize().into_bytes());
    let scope = format!("{endpoint}_ip");
    let decision = if record_attempt {
        app_services
            .metadata_db
            .check_endpoint_rate_limit(&scope, &identifier, max_attempts, window_minutes)
            .await
            .map(|decision| (!decision.allowed, decision.retry_after_seconds))
    } else {
        app_services
            .metadata_db
            .is_endpoint_rate_limited(&scope, &identifier)
            .await
            .map(|limited| (limited, None))
    };
    let (limited, retry_after_seconds) = decision
        .with_context(|| format!("Failed to check {endpoint} IP rate limit"))
        .or_internal("Failed to validate request rate limit")?;

    if limited {
        Err(rate_limited(
            "endpoint_rate_limit",
            "Too many requests. Please try again later.",
            retry_after_seconds.unwrap_or(window_minutes * 60),
        ))
    } else {
        Ok(())
    }
}

pub(super) async fn enforce_failed_login_email_rate_limit(
    app_services: &AppServicesState,
    email: &str,
) -> ApiResult<()> {
    let allowed = app_services
        .metadata_db
        .check_auth_rate_limit(
            "login_email",
            email,
            MAX_FAILED_LOGIN_ATTEMPTS_PER_EMAIL,
            FAILED_LOGIN_EMAIL_RATE_LIMIT_WINDOW_MINUTES,
        )
        .await
        .or_internal("Failed to validate login rate limit")?;
    if allowed {
        Ok(())
    } else {
        Err(rate_limited(
            "invalid_credentials",
            "Invalid credentials",
            FAILED_LOGIN_EMAIL_RATE_LIMIT_WINDOW_MINUTES * 60,
        ))
    }
}

pub(crate) fn client_ip(
    headers: &HeaderMap,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    config: &AppConfig,
) -> Option<SocketAddr> {
    let Extension(ConnectInfo(peer)) = peer?;
    client_ip_from_forwarded_for(headers, peer, config.trusted_proxy_ips())
}

fn client_ip_from_forwarded_for(
    headers: &HeaderMap,
    peer: SocketAddr,
    trusted_proxies: &HashSet<IpAddr>,
) -> Option<SocketAddr> {
    if !trusted_proxies.contains(&peer.ip()) {
        return Some(peer);
    }

    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .into_iter()
        .flat_map(|value| value.rsplit(','))
        .filter_map(|value| value.trim().parse::<IpAddr>().ok())
        .find(|ip| !trusted_proxies.contains(ip))
        .map(|ip| SocketAddr::new(ip, peer.port()))
        .or(Some(peer))
}

#[cfg(test)]
mod tests {
    use super::{client_ip_from_forwarded_for, pad_response_to_duration};
    use axum::http::HeaderMap;
    use std::{
        collections::HashSet,
        net::{IpAddr, Ipv4Addr, SocketAddr},
    };

    #[tokio::test]
    async fn pad_response_to_duration_waits_until_floor() {
        let start_time = std::time::Instant::now();
        let target_duration = std::time::Duration::from_millis(20);

        pad_response_to_duration(start_time, target_duration).await;

        assert!(start_time.elapsed() >= target_duration);
    }

    #[test]
    fn client_ip_ignores_forwarded_headers_from_untrusted_peers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.10".parse().unwrap());
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3000);

        assert_eq!(
            client_ip_from_forwarded_for(&headers, peer, &HashSet::new()),
            Some(peer)
        );
    }

    #[test]
    fn client_ip_skips_trusted_proxies_from_right_to_left() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "198.51.100.10, 192.0.2.10, 192.0.2.11".parse().unwrap(),
        );
        let peer = SocketAddr::new("192.0.2.12".parse().unwrap(), 3000);
        let trusted_proxies = HashSet::from([
            "192.0.2.10".parse().unwrap(),
            "192.0.2.11".parse().unwrap(),
            peer.ip(),
        ]);

        assert_eq!(
            client_ip_from_forwarded_for(&headers, peer, &trusted_proxies),
            Some(SocketAddr::new("198.51.100.10".parse().unwrap(), 3000))
        );
    }
}
