//! Authentication and user management handlers

use crate::admin_notifications::AdminNotifications;
use crate::api::{AppServicesState, StripeBillingState};
use crate::auth::{AuthResponse, AuthService, AuthUserResponse};
use crate::config::AppConfig;
use crate::email_service::EmailService;
use crate::exchange_rates;
use crate::extractors::AuthenticatedUser;
use crate::handlers::helpers::{get_user_or_error, DatabaseErrorMessage};
use crate::models::{ContactFormRequest, ContactFormResponse, DemoLoginRequest, ErrorResponse};
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use std::sync::Arc;
use tracing::info;

// Request types imported for handler use only
use crate::auth::{
    ForgotPasswordRequest, LoginRequest, RegisterRequest, ResetPasswordRequest, UpdateUserRequest,
    UpdateUserResponse,
};

/// Cookie name for storing the JWT token
const AUTH_COOKIE_NAME: &str = "auth_token";

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

/// User registration endpoint
pub async fn register(
    State(app_services): State<AppServicesState>,
    State(stripe_billing): State<StripeBillingState>,
    State(config): State<Arc<AppConfig>>,
    Json(request): Json<RegisterRequest>,
) -> Response {
    let start_time = std::time::Instant::now();

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
    if request.password.len() < 6 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::coded(
                "password_too_short",
                "Password must be at least 6 characters long",
            )),
        )
            .into_response();
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
            let target_duration = std::time::Duration::from_millis(1500);
            let elapsed = start_time.elapsed();
            if elapsed < target_duration {
                tokio::time::sleep(target_duration - elapsed).await;
            }

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
        let admin_notifications = AdminNotifications::new();
        if admin_notifications.is_enabled() {
            let email = request.email.clone();
            let name = request.name.clone();
            tokio::spawn(async move {
                admin_notifications
                    .notify_user_signup(&email, Some(&name))
                    .await;
            });
        }
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
const MAX_FAILED_ATTEMPTS_PER_EMAIL: i64 = 5; // Lock account after 5 failed attempts
const ACCOUNT_LOCKOUT_MINUTES: i64 = 15; // How long to lock an account

/// User login endpoint
pub async fn login(
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    Json(request): Json<LoginRequest>,
) -> Response {
    // Check if account is locked
    match app_services
        .metadata_db
        .check_account_lockout(&request.email)
        .await
    {
        Ok(Some(locked_until)) => {
            tracing::warn!("Login attempt for locked account: {}", request.email);
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(ErrorResponse::coded("account_locked", format!(
                        "Account temporarily locked due to too many failed login attempts. Try again after {}.",
                        locked_until
                    ))),
            )
                .into_response();
        }
        Ok(None) => {
            // Account is not actively locked - check if lockout just expired
            // If so, reset the counter to give user a fresh start
            if let Ok(true) = app_services
                .metadata_db
                .clear_expired_lockout(&request.email)
                .await
            {
                tracing::info!("Cleared expired lockout for {}", request.email);
            }
        }
        Err(e) => {
            tracing::error!("Failed to check account lockout: {}", e);
            // Continue with login attempt if lockout check fails
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
            // Record failed attempt even for non-existent users (prevents user enumeration timing attacks)
            let _ = app_services
                .metadata_db
                .record_login_attempt(&request.email, false)
                .await;

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
        // Record failed login attempt
        let _ = app_services
            .metadata_db
            .record_login_attempt(&request.email, false)
            .await;

        // Increment failed login counter and check if we need to lock the account
        match app_services
            .metadata_db
            .increment_failed_login_count(&request.email)
            .await
        {
            Ok(failed_count) if failed_count >= MAX_FAILED_ATTEMPTS_PER_EMAIL => {
                // Lock the account
                if let Err(e) = app_services
                    .metadata_db
                    .lock_account(&request.email, ACCOUNT_LOCKOUT_MINUTES)
                    .await
                {
                    tracing::error!("Failed to lock account {}: {}", request.email, e);
                } else {
                    tracing::warn!(
                        "Account {} locked after {} failed attempts",
                        request.email,
                        failed_count
                    );

                    // Send account locked notification email (fire-and-forget)
                    if let Ok(email_service) = EmailService::from_env() {
                        let email = user_record.email.clone();
                        let name = user_record
                            .name
                            .clone()
                            .unwrap_or_else(|| "User".to_string());
                        let language = user_record
                            .preferred_language
                            .clone()
                            .unwrap_or_else(|| "en-US".to_string());
                        let lockout_minutes = ACCOUNT_LOCKOUT_MINUTES;

                        tokio::spawn(async move {
                            if let Err(e) = email_service
                                .send_account_locked(&email, &name, lockout_minutes, &language)
                                .await
                            {
                                tracing::error!(
                                    "Failed to send account locked email to {}: {}",
                                    email,
                                    e
                                );
                            }
                        });
                    }
                }

                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(ErrorResponse::coded("account_locked", format!(
                            "Account temporarily locked due to too many failed login attempts. Try again in {} minutes.",
                            ACCOUNT_LOCKOUT_MINUTES
                        ))),
                )
                    .into_response();
            }
            Err(e) => {
                tracing::error!("Failed to increment failed login count: {}", e);
            }
            _ => {}
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

    let _ = app_services
        .metadata_db
        .reset_failed_login_count(&request.email)
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
    State(app_services): State<AppServicesState>,
    Path(token): Path<String>,
) -> Response {
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
    Json(request): Json<ForgotPasswordRequest>,
) -> Response {
    let start_time = std::time::Instant::now();

    // Check if user exists - no mutex blocking!
    let user_record = match app_services
        .metadata_db
        .get_user_by_email(&request.email)
        .await
    {
        Ok(Some(user)) => user,
        Ok(None) => {
            // Don't reveal whether user exists or not
            return Json(serde_json::json!({
                "message": "If an account with that email exists, a password reset link has been sent."
            }))
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
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(format!(
                "Failed to create reset token: {}",
                e
            ))),
        )
            .into_response();
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
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(format!(
                "Failed to send reset email: {}",
                e
            ))),
        )
            .into_response();
    }

    let elapsed = start_time.elapsed();
    info!("forgot_password completed in {:?}", elapsed);

    Json(serde_json::json!({
        "message": "If an account with that email exists, a password reset link has been sent."
    }))
    .into_response()
}

/// Contact form submission endpoint
pub async fn submit_contact_form(Json(payload): Json<ContactFormRequest>) -> Response {
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
    let start_time = std::time::Instant::now();

    // Validate password strength
    if request.password.len() < 6 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::coded(
                "password_too_short",
                "Password must be at least 6 characters long",
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

    // Update password and clear reset tokens - no mutex blocking!
    if let Err(e) = app_services
        .metadata_db
        .update_user_password(&user_id, &password_hash)
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(format!(
                "Failed to update password: {}",
                e
            ))),
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
