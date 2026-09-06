//! Authentication and user management handlers

use crate::admin_notifications::AdminNotifications;
use crate::api::{AppServicesState, StripeBillingState};
use crate::auth::{AuthResponse, AuthService, AuthUserResponse};
use crate::config::AppConfig;
use crate::email_service::EmailService;
use crate::exchange_rates;
use crate::extractors::AuthenticatedUser;
use crate::handlers::helpers::{get_user_or_error, DatabaseErrorMessage};
use crate::metadata::MetadataDb;
use crate::models::{ContactFormRequest, ContactFormResponse, DemoLoginRequest, ErrorResponse};
use anyhow::Result;
use axum::{
    extract::{ConnectInfo, Extension, Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use std::{
    collections::HashSet,
    net::{IpAddr, SocketAddr},
    sync::Arc,
};
use tracing::info;

// Request types imported for handler use only
use crate::auth::{
    ForgotPasswordRequest, LoginRequest, RegisterRequest, ResetPasswordRequest, UpdateUserRequest,
    UpdateUserResponse,
};

/// Cookie name for storing the JWT token
const AUTH_COOKIE_NAME: &str = "auth_token";
const ENUMERATION_RESPONSE_FLOOR: std::time::Duration = std::time::Duration::from_millis(1500);
const FORGOT_PASSWORD_SUCCESS_MESSAGE: &str =
    "If an account with that email exists, a password reset link has been sent.";
const MIN_PASSWORD_LENGTH: usize = 12;
const MAX_AUTH_REQUESTS_PER_IP: i64 = 10;
const AUTH_IP_RATE_LIMIT_WINDOW_MINUTES: i64 = 15;
const MAX_FAILED_LOGIN_ATTEMPTS_PER_EMAIL: i64 = 50;
const FAILED_LOGIN_EMAIL_RATE_LIMIT_WINDOW_MINUTES: i64 = 60;
const MAX_CONTACT_REQUESTS_PER_IP: i64 = 3;
const CONTACT_RATE_LIMIT_WINDOW_MINUTES: i64 = 60;

async fn pad_enumeration_response(start_time: std::time::Instant) {
    pad_response_to_duration(start_time, ENUMERATION_RESPONSE_FLOOR).await;
}

async fn finalize_forgot_password_response(
    start_time: std::time::Instant,
    response: Response,
) -> Response {
    pad_enumeration_response(start_time).await;
    response
}

async fn pad_response_to_duration(
    start_time: std::time::Instant,
    target_duration: std::time::Duration,
) {
    let elapsed = start_time.elapsed();
    if elapsed < target_duration {
        tokio::time::sleep(target_duration - elapsed).await;
    }
}

async fn enforce_ip_rate_limit(
    app_services: &AppServicesState,
    config: &AppConfig,
    endpoint: &str,
    address: Option<SocketAddr>,
    max_attempts: i64,
    window_minutes: i64,
    record_attempt: bool,
) -> Result<(), Response> {
    let Some(address) = address else {
        return Ok(());
    };

    // Only an HMAC is stored. Raw client addresses cannot be recovered without the server secret.
    let secret = match config.get_jwt_secret() {
        Ok(secret) => secret,
        Err(e) => {
            return Err(
                (StatusCode::SERVICE_UNAVAILABLE, Json(ErrorResponse::new(e))).into_response(),
            )
        }
    };
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(address.ip().to_string().as_bytes());
    let identifier = hex::encode(mac.finalize().into_bytes());
    let scope = format!("{}_ip", endpoint);
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
    match decision {
        Ok((false, _)) => Ok(()),
        Ok((true, retry_after_seconds)) => Err((
            StatusCode::TOO_MANY_REQUESTS,
            [(
                header::RETRY_AFTER,
                retry_after_seconds
                    .unwrap_or(window_minutes * 60)
                    .to_string(),
            )],
            Json(ErrorResponse::coded(
                "endpoint_rate_limit",
                "Too many requests. Please try again later.",
            )),
        )
            .into_response()),
        Err(e) => {
            tracing::error!("Failed to check {} IP rate limit: {}", endpoint, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to validate request rate limit")),
            )
                .into_response())
        }
    }
}

async fn enforce_failed_login_email_rate_limit(
    app_services: &AppServicesState,
    email: &str,
) -> Result<(), Response> {
    match app_services
        .metadata_db
        .check_auth_rate_limit(
            "login_email",
            email,
            MAX_FAILED_LOGIN_ATTEMPTS_PER_EMAIL,
            FAILED_LOGIN_EMAIL_RATE_LIMIT_WINDOW_MINUTES,
        )
        .await
    {
        Ok(true) => Ok(()),
        Ok(false) => Err((
            StatusCode::TOO_MANY_REQUESTS,
            [(
                header::RETRY_AFTER,
                (FAILED_LOGIN_EMAIL_RATE_LIMIT_WINDOW_MINUTES * 60).to_string(),
            )],
            Json(ErrorResponse::coded(
                "invalid_credentials",
                "Invalid credentials",
            )),
        )
            .into_response()),
        Err(e) => {
            tracing::error!("Failed to check login email rate limit: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to validate login rate limit")),
            )
                .into_response())
        }
    }
}

