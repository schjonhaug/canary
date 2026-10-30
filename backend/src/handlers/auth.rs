//! Authentication and user management handlers

use super::error::{ApiError, ApiResult, OrInternal};
use crate::admin_notifications::AdminNotifications;
use crate::api::{
    AppServicesState, AuthServiceState, EmailServiceState, SharedServices, StripeBillingState,
};
use crate::auth::{AuthResponse, AuthService, AuthUserResponse};
use crate::config::{AppConfig, OperatingMode};
use crate::exchange_rates;
use crate::extractors::AuthenticatedUser;
use crate::handlers::helpers::get_user_or_error;
use crate::metadata::{MetadataDb, UserRecord};
use crate::models::{ContactFormRequest, ContactFormResponse, DemoLoginRequest};
use anyhow::Context;
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
    time::{Duration, Instant},
};
use tracing::info;

// Request types imported for handler use only
use crate::auth::{
    ForgotPasswordRequest, LoginRequest, RegisterRequest, ResetPasswordRequest, UpdateUserRequest,
    UpdateUserResponse,
};

/// Cookie name for storing the JWT token
const AUTH_COOKIE_NAME: &str = "auth_token";
const ENUMERATION_RESPONSE_FLOOR: Duration = Duration::from_millis(1500);
const FORGOT_PASSWORD_SUCCESS_MESSAGE: &str =
    "If an account with that email exists, a password reset link has been sent.";
const REGISTRATION_SUCCESS_MESSAGE: &str =
    "Registration successful. Please check your email to verify your account.";
const DEMO_EMAIL: &str = "demo@canarybitcoin.com";
const MIN_PASSWORD_LENGTH: usize = 12;
const MAX_AUTH_REQUESTS_PER_IP: i64 = 10;
const AUTH_IP_RATE_LIMIT_WINDOW_MINUTES: i64 = 15;
const MAX_FAILED_LOGIN_ATTEMPTS_PER_EMAIL: i64 = 50;
const FAILED_LOGIN_EMAIL_RATE_LIMIT_WINDOW_MINUTES: i64 = 60;
const MAX_CONTACT_REQUESTS_PER_IP: i64 = 3;
const CONTACT_RATE_LIMIT_WINDOW_MINUTES: i64 = 60;
const MAX_ADMIN_MFA_ATTEMPTS: i64 = 5;
const ADMIN_MFA_RATE_LIMIT_WINDOW_MINUTES: i64 = 5;

fn rate_limited(code: &'static str, message: &'static str, retry_after_seconds: i64) -> ApiError {
    ApiError::too_many_requests(
        code,
        message,
        u64::try_from(retry_after_seconds).unwrap_or(0),
    )
}

/// A per-email limit on attempts at an unauthenticated endpoint.
struct EmailRateLimit {
    scope: &'static str,
    max_attempts: i64,
    window_minutes: i64,
    code: &'static str,
    message: &'static str,
    error_message: &'static str,
}

const REGISTRATION_EMAIL_RATE_LIMIT: EmailRateLimit = EmailRateLimit {
    scope: "register",
    max_attempts: 3,
    window_minutes: 60,
    code: "registration_rate_limit",
    message: "Too many registration attempts. Please try again later.",
    error_message: "Failed to validate registration rate limit",
};

const FORGOT_PASSWORD_EMAIL_RATE_LIMIT: EmailRateLimit = EmailRateLimit {
    scope: "forgot_password",
    max_attempts: 3,
    window_minutes: 60,
    code: "forgot_password_rate_limit",
    message: "Too many password reset attempts. Please try again later.",
    error_message: "Failed to validate password reset rate limit",
};

