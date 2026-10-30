//! Sign-in: password login with cloud administrator MFA, and the
//! password-less demo login.

use super::rate_limit::{
    client_ip, enforce_failed_login_email_rate_limit, enforce_ip_rate_limit, rate_limited,
    AUTH_IP_RATE_LIMIT_WINDOW_MINUTES, MAX_AUTH_REQUESTS_PER_IP,
};
use super::session::{create_session, generate_token, signed_in_response};
use super::shared_auth_service;
use crate::api::{AppServicesState, AuthServiceState};
use crate::auth::{AuthService, LoginRequest};
use crate::config::AppConfig;
use crate::exchange_rates;
use crate::handlers::error::{ApiError, ApiResult, OrInternal};
use crate::metadata::{MetadataDb, UserRecord};
use crate::models::DemoLoginRequest;
use axum::{
    extract::{ConnectInfo, Extension, State},
    http::{HeaderMap, StatusCode},
    response::{Json, Response},
};
use std::{net::SocketAddr, sync::Arc, time::Instant};
use tracing::info;

const DEMO_EMAIL: &str = "demo@canarybitcoin.com";
const MAX_ADMIN_MFA_ATTEMPTS: i64 = 5;
const ADMIN_MFA_RATE_LIMIT_WINDOW_MINUTES: i64 = 5;

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
        config.operating_mode,
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
        config.operating_mode,
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