fn client_ip(headers: &HeaderMap, peer: Option<SocketAddr>) -> Option<SocketAddr> {
    let peer = peer?;
    let trusted_proxies = std::env::var("CANARY_TRUSTED_PROXY_IPS")
        .ok()
        .into_iter()
        .flat_map(|value| {
            value
                .split(',')
                .map(str::trim)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .filter_map(|value| value.parse().ok())
        .collect();

    client_ip_from_forwarded_for(headers, peer, &trusted_proxies)
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

/// Build an HttpOnly, Secure, SameSite=Lax cookie for authentication
/// The cookie expires in 7 days, matching the JWT token expiration
///
/// SameSite=Lax is used because:
/// - It allows cookies on same-site navigation (clicking links) while blocking cross-site POST
/// - Works for same-origin deployments (frontend and backend on same domain)
/// - For cross-origin setups, clients should use the Authorization header with the token from login response
fn build_auth_cookie(token: &str, is_production: bool) -> String {
    let secure = if is_production { "; Secure" } else { "" };
    format!(
        "{}={}; HttpOnly; SameSite=Lax; Path=/; Max-Age={}{}",
        AUTH_COOKIE_NAME,
        token,
        7 * 24 * 60 * 60, // 7 days in seconds
        secure
    )
}

/// Build a cookie that clears the auth token (for logout)
fn build_clear_auth_cookie(is_production: bool) -> String {
    let secure = if is_production { "; Secure" } else { "" };
    format!(
        "{}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0{}",
        AUTH_COOKIE_NAME, secure
    )
}

/// Extract auth token from cookie header
pub fn extract_token_from_cookies(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|h| h.to_str().ok())
        .and_then(|cookie_str| {
            cookie_str.split(';').find_map(|cookie| {
                let cookie = cookie.trim();
                if let Some(value) = cookie.strip_prefix(&format!("{}=", AUTH_COOKIE_NAME)) {
                    if !value.is_empty() {
                        return Some(value.to_string());
                    }
                }
                None
            })
        })
}

pub(crate) async fn update_password_and_revoke_sessions(
    metadata_db: &MetadataDb,
    user_id: &str,
    password_hash: &str,
) -> Result<()> {
    metadata_db
        .update_user_password_and_revoke_sessions(user_id, password_hash)
        .await?;
    Ok(())
}

/// User registration endpoint
pub async fn register(
    State(app_services): State<AppServicesState>,
    State(stripe_billing): State<StripeBillingState>,
    State(config): State<Arc<AppConfig>>,
    address: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(request): Json<RegisterRequest>,
) -> Response {
    if config.is_self_hosted_mode() {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new(
                "Registration is disabled in self-hosted mode",
            )),
        )
            .into_response();
    }

    let start_time = std::time::Instant::now();
    if let Err(response) = enforce_ip_rate_limit(
        &app_services,
        &config,
        "register",
        client_ip(
            &headers,
            address.map(|Extension(ConnectInfo(address))| address),
        ),
        MAX_AUTH_REQUESTS_PER_IP,
        AUTH_IP_RATE_LIMIT_WINDOW_MINUTES,
        true,
    )
    .await
    {
        return response;
    }

    // Validate email format
    if !request.email.contains('@') || request.email.len() < 5 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::coded(
                "invalid_email_format",
                "Invalid email format",
            )),
        )
            .into_response();
    }

    // Validate password strength
    if request.password.chars().count() < MIN_PASSWORD_LENGTH {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::coded(
                "password_too_short",
                "Password must be at least 12 characters long",
            )),
        )
            .into_response();
    }

    match app_services
        .metadata_db
        .check_auth_rate_limit(
            REGISTRATION_RATE_LIMIT_SCOPE,
            &request.email,
            MAX_REGISTRATION_ATTEMPTS_PER_EMAIL,
            REGISTRATION_RATE_LIMIT_WINDOW_MINUTES,
        )
        .await
    {
        Ok(false) => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(ErrorResponse::coded(
                    "registration_rate_limit",
                    "Too many registration attempts. Please try again later.",
                )),
            )
                .into_response();
        }
        Ok(true) => {}
        Err(e) => {
            tracing::error!("Failed to check registration rate limit: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(
                    "Failed to validate registration rate limit".to_string(),
                )),
            )
                .into_response();
        }
    }

    // Check if user already exists - no mutex blocking!
    match app_services
        .metadata_db
        .get_user_by_email(&request.email)
        .await
    {
        Ok(Some(existing_user)) => {
            // Return same response as successful registration to prevent email enumeration.
            // Send notification email to existing user (fire-and-forget).
            let email_service = EmailService::from_env().ok();
            if let Some(email_service) = email_service {
                let email = existing_user.email.clone();
                let name = existing_user
                    .name
                    .clone()
                    .unwrap_or_else(|| "User".to_string());
                let language = existing_user
                    .preferred_language
                    .clone()
                    .unwrap_or_else(|| "en-US".to_string());

                tokio::spawn(async move {
                    if let Err(e) = email_service
                        .send_registration_attempt_notification(&email, &name, &language)
                        .await
                    {
                        tracing::error!(
                            "Failed to send registration attempt notification to {}: {}",
                            email,
                            e
                        );
                    }
                });
            }

            // Ensure response timing matches a real registration to prevent timing attacks.
            // Real registrations involve password hashing + Stripe + email (~1-2s).
            // Sleep for the remaining time up to the target, rather than a fixed delay.
            pad_enumeration_response(start_time).await;

            let elapsed = start_time.elapsed();
            info!("register (existing email) completed in {:?}", elapsed);

            return Json(serde_json::json!({
                "message": "Registration successful. Please check your email to verify your account."
            }))
            .into_response();
        }
        Ok(None) => {} // User doesn't exist, proceed
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to check user existence: {}",
                    e
                ))),
            )
                .into_response();
        }
    }

    let jwt_secret = match config.get_jwt_secret() {
        Ok(secret) => secret.to_string(),
        Err(e) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse::new(e.to_string())),
            )
                .into_response();
        }
    };

    // Email service not configured, will work in dev mode
    let email_service = EmailService::from_env().ok();

    let auth_service = AuthService::new(jwt_secret, email_service);

    // Check if this is a dev test email
    let is_dev_email = auth_service.is_dev_test_email(&request.email);

    // Hash password
    let password_hash = match auth_service.hash_password(&request.password) {
        Ok(hash) => hash,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to hash password: {}",
                    e
                ))),
            )
                .into_response();
        }
    };

    // For dev mode, auto-verify emails. For production, require verification.
    let email_verified = is_dev_email;

    // Determine preferred currency and language from browser locale
    let preferred_currency = request
        .browser_locale
        .as_ref()
        .map(|locale| exchange_rates::ExchangeRateService::locale_to_currency(locale));
    let preferred_language = request
        .browser_locale
        .as_ref()
        .map(|locale| crate::metadata::locale_to_language(locale));

    // Create user - no mutex blocking!
    let user_id = match app_services
        .metadata_db
        .create_user(
            &request.email,
            &password_hash,
            Some(&request.name),
            email_verified,
            preferred_currency,
            preferred_language,
        )
        .await
    {
        Ok(id) => id,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!("Failed to create user: {}", e))),
            )
                .into_response();
        }
    };

    // Send admin notification for new user signup (fire-and-forget)
    {
        AdminNotifications::spawn_if_enabled(
            config.ntfy_server_url(),
            move |admin_notifications| async move {
                admin_notifications.notify_user_signup().await;
            },
        );
    }

    // Create Stripe trial subscription for the user
    {
        // Get the created user to pass to Stripe - no mutex blocking!
        let user_record = match app_services.metadata_db.get_user_by_id(&user_id).await {
            Ok(Some(user)) => user,
            Ok(None) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(
                        "User was created but could not be retrieved",
                    )),
                )
                    .into_response();
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(format!(
                        "Failed to retrieve user: {}",
                        e
                    ))),
                )
                    .into_response();
            }
        };

        // Create Stripe customer (but not subscription yet - that starts when they add their first wallet)
        if let Some(stripe_service) = &stripe_billing {
            if let Err(e) = stripe_service
                .create_stripe_customer_only(&user_record, &app_services.metadata_db)
                .await
            {
                tracing::error!(
                    "Failed to create Stripe customer for user {}: {}",
                    user_record.email,
                    e
                );
                // Don't fail registration if Stripe fails, but log the error
                // User can still use the service, they just won't have Stripe integration
            }
        } else {
            tracing::info!(
                "Stripe not enabled, user {} registered without Stripe integration",
                user_record.email
            );
        }
    }

    // Add to marketing audience if opted in
    if request.marketing_emails_opt_in {
        if let Some(email_service) = &auth_service.email_service {
            tokio::spawn({
                let email = request.email.clone();
                let name = request.name.clone();
                let email_service = email_service.clone();
                async move {
                    if let Err(e) = email_service.add_to_marketing_audience(&email, &name).await {
                        eprintln!("Failed to add user to marketing audience: {}", e);
                    }
                }
            });
        }
    }

    // Send verification email for non-dev accounts
    if !is_dev_email {
        let token = auth_service.generate_verification_token();
        let token_hash = AuthService::hash_token(&token);

        // Store verification token (hashed for security) - no mutex blocking!
        if let Err(e) = app_services
            .metadata_db
            .create_email_verification_token(&user_id, &token_hash)
            .await
        {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to create verification token: {}",
                    e
                ))),
            )
                .into_response();
        }

        // Send verification email
        let user_language = preferred_language.unwrap_or("en-US");
        if let Err(e) = auth_service
            .send_email_verification(&request.email, &request.name, &token, user_language)
            .await
        {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(format!(
                    "Failed to send verification email: {}",
                    e
                ))),
            )
                .into_response();
        }

        let elapsed = start_time.elapsed();
        info!("register completed in {:?}", elapsed);

        Json(serde_json::json!({
            "message": "Registration successful. Please check your email to verify your account."
        }))
        .into_response()
    } else {
        // Dev mode: return success immediately
        let elapsed = start_time.elapsed();
        info!("register completed in {:?}", elapsed);

        Json(serde_json::json!({
            "message": "Registration successful (dev mode - no email verification required)."
        }))
        .into_response()
    }
}

