//! Contact management handlers

use crate::api::AppServicesState;
use crate::config::AppConfig;
use crate::extractors::{require_non_demo, AuthenticatedUser};
use crate::metadata::ProviderType;
use crate::models::{
    validate_phone_number, CreateContactResponse, CreateContactWithMethodsRequest, ErrorResponse,
    NotificationMethodRequest, UpdateContactRequest,
};
use crate::stripe_billing::StripeBilling;
use crate::subscription::check_limit;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use std::sync::Arc;
use tracing::info;

/// Validates an ntfy topic (1-64 chars, alphanumeric and dashes)
fn validate_ntfy_topic(topic: &str) -> Result<String, String> {
    let topic = topic.trim();
    if topic.is_empty() {
        return Err("ntfy topic cannot be empty".to_string());
    }
    if topic.len() > 64 {
        return Err("ntfy topic must be at most 64 characters".to_string());
    }
    // Allow alphanumeric, dashes, and underscores
    if !topic
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(
            "ntfy topic can only contain letters, numbers, dashes, and underscores".to_string(),
        );
    }
    Ok(topic.to_string())
}

/// Type alias for Stripe billing state
pub type StripeBillingState = Option<Arc<StripeBilling>>;

/// Create a new contact for a wallet
pub async fn create_wallet_contact(
    AuthenticatedUser(user): AuthenticatedUser,
    Path(wallet_checksum): Path<String>,
    State(app_services): State<AppServicesState>,
    State(stripe_billing): State<StripeBillingState>,
    State(config): State<Arc<AppConfig>>,
    Json(payload): Json<CreateContactWithMethodsRequest>,
) -> Response {
    let _ = stripe_billing; // Used for state extraction pattern consistency
    let start_time = std::time::Instant::now();

    // Reject demo users from creating contacts
    if let Err(response) = require_non_demo(&user) {
        return response;
    }

    // Direct metadata access - no mutex blocking!
    let _wallet = match app_services
        .metadata_db
        .get_wallet_by_checksum(&wallet_checksum)
        .await
    {
        Ok(Some(wallet)) => {
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
            wallet
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
    };

    // Get user's subscription tier and check contact limit
    let user_record = match app_services.metadata_db.get_user_by_id(&user.user_id).await {
        Ok(Some(record)) => record,
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
                    error: format!("Failed to get user information: {}", e),
                }),
            )
                .into_response();
        }
    };

    // Count existing contacts for the wallet and check limit unless limits are bypassed
    let bypass_limits = config.is_self_hosted_mode() || user_record.is_admin;
    if !bypass_limits {
        match app_services
            .metadata_db
            .count_contacts_for_wallet(&wallet_checksum)
            .await
        {
            Ok(contact_count) => {
                let tier_limits = user_record.subscription_tier.limits_for_api();
                if let Err(limit_err) = check_limit(
                    contact_count,
                    tier_limits.max_contacts_per_wallet,
                    "Contact",
                ) {
                    return (
                        StatusCode::FORBIDDEN,
                        Json(ErrorResponse {
                            error: limit_err.to_string(),
                        }),
                    )
                        .into_response();
                }
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Failed to check contact limit: {}", e),
                    }),
                )
                    .into_response();
            }
        }
    }

    // Process notification methods
    let mut processed_methods = Vec::new();

    for method in &payload.notification_methods {
        match method.provider_type {
            ProviderType::Sms => {
                // Validate and normalize the phone number
                let normalized_phone = match validate_phone_number(&method.notification_target) {
                    Ok(phone) => phone,
                    Err(e) => {
                        return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e }))
                            .into_response();
                    }
                };

                // Check if this phone number is already verified for this user (cross-wallet)
                if let Ok(true) = app_services
                    .metadata_db
                    .is_notification_target_verified_for_user(
                        &user.user_id,
                        "sms",
                        &normalized_phone,
                    )
                    .await
                {
                    // Phone already verified for another wallet, skip OTP
                    processed_methods.push((ProviderType::Sms, normalized_phone));
                    continue;
                }

                // SECURITY: Check if this phone number was recently verified for THIS wallet
                match app_services
                    .metadata_db
                    .was_recently_verified(&wallet_checksum, &normalized_phone)
                    .await
                {
                    Ok(true) => {
                        // Phone was verified, safe to add
                        processed_methods.push((ProviderType::Sms, normalized_phone));
                    }
                    Ok(false) => {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse {
                                error: "Phone number must be verified before adding contact"
                                    .to_string(),
                            }),
                        )
                            .into_response();
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Failed to check phone verification: {}", e),
                            }),
                        )
                            .into_response();
                    }
                }
            }
            ProviderType::Ntfy => {
                // Push notifications are always allowed (ntfy is free)
                // Use user-provided topic from request
                match validate_ntfy_topic(&method.notification_target) {
                    Ok(topic) => processed_methods.push((ProviderType::Ntfy, topic)),
                    Err(e) => {
                        return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e }))
                            .into_response();
                    }
                }
            }
            ProviderType::Email => {
                // Basic email validation
                let email = method.notification_target.trim().to_lowercase();
                if !email.contains('@') || email.len() < 5 {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            error: "Invalid email address format".to_string(),
                        }),
                    )
                        .into_response();
                }

                // Check if this email is already verified for this user (cross-wallet)
                if let Ok(true) = app_services
                    .metadata_db
                    .is_notification_target_verified_for_user(&user.user_id, "email", &email)
                    .await
                {
                    // Email already verified for another wallet, skip OTP
                    processed_methods.push((ProviderType::Email, email));
                    continue;
                }

                // SECURITY: Check if this email address was recently verified for THIS wallet
                match app_services
                    .metadata_db
                    .was_recently_verified(&wallet_checksum, &email)
                    .await
                {
                    Ok(true) => {
                        // Email was verified, safe to add
                        processed_methods.push((ProviderType::Email, email));
                    }
                    Ok(false) => {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse {
                                error: "Email address must be verified before adding contact"
                                    .to_string(),
                            }),
                        )
                            .into_response();
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Failed to check email verification: {}", e),
                            }),
                        )
                            .into_response();
                    }
                }
            }
        }
    }

    // Ensure at least one method was provided
    if processed_methods.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "At least one notification method must be provided".to_string(),
            }),
        )
            .into_response();
    }

    // Check for duplicate notification targets (email/phone) within the same wallet
    let methods_for_validation: Vec<(String, String)> = processed_methods
        .iter()
        .map(|(provider, target)| (provider.as_str().to_string(), target.clone()))
        .collect();

    match app_services
        .metadata_db
        .check_duplicate_notification_targets(&wallet_checksum, &methods_for_validation, None)
        .await
    {
        Ok(duplicates) => {
            if !duplicates.is_empty() {
                return (
                    StatusCode::CONFLICT,
                    Json(ErrorResponse {
                        error: format!("Duplicate notification targets: {}", duplicates.join(", ")),
                    }),
                )
                    .into_response();
            }
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to check for duplicates: {}", e),
                }),
            )
                .into_response();
        }
    }

    let create_result = app_services
        .metadata_db
        .insert_contact_with_notification_methods(
            &wallet_checksum,
            &payload.name,
            processed_methods,
        )
        .await;

    let elapsed = start_time.elapsed();
    info!("create_wallet_contact completed in {:?}", elapsed);

    match create_result {
        Ok(contact_id) => (
            StatusCode::CREATED,
            Json(CreateContactResponse {
                message: "Contact created successfully".to_string(),
                contact_id,
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

/// Delete a contact from a wallet
pub async fn delete_wallet_contact(
    AuthenticatedUser(user): AuthenticatedUser,
    Path((wallet_checksum, contact_id)): Path<(String, String)>,
    State(app_services): State<AppServicesState>,
) -> Response {
    let start_time = std::time::Instant::now();

    // Reject demo users from deleting contacts
    if let Err(response) = require_non_demo(&user) {
        return response;
    }

    // Direct metadata access - no mutex blocking!
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
                    error: format!("Database error: {}", e),
                }),
            )
                .into_response();
        }
    }

    let delete_result = app_services
        .metadata_db
        .delete_wallet_contact(&wallet_checksum, &contact_id)
        .await;

    let elapsed = start_time.elapsed();
    info!("delete_wallet_contact completed in {:?}", elapsed);

    match delete_result {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Contact not found".to_string(),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

/// Update a contact for a wallet
pub async fn update_wallet_contact(
    AuthenticatedUser(user): AuthenticatedUser,
    Path((wallet_checksum, contact_id)): Path<(String, String)>,
    State(app_services): State<AppServicesState>,
    Json(payload): Json<UpdateContactRequest>,
) -> Response {
    let start_time = std::time::Instant::now();

    // Reject demo users from updating contacts
    if let Err(response) = require_non_demo(&user) {
        return response;
    }

    // Check if wallet exists and user has access
    let _wallet = match app_services
        .metadata_db
        .get_wallet_by_checksum(&wallet_checksum)
        .await
    {
        Ok(Some(wallet)) => {
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
            wallet
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
    };

    // Get existing contact to compare notification methods
    let existing_contact = match app_services
        .metadata_db
        .get_single_contact_with_methods(&contact_id, &wallet_checksum)
        .await
    {
        Ok(Some(contact)) => contact,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Contact not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to get existing contact: {}", e),
                }),
            )
                .into_response();
        }
    };

    // Helper function to check if a notification method has changed
    let has_method_changed = |new_method: &NotificationMethodRequest| -> bool {
        let existing_method = existing_contact
            .notification_methods
            .iter()
            .find(|m| m.provider_type == new_method.provider_type);

        match existing_method {
            Some(existing) => {
                // For SMS, normalize both numbers for comparison
                if new_method.provider_type == ProviderType::Sms {
                    let new_normalized = validate_phone_number(&new_method.notification_target)
                        .unwrap_or_else(|_| new_method.notification_target.clone());
                    existing.notification_target != new_normalized
                } else {
                    // For email and ntfy, compare normalized strings
                    let new_normalized = if new_method.provider_type == ProviderType::Email {
                        new_method.notification_target.trim().to_lowercase()
                    } else {
                        new_method.notification_target.clone()
                    };
                    existing.notification_target != new_normalized
                }
            }
            None => true, // Method doesn't exist, so it's new
        }
    };

    // Process notification methods with security checks
    let mut processed_methods = Vec::new();

    for method in &payload.notification_methods {
        match method.provider_type {
            ProviderType::Sms => {
                // Validate and normalize the phone number
                let normalized_phone = match validate_phone_number(&method.notification_target) {
                    Ok(phone) => phone,
                    Err(e) => {
                        return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e }))
                            .into_response();
                    }
                };

                // SECURITY: Only verify if the phone number has changed
                if has_method_changed(method) {
                    // Check cross-wallet verification first
                    if let Ok(true) = app_services
                        .metadata_db
                        .is_notification_target_verified_for_user(
                            &user.user_id,
                            "sms",
                            &normalized_phone,
                        )
                        .await
                    {
                        processed_methods.push((ProviderType::Sms, normalized_phone));
                        continue;
                    }

                    // Check if this phone number was recently verified for THIS wallet
                    match app_services
                        .metadata_db
                        .was_recently_verified(&wallet_checksum, &normalized_phone)
                        .await
                    {
                        Ok(true) => {
                            processed_methods.push((ProviderType::Sms, normalized_phone));
                        }
                        Ok(false) => {
                            return (
                                StatusCode::BAD_REQUEST,
                                Json(ErrorResponse {
                                    error: "Phone number must be verified before updating contact"
                                        .to_string(),
                                }),
                            )
                                .into_response();
                        }
                        Err(e) => {
                            return (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(ErrorResponse {
                                    error: format!("Failed to check phone verification: {}", e),
                                }),
                            )
                                .into_response();
                        }
                    }
                } else {
                    // Phone number hasn't changed, so we can reuse it without verification
                    processed_methods.push((ProviderType::Sms, normalized_phone));
                }
            }
            ProviderType::Ntfy => {
                // Push notifications are always allowed (ntfy is free)
                // Use user-provided topic from request
                match validate_ntfy_topic(&method.notification_target) {
                    Ok(topic) => processed_methods.push((ProviderType::Ntfy, topic)),
                    Err(e) => {
                        return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e }))
                            .into_response();
                    }
                }
            }
            ProviderType::Email => {
                // Basic email validation
                let email = method.notification_target.trim().to_lowercase();
                if !email.contains('@') || email.len() < 5 {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            error: "Invalid email address format".to_string(),
                        }),
                    )
                        .into_response();
                }

                // SECURITY: Only verify if the email address has changed
                if has_method_changed(method) {
                    // Check cross-wallet verification first
                    if let Ok(true) = app_services
                        .metadata_db
                        .is_notification_target_verified_for_user(&user.user_id, "email", &email)
                        .await
                    {
                        processed_methods.push((ProviderType::Email, email));
                        continue;
                    }

                    // Check if this email address was recently verified for THIS wallet
                    match app_services
                        .metadata_db
                        .was_recently_verified(&wallet_checksum, &email)
                        .await
                    {
                        Ok(true) => {
                            processed_methods.push((ProviderType::Email, email));
                        }
                        Ok(false) => {
                            return (
                                StatusCode::BAD_REQUEST,
                                Json(ErrorResponse {
                                    error: "Email address must be verified before updating contact"
                                        .to_string(),
                                }),
                            )
                                .into_response();
                        }
                        Err(e) => {
                            return (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(ErrorResponse {
                                    error: format!("Failed to check email verification: {}", e),
                                }),
                            )
                                .into_response();
                        }
                    }
                } else {
                    // Email address hasn't changed, so we can reuse it without verification
                    processed_methods.push((ProviderType::Email, email));
                }
            }
        }
    }

    // Ensure at least one method was provided
    if processed_methods.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "At least one notification method must be provided".to_string(),
            }),
        )
            .into_response();
    }

    // Check for duplicate notification targets (email/phone) within the same wallet, excluding current contact
    let methods_for_validation: Vec<(String, String)> = processed_methods
        .iter()
        .map(|(provider, target)| (provider.as_str().to_string(), target.clone()))
        .collect();

    match app_services
        .metadata_db
        .check_duplicate_notification_targets(
            &wallet_checksum,
            &methods_for_validation,
            Some(&contact_id),
        )
        .await
    {
        Ok(duplicates) => {
            if !duplicates.is_empty() {
                return (
                    StatusCode::CONFLICT,
                    Json(ErrorResponse {
                        error: format!("Duplicate notification targets: {}", duplicates.join(", ")),
                    }),
                )
                    .into_response();
            }
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to check for duplicates: {}", e),
                }),
            )
                .into_response();
        }
    }

    // Update contact using transaction
    match app_services
        .metadata_db
        .update_contact_with_methods(
            &contact_id,
            &wallet_checksum,
            &payload.name,
            processed_methods,
        )
        .await
    {
        Ok(()) => {
            // Get the updated contact to return
            match app_services
                .metadata_db
                .get_contacts_with_notification_methods(&wallet_checksum)
                .await
            {
                Ok(contacts) => {
                    if let Some(updated_contact) =
                        contacts.iter().find(|c| c.id.as_ref() == Some(&contact_id))
                    {
                        let elapsed = start_time.elapsed();
                        info!("update_wallet_contact completed in {:?}", elapsed);
                        (StatusCode::OK, Json(updated_contact.clone())).into_response()
                    } else {
                        (
                            StatusCode::NOT_FOUND,
                            Json(ErrorResponse {
                                error: "Updated contact not found".to_string(),
                            }),
                        )
                            .into_response()
                    }
                }
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Failed to fetch updated contact: {}", e),
                    }),
                )
                    .into_response(),
            }
        }
        Err(e) => {
            let elapsed = start_time.elapsed();
            info!("update_wallet_contact failed in {:?}", elapsed);
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// Get all contacts for a wallet
pub async fn get_wallet_contacts(
    AuthenticatedUser(user): AuthenticatedUser,
    Path(wallet_checksum): Path<String>,
    State(app_services): State<AppServicesState>,
) -> Response {
    let start_time = std::time::Instant::now();

    // Direct metadata access - no mutex blocking!
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

    let contacts_result = app_services
        .metadata_db
        .get_contacts_with_notification_methods_filtered(&wallet_checksum, true)
        .await;

    let elapsed = start_time.elapsed();
    info!("get_wallet_contacts completed in {:?}", elapsed);

    match contacts_result {
        Ok(contacts) => (StatusCode::OK, Json(contacts)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}