async fn enforce_email_rate_limit(
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

async fn pad_enumeration_response(start_time: Instant) {
    pad_response_to_duration(start_time, ENUMERATION_RESPONSE_FLOOR).await;
}

async fn pad_response_to_duration(start_time: Instant, target_duration: Duration) {
    let elapsed = start_time.elapsed();
    if elapsed < target_duration {
        tokio::time::sleep(target_duration.saturating_sub(elapsed)).await;
    }
}

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

async fn enforce_ip_rate_limit(
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

async fn enforce_failed_login_email_rate_limit(
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

fn client_ip(
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

/// Build an `HttpOnly`, Secure, SameSite=Lax cookie for authentication
/// Cloud cookies always require HTTPS. Self-hosted mode supports local HTTP.
/// The cookie expires in 7 days, matching the JWT token expiration
///
/// SameSite=Lax is used because:
/// - It allows cookies on same-site navigation (clicking links) while blocking cross-site POST
/// - Works for same-origin deployments (frontend and backend on same domain)
/// - For cross-origin setups, clients should use the Authorization header with the token from login response
fn build_auth_cookie(token: &str, mode: &OperatingMode) -> String {
    let secure = if *mode == OperatingMode::Cloud {
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
fn build_clear_auth_cookie(mode: &OperatingMode) -> String {
    let secure = if *mode == OperatingMode::Cloud {
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

pub(crate) async fn update_password_and_revoke_sessions(
    metadata_db: &MetadataDb,
    user_id: &str,
    password_hash: &str,
) -> anyhow::Result<()> {
    metadata_db
        .update_user_password_and_revoke_sessions(user_id, password_hash)
        .await?;
    Ok(())
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

fn auth_user_response(user: UserRecord) -> AuthUserResponse {
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

fn generate_token(auth_service: &AuthService, user: &UserRecord) -> ApiResult<String> {
    auth_service
        .generate_token(&user.id, &user.email, user.is_admin, user.is_demo)
        .or_internal("Failed to generate token")
}

/// Stores a seven-day session for `token` and returns the token hash.
async fn create_session(
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
fn signed_in_response(token: String, user: UserRecord, mode: &OperatingMode) -> Response {
    let cookie = build_auth_cookie(&token, mode);
    let body = AuthResponse {
        token,
        user: auth_user_response(user),
        requires_name: None,
    };
    ([(header::SET_COOKIE, cookie)], Json(body)).into_response()
}

/// User registration endpoint
pub async fn register(
    State(app_services): State<AppServicesState>,
    State(stripe_billing): State<StripeBillingState>,
    State(config): State<Arc<AppConfig>>,
    State(services): State<SharedServices>,
    address: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(request): Json<RegisterRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if config.is_self_hosted_mode() {
        return Err(ApiError::uncoded(
            StatusCode::FORBIDDEN,
            "Registration is disabled in self-hosted mode",
        ));
    }

    let start_time = Instant::now();
    enforce_ip_rate_limit(
        &app_services,
        &config,
        "register",
        client_ip(&headers, address, &config),
        MAX_AUTH_REQUESTS_PER_IP,
        AUTH_IP_RATE_LIMIT_WINDOW_MINUTES,
        true,
    )
    .await?;
    validate_registration(&request)?;
    enforce_email_rate_limit(
        &app_services,
        &REGISTRATION_EMAIL_RATE_LIMIT,
        &request.email,
    )
    .await?;

    let existing_user = app_services
        .metadata_db
        .get_user_by_email(&request.email)
        .await
        .or_internal("Failed to check user existence")?;
    if let Some(existing_user) = existing_user {
        // Return same response as successful registration to prevent email enumeration.
        notify_registration_attempt(services.email_service, &existing_user);

        // Ensure response timing matches a real registration to prevent timing attacks.
        // Real registrations involve password hashing + Stripe + email (~1-2s).
        // Sleep for the remaining time up to the target, rather than a fixed delay.
        pad_enumeration_response(start_time).await;
        info!(
            "register (existing email) completed in {:?}",
            start_time.elapsed()
        );
        return Ok(message_json(REGISTRATION_SUCCESS_MESSAGE));
    }

    // Without an email service, delivery is skipped in dev mode.
    let auth_service = shared_auth_service(&services.auth_service, &config)?;

    // Dev test emails are verified on creation; everyone else confirms by email.
    let is_dev_email = AuthService::is_dev_test_email(&request.email);
    let preferred_language = request
        .browser_locale
        .as_deref()
        .map(crate::metadata::locale_to_language);
    let user_id = create_user(&app_services, &request, is_dev_email, preferred_language).await?;

    // Send admin notification for new user signup (fire-and-forget)
    AdminNotifications::spawn_if_enabled(&config, move |admin_notifications| async move {
        admin_notifications.notify_user_signup().await;
    });
    create_stripe_customer(&app_services, &stripe_billing, &user_id).await?;
    if request.marketing_emails_opt_in {
        add_to_marketing_audience(auth_service, &request);
    }

    if is_dev_email {
        info!("register completed in {:?}", start_time.elapsed());
        return Ok(message_json(
            "Registration successful (dev mode - no email verification required).",
        ));
    }

    send_verification_email(
        &app_services,
        auth_service,
        &user_id,
        &request,
        preferred_language.unwrap_or("en-US"),
    )
    .await?;
    info!("register completed in {:?}", start_time.elapsed());
    Ok(message_json(REGISTRATION_SUCCESS_MESSAGE))
}

fn validate_registration(request: &RegisterRequest) -> ApiResult<()> {
    if !request.email.contains('@') || request.email.len() < 5 {
        return Err(ApiError::bad_request(
            "invalid_email_format",
            "Invalid email format",
        ));
    }
    validate_password_length(&request.password)
}

/// Tells the owner of an existing account that someone tried to register
/// with their email (fire-and-forget).
fn notify_registration_attempt(email_service: EmailServiceState, existing_user: &UserRecord) {
    let Some(email_service) = email_service else {
        return;
    };
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
            tracing::error!("Failed to send registration attempt notification to {email}: {e}");
        }
    });
}

async fn create_user(
    app_services: &AppServicesState,
    request: &RegisterRequest,
    email_verified: bool,
    preferred_language: Option<&str>,
) -> ApiResult<String> {
    let password_hash = AuthService::hash_password_async(&request.password)
        .await
        .or_internal("Failed to hash password")?;
    let preferred_currency = request
        .browser_locale
        .as_deref()
        .map(exchange_rates::ExchangeRateService::locale_to_currency);

    app_services
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
        .or_internal("Failed to create user")
}

/// Creates the Stripe customer for a new user. The subscription itself starts
/// when they add their first wallet.
async fn create_stripe_customer(
    app_services: &AppServicesState,
    stripe_billing: &StripeBillingState,
    user_id: &str,
) -> ApiResult<()> {
    let user_record = app_services
        .metadata_db
        .get_user_by_id(user_id)
        .await
        .or_internal("Failed to retrieve user")?
        .ok_or_else(|| {
            ApiError::internal(
                "User was created but could not be retrieved",
                anyhow::anyhow!("user {user_id} is missing right after creation"),
            )
        })?;

    if let Some(stripe_service) = stripe_billing {
        // A Stripe failure does not fail registration. The user can still use
        // the service, just without Stripe integration.
        if let Err(e) = stripe_service
            .create_stripe_customer_only(&user_record, &app_services.metadata_db)
            .await
        {
            tracing::error!(
                "Failed to create Stripe customer for user {}: {e}",
                user_record.email
            );
        }
    } else {
        info!(
            "Stripe not enabled, user {} registered without Stripe integration",
            user_record.email
        );
    }
    Ok(())
}

/// Adds the user to the marketing audience (fire-and-forget).
fn add_to_marketing_audience(auth_service: &AuthService, request: &RegisterRequest) {
    let Some(email_service) = auth_service.email_service.clone() else {
        return;
    };
    let email = request.email.clone();
    let name = request.name.clone();
    tokio::spawn(async move {
        if let Err(e) = email_service.add_to_marketing_audience(&email, &name).await {
            tracing::error!("Failed to add user to marketing audience: {e}");
        }
    });
}

async fn send_verification_email(
    app_services: &AppServicesState,
    auth_service: &AuthService,
    user_id: &str,
    request: &RegisterRequest,
    language: &str,
) -> ApiResult<()> {
    let token = AuthService::generate_verification_token();
    let token_hash = AuthService::hash_token(&token);

    // Only the hash is stored, so a database leak does not expose usable tokens.
    app_services
        .metadata_db
        .create_email_verification_token(user_id, &token_hash)
        .await
        .or_internal("Failed to create verification token")?;

    auth_service
        .send_email_verification(&request.email, &request.name, &token, language)
        .await
        // Provider errors can echo the recipient address, so they are not returned.
        .map_err(|_| {
            ApiError::uncoded(StatusCode::BAD_REQUEST, "Failed to send verification email")
        })
}

/// The request details that a failed login attempt is charged against.
struct LoginAttempt<'a> {
    app_services: &'a AppServicesState,
    config: &'a AppConfig,
    client_address: Option<SocketAddr>,
    email: &'a str,
}

impl LoginAttempt<'_> {
    async fn enforce_ip_rate_limit(&self, record_attempt: bool) -> ApiResult<()> {
        enforce_ip_rate_limit(
            self.app_services,
            self.config,
            "login",
            self.client_address,
            MAX_AUTH_REQUESTS_PER_IP,
            AUTH_IP_RATE_LIMIT_WINDOW_MINUTES,
            record_attempt,
        )
        .await
    }

    /// Counts a failure against both the client address and the email.
    async fn charge_failure(&self) -> ApiResult<()> {
        self.enforce_ip_rate_limit(true).await?;
        enforce_failed_login_email_rate_limit(self.app_services, self.email).await
    }

    async fn record_login(&self, success: bool) {
        let _ = self
            .app_services
            .metadata_db
            .record_login_attempt(self.email, success)
            .await;
    }
}

/// User login endpoint
pub async fn login(
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    State(auth_service): State<AuthServiceState>,
    address: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> ApiResult<Response> {
    let attempt = LoginAttempt {
        app_services: &app_services,
        config: &config,
        client_address: client_ip(&headers, address, &config),
        email: &request.email,
    };
    attempt.enforce_ip_rate_limit(false).await?;

    let user_record = find_login_user(&attempt, &request.password).await?;
    let auth_service = shared_auth_service(&auth_service, &config)?;

    // Dev test emails sign in with the shared dev password instead.
    let is_dev_email = AuthService::is_dev_test_email(&request.email);
    verify_login_password(&attempt, &user_record, &request.password, is_dev_email).await?;

    if !is_dev_email && !user_record.email_verified {
        return Err(ApiError::forbidden(
            "email_not_verified",
            "Email not verified. Please check your email and click the verification link.",
        ));
    }

    let mfa_version = if config.is_cloud_mode() && user_record.is_admin {
        Some(verify_admin_mfa(&attempt, &user_record, request.mfa_code.as_deref()).await?)
    } else {
        None
    };

    // Successful login - record it and reset failed login counter
    attempt.record_login(true).await;
    if let Err(e) = app_services
        .metadata_db
        .update_last_login(&user_record.id)
        .await
    {
        tracing::warn!(
            "Failed to update last login for user {}: {e:?}",
            user_record.id
        );
    }

    let token = generate_token(auth_service, &user_record)?;
    let token_hash = create_session(&app_services, &user_record.id, &token).await?;
    if let Some(version) = mfa_version {
        bind_admin_session(&app_services, &user_record.id, &token_hash, &version).await?;
    }

    Ok(signed_in_response(
        token,
        user_record,
        &config.operating_mode,
    ))
}

/// Loads the user signing in, or rejects the attempt as invalid credentials.
async fn find_login_user(attempt: &LoginAttempt<'_>, password: &str) -> ApiResult<UserRecord> {
    let user = attempt
        .app_services
        .metadata_db
        .get_user_by_email(attempt.email)
        .await
        .or_internal("Failed to check user")?;
    if let Some(user) = user {
        return Ok(user);
    }

    // Spend the same hashing time as a real check so the response does not
    // reveal whether the account exists.
    AuthService::verify_dummy_password_async(password)
        .await
        .or_internal("Failed to verify credentials")?;
    attempt.charge_failure().await?;
    Err(ApiError::bad_request(
        "invalid_credentials",
        "Invalid credentials",
    ))
}

async fn verify_login_password(
    attempt: &LoginAttempt<'_>,
    user_record: &UserRecord,
    password: &str,
    is_dev_email: bool,
) -> ApiResult<()> {
    let password_valid = if is_dev_email {
        password == AuthService::get_dev_test_password()
    } else {
        AuthService::verify_password_async(password, &user_record.password_hash)
            .await
            .or_internal("Failed to verify password")?
    };
    if password_valid {
        return Ok(());
    }

    attempt.charge_failure().await?;
    attempt.record_login(false).await;
    let error = if attempt.config.is_self_hosted_mode() {
        ApiError::bad_request("invalid_self_hosted_password", "Incorrect password")
    } else {
        ApiError::bad_request("invalid_credentials", "Invalid credentials")
    };
    Err(error)
}

/// Checks a cloud administrator's authenticator code and returns the MFA
/// version the new session is bound to.
async fn verify_admin_mfa(
    attempt: &LoginAttempt<'_>,
    user_record: &UserRecord,
    code: Option<&str>,
) -> ApiResult<String> {
    let Some(code) = code else {
        return Err(ApiError::unauthorized(
            "admin_mfa_required",
            "Enter your authenticator code to sign in.",
        ));
    };

    // Charge every code attempt before verification so a blocked account
    // cannot keep testing codes, including attempts from different addresses.
    let metadata_db = &attempt.app_services.metadata_db;
    let allowed = metadata_db
        .check_auth_rate_limit(
            "admin_mfa",
            &user_record.id,
            MAX_ADMIN_MFA_ATTEMPTS,
            ADMIN_MFA_RATE_LIMIT_WINDOW_MINUTES,
        )
        .await
        .map_err(|_| {
            ApiError::uncoded(
                StatusCode::SERVICE_UNAVAILABLE,
                "Authenticator verification unavailable",
            )
        })?;
    if !allowed {
        return Err(rate_limited(
            "admin_mfa_rate_limited",
            "Too many authenticator attempts. Try again in five minutes.",
            ADMIN_MFA_RATE_LIMIT_WINDOW_MINUTES * 60,
        ));
    }

    let verification_unavailable = match crate::admin_mfa::verify(
        metadata_db,
        attempt.config.admin_mfa_secrets_file(),
        &user_record.id,
        code,
    )
    .await
    {
        // Demo accounts are never enrolled as cloud administrators; even
        // a matching synthetic factor must not create an admin session.
        Ok(Some(version)) if !user_record.is_demo => return Ok(version),
        result => result.is_err(),
    };

    attempt.charge_failure().await?;
    attempt.record_login(false).await;
    if verification_unavailable {
        tracing::warn!("Cloud administrator MFA validation unavailable");
    }
    Err(ApiError::unauthorized(
        "admin_mfa_invalid",
        "Authenticator verification failed. Try a fresh code or contact the operator.",
    ))
}

/// Marks a new session as MFA-verified, or deletes it again when that fails.
async fn bind_admin_session(
    app_services: &AppServicesState,
    user_id: &str,
    token_hash: &str,
    mfa_version: &str,
) -> ApiResult<()> {
    let metadata_db = &app_services.metadata_db;
    if metadata_db
        .verify_admin_session(user_id, token_hash, mfa_version)
        .await
        .is_err()
    {
        let _ = metadata_db.delete_session(token_hash).await;
        return Err(ApiError::uncoded(
            StatusCode::SERVICE_UNAVAILABLE,
            "Administrator session could not be established",
        ));
    }
    Ok(())
}

/// Auto-login endpoint for demo account (no password required)
pub async fn demo_login(
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    State(auth_service): State<AuthServiceState>,
    Json(request): Json<DemoLoginRequest>,
) -> ApiResult<Response> {
    let start_time = Instant::now();

    if !config.is_cloud_mode() {
        return Err(ApiError::uncoded(
            StatusCode::FORBIDDEN,
            "Demo login is only available in cloud mode",
        ));
    }

    let mut user_record = app_services
        .metadata_db
        .get_user_by_email(DEMO_EMAIL)
        .await
        .or_internal("Failed to get demo user")?
        .ok_or_else(|| {
            ApiError::uncoded(
                StatusCode::NOT_FOUND,
                "Demo user not found. Please ensure backend is running in dev mode.",
            )
        })?;
    if !user_record.is_demo {
        return Err(ApiError::uncoded(
            StatusCode::FORBIDDEN,
            "This user is not a demo account",
        ));
    }

    if let Some(browser_locale) = &request.browser_locale {
        apply_demo_locale(&app_services.metadata_db, &mut user_record, browser_locale).await;
    }

    let auth_service = shared_auth_service(&auth_service, &config)?;
    let token = generate_token(auth_service, &user_record)?;

    // Clean up any existing sessions for this user to prevent token collision
    let _ = app_services
        .metadata_db
        .delete_user_sessions(&user_record.id)
        .await;
    create_session(&app_services, &user_record.id, &token).await?;

    info!("demo_login completed in {:?}", start_time.elapsed());
    Ok(signed_in_response(
        token,
        user_record,
        &config.operating_mode,
    ))
}

/// Updates the shared demo user's language and currency to match the
/// visitor's browser locale. Failures are logged and otherwise ignored.
async fn apply_demo_locale(
    metadata_db: &MetadataDb,
    user_record: &mut UserRecord,
    browser_locale: &str,
) {
    let preferred_language = crate::metadata::locale_to_language(browser_locale);
    if user_record.preferred_language.as_deref() != Some(preferred_language) {
        if let Err(e) = metadata_db
            .update_user_preferred_language(&user_record.id, preferred_language)
            .await
        {
            tracing::warn!("Failed to update demo user language to {preferred_language}: {e:?}");
        } else {
            user_record.preferred_language = Some(preferred_language.to_string());
        }
    }

    let preferred_currency =
        exchange_rates::ExchangeRateService::locale_to_currency(browser_locale);
    if user_record.preferred_fiat_currency.as_deref() != Some(preferred_currency) {
        if let Err(e) = metadata_db
            .update_user_preferred_currency(&user_record.id, preferred_currency)
            .await
        {
            tracing::warn!("Failed to update demo user currency to {preferred_currency}: {e:?}");
        } else {
            user_record.preferred_fiat_currency = Some(preferred_currency.to_string());
        }
    }
}

/// Email verification endpoint
pub async fn verify_email(
    State(config): State<Arc<AppConfig>>,
    State(app_services): State<AppServicesState>,
    Path(token): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    if config.is_self_hosted_mode() {
        return Err(ApiError::uncoded(
            StatusCode::FORBIDDEN,
            "Email verification is unavailable in self-hosted mode",
        ));
    }

    let start_time = Instant::now();

    // Hash the incoming token for verification (tokens are stored hashed)
    let token_hash = AuthService::hash_token(&token);
    let verified = app_services
        .metadata_db
        .verify_email_token(&token_hash)
        .await;
    info!("verify_email completed in {:?}", start_time.elapsed());

    verified
        .or_internal("Failed to verify email")?
        .ok_or_else(|| {
            ApiError::bad_request(
                "invalid_verification_token",
                "Invalid or expired verification token",
            )
        })?;
    Ok(message_json(
        "Email verified successfully. You can now log in.",
    ))
}

/// Forgot password endpoint
pub async fn forgot_password(
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    State(auth_service): State<AuthServiceState>,
    address: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(request): Json<ForgotPasswordRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if config.is_self_hosted_mode() {
        return Err(ApiError::uncoded(
            StatusCode::FORBIDDEN,
            "Password reset is unavailable in self-hosted mode",
        ));
    }

    let start_time = Instant::now();
    enforce_ip_rate_limit(
        &app_services,
        &config,
        "forgot_password",
        client_ip(&headers, address, &config),
        MAX_AUTH_REQUESTS_PER_IP,
        AUTH_IP_RATE_LIMIT_WINDOW_MINUTES,
        true,
    )
    .await?;
    enforce_email_rate_limit(
        &app_services,
        &FORGOT_PASSWORD_EMAIL_RATE_LIMIT,
        &request.email,
    )
    .await?;

    let user_record = app_services
        .metadata_db
        .get_user_by_email(&request.email)
        .await
        .or_internal("Failed to check user")?;

    // Every outcome from here on is padded, so response timing does not
    // reveal whether an account exists for the email.
    let Some(user_record) = user_record else {
        pad_enumeration_response(start_time).await;
        info!(
            "forgot_password (unknown email) completed in {:?}",
            start_time.elapsed()
        );
        return Ok(message_json(FORGOT_PASSWORD_SUCCESS_MESSAGE));
    };

    let sent = send_password_reset(&app_services, &config, &auth_service, user_record).await;
    pad_enumeration_response(start_time).await;
    sent?;
    info!("forgot_password completed in {:?}", start_time.elapsed());
    Ok(message_json(FORGOT_PASSWORD_SUCCESS_MESSAGE))
}

async fn send_password_reset(
    app_services: &AppServicesState,
    config: &AppConfig,
    auth_service: &AuthServiceState,
    user_record: UserRecord,
) -> ApiResult<()> {
    let auth_service = shared_auth_service(auth_service, config)?;
    let token = AuthService::generate_verification_token();
    let token_hash = AuthService::hash_token(&token);

    // Only the hash is stored, so a database leak does not expose usable tokens.
    app_services
        .metadata_db
        .create_password_reset_token(&user_record.id, &token_hash)
        .await
        .or_internal("Failed to create reset token")?;

    let user_language = user_record.preferred_language.as_deref().unwrap_or("en-US");
    auth_service
        .send_password_reset(
            &user_record.email,
            user_record.name.as_deref().unwrap_or("User"),
            &token,
            user_language,
        )
        .await
        .or_internal("Failed to send reset email")
}

/// Contact form submission endpoint
pub async fn submit_contact_form(
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    State(email_service): State<EmailServiceState>,
    address: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(payload): Json<ContactFormRequest>,
) -> ApiResult<Json<ContactFormResponse>> {
    enforce_ip_rate_limit(
        &app_services,
        &config,
        "contact",
        client_ip(&headers, address, &config),
        MAX_CONTACT_REQUESTS_PER_IP,
        CONTACT_RATE_LIMIT_WINDOW_MINUTES,
        true,
    )
    .await?;
    let email = payload.email.trim();
    let message = payload.message.trim();
    validate_contact_form(email, message)?;

    let email_service = email_service
        .ok_or_else(|| anyhow::anyhow!("Email service not configured"))
        .or_internal("Contact form is temporarily unavailable.")?;
    email_service
        .send_contact_form_submission(email, message)
        .await
        .or_internal("Failed to send message. Please try again later.")?;

    info!("Contact form submitted");
    Ok(Json(ContactFormResponse {
        message: "Thank you for your message. We'll get back to you soon.".to_string(),
    }))
}

fn validate_contact_form(email: &str, message: &str) -> ApiResult<()> {
    if !valid_contact_email(email) {
        return Err(ApiError::bad_request(
            "invalid_email_format",
            "Please provide a valid email address",
        ));
    }
    if message.len() < 10 {
        return Err(ApiError::bad_request(
            "message_too_short",
            "Message must be at least 10 characters",
        ));
    }
    if message.len() > 5000 {
        return Err(ApiError::bad_request(
            "message_too_long",
            "Message must be less than 5000 characters",
        ));
    }
    Ok(())
}

fn valid_contact_email(email: &str) -> bool {
    if email.len() > 255 || email.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.contains('@')
        && domain.contains('.')
        && domain.split('.').all(|part| !part.is_empty())
}

/// Reset password endpoint
pub async fn reset_password(
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    Path(token): Path<String>,
    Json(request): Json<ResetPasswordRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if config.is_self_hosted_mode() {
        return Err(ApiError::uncoded(
            StatusCode::FORBIDDEN,
            "Password reset is unavailable in self-hosted mode",
        ));
    }

    let start_time = Instant::now();
    validate_password_length(&request.password)?;

    // Hash the incoming token for verification (tokens are stored hashed)
    let token_hash = AuthService::hash_token(&token);
    let user_id = app_services
        .metadata_db
        .verify_password_reset_token(&token_hash)
        .await
        .or_internal("Failed to verify token")?
        .ok_or_else(|| {
            ApiError::bad_request("invalid_reset_token", "Invalid or expired reset token")
        })?;

    let password_hash = AuthService::hash_password_async(&request.password)
        .await
        .or_internal("Failed to hash password")?;
    update_password_and_revoke_sessions(&app_services.metadata_db, &user_id, &password_hash)
        .await
        .with_context(|| format!("Failed to complete password reset for user {user_id}"))
        .or_internal("Failed to complete password reset")?;

    info!("reset_password completed in {:?}", start_time.elapsed());
    Ok(message_json(
        "Password reset successfully. You can now log in with your new password.",
    ))
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
    let clear_cookie = build_clear_auth_cookie(&config.operating_mode);
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

#[cfg(test)]
mod tests {
    use super::{
        build_auth_cookie, build_clear_auth_cookie, client_ip_from_forwarded_for,
        pad_response_to_duration, valid_contact_email,
    };

    #[test]
    fn contact_reply_address_rejects_invalid_header_values() {
        assert!(valid_contact_email("visitor@example.com"));
        for address in [
            "a@b@c.com",
            "a@b.com\r\nBcc: other@example.com",
            "@example.com",
            "visitor@",
            "visitor@example..com",
            "visitor@example",
        ] {
            assert!(!valid_contact_email(address), "accepted {address:?}");
        }
    }
    use crate::config::OperatingMode;
    use axum::http::HeaderMap;
    use std::{
        collections::HashSet,
        net::{IpAddr, Ipv4Addr, SocketAddr},
    };

    #[test]
    fn cloud_authentication_and_logout_cookies_always_require_https() {
        for cookie in [
            build_auth_cookie("synthetic-token", &OperatingMode::Cloud),
            build_clear_auth_cookie(&OperatingMode::Cloud),
        ] {
            let attributes: Vec<_> = cookie.split("; ").collect();
            assert!(attributes.contains(&"Secure"));
            assert!(attributes.contains(&"HttpOnly"));
            assert!(attributes.contains(&"SameSite=Lax"));
            assert!(attributes.contains(&"Path=/"));
        }
        assert!(build_clear_auth_cookie(&OperatingMode::Cloud).contains("Max-Age=0"));
    }

    #[test]
    fn self_hosted_authentication_and_logout_support_local_http() {
        for cookie in [
            build_auth_cookie("synthetic-token", &OperatingMode::SelfHosted),
            build_clear_auth_cookie(&OperatingMode::SelfHosted),
        ] {
            assert!(!cookie.split("; ").any(|attribute| attribute == "Secure"));
            assert!(cookie.contains("HttpOnly"));
        }
    }

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