// Rate limiting constants
const MAX_REGISTRATION_ATTEMPTS_PER_EMAIL: i64 = 3;
const REGISTRATION_RATE_LIMIT_WINDOW_MINUTES: i64 = 60;
const MAX_FORGOT_PASSWORD_ATTEMPTS_PER_EMAIL: i64 = 3;
const FORGOT_PASSWORD_RATE_LIMIT_WINDOW_MINUTES: i64 = 60;
const REGISTRATION_RATE_LIMIT_SCOPE: &str = "register";
const FORGOT_PASSWORD_RATE_LIMIT_SCOPE: &str = "forgot_password";

/// User login endpoint
pub async fn login(
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    address: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Response {
    let client_address = client_ip(
        &headers,
        address.map(|Extension(ConnectInfo(address))| address),
    );
    if let Err(response) = enforce_ip_rate_limit(
        &app_services,
        &config,
        "login",
        client_address,
        MAX_AUTH_REQUESTS_PER_IP,
        AUTH_IP_RATE_LIMIT_WINDOW_MINUTES,
        false,
    )
    .await
    {
        return response;
    }

    // Check if user exists - no mutex blocking!
    let user_record = match app_services
        .metadata_db
        .get_user_by_email(&request.email)
        .await
    {
        Ok(Some(user)) => user,
        Ok(None) => {
            if let Err(e) = AuthService::verify_dummy_password(&request.password) {
                tracing::error!("Failed to perform dummy password verification: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("Failed to verify credentials")),
                )
                    .into_response();
            }

            if let Err(response) = enforce_ip_rate_limit(
                &app_services,
                &config,
                "login",
                client_address,
                MAX_AUTH_REQUESTS_PER_IP,
                AUTH_IP_RATE_LIMIT_WINDOW_MINUTES,
                true,
            )
            .await
            {
                return response;
            }
            if let Err(response) =
                enforce_failed_login_email_rate_limit(&app_services, &request.email).await
            {
                return response;
            }

            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::coded(
                    "invalid_credentials",
                    "Invalid credentials",
                )),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!("Failed to check user: {}", e))),
            )
                .into_response();
        }
    };

    let jwt_secret = match config.get_jwt_secret() {
        Ok(secret) => secret.to_string(),
        Err(e) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse::new(e.to_string())),
            )
                .into_response();
        }
    };

    let email_service = match EmailService::from_env() {
        Ok(service) => {
            tracing::debug!("Email service initialized successfully");
            Some(service)
        }
        Err(e) => {
            tracing::debug!("Email service not configured: {}", e);
            None // Email service not configured, will work in dev mode
        }
    };

    let auth_service = AuthService::new(jwt_secret, email_service);

    // Check if this is a dev test email (bypass password check)
    let is_dev_email = auth_service.is_dev_test_email(&request.email);

    // Verify password
    let password_valid = if is_dev_email {
        // For dev emails, check against dev test password
        request.password == auth_service.get_dev_test_password()
    } else {
        // For regular users, verify against stored hash
        match auth_service.verify_password(&request.password, &user_record.password_hash) {
            Ok(valid) => valid,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(format!(
                        "Failed to verify password: {}",
                        e
                    ))),
                )
                    .into_response();
            }
        }
    };

    if !password_valid {
        if let Err(response) = enforce_ip_rate_limit(
            &app_services,
            &config,
            "login",
            client_address,
            MAX_AUTH_REQUESTS_PER_IP,
            AUTH_IP_RATE_LIMIT_WINDOW_MINUTES,
            true,
        )
        .await
        {
            return response;
        }
        if let Err(response) =
            enforce_failed_login_email_rate_limit(&app_services, &request.email).await
        {
            return response;
        }

        // Record failed login attempt
        let _ = app_services
            .metadata_db
            .record_login_attempt(&request.email, false)
            .await;

        let error = if config.is_self_hosted_mode() {
            ErrorResponse::coded("invalid_self_hosted_password", "Incorrect password")
        } else {
            ErrorResponse::coded("invalid_credentials", "Invalid credentials")
        };
        return (StatusCode::BAD_REQUEST, Json(error)).into_response();
    }

    // Check email verification (skip for dev emails)
    if !is_dev_email && !user_record.email_verified {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::coded(
                "email_not_verified",
                "Email not verified. Please check your email and click the verification link.",
            )),
        )
            .into_response();
    }

    // Successful login - record it and reset failed login counter
    let _ = app_services
        .metadata_db
        .record_login_attempt(&request.email, true)
        .await;

    // Update last login
    if let Err(e) = app_services
        .metadata_db
        .update_last_login(&user_record.id)
        .await
    {
        tracing::warn!(
            "Failed to update last login for user {}: {:?}",
            user_record.id,
            e
        );
    }

    // Generate JWT token
    let token = match auth_service.generate_token(
        &user_record.id,
        &user_record.email,
        user_record.is_admin,
        user_record.is_demo,
    ) {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to generate token: {}",
                    e
                ))),
            )
                .into_response();
        }
    };

    // Create session
    let token_hash = AuthService::hash_token(&token);
    let expires_at = chrono::Utc::now() + chrono::Duration::days(7);

    if let Err(e) = app_services
        .metadata_db
        .create_session(&user_record.id, &token_hash, expires_at)
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(format!(
                "Failed to create session: {}",
                e
            ))),
        )
            .into_response();
    }

    // Build user response
    let user_info = AuthUserResponse {
        id: user_record.id,
        email: user_record.email,
        name: user_record.name,
        is_admin: user_record.is_admin,
        is_demo: user_record.is_demo,
        email_verified: user_record.email_verified,
        subscription_tier: user_record.subscription_tier,
        created_at: user_record.created_at,
        preferred_fiat_currency: user_record.preferred_fiat_currency,
        preferred_language: user_record.preferred_language,
    };

    // Build response with HttpOnly cookie for secure token storage
    // Web browsers use the HttpOnly cookie (XSS-protected)
    // CLI/mobile clients can use the token from the response body with Authorization header
    let is_production = std::env::var("CANARY_PRODUCTION").is_ok();
    let cookie = build_auth_cookie(&token, is_production);

    let response_body = AuthResponse {
        token: token.clone(), // Keep token in response for CLI/mobile backward compatibility
        user: user_info,
        requires_name: None,
    };

    ([(header::SET_COOKIE, cookie)], Json(response_body)).into_response()
}

