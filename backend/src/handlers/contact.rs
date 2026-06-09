//! Contact management handlers

use crate::api::AppServicesState;
use crate::config::AppConfig;
use crate::extractors::{require_non_demo, AuthenticatedUser};
use crate::handlers::helpers::{
    check_resource_limit, get_user_or_error, require_recent_verification, verify_wallet_access,
    DatabaseErrorMessage, ResourceLimit,
};
use crate::metadata::ProviderType;
use crate::models::{
    validate_phone_number, CreateContactResponse, CreateContactWithMethodsRequest, ErrorResponse,
    NotificationMethodRequest, UpdateContactRequest,
};
use crate::stripe_billing::StripeBilling;
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
    let _wallet = match verify_wallet_access(
        &app_services,
        &user,
        &wallet_checksum,
        DatabaseErrorMessage::Raw,
    )
    .await
    {
        Ok(wallet) => wallet,
        Err(response) => return response,
    };

    // Get user's subscription tier and check contact limit
    let user_record = match get_user_or_error(
        &app_services,
        &user.user_id,
        Some("user_not_found"),
        "User not found",
        DatabaseErrorMessage::Prefix("Failed to get user information"),
    )
    .await
    {
        Ok(record) => record,
        Err(response) => return response,
    };

    if let Err(response) = check_resource_limit(
        &app_services,
        config.as_ref(),
        &user_record,
        ResourceLimit::Contact {
            wallet_checksum: &wallet_checksum,
        },
    )
    .await
    {
        return response;
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
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse::coded("invalid_phone_number", e)),
                        )
                            .into_response();
                    }
                };

                // Check if this phone number is already verified for this user (cross-wallet)
                match app_services
                    .metadata_db
                    .is_notification_target_verified_for_user(
                        &user.user_id,
                        "sms",
                        &normalized_phone,
                    )
                    .await
                {
                    Ok(true) => {
                        // Phone already verified for another wallet, skip OTP
                        processed_methods.push((
                            ProviderType::Sms,
                            normalized_phone,
                            method.is_enabled,
                        ));
                        continue;
                    }
                    Ok(false) => {} // Not verified for another wallet, check wallet-specific verification
                    Err(e) => {
                        tracing::warn!(
                            "Failed to check cross-wallet SMS verification, requiring OTP: {}",
                            e
                        );
                    }
                }

                // SECURITY: Check if this phone number was recently verified for THIS wallet
                match require_recent_verification(
                    &app_services,
                    &wallet_checksum,
                    &normalized_phone,
                    "phone_not_verified",
                    "Phone number must be verified before adding contact",
                    "Failed to check phone verification",
                )
                .await
                {
                    Ok(()) => processed_methods.push((
                        ProviderType::Sms,
                        normalized_phone,
                        method.is_enabled,
                    )),
                    Err(response) => return response,
                }
            }
            ProviderType::Ntfy => {
                // Push notifications are always allowed (ntfy is free)
                // Use user-provided topic from request
                match validate_ntfy_topic(&method.notification_target) {
                    Ok(topic) => {
                        processed_methods.push((ProviderType::Ntfy, topic, method.is_enabled))
                    }
                    Err(e) => {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse::coded("invalid_ntfy_topic", e)),
                        )
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
                        Json(ErrorResponse::coded(
                            "invalid_email_format",
                            "Invalid email address format",
                        )),
                    )
                        .into_response();
                }

                // Check if this email is already verified for this user (cross-wallet)
                match app_services
                    .metadata_db
                    .is_notification_target_verified_for_user(&user.user_id, "email", &email)
                    .await
                {
                    Ok(true) => {
                        // Email already verified for another wallet, skip OTP
                        processed_methods.push((ProviderType::Email, email, method.is_enabled));
                        continue;
                    }
                    Ok(false) => {} // Not verified for another wallet, check wallet-specific verification
                    Err(e) => {
                        tracing::warn!(
                            "Failed to check cross-wallet email verification, requiring OTP: {}",
                            e
                        );
                    }
                }

                // SECURITY: Check if this email address was recently verified for THIS wallet
                match require_recent_verification(
                    &app_services,
                    &wallet_checksum,
                    &email,
                    "email_not_verified_contact",
                    "Email address must be verified before adding contact",
                    "Failed to check email verification",
                )
                .await
                {
                    Ok(()) => {
                        processed_methods.push((ProviderType::Email, email, method.is_enabled))
                    }
                    Err(response) => return response,
                }
            }
        }
    }

    // Ensure at least one method was provided
    if processed_methods.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::coded(
                "no_notification_method",
                "At least one notification method must be provided",
            )),
        )
            .into_response();
    }

    // Check for duplicate notification targets (email/phone) within the same wallet
    let methods_for_validation: Vec<(String, String)> = processed_methods
        .iter()
        .map(|(provider, target, _)| (provider.as_str().to_string(), target.clone()))
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
                    Json(ErrorResponse::coded(
                        "duplicate_notification_targets",
                        format!("Duplicate notification targets: {}", duplicates.join(", ")),
                    )),
                )
                    .into_response();
            }
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to check for duplicates: {}",
                    e
                ))),
            )
                .into_response();
        }
    }

    let create_result = app_services
        .metadata_db
        .insert_contact_with_notification_settings(
            &wallet_checksum,
            &payload.name,
            processed_methods,
            payload.notify_sending,
            payload.notify_sent,
            payload.notify_receiving,
            payload.notify_received,
            payload.notify_cpfp,
            payload.notify_rbf,
            payload.include_wallet_balance_in_tx_notifications,
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
            Json(ErrorResponse::new(e.to_string())),
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
    if let Err(response) = verify_wallet_access(
        &app_services,
        &user,
        &wallet_checksum,
        DatabaseErrorMessage::Prefix("Database error"),
    )
    .await
    {
        return response;
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
            Json(ErrorResponse::coded(
                "contact_not_found",
                "Contact not found",
            )),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
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
    let _wallet = match verify_wallet_access(
        &app_services,
        &user,
        &wallet_checksum,
        DatabaseErrorMessage::Raw,
    )
    .await
    {
        Ok(wallet) => wallet,
        Err(response) => return response,
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
                Json(ErrorResponse::coded(
                    "contact_not_found",
                    "Contact not found",
                )),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to get existing contact: {}",
                    e
                ))),
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
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse::coded("invalid_phone_number", e)),
                        )
                            .into_response();
                    }
                };

                // SECURITY: Only verify if the phone number has changed
                if has_method_changed(method) {
                    // Check cross-wallet verification first
                    match app_services
                        .metadata_db
                        .is_notification_target_verified_for_user(
                            &user.user_id,
                            "sms",
                            &normalized_phone,
                        )
                        .await
                    {
                        Ok(true) => {
                            processed_methods.push((
                                ProviderType::Sms,
                                normalized_phone,
                                method.is_enabled,
                            ));
                            continue;
                        }
                        Ok(false) => {} // Not verified for another wallet, check wallet-specific verification
                        Err(e) => {
                            tracing::warn!(
                                "Failed to check cross-wallet SMS verification, requiring OTP: {}",
                                e
                            );
                        }
                    }

                    // Check if this phone number was recently verified for THIS wallet
                    match require_recent_verification(
                        &app_services,
                        &wallet_checksum,
                        &normalized_phone,
                        "phone_not_verified",
                        "Phone number must be verified before updating contact",
                        "Failed to check phone verification",
                    )
                    .await
                    {
                        Ok(()) => processed_methods.push((
                            ProviderType::Sms,
                            normalized_phone,
                            method.is_enabled,
                        )),
                        Err(response) => return response,
                    }
                } else {
                    // Phone number hasn't changed, so we can reuse it without verification
                    processed_methods.push((
                        ProviderType::Sms,
                        normalized_phone,
                        method.is_enabled,
                    ));
                }
            }
            ProviderType::Ntfy => {
                // Push notifications are always allowed (ntfy is free)
                // Use user-provided topic from request
                match validate_ntfy_topic(&method.notification_target) {
                    Ok(topic) => {
                        processed_methods.push((ProviderType::Ntfy, topic, method.is_enabled))
                    }
                    Err(e) => {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse::coded("invalid_ntfy_topic", e)),
                        )
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
                        Json(ErrorResponse::coded(
                            "invalid_email_format",
                            "Invalid email address format",
                        )),
                    )
                        .into_response();
                }

                // SECURITY: Only verify if the email address has changed
                if has_method_changed(method) {
                    // Check cross-wallet verification first
                    match app_services
                        .metadata_db
                        .is_notification_target_verified_for_user(&user.user_id, "email", &email)
                        .await
                    {
                        Ok(true) => {
                            processed_methods.push((ProviderType::Email, email, method.is_enabled));
                            continue;
                        }
                        Ok(false) => {} // Not verified for another wallet, check wallet-specific verification
                        Err(e) => {
                            tracing::warn!(
                                "Failed to check cross-wallet email verification, requiring OTP: {}",
                                e
                            );
                        }
                    }

                    // Check if this email address was recently verified for THIS wallet
                    match require_recent_verification(
                        &app_services,
                        &wallet_checksum,
                        &email,
                        "email_not_verified_contact",
                        "Email address must be verified before updating contact",
                        "Failed to check email verification",
                    )
                    .await
                    {
                        Ok(()) => {
                            processed_methods.push((ProviderType::Email, email, method.is_enabled))
                        }
                        Err(response) => return response,
                    }
                } else {
                    // Email address hasn't changed, so we can reuse it without verification
                    processed_methods.push((ProviderType::Email, email, method.is_enabled));
                }
            }
        }
    }

    // Ensure at least one method was provided
    if processed_methods.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::coded(
                "no_notification_method",
                "At least one notification method must be provided",
            )),
        )
            .into_response();
    }

    // Check for duplicate notification targets (email/phone) within the same wallet, excluding current contact
    let methods_for_validation: Vec<(String, String)> = processed_methods
        .iter()
        .map(|(provider, target, _)| (provider.as_str().to_string(), target.clone()))
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
                    Json(ErrorResponse::coded(
                        "duplicate_notification_targets",
                        format!("Duplicate notification targets: {}", duplicates.join(", ")),
                    )),
                )
                    .into_response();
            }
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to check for duplicates: {}",
                    e
                ))),
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
            payload
                .notify_sending
                .unwrap_or(existing_contact.notify_sending),
            payload.notify_sent.unwrap_or(existing_contact.notify_sent),
            payload
                .notify_receiving
                .unwrap_or(existing_contact.notify_receiving),
            payload
                .notify_received
                .unwrap_or(existing_contact.notify_received),
            payload.notify_cpfp.unwrap_or(existing_contact.notify_cpfp),
            payload.notify_rbf.unwrap_or(existing_contact.notify_rbf),
            payload
                .include_wallet_balance_in_tx_notifications
                .unwrap_or(existing_contact.include_wallet_balance_in_tx_notifications),
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
                            Json(ErrorResponse::coded(
                                "contact_not_found",
                                "Updated contact not found",
                            )),
                        )
                            .into_response()
                    }
                }
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(format!(
                        "Failed to fetch updated contact: {}",
                        e
                    ))),
                )
                    .into_response(),
            }
        }
        Err(e) => {
            let elapsed = start_time.elapsed();
            info!("update_wallet_contact failed in {:?}", elapsed);
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(e.to_string())),
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
    if let Err(response) = verify_wallet_access(
        &app_services,
        &user,
        &wallet_checksum,
        DatabaseErrorMessage::Raw,
    )
    .await
    {
        return response;
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
            Json(ErrorResponse::new(e.to_string())),
        )
            .into_response(),
    }
}
