//! Sign-up: account creation, the Stripe customer and marketing audience
//! that come with it, and email verification.

use super::rate_limit::{
    AUTH_IP_RATE_LIMIT_WINDOW_MINUTES, MAX_AUTH_REQUESTS_PER_IP, REGISTRATION_EMAIL_RATE_LIMIT,
    client_ip, enforce_email_rate_limit, enforce_ip_rate_limit, pad_enumeration_response,
};
use super::{message_json, shared_auth_service, validate_password_length};
use crate::admin_notifications::AdminNotifications;
use crate::api::{AppServicesState, EmailServiceState, SharedServices, StripeBillingState};
use crate::auth::{AuthService, RegisterRequest};
use crate::config::AppConfig;
use crate::exchange_rates;
use crate::handlers::error::{ApiError, ApiResult, OrInternal};
use crate::metadata::UserRecord;
use axum::{
    extract::{ConnectInfo, Extension, Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use std::{net::SocketAddr, sync::Arc, time::Instant};
use tracing::info;

const REGISTRATION_SUCCESS_MESSAGE: &str =
    "Registration successful. Please check your email to verify your account.";

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