/// Auto-login endpoint for demo account (no password required)
pub async fn demo_login(
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    Json(request): Json<DemoLoginRequest>,
) -> Response {
    let start_time = std::time::Instant::now();

    // Only works in cloud mode
    if !config.is_cloud_mode() {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new(
                "Demo login is only available in cloud mode",
            )),
        )
            .into_response();
    }

    let demo_email = "demo@canarybitcoin.com";

    // Get demo user from database
    let mut user_record = match app_services.metadata_db.get_user_by_email(demo_email).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new(
                    "Demo user not found. Please ensure backend is running in dev mode.",
                )),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to get demo user: {}",
                    e
                ))),
            )
                .into_response();
        }
    };

    // Verify this is actually a demo user
    if !user_record.is_demo {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("This user is not a demo account")),
        )
            .into_response();
    }

    // Update demo user's language and currency based on browser locale
    if let Some(browser_locale) = &request.browser_locale {
        // Update language
        let preferred_language = crate::metadata::locale_to_language(browser_locale);
        if user_record.preferred_language.as_deref() != Some(preferred_language) {
            if let Err(e) = app_services
                .metadata_db
                .update_user_preferred_language(&user_record.id, preferred_language)
                .await
            {
                eprintln!(
                    "Failed to update demo user language to {}: {:?}",
                    preferred_language, e
                );
            } else {
                user_record.preferred_language = Some(preferred_language.to_string());
            }
        }

        // Update currency
        let preferred_currency =
            exchange_rates::ExchangeRateService::locale_to_currency(browser_locale);
        if user_record.preferred_fiat_currency.as_deref() != Some(preferred_currency) {
            if let Err(e) = app_services
                .metadata_db
                .update_user_preferred_currency(&user_record.id, preferred_currency)
                .await
            {
                eprintln!(
                    "Failed to update demo user currency to {}: {:?}",
                    preferred_currency, e
                );
            } else {
                user_record.preferred_fiat_currency = Some(preferred_currency.to_string());
            }
        }
    }

    // Generate JWT token for demo user
    let jwt_secret = match config.get_jwt_secret() {
        Ok(secret) => secret.to_string(),
        Err(e) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse::new(e.to_string())),
            )
                .into_response();
        }
    };
    let auth_service = AuthService::new(jwt_secret, None);

    let token = match auth_service.generate_token(
        &user_record.id,
        &user_record.email,
        user_record.is_admin,
        user_record.is_demo,
    ) {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to generate token: {}",
                    e
                ))),
            )
                .into_response();
        }
    };

    // Clean up any existing sessions for this user to prevent token collision
    let _ = app_services
        .metadata_db
        .delete_user_sessions(&user_record.id)
        .await;

    // Create session
    let token_hash = AuthService::hash_token(&token);
    let expires_at = chrono::Utc::now() + chrono::Duration::days(7);

    if let Err(e) = app_services
        .metadata_db
        .create_session(&user_record.id, &token_hash, expires_at)
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(format!(
                "Failed to create session: {}",
                e
            ))),
        )
            .into_response();
    }

    // Build user response
    let user_info = AuthUserResponse {
        id: user_record.id,
        email: user_record.email,
        name: user_record.name,
        is_admin: user_record.is_admin,
        is_demo: user_record.is_demo,
        email_verified: user_record.email_verified,
        subscription_tier: user_record.subscription_tier,
        created_at: user_record.created_at,
        preferred_fiat_currency: user_record.preferred_fiat_currency,
        preferred_language: user_record.preferred_language,
    };

    let elapsed = start_time.elapsed();
    info!("demo_login completed in {:?}", elapsed);

    // Build response with HttpOnly cookie for secure token storage
    // Web browsers use the HttpOnly cookie (XSS-protected)
    // CLI/mobile clients can use the token from the response body with Authorization header
    let is_production = std::env::var("CANARY_PRODUCTION").is_ok();
    let cookie = build_auth_cookie(&token, is_production);

    let response_body = AuthResponse {
        token: token.clone(), // Keep token in response for CLI/mobile backward compatibility
        user: user_info,
        requires_name: None,
    };

    ([(header::SET_COOKIE, cookie)], Json(response_body)).into_response()
}

