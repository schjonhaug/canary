//! Authentication and user management handlers

use crate::admin_notifications::AdminNotifications;
use crate::api::{AppServicesState, StripeBillingState};
use crate::auth::{AuthResponse, AuthService, AuthUserResponse};
use crate::config::AppConfig;
use crate::email_service::EmailService;
use crate::exchange_rates;
use crate::extractors::AuthenticatedUser;
use crate::models::{ContactFormRequest, ContactFormResponse, DemoLoginRequest, ErrorResponse};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use std::sync::Arc;
use tracing::info;

// Request types imported for handler use only
use crate::auth::{
    ForgotPasswordRequest, LoginRequest, RegisterRequest, ResetPasswordRequest, UpdateUserRequest,
    UpdateUserResponse,
};

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
            Json(ErrorResponse {
                error: "Invalid email format".to_string(),
            }),
        )
            .into_response();
    }

    // Validate password strength
    if request.password.len() < 6 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Password must be at least 6 characters long".to_string(),
            }),
        )
            .into_response();
    }

    // Check if user already exists - no mutex blocking!
    match app_services
        .metadata_db
        .get_user_by_email(&request.email)
        .await
    {
        Ok(Some(_)) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "User with this email already exists".to_string(),
                }),
            )
                .into_response();
        }
        Ok(None) => {} // User doesn't exist, proceed
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to check user existence: {}", e),
                }),
            )
                .into_response();
        }
    }

    let jwt_secret = match config.get_jwt_secret() {
        Ok(secret) => secret.to_string(),
        Err(e) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
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
                Json(ErrorResponse {
                    error: format!("Failed to hash password: {}", e),
                }),
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
                Json(ErrorResponse {
                    error: format!("Failed to create user: {}", e),
                }),
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
                    Json(ErrorResponse {
                        error: "User was created but could not be retrieved".to_string(),
                    }),
                )
                    .into_response();
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Failed to retrieve user: {}", e),
                    }),
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
                Json(ErrorResponse {
                    error: format!("Failed to create verification token: {}", e),
                }),
            )
                .into_response();
        }

        // Send verification email
        let user_language = preferred_language.unwrap_or("en");
        if let Err(e) = auth_service
            .send_email_verification(&request.email, &request.name, &token, user_language)
            .await
        {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Failed to send verification email: {}", e),
                }),
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
const MAX_FAILED_ATTEMPTS_PER_IP: i64 = 20; // Rate limit IP after 20 failed attempts
const RATE_LIMIT_WINDOW_MINUTES: i64 = 15; // Time window for counting attempts
const ACCOUNT_LOCKOUT_MINUTES: i64 = 15; // How long to lock an account

/// Extract client IP address from headers (supports X-Forwarded-For for proxied requests)
fn extract_client_ip(headers: &HeaderMap) -> Option<String> {
    // First try X-Forwarded-For (for proxied requests)
    if let Some(forwarded_for) = headers.get("x-forwarded-for") {
        if let Ok(value) = forwarded_for.to_str() {
            // Take the first IP in the chain (original client)
            if let Some(ip) = value.split(',').next() {
                return Some(ip.trim().to_string());
            }
        }
    }

    // Try X-Real-IP
    if let Some(real_ip) = headers.get("x-real-ip") {
        if let Ok(value) = real_ip.to_str() {
            return Some(value.trim().to_string());
        }
    }

    None
}

