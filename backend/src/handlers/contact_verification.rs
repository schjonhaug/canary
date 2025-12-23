//! Contact verification handlers

use crate::api::AppServicesState;
use crate::auth::{load_twilio_config_from_env, AuthService};
use crate::config::AppConfig;
use crate::extractors::AuthenticatedUser;
use crate::models::{
    validate_phone_number, ErrorResponse, SendContactVerificationRequest, VerifyContactRequest,
    VerifyContactResponse,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use std::sync::Arc;
use tracing::info;

/// Send a verification code to a contact
pub async fn send_contact_verification(
    AuthenticatedUser(user): AuthenticatedUser,
    Path(wallet_checksum): Path<String>,
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    Json(request): Json<SendContactVerificationRequest>,
) -> Response {
    let _ = config; // Used for state extraction pattern consistency
    let start_time = std::time::Instant::now();

    // Check if wallet exists and user has access - no mutex blocking!
    match app_services
        .metadata_db
        .get_wallet_by_checksum(&wallet_checksum)
        .await
    {
        Ok(Some(_)) => {
            if !user.is_admin {
                match app_services
                    .metadata_db
                    .is_wallet_owned_by_user(&wallet_checksum, &user.user_id)
                    .await
                {
                    Ok(true) => {} // User owns the wallet
                    Ok(false) => {
                        return (
                            StatusCode::FORBIDDEN,
                            Json(ErrorResponse {
                                error: "Access denied".to_string(),
                            }),
                        )
                            .into_response();
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Database error: {}", e),
                            }),
                        )
                            .into_response();
                    }
                }
            }
        }
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Wallet not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
                .into_response();
        }
    }

    // Determine verification type and validate input
    let (provider_type, notification_target, is_dev_mode) = if let Some(phone_number) =
        &request.phone_number
    {
        // SMS verification
        let normalized_phone = match validate_phone_number(phone_number) {
            Ok(phone) => phone,
            Err(e) => {
                return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e })).into_response();
            }
        };

        // Check for duplicate phone number in this wallet BEFORE any verification
        match app_services
            .metadata_db
            .check_duplicate_notification_target(&wallet_checksum, "sms", &normalized_phone, None)
            .await
        {
            Ok(Some(existing_contact_name)) => {
                return (
                    StatusCode::CONFLICT,
                    Json(ErrorResponse {
                        error: format!(
                            "Phone number '{}' is already used by contact '{}' in this wallet",
                            normalized_phone, existing_contact_name
                        ),
                    }),
                )
                    .into_response();
            }
            Ok(None) => {} // No duplicate found, continue
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Failed to check for duplicate phone number: {}", e),
                    }),
                )
                    .into_response();
            }
        }

        let is_dev_phone = cfg!(debug_assertions)
            && ["+4799999901", "+4699999902", "+3399999903"].contains(&normalized_phone.as_str());

        ("sms", normalized_phone, is_dev_phone)
    } else if let Some(email_address) = &request.email_address {
        // Email verification
        let email = email_address.trim().to_lowercase();

        // Basic email validation
        if !email.contains('@') || email.len() < 5 {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Invalid email address format".to_string(),
                }),
            )
                .into_response();
        }

        // Check for duplicate email in this wallet BEFORE any verification
        match app_services
            .metadata_db
            .check_duplicate_notification_target(&wallet_checksum, "email", &email, None)
            .await
        {
            Ok(Some(existing_contact_name)) => {
                return (
                    StatusCode::CONFLICT,
                    Json(ErrorResponse {
                        error: format!(
                            "Email '{}' is already used by contact '{}' in this wallet",
                            email, existing_contact_name
                        ),
                    }),
                )
                    .into_response();
            }
            Ok(None) => {} // No duplicate found, continue
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Failed to check for duplicate email: {}", e),
                    }),
                )
                    .into_response();
            }
        }

        // Check if email matches current user's account email (skip verification) - no mutex blocking!
        if let Ok(Some(user_record)) = app_services.metadata_db.get_user_by_id(&user.user_id).await
        {
            let jwt_secret = std::env::var("JWT_SECRET")
                .unwrap_or_else(|_| "your-secret-key-change-in-production".to_string());
            let auth_service = AuthService::new(jwt_secret.clone(), None);

            if auth_service.should_skip_email_verification(&email, &user_record.email) {
                // Auto-approve for user's own email, but still create verification record
                match app_services
                    .metadata_db
                    .create_pending_contact_verification(
                        &wallet_checksum,
                        "email",
                        &email,
                        &request.name,
                        &request.language,
                        None, // No code needed for auto-verification
                    )
                    .await
                {
                    Ok(verification_id) => {
                        // Mark as verified immediately
                        if let Err(e) = app_services
                            .metadata_db
                            .mark_verification_completed(verification_id)
                            .await
                        {
                            return (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(ErrorResponse {
                                    error: format!("Failed to complete auto-verification: {}", e),
                                }),
                            )
                                .into_response();
                        }

                        return Json(serde_json::json!({
                            "message": "Email verified automatically for user accounts",
                            "auto_verified": true
                        }))
                        .into_response();
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Failed to create verification record: {}", e),
                            }),
                        )
                            .into_response();
                    }
                }
            }
        }

        let is_dev_email = cfg!(debug_assertions)
            && ["test@example.com", "dev@canary.local"].contains(&email.as_str());

        ("email", email, is_dev_email)
    } else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Either phone_number or email_address must be provided".to_string(),
            }),
        )
            .into_response();
    };

    // Check rate limit (skip for dev mode) - no mutex blocking!
    if !is_dev_mode {
        match app_services
            .metadata_db
            .check_rate_limit(&notification_target)
            .await
        {
            Ok(true) => {} // Allowed
            Ok(false) => {
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(ErrorResponse {
                        error: "Too many verification attempts. Please try again later."
                            .to_string(),
                    }),
                )
                    .into_response();
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Failed to check rate limit: {}", e),
                    }),
                )
                    .into_response();
            }
        }
    }

    // Clean up any expired verifications first - no mutex blocking!
    let _ = app_services
        .metadata_db
        .cleanup_expired_verifications()
        .await;

    // Generate verification code
    let verification_code = if is_dev_mode {
        "123456".to_string()
    } else if provider_type == "email" {
        use crate::email_service::EmailService;
        EmailService::generate_otp_code()
    } else {
        // SMS uses Twilio Verify which generates its own codes
        "".to_string() // Will be None in database
    };

    // Store pending verification
    let stored_code = if verification_code.is_empty() {
        None
    } else {
        Some(verification_code.as_str())
    };

    match app_services
        .metadata_db
        .create_pending_contact_verification(
            &wallet_checksum,
            provider_type,
            &notification_target,
            &request.name,
            &request.language,
            stored_code,
        )
        .await
    {
        Ok(_) => {}
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to store verification: {}", e),
                }),
            )
                .into_response();
        }
    }

    // Send verification code
    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "your-secret-key-change-in-production".to_string());

    let result = if provider_type == "sms" {
        // SMS verification via Twilio
        let twilio_config = match load_twilio_config_from_env() {
            Ok(config) => config,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Twilio configuration error: {}", e),
                    }),
                )
                    .into_response();
            }
        };

        if !is_dev_mode && twilio_config.verify_service_sid.is_none() {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Twilio Verify service not configured".to_string(),
                }),
            )
                .into_response();
        }

        let auth_service = AuthService::new(jwt_secret, None);
        auth_service
            .send_contact_otp(&twilio_config, &notification_target)
            .await
    } else {
        // Email verification via Resend
        use crate::email_service::EmailService;
        let email_service = match EmailService::from_env() {
            Ok(service) => service,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Email service configuration error: {}", e),
                    }),
                )
                    .into_response();
            }
        };

        let auth_service = AuthService::new(jwt_secret, Some(email_service));
        auth_service
            .send_email_contact_otp(
                &notification_target,
                &request.name,
                &verification_code,
                &request.language,
            )
            .await
    };

    let final_result = match result {
        Ok(_) => Json(serde_json::json!({
            "message": "Verification code sent successfully"
        }))
        .into_response(),
        Err(e) => {
            // Clean up pending verification on error - no mutex blocking!
            let _ = app_services
                .metadata_db
                .cleanup_expired_verifications()
                .await;

            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Failed to send verification: {}", e),
                }),
            )
                .into_response()
        }
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
    Json(request): Json<VerifyContactRequest>,
) -> Response {
    let start_time = std::time::Instant::now();

    // Check if wallet exists and user has access - no mutex blocking!
    match app_services
        .metadata_db
        .get_wallet_by_checksum(&wallet_checksum)
        .await
    {
        Ok(Some(_)) => {
            if !user.is_admin {
                match app_services
                    .metadata_db
                    .is_wallet_owned_by_user(&wallet_checksum, &user.user_id)
                    .await
                {
                    Ok(true) => {} // User owns the wallet
                    Ok(false) => {
                        return (
                            StatusCode::FORBIDDEN,
                            Json(ErrorResponse {
                                error: "Access denied".to_string(),
                            }),
                        )
                            .into_response();
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Database error: {}", e),
                            }),
                        )
                            .into_response();
                    }
                }
            }
        }
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Wallet not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
                .into_response();
        }
    }

    // Determine what we're verifying and validate input
    let (provider_type, notification_target) = if let Some(phone_number) = &request.phone_number {
        // SMS verification
        let normalized_phone = match validate_phone_number(phone_number) {
            Ok(phone) => phone,
            Err(e) => {
                return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e })).into_response();
            }
        };
        ("sms", normalized_phone)
    } else if let Some(email_address) = &request.email_address {
        // Email verification
        let email = email_address.trim().to_lowercase();

        // Basic email validation
        if !email.contains('@') || email.len() < 5 {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Invalid email address format".to_string(),
                }),
            )
                .into_response();
        }
        ("email", email)
    } else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Either phone_number or email_address must be provided".to_string(),
            }),
        )
            .into_response();
    };

    // Look up the pending verification - no mutex blocking!
    let (verification_id, _contact_name, _language, verification_code) = match app_services
        .metadata_db
        .get_pending_verification(&wallet_checksum, &notification_target)
        .await
    {
        Ok(Some(verification)) => verification,
        Ok(None) => {
            let error_msg = if provider_type == "email" {
                "No pending verification found for this email address"
            } else {
                "No pending verification found for this phone number"
            };
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: error_msg.to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Database error: {}", e),
                }),
            )
                .into_response();
        }
    };

    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "your-secret-key-change-in-production".to_string());

    // Verify the code based on provider type
    let is_valid = if provider_type == "email" {
        // Email verification - direct code comparison
        if let Some(stored_code) = verification_code {
            let auth_service = AuthService::new(jwt_secret, None);
            auth_service.verify_email_contact_otp(&stored_code, &request.code)
        } else {
            false // No stored code means invalid
        }
    } else {
        // SMS verification
        if let Some(stored_code) = verification_code {
            // Dev mode - direct comparison
            stored_code == request.code
        } else {
            // Production mode - verify with Twilio
            let twilio_config = match load_twilio_config_from_env() {
                Ok(config) => config,
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: format!("Twilio configuration error: {}", e),
                        }),
                    )
                        .into_response();
                }
            };

            let auth_service = AuthService::new(jwt_secret, None);
            match auth_service
                .verify_contact_otp(&twilio_config, &notification_target, &request.code)
                .await
            {
                Ok(valid) => valid,
                Err(e) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            error: format!("Failed to verify code: {}", e),
                        }),
                    )
                        .into_response();
                }
            }
        }
    };

    if is_valid {
        // Clear rate limit on successful verification - no mutex blocking!
        let _ = app_services
            .metadata_db
            .clear_rate_limit(&notification_target)
            .await;

        // Mark verification as completed - no mutex blocking!
        let _ = app_services
            .metadata_db
            .mark_verification_completed(verification_id)
            .await;

        let success_message = if provider_type == "email" {
            "Email address verified successfully"
        } else {
            "Phone number verified successfully"
        };

        let elapsed = start_time.elapsed();
        info!("verify_contact completed in {:?}", elapsed);

        (
            StatusCode::OK,
            Json(VerifyContactResponse {
                valid: true,
                message: success_message.to_string(),
            }),
        )
            .into_response()
    } else {
        let elapsed = start_time.elapsed();
        info!("verify_contact completed in {:?}", elapsed);

        (
            StatusCode::BAD_REQUEST,
            Json(VerifyContactResponse {
                valid: false,
                message: "Invalid verification code".to_string(),
            }),
        )
            .into_response()
    }
}