/// Email verification endpoint
pub async fn verify_email(
    State(config): State<Arc<AppConfig>>,
    State(app_services): State<AppServicesState>,
    Path(token): Path<String>,
) -> Response {
    if config.is_self_hosted_mode() {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new(
                "Email verification is unavailable in self-hosted mode",
            )),
        )
            .into_response();
    }

    let start_time = std::time::Instant::now();

    // Hash the incoming token for verification (tokens are stored hashed)
    let token_hash = AuthService::hash_token(&token);

    // Direct metadata access - no mutex blocking!
    let result = match app_services
        .metadata_db
        .verify_email_token(&token_hash)
        .await
    {
        Ok(Some(_user_id)) => Json(serde_json::json!({
            "message": "Email verified successfully. You can now log in."
        }))
        .into_response(),
        Ok(None) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::coded(
                "invalid_verification_token",
                "Invalid or expired verification token",
            )),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(format!("Failed to verify email: {}", e))),
        )
            .into_response(),
    };

    let elapsed = start_time.elapsed();
    info!("verify_email completed in {:?}", elapsed);
    result
}

/// Forgot password endpoint
pub async fn forgot_password(
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    address: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(request): Json<ForgotPasswordRequest>,
) -> Response {
    if config.is_self_hosted_mode() {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new(
                "Password reset is unavailable in self-hosted mode",
            )),
        )
            .into_response();
    }

    let start_time = std::time::Instant::now();
    if let Err(response) = enforce_ip_rate_limit(
        &app_services,
        &config,
        "forgot_password",
        client_ip(
            &headers,
            address.map(|Extension(ConnectInfo(address))| address),
        ),
        MAX_AUTH_REQUESTS_PER_IP,
        AUTH_IP_RATE_LIMIT_WINDOW_MINUTES,
        true,
    )
    .await
    {
        return response;
    }

    match app_services
        .metadata_db
        .check_auth_rate_limit(
            FORGOT_PASSWORD_RATE_LIMIT_SCOPE,
            &request.email,
            MAX_FORGOT_PASSWORD_ATTEMPTS_PER_EMAIL,
            FORGOT_PASSWORD_RATE_LIMIT_WINDOW_MINUTES,
        )
        .await
    {
        Ok(false) => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(ErrorResponse::coded(
                    "forgot_password_rate_limit",
                    "Too many password reset attempts. Please try again later.",
                )),
            )
                .into_response();
        }
        Ok(true) => {}
        Err(e) => {
            tracing::error!("Failed to check forgot-password rate limit: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(
                    "Failed to validate password reset rate limit".to_string(),
                )),
            )
                .into_response();
        }
    }

    // Check if user exists - no mutex blocking!
    let user_record = match app_services
        .metadata_db
        .get_user_by_email(&request.email)
        .await
    {
        Ok(Some(user)) => user,
        Ok(None) => {
            // Don't reveal whether user exists or not
            let response = Json(serde_json::json!({
                "message": FORGOT_PASSWORD_SUCCESS_MESSAGE
            }))
            .into_response();

            let response = finalize_forgot_password_response(start_time, response).await;
            let elapsed = start_time.elapsed();
            info!("forgot_password (unknown email) completed in {:?}", elapsed);

            return response;
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!("Failed to check user: {}", e))),
            )
                .into_response();
        }
    };

    let jwt_secret = match config.get_jwt_secret() {
        Ok(secret) => secret.to_string(),
        Err(e) => {
            let response = (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse::new(e.to_string())),
            )
                .into_response();
            return finalize_forgot_password_response(start_time, response).await;
        }
    };

    let email_service = EmailService::from_env().ok();

    let auth_service = AuthService::new(jwt_secret, email_service);
    let token = auth_service.generate_verification_token();
    let token_hash = AuthService::hash_token(&token);

    // Store password reset token (hashed for security) - no mutex blocking!
    if let Err(e) = app_services
        .metadata_db
        .create_password_reset_token(&user_record.id, &token_hash)
        .await
    {
        let response = (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(format!(
                "Failed to create reset token: {}",
                e
            ))),
        )
            .into_response();
        return finalize_forgot_password_response(start_time, response).await;
    }

    // Send password reset email
    let user_language = user_record.preferred_language.as_deref().unwrap_or("en-US");
    if let Err(e) = auth_service
        .send_password_reset(
            &user_record.email,
            &user_record.name.unwrap_or_else(|| "User".to_string()),
            &token,
            user_language,
        )
        .await
    {
        let response = (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(format!(
                "Failed to send reset email: {}",
                e
            ))),
        )
            .into_response();
        return finalize_forgot_password_response(start_time, response).await;
    }

    let response = Json(serde_json::json!({
        "message": FORGOT_PASSWORD_SUCCESS_MESSAGE
    }))
    .into_response();

    let response = finalize_forgot_password_response(start_time, response).await;
    let elapsed = start_time.elapsed();
    info!("forgot_password completed in {:?}", elapsed);

    response
}

