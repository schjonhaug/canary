//! Contact verification handlers

use super::contact::{is_verified_for_user, normalize_email, normalize_phone};
use crate::api::AppServicesState;
use crate::auth::{load_twilio_config_from_env, AuthService};
use crate::config::AppConfig;
use crate::email_service::EmailService;
use crate::extractors::{require_non_demo, AuthenticatedUser};
use crate::handlers::error::{ApiError, ApiResult, OrInternal};
use crate::handlers::helpers::verify_wallet_access;
use crate::metadata::Language;
use crate::models::{SendContactVerificationRequest, VerifyContactRequest, VerifyContactResponse};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::info;

/// Where a verification code should go, or why none is needed.
enum VerificationTarget {
    /// The target is already trusted; carries the success message.
    AutoVerified(&'static str),
    /// A code must be sent to the normalized target.
    Pending { target: String, is_dev_mode: bool },
}

fn is_dev_phone(phone: &str) -> bool {
    cfg!(debug_assertions) && ["+4799999901", "+4699999902", "+3399999903"].contains(&phone)
}

fn is_dev_email(email: &str) -> bool {
    cfg!(debug_assertions) && ["test@example.com", "dev@canary.local"].contains(&email)
}

fn jwt_secret(config: &AppConfig) -> ApiResult<String> {
    config
        .get_jwt_secret()
        .map(str::to_string)
        .map_err(|e| ApiError::uncoded(StatusCode::SERVICE_UNAVAILABLE, e))
}

fn verification_input_required() -> ApiError {
    ApiError::bad_request(
        "verification_input_required",
        "Either phone_number or email_address must be provided",
    )
}

fn auto_verified(message: &'static str) -> Json<Value> {
    Json(json!({
        "message": message,
        "auto_verified": true
    }))
}

/// Records a verification for `target` that needs no code.
async fn create_auto_verification(
    app_services: &AppServicesState,
    wallet_checksum: &str,
    provider_type: &str,
    target: &str,
    contact_name: &str,
) -> ApiResult<i64> {
    app_services
        .metadata_db
        .create_pending_contact_verification(
            wallet_checksum,
            provider_type,
            target,
            contact_name,
            None,
        )
        .await
        .or_internal("Failed to create verification record")
}

/// Auto-verifies `target` when the user already verified it for another wallet.
async fn auto_verify_cross_wallet(
    app_services: &AppServicesState,
    user_id: &str,
    wallet_checksum: &str,
    provider_type: &str,
    target: &str,
    contact_name: &str,
) -> ApiResult<bool> {
    if !is_verified_for_user(app_services, user_id, provider_type, target).await {
        return Ok(false);
    }
    let verification_id = create_auto_verification(
        app_services,
        wallet_checksum,
        provider_type,
        target,
        contact_name,
    )
    .await?;
    if let Err(e) = app_services
        .metadata_db
        .mark_verification_completed(verification_id)
        .await
    {
        tracing::error!(
            error = %e,
            provider_type,
            "Failed to mark cross-wallet verification as completed"
        );
    }
    Ok(true)
}

async fn sms_verification_target(
    app_services: &AppServicesState,
    user_id: &str,
    wallet_checksum: &str,
    phone_number: &str,
    contact_name: &str,
) -> ApiResult<VerificationTarget> {
    let normalized_phone = normalize_phone(phone_number)?;

    // Check for duplicate phone number in this wallet BEFORE any verification
    if let Some(existing_contact_name) = app_services
        .metadata_db
        .check_duplicate_notification_target(wallet_checksum, "sms", &normalized_phone, None)
        .await
        .or_internal("Failed to check for duplicate phone number")?
    {
        return Err(ApiError::conflict(
            "duplicate_phone_number",
            format!(
                "Phone number '{normalized_phone}' is already used by contact '{existing_contact_name}' in this wallet"
            ),
        ));
    }

    if auto_verify_cross_wallet(
        app_services,
        user_id,
        wallet_checksum,
        "sms",
        &normalized_phone,
        contact_name,
    )
    .await?
    {
        return Ok(VerificationTarget::AutoVerified(
            "Phone number verified automatically (already verified for another wallet)",
        ));
    }

    let is_dev_mode = is_dev_phone(&normalized_phone);
    Ok(VerificationTarget::Pending {
        target: normalized_phone,
        is_dev_mode,
    })
}

async fn email_verification_target(
    app_services: &AppServicesState,
    user_id: &str,
    wallet_checksum: &str,
    email_address: &str,
    contact_name: &str,
) -> ApiResult<VerificationTarget> {
    let email = normalize_email(email_address)?;

    // Check for duplicate email in this wallet BEFORE any verification
    if let Some(existing_contact_name) = app_services
        .metadata_db
        .check_duplicate_notification_target(wallet_checksum, "email", &email, None)
        .await
        .or_internal("Failed to check for duplicate email")?
    {
        return Err(ApiError::conflict(
            "duplicate_email_address",
            format!(
                "Email '{email}' is already used by contact '{existing_contact_name}' in this wallet"
            ),
        ));
    }

    if auto_verify_cross_wallet(
        app_services,
        user_id,
        wallet_checksum,
        "email",
        &email,
        contact_name,
    )
    .await?
    {
        return Ok(VerificationTarget::AutoVerified(
            "Email verified automatically (already verified for another wallet)",
        ));
    }

    // The user's own account email is already verified, but still gets a record
    if let Ok(Some(user_record)) = app_services.metadata_db.get_user_by_id(user_id).await {
        if AuthService::should_skip_email_verification(&email, &user_record.email) {
            let verification_id = create_auto_verification(
                app_services,
                wallet_checksum,
                "email",
                &email,
                contact_name,
            )
            .await?;
            app_services
                .metadata_db
                .mark_verification_completed(verification_id)
                .await
                .or_internal("Failed to complete auto-verification")?;
            return Ok(VerificationTarget::AutoVerified(
                "Email verified automatically for user accounts",
            ));
        }
    }

    let is_dev_mode = is_dev_email(&email);
    Ok(VerificationTarget::Pending {
        target: email,
        is_dev_mode,
    })
}

/// Sends the verification code through Twilio Verify or email.
///
/// The outer error is a missing provider configuration. The inner result is
/// the delivery itself, which the caller reports as a client error after
/// cleaning up.
async fn send_verification_code(
    provider_type: &str,
    jwt_secret: String,
    target: &str,
    contact_name: &str,
    verification_code: &str,
    is_dev_mode: bool,
    user_language: Language,
) -> ApiResult<anyhow::Result<()>> {
    if provider_type == "sms" {
        let twilio_config =
            load_twilio_config_from_env().or_internal("Twilio configuration error")?;
        if !is_dev_mode && twilio_config.verify_service_sid.is_none() {
            return Err(ApiError::internal(
                "Twilio Verify service not configured",
                anyhow::anyhow!("TWILIO_VERIFY_SERVICE_SID is not set"),
            ));
        }

        let twilio_locale = user_language.twilio_locale();
        info!(
            "Sending SMS OTP to {} with locale {}",
            target, twilio_locale
        );

        let auth_service = AuthService::new(jwt_secret, None);
        Ok(auth_service
            .send_contact_otp(&twilio_config, target, twilio_locale)
            .await)
    } else {
        let email_service =
            EmailService::from_env().or_internal("Email service configuration error")?;
        let auth_service = AuthService::new(jwt_secret, Some(email_service));
        Ok(auth_service
            .send_email_contact_otp(
                target,
                contact_name,
                verification_code,
                user_language.as_str(),
            )
            .await)
    }
}

/// Send a verification code to a contact
pub async fn send_contact_verification(
    AuthenticatedUser(user): AuthenticatedUser,
    Path(wallet_checksum): Path<String>,
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    Json(request): Json<SendContactVerificationRequest>,
) -> ApiResult<Json<Value>> {
    let start_time = std::time::Instant::now();

    require_non_demo(&user)?;

    // Get user's preferred language for verification emails
    let user_language = app_services
        .metadata_db
        .get_user_preferred_language(&user.user_id)
        .await
        .unwrap_or(Language::English);

    verify_wallet_access(
        &app_services,
        &user,
        &wallet_checksum,
        "Internal server error",
    )
    .await?;

    let (provider_type, verification_target) = if let Some(phone_number) = &request.phone_number {
        let target = sms_verification_target(
            &app_services,
            &user.user_id,
            &wallet_checksum,
            phone_number,
            &request.name,
        )
        .await?;
        ("sms", target)
    } else if let Some(email_address) = &request.email_address {
        let target = email_verification_target(
            &app_services,
            &user.user_id,
            &wallet_checksum,
            email_address,
            &request.name,
        )
        .await?;
        ("email", target)
    } else {
        return Err(verification_input_required());
    };
    let (notification_target, is_dev_mode) = match verification_target {
        VerificationTarget::AutoVerified(message) => return Ok(auto_verified(message)),
        VerificationTarget::Pending {
            target,
            is_dev_mode,
        } => (target, is_dev_mode),
    };

    // Check rate limit (skip for dev mode)
    if !is_dev_mode
        && !app_services
            .metadata_db
            .check_rate_limit(&notification_target)
            .await
            .or_internal("Failed to check rate limit")?
    {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "verification_rate_limit",
            "Too many verification attempts. Please try again later.",
        ));
    }

    // Clean up any expired verifications first
    let _ = app_services
        .metadata_db
        .cleanup_expired_verifications()
        .await;

    // Generate verification code
    let verification_code = if is_dev_mode {
        "123456".to_string()
    } else if provider_type == "email" {
        EmailService::generate_otp_code()
    } else {
        // SMS uses Twilio Verify which generates its own codes
        String::new() // Will be None in database
    };

    let jwt_secret = jwt_secret(&config)?;

    // Store pending verification
    let stored_code = if verification_code.is_empty() {
        None
    } else if provider_type == "email" {
        Some(AuthService::new(jwt_secret.clone(), None).hash_email_contact_otp(&verification_code))
    } else {
        Some(verification_code.clone())
    };

    app_services
        .metadata_db
        .create_pending_contact_verification(
            &wallet_checksum,
            provider_type,
            &notification_target,
            &request.name,
            stored_code.as_deref(),
        )
        .await
        .or_internal("Failed to store verification")?;

    let send_result = send_verification_code(
        provider_type,
        jwt_secret,
        &notification_target,
        &request.name,
        &verification_code,
        is_dev_mode,
        user_language,
    )
    .await?;

    let final_result = if send_result.is_ok() {
        Ok(Json(json!({
            "message": "Verification code sent successfully"
        })))
    } else {
        // Clean up pending verification on error
        let _ = app_services
            .metadata_db
            .cleanup_expired_verifications()
            .await;

        // Provider errors can echo the phone number or address.
        Err(ApiError::uncoded(
            StatusCode::BAD_REQUEST,
            "Failed to send verification",
        ))
    };

    let elapsed = start_time.elapsed();
    info!("send_contact_verification completed in {:?}", elapsed);
    final_result
}