/// User login endpoint
pub async fn login(
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Response {
    let client_ip = extract_client_ip(&headers);

    // Check IP-based rate limiting first (before any user lookup)
    if let Some(ref ip) = client_ip {
        match app_services
            .metadata_db
            .get_failed_login_count_by_ip(ip, RATE_LIMIT_WINDOW_MINUTES)
            .await
        {
            Ok(count) if count >= MAX_FAILED_ATTEMPTS_PER_IP => {
                tracing::warn!(
                    "IP {} rate limited: {} failed attempts in {} minutes",
                    ip,
                    count,
                    RATE_LIMIT_WINDOW_MINUTES
                );
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(ErrorResponse {
                        error: "Too many login attempts. Please try again later.".to_string(),
                    }),
                )
                    .into_response();
            }
            Err(e) => {
                tracing::error!("Failed to check IP rate limit: {}", e);
                // Continue with login attempt if rate limit check fails
            }
            _ => {}
        }
    }

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
                Json(ErrorResponse {
                    error: format!(
                        "Account temporarily locked due to too many failed login attempts. Try again after {}.",
                        locked_until
                    ),
                }),
            )
                .into_response();
        }
        Ok(None) => {
            // Lockout has expired - reset the failed login counter
            // This prevents immediate re-locking on the next failed attempt
            let _ = app_services
                .metadata_db
                .reset_failed_login_count(&request.email)
                .await;
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
                .record_login_attempt(&request.email, client_ip.as_deref(), false)
                .await;

            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Invalid credentials".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to check user: {}", e),
                }),
            )
                .into_response();
        }
    };

    let jwt_secret = match config.get_jwt_secret() {
        Ok(secret) => secret.to_string(),
        Err(e) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
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
                    Json(ErrorResponse {
                        error: format!("Failed to verify password: {}", e),
                    }),
                )
                    .into_response();
            }
        }
    };

    if !password_valid {
        // Record failed login attempt
        let _ = app_services
            .metadata_db
            .record_login_attempt(&request.email, client_ip.as_deref(), false)
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
                }

                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(ErrorResponse {
                        error: format!(
                            "Account temporarily locked due to too many failed login attempts. Try again in {} minutes.",
                            ACCOUNT_LOCKOUT_MINUTES
                        ),
                    }),
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
            Json(ErrorResponse {
                error: "Invalid credentials".to_string(),
            }),
        )
            .into_response();
    }

    // Check email verification (skip for dev emails)
    if !is_dev_email && !user_record.email_verified {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error:
                    "Email not verified. Please check your email and click the verification link."
                        .to_string(),
            }),
        )
            .into_response();
    }

    // Successful login - record it and reset failed login counter
    let _ = app_services
        .metadata_db
        .record_login_attempt(&request.email, client_ip.as_deref(), true)
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
                Json(ErrorResponse {
                    error: format!("Failed to generate token: {}", e),
                }),
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
            Json(ErrorResponse {
                error: format!("Failed to create session: {}", e),
            }),
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

    Json(AuthResponse {
        token,
        user: user_info,
        requires_name: None, // No longer used with email auth
    })
    .into_response()
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
            Json(ErrorResponse {
                error: "Demo login is only available in cloud mode".to_string(),
            }),
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
                Json(ErrorResponse {
                    error: "Demo user not found. Please ensure backend is running in dev mode."
                        .to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to get demo user: {}", e),
                }),
            )
                .into_response();
        }
    };

    // Verify this is actually a demo user
    if !user_record.is_demo {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "This user is not a demo account".to_string(),
            }),
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
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
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
                Json(ErrorResponse {
                    error: format!("Failed to generate token: {}", e),
                }),
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
            Json(ErrorResponse {
                error: format!("Failed to create session: {}", e),
            }),
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

    Json(AuthResponse {
        token,
        user: user_info,
        requires_name: None,
    })
    .into_response()
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
            Json(ErrorResponse {
                error: "Invalid or expired verification token".to_string(),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to verify email: {}", e),
            }),
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
                Json(ErrorResponse {
                    error: format!("Failed to check user: {}", e),
                }),
            )
                .into_response();
        }
    };

    let jwt_secret = match config.get_jwt_secret() {
        Ok(secret) => secret.to_string(),
        Err(e) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
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
            Json(ErrorResponse {
                error: format!("Failed to create reset token: {}", e),
            }),
        )
            .into_response();
    }

    // Send password reset email
    let user_language = user_record.preferred_language.as_deref().unwrap_or("en");
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
            Json(ErrorResponse {
                error: format!("Failed to send reset email: {}", e),
            }),
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
            Json(ErrorResponse {
                error: "Please provide a valid email address".to_string(),
            }),
        )
            .into_response();
    }

    // Validate message length
    if message.len() < 10 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Message must be at least 10 characters".to_string(),
            }),
        )
            .into_response();
    }

    if message.len() > 5000 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Message must be less than 5000 characters".to_string(),
            }),
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
                        Json(ErrorResponse {
                            error: "Failed to send message. Please try again later.".to_string(),
                        }),
                    )
                        .into_response()
                }
            }
        }
        Err(e) => {
            tracing::error!("Email service not configured: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Contact form is temporarily unavailable.".to_string(),
                }),
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
            Json(ErrorResponse {
                error: "Password must be at least 6 characters long".to_string(),
            }),
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
                Json(ErrorResponse {
                    error: "Invalid or expired reset token".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to verify token: {}", e),
                }),
            )
                .into_response();
        }
    };

    let jwt_secret = match config.get_jwt_secret() {
        Ok(secret) => secret.to_string(),
        Err(e) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
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
                Json(ErrorResponse {
                    error: format!("Failed to hash password: {}", e),
                }),
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
            Json(ErrorResponse {
                error: format!("Failed to update password: {}", e),
            }),
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

    // Get the token from the Authorization header
    let auth_header = match headers.get("authorization").and_then(|h| h.to_str().ok()) {
        Some(header) => header,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Authentication required".to_string(),
                }),
            )
                .into_response();
        }
    };

    if !auth_header.starts_with("Bearer ") {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Invalid authorization header".to_string(),
            }),
        )
            .into_response();
    }

    let token = &auth_header[7..]; // Skip "Bearer "

    // Hash the token to find it in the database
    let token_hash = AuthService::hash_token(token);

    // Direct metadata access - no mutex blocking!
    let result = app_services.metadata_db.delete_session(&token_hash).await;

    let elapsed = start_time.elapsed();
    info!("logout completed in {:?}", elapsed);

    if let Err(e) = result {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to delete session: {}", e),
            }),
        )
            .into_response();
    }

    Json(serde_json::json!({
        "message": "Logged out successfully"
    }))
    .into_response()
}

/// Get current user info endpoint
pub async fn me(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
) -> Response {
    let start_time = std::time::Instant::now();

    // Get user info from database - no mutex blocking!
    let user_info = match app_services.metadata_db.get_user_by_id(&user.user_id).await {
        Ok(Some(db_user)) => AuthUserResponse {
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
        },
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "User not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to get user info".to_string(),
                }),
            )
                .into_response();
        }
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
            Json(ErrorResponse {
                error: "Name cannot be empty".to_string(),
            }),
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
            Json(ErrorResponse {
                error: format!("Failed to update user: {}", e),
            }),
        )
            .into_response();
    }

    // Get updated user info - no mutex blocking!
    let user_info = match app_services.metadata_db.get_user_by_id(&user.user_id).await {
        Ok(Some(db_user)) => AuthUserResponse {
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
        },
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "User not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to get user: {}", e),
                }),
            )
                .into_response();
        }
    };

    let elapsed = start_time.elapsed();
    info!("update_user completed in {:?}", elapsed);

    Json(UpdateUserResponse { user: user_info }).into_response()
}