/// Contact form submission endpoint
pub async fn submit_contact_form(
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    address: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(payload): Json<ContactFormRequest>,
) -> Response {
    if let Err(response) = enforce_ip_rate_limit(
        &app_services,
        &config,
        "contact",
        client_ip(
            &headers,
            address.map(|Extension(ConnectInfo(address))| address),
        ),
        MAX_CONTACT_REQUESTS_PER_IP,
        CONTACT_RATE_LIMIT_WINDOW_MINUTES,
        true,
    )
    .await
    {
        return response;
    }
    let email = payload.email.trim();
    let message = payload.message.trim();

    // Validate email format
    if email.is_empty() || !email.contains('@') || email.len() > 255 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::coded(
                "invalid_email_format",
                "Please provide a valid email address",
            )),
        )
            .into_response();
    }

    // Validate message length
    if message.len() < 10 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::coded(
                "message_too_short",
                "Message must be at least 10 characters",
            )),
        )
            .into_response();
    }

    if message.len() > 5000 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::coded(
                "message_too_long",
                "Message must be less than 5000 characters",
            )),
        )
            .into_response();
    }

    // Send email using EmailService
    match EmailService::from_env() {
        Ok(email_service) => {
            match email_service
                .send_contact_form_submission(email, message)
                .await
            {
                Ok(_) => {
                    info!("Contact form submitted from {}", email);
                    (
                        StatusCode::OK,
                        Json(ContactFormResponse {
                            message: "Thank you for your message. We'll get back to you soon."
                                .to_string(),
                        }),
                    )
                        .into_response()
                }
                Err(e) => {
                    tracing::error!("Failed to send contact form email: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse::new(
                            "Failed to send message. Please try again later.",
                        )),
                    )
                        .into_response()
                }
            }
        }
        Err(e) => {
            tracing::error!("Email service not configured: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(
                    "Contact form is temporarily unavailable.",
                )),
            )
                .into_response()
        }
    }
}