/// Verify a contact's verification code
pub async fn verify_contact(
    AuthenticatedUser(user): AuthenticatedUser,
    Path(wallet_checksum): Path<String>,
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    Json(request): Json<VerifyContactRequest>,
) -> ApiResult<(StatusCode, Json<VerifyContactResponse>)> {
    let start_time = std::time::Instant::now();

    require_non_demo(&user)?;
    verify_wallet_access(
        &app_services,
        &user,
        &wallet_checksum,
        "Internal server error",
    )
    .await?;

    // Determine what we're verifying and validate input
    let (provider_type, notification_target, is_dev_mode) =
        if let Some(phone_number) = &request.phone_number {
            let normalized_phone = normalize_phone(phone_number)?;
            let is_dev_mode = is_dev_phone(&normalized_phone);
            ("sms", normalized_phone, is_dev_mode)
        } else if let Some(email_address) = &request.email_address {
            let email = normalize_email(email_address)?;
            let is_dev_mode = is_dev_email(&email);
            ("email", email, is_dev_mode)
        } else {
            return Err(verification_input_required());
        };

    // Check verification rate limit (skip for dev mode).
    // Fail closed: reject verification attempts if the rate limit check fails.
    if !is_dev_mode {
        let allowed = app_services
            .metadata_db
            .check_verification_rate_limit(&notification_target)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to check verification rate limit");
                ApiError::uncoded(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Unable to verify at this time. Please try again later.",
                )
            })?;
        if !allowed {
            return Err(ApiError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "verification_rate_limit",
                "Too many failed verification attempts. Please try again later.",
            ));
        }
    }

    // Look up the pending verification
    let (verification_id, _contact_name, verification_code) = app_services
        .metadata_db
        .get_pending_verification(&wallet_checksum, &notification_target)
        .await
        .or_internal("Database error")?
        .ok_or_else(|| {
            let message = if provider_type == "email" {
                "No pending verification found for this email address"
            } else {
                "No pending verification found for this phone number"
            };
            ApiError::bad_request("no_pending_verification", message)
        })?;

    let jwt_secret = jwt_secret(&config)?;

    // Verify the code based on provider type
    let is_valid = match (provider_type, verification_code) {
        // Email verification - HMAC-SHA256 code comparison
        ("email", Some(stored_code)) => {
            AuthService::new(jwt_secret, None).verify_email_contact_otp(&stored_code, &request.code)
        }
        // No stored email code means invalid
        ("email", None) => false,
        // Dev mode SMS - direct comparison
        (_, Some(stored_code)) => stored_code == request.code,
        // Production SMS - verify with Twilio
        (_, None) => {
            let twilio_config =
                load_twilio_config_from_env().or_internal("Twilio configuration error")?;
            AuthService::new(jwt_secret, None)
                .verify_contact_otp(&twilio_config, &notification_target, &request.code)
                .await
                .map_err(|_| ApiError::uncoded(StatusCode::BAD_REQUEST, "Failed to verify code"))?
        }
    };

    if is_valid {
        // Clear OTP sending rate limit on successful verification
        let _ = app_services
            .metadata_db
            .clear_rate_limit(&notification_target)
            .await;

        // Clear verification brute-force rate limit on successful verification
        let _ = app_services
            .metadata_db
            .clear_verification_rate_limit(&notification_target)
            .await;

        // Mark verification as completed
        let _ = app_services
            .metadata_db
            .mark_verification_completed(verification_id)
            .await;
    } else if !is_dev_mode {
        // Record failed verification attempt for brute-force protection
        let _ = app_services
            .metadata_db
            .record_failed_verification_attempt(&notification_target)
            .await;
    }

    let elapsed = start_time.elapsed();
    info!("verify_contact completed in {:?}", elapsed);

    let (status, message) = match (is_valid, provider_type) {
        (true, "email") => (StatusCode::OK, "Email address verified successfully"),
        (true, _) => (StatusCode::OK, "Phone number verified successfully"),
        (false, _) => (StatusCode::BAD_REQUEST, "Invalid verification code"),
    };
    Ok((
        status,
        Json(VerifyContactResponse {
            valid: is_valid,
            message: message.to_string(),
        }),
    ))
}