/// Reset password endpoint
pub async fn reset_password(
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    Path(token): Path<String>,
    Json(request): Json<ResetPasswordRequest>,
) -> Response {
    if config.is_self_hosted_mode() {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new(
                "Password reset is unavailable in self-hosted mode",
            )),
        )
            .into_response();
    }

    let start_time = std::time::Instant::now();

    // Validate password strength
    if request.password.chars().count() < MIN_PASSWORD_LENGTH {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::coded(
                "password_too_short",
                "Password must be at least 12 characters long",
            )),
        )
            .into_response();
    }

    // Hash the incoming token for verification (tokens are stored hashed)
    let token_hash = AuthService::hash_token(&token);

    // Verify token and get user ID - no mutex blocking!
    let user_id = match app_services
        .metadata_db
        .verify_password_reset_token(&token_hash)
        .await
    {
        Ok(Some(id)) => id,
        Ok(None) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::coded(
                    "invalid_reset_token",
                    "Invalid or expired reset token",
                )),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!("Failed to verify token: {}", e))),
            )
                .into_response();
        }
    };

    let jwt_secret = match config.get_jwt_secret() {
        Ok(secret) => secret.to_string(),
        Err(e) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse::new(e.to_string())),
            )
                .into_response();
        }
    };

    let auth_service = AuthService::new(jwt_secret, None);

    // Hash new password
    let password_hash = match auth_service.hash_password(&request.password) {
        Ok(hash) => hash,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to hash password: {}",
                    e
                ))),
            )
                .into_response();
        }
    };

    if let Err(e) =
        update_password_and_revoke_sessions(&app_services.metadata_db, &user_id, &password_hash)
            .await
    {
        tracing::error!(
            "Failed to complete password reset for user {}: {}",
            user_id,
            e
        );
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Failed to complete password reset")),
        )
            .into_response();
    }

    let elapsed = start_time.elapsed();
    info!("reset_password completed in {:?}", elapsed);

    Json(serde_json::json!({
        "message": "Password reset successfully. You can now log in with your new password."
    }))
    .into_response()
}

/// Logout endpoint
pub async fn logout(State(app_services): State<AppServicesState>, headers: HeaderMap) -> Response {
    let start_time = std::time::Instant::now();

    // Try to get the token from cookie first (new secure method), then fall back to Authorization header
    let token = if let Some(cookie_token) = extract_token_from_cookies(&headers) {
        cookie_token
    } else if let Some(auth_header) = headers.get("authorization").and_then(|h| h.to_str().ok()) {
        // Fallback to Authorization header for backwards compatibility
        if !auth_header.starts_with("Bearer ") {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::new("Invalid authorization header")),
            )
                .into_response();
        }
        auth_header[7..].to_string() // Skip "Bearer "
    } else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("Authentication required")),
        )
            .into_response();
    };

    // Hash the token to find it in the database
    let token_hash = AuthService::hash_token(&token);

    // Direct metadata access - no mutex blocking!
    let result = app_services.metadata_db.delete_session(&token_hash).await;

    let elapsed = start_time.elapsed();
    info!("logout completed in {:?}", elapsed);

    // Always clear the cookie, even if session deletion fails
    let is_production = std::env::var("CANARY_PRODUCTION").is_ok();
    let clear_cookie = build_clear_auth_cookie(is_production);

    if let Err(e) = result {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(header::SET_COOKIE, clear_cookie)],
            Json(ErrorResponse::new(format!(
                "Failed to delete session: {}",
                e
            ))),
        )
            .into_response();
    }

    (
        [(header::SET_COOKIE, clear_cookie)],
        Json(serde_json::json!({
            "message": "Logged out successfully"
        })),
    )
        .into_response()
}

/// Get current user info endpoint
pub async fn me(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
) -> Response {
    let start_time = std::time::Instant::now();

    // Get user info from database - no mutex blocking!
    let db_user = match get_user_or_error(
        &app_services,
        &user.user_id,
        None,
        "User not found",
        DatabaseErrorMessage::Fixed("Failed to get user info"),
    )
    .await
    {
        Ok(db_user) => db_user,
        Err(response) => return response,
    };
    let user_info = AuthUserResponse {
        id: db_user.id,
        email: db_user.email,
        name: db_user.name,
        is_admin: user.is_admin,
        is_demo: user.is_demo,
        email_verified: db_user.email_verified,
        subscription_tier: db_user.subscription_tier,
        created_at: db_user.created_at,
        preferred_fiat_currency: db_user.preferred_fiat_currency,
        preferred_language: db_user.preferred_language,
    };

    let elapsed = start_time.elapsed();
    info!("me completed in {:?}", elapsed);

    Json(serde_json::json!({ "user": user_info })).into_response()
}

/// Update user info endpoint
pub async fn update_user(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    Json(request): Json<UpdateUserRequest>,
) -> Response {
    let start_time = std::time::Instant::now();

    // Validate name is not empty
    if request.name.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::coded(
                "name_required",
                "Name cannot be empty",
            )),
        )
            .into_response();
    }

    // Update user name in database - no mutex blocking!
    if let Err(e) = app_services
        .metadata_db
        .update_user_name(&user.user_id, request.name.trim())
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(format!("Failed to update user: {}", e))),
        )
            .into_response();
    }

    // Get updated user info - no mutex blocking!
    let db_user = match get_user_or_error(
        &app_services,
        &user.user_id,
        None,
        "User not found",
        DatabaseErrorMessage::Prefix("Failed to get user"),
    )
    .await
    {
        Ok(db_user) => db_user,
        Err(response) => return response,
    };
    let user_info = AuthUserResponse {
        id: db_user.id,
        email: db_user.email,
        name: db_user.name,
        is_admin: user.is_admin,
        is_demo: user.is_demo,
        email_verified: db_user.email_verified,
        subscription_tier: db_user.subscription_tier,
        created_at: db_user.created_at,
        preferred_fiat_currency: db_user.preferred_fiat_currency,
        preferred_language: db_user.preferred_language,
    };

    let elapsed = start_time.elapsed();
    info!("update_user completed in {:?}", elapsed);

    Json(UpdateUserResponse { user: user_info }).into_response()
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
