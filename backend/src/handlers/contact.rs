//! Contact management handlers

use crate::api::AppServicesState;
use crate::config::AppConfig;
use crate::extractors::{require_non_demo, AuthenticatedUser};
use crate::handlers::error::{ApiError, ApiResult, OrInternal};
use crate::handlers::helpers::{
    check_resource_limit, get_user_or_error, reject_nostr_in_cloud_mode,
    reject_telegram_if_unconfigured, reject_telegram_in_cloud_mode, reject_webhook_in_cloud_mode,
    require_recent_verification, verify_wallet_access, verify_wallet_read_access, ResourceLimit,
};
use crate::metadata::{
    Contact, ContactNotificationSettings, NotificationContentFields, ProviderType,
};
use crate::models::{
    validate_phone_number, CreateContactResponse, CreateContactWithMethodsRequest,
    NotificationMethodRequest, UpdateContactRequest,
};
use crate::nostr_provider::normalize_nostr_recipient_or_error;
use crate::stripe_billing::StripeBilling;
use crate::telegram_provider::validate_telegram_chat_id;
use crate::webhook_provider::validate_webhook_url;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use std::sync::Arc;
use tracing::info;

/// Type alias for Stripe billing state
pub type StripeBillingState = Option<Arc<StripeBilling>>;

/// A validated notification method, before content-field defaults are applied.
type ProcessedMethod = (
    ProviderType,
    String,
    bool,
    Option<NotificationContentFields>,
);

/// Validates and normalizes a phone number.
pub(super) fn normalize_phone(phone: &str) -> ApiResult<String> {
    validate_phone_number(phone).map_err(|e| ApiError::bad_request("invalid_phone_number", e))
}

/// Normalizes an email address and applies a basic format check.
pub(super) fn normalize_email(email: &str) -> ApiResult<String> {
    let email = email.trim().to_lowercase();
    if !email.contains('@') || email.len() < 5 {
        return Err(ApiError::bad_request(
            "invalid_email_format",
            "Invalid email address format",
        ));
    }
    Ok(email)
}

/// Whether the user already verified `target` for another wallet.
///
/// A failed lookup counts as unverified, so the caller falls back to requiring
/// an OTP for this wallet.
pub(super) async fn is_verified_for_user(
    app_services: &AppServicesState,
    user_id: &str,
    provider_type: &str,
    target: &str,
) -> bool {
    app_services
        .metadata_db
        .is_notification_target_verified_for_user(user_id, provider_type, target)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                error = %e,
                provider_type,
                "Failed to check cross-wallet verification, requiring OTP"
            );
            false
        })
}

/// Validates an ntfy topic (1-64 chars, alphanumeric and dashes)
fn validate_ntfy_topic(topic: &str) -> ApiResult<String> {
    let invalid = |message| ApiError::bad_request("invalid_ntfy_topic", message);
    let topic = topic.trim();
    if topic.is_empty() {
        return Err(invalid("ntfy topic cannot be empty"));
    }
    if topic.len() > 64 {
        return Err(invalid("ntfy topic must be at most 64 characters"));
    }
    // Allow alphanumeric, dashes, and underscores
    if !topic
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(invalid(
            "ntfy topic can only contain letters, numbers, dashes, and underscores",
        ));
    }
    Ok(topic.to_string())
}

fn normalize_nostr(target: &str) -> ApiResult<String> {
    normalize_nostr_recipient_or_error(target)
        .map_err(|e| ApiError::bad_request("invalid_nostr_recipient", e))
}

fn normalize_telegram(target: &str) -> ApiResult<String> {
    validate_telegram_chat_id(target)
        .map_err(|e| ApiError::bad_request("invalid_telegram_chat_id", e))
}

/// Validates a method's target and returns its canonical form.
async fn validated_target(method: &NotificationMethodRequest) -> ApiResult<String> {
    let target = &method.notification_target;
    match method.provider_type {
        ProviderType::Sms => normalize_phone(target),
        ProviderType::Ntfy => validate_ntfy_topic(target),
        ProviderType::Email => normalize_email(target),
        ProviderType::Nostr => normalize_nostr(target),
        ProviderType::Webhook => validate_webhook_url(target)
            .await
            .map_err(|e| ApiError::bad_request("invalid_webhook_url", e)),
        ProviderType::Telegram => normalize_telegram(target),
    }
}

/// Rejects providers that are only available in self-hosted mode.
fn reject_unavailable_provider(provider_type: &ProviderType, config: &AppConfig) -> ApiResult<()> {
    match provider_type {
        ProviderType::Nostr => reject_nostr_in_cloud_mode(config),
        ProviderType::Webhook => reject_webhook_in_cloud_mode(config),
        ProviderType::Telegram => reject_telegram_in_cloud_mode(config),
        ProviderType::Sms | ProviderType::Ntfy | ProviderType::Email => Ok(()),
    }
}

/// Whether `method` targets something other than the contact's existing
/// method of the same provider.
fn has_method_changed(existing: &Contact, method: &NotificationMethodRequest) -> ApiResult<bool> {
    let Some(existing_method) = existing
        .notification_methods
        .iter()
        .find(|m| m.provider_type == method.provider_type)
    else {
        return Ok(true);
    };

    let target = &method.notification_target;
    // Compare normalized strings for providers with canonical target forms.
    let new_normalized = match method.provider_type {
        ProviderType::Sms => validate_phone_number(target).unwrap_or_else(|_| target.clone()),
        ProviderType::Email => target.trim().to_lowercase(),
        ProviderType::Nostr => normalize_nostr(target)?,
        ProviderType::Telegram => normalize_telegram(target)?,
        ProviderType::Ntfy | ProviderType::Webhook => target.clone(),
    };
    Ok(existing_method.notification_target != new_normalized)
}

/// Requires that an SMS or email target was verified, either for another of
/// the user's wallets or recently for this one. Other providers need no
/// verification.
async fn require_verified_target(
    app_services: &AppServicesState,
    user_id: &str,
    wallet_checksum: &str,
    provider_type: &ProviderType,
    target: &str,
    is_update: bool,
) -> ApiResult<()> {
    let (code, message, error_message) = match (provider_type, is_update) {
        (ProviderType::Sms, false) => (
            "phone_not_verified",
            "Phone number must be verified before adding contact",
            "Failed to check phone verification",
        ),
        (ProviderType::Sms, true) => (
            "phone_not_verified",
            "Phone number must be verified before updating contact",
            "Failed to check phone verification",
        ),
        (ProviderType::Email, false) => (
            "email_not_verified_contact",
            "Email address must be verified before adding contact",
            "Failed to check email verification",
        ),
        (ProviderType::Email, true) => (
            "email_not_verified_contact",
            "Email address must be verified before updating contact",
            "Failed to check email verification",
        ),
        _ => return Ok(()),
    };

    if is_verified_for_user(app_services, user_id, provider_type.as_str(), target).await {
        return Ok(());
    }
    // SECURITY: Check if this target was recently verified for THIS wallet
    require_recent_verification(
        app_services,
        wallet_checksum,
        target,
        code,
        message,
        error_message,
    )
    .await
}

/// Validates the requested notification methods.
///
/// `existing` is the contact being updated, or `None` when creating one. On
/// update, only changed targets need fresh verification.
async fn process_notification_methods(
    app_services: &AppServicesState,
    config: &AppConfig,
    user_id: &str,
    wallet_checksum: &str,
    methods: &[NotificationMethodRequest],
    legacy_transaction_balance: bool,
    existing: Option<&Contact>,
) -> ApiResult<Vec<ProcessedMethod>> {
    let method_changed = |method: &NotificationMethodRequest| {
        existing.map_or(Ok(true), |contact| has_method_changed(contact, method))
    };
    let mut processed_methods = Vec::with_capacity(methods.len());

    for method in methods {
        let requested_content_fields = method.content_fields.or_else(|| {
            method.content_privacy_level.map(|level| {
                NotificationContentFields::from_legacy(level, legacy_transaction_balance)
            })
        });
        let provider_type = &method.provider_type;

        reject_unavailable_provider(provider_type, config)?;
        if *provider_type == ProviderType::Telegram && method_changed(method)? {
            reject_telegram_if_unconfigured(app_services).await?;
        }
        let target = validated_target(method).await?;
        // SECURITY: SMS and email targets must be verified, unless an update
        // keeps them unchanged.
        if matches!(provider_type, ProviderType::Sms | ProviderType::Email)
            && method_changed(method)?
        {
            require_verified_target(
                app_services,
                user_id,
                wallet_checksum,
                provider_type,
                &target,
                existing.is_some(),
            )
            .await?;
        }

        processed_methods.push((
            provider_type.clone(),
            target,
            method.is_enabled,
            requested_content_fields,
        ));
    }

    if processed_methods.is_empty() {
        return Err(ApiError::bad_request(
            "no_notification_method",
            "At least one notification method must be provided",
        ));
    }
    Ok(processed_methods)
}

/// Rejects targets already used by another contact of the wallet.
async fn reject_duplicate_targets(
    app_services: &AppServicesState,
    wallet_checksum: &str,
    processed_methods: &[ProcessedMethod],
    exclude_contact_id: Option<&str>,
) -> ApiResult<()> {
    let methods_for_validation: Vec<(String, String)> = processed_methods
        .iter()
        .map(|(provider, target, _, _)| (provider.as_str().to_string(), target.clone()))
        .collect();

    let duplicates = app_services
        .metadata_db
        .check_duplicate_notification_targets(
            wallet_checksum,
            &methods_for_validation,
            exclude_contact_id,
        )
        .await
        .or_internal("Failed to check for duplicates")?;
    if duplicates.is_empty() {
        Ok(())
    } else {
        Err(ApiError::conflict(
            "duplicate_notification_targets",
            format!("Duplicate notification targets: {}", duplicates.join(", ")),
        ))
    }
}

/// Create a new contact for a wallet
pub async fn create_wallet_contact(
    AuthenticatedUser(user): AuthenticatedUser,
    Path(wallet_checksum): Path<String>,
    State(app_services): State<AppServicesState>,
    State(stripe_billing): State<StripeBillingState>,
    State(config): State<Arc<AppConfig>>,
    Json(payload): Json<CreateContactWithMethodsRequest>,
) -> ApiResult<(StatusCode, Json<CreateContactResponse>)> {
    let _ = stripe_billing; // Used for state extraction pattern consistency
    let start_time = std::time::Instant::now();

    require_non_demo(&user)?;
    verify_wallet_access(
        &app_services,
        &user,
        &wallet_checksum,
        "Internal server error",
    )
    .await?;

    // Get user's subscription tier and check contact limit
    let user_record = get_user_or_error(
        &app_services,
        &user.user_id,
        Some("user_not_found"),
        "User not found",
        "Failed to get user information",
    )
    .await?;
    check_resource_limit(
        &app_services,
        config.as_ref(),
        &user_record,
        ResourceLimit::Contact {
            wallet_checksum: &wallet_checksum,
        },
    )
    .await?;

    let processed_methods = process_notification_methods(
        &app_services,
        config.as_ref(),
        &user.user_id,
        &wallet_checksum,
        &payload.notification_methods,
        payload.include_wallet_balance_in_tx_notifications,
        None,
    )
    .await?;
    reject_duplicate_targets(&app_services, &wallet_checksum, &processed_methods, None).await?;

    let processed_methods = processed_methods
        .into_iter()
        .map(|(provider, target, is_enabled, requested_content_fields)| {
            (
                provider,
                target,
                is_enabled,
                requested_content_fields.unwrap_or(NotificationContentFields::standard()),
            )
        })
        .collect();

    let create_result = app_services
        .metadata_db
        .insert_contact_with_notification_settings(
            &wallet_checksum,
            &payload.name,
            processed_methods,
            ContactNotificationSettings {
                notify_sending: payload.notify_sending,
                notify_sent: payload.notify_sent,
                notify_receiving: payload.notify_receiving,
                notify_received: payload.notify_received,
                notify_cpfp: payload.notify_cpfp,
                notify_rbf: payload.notify_rbf,
                include_wallet_balance_in_tx_notifications: payload
                    .include_wallet_balance_in_tx_notifications,
            },
        )
        .await;

    let elapsed = start_time.elapsed();
    info!("create_wallet_contact completed in {:?}", elapsed);

    let contact_id =
        create_result.map_err(|e| ApiError::uncoded(StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok((
        StatusCode::CREATED,
        Json(CreateContactResponse {
            message: "Contact created successfully".to_string(),
            contact_id,
        }),
    ))
}

/// Delete a contact from a wallet
pub async fn delete_wallet_contact(
    AuthenticatedUser(user): AuthenticatedUser,
    Path((wallet_checksum, contact_id)): Path<(String, String)>,
    State(app_services): State<AppServicesState>,
) -> ApiResult<StatusCode> {
    let start_time = std::time::Instant::now();

    require_non_demo(&user)?;
    verify_wallet_access(&app_services, &user, &wallet_checksum, "Database error").await?;

    let delete_result = app_services
        .metadata_db
        .delete_wallet_contact(&wallet_checksum, &contact_id)
        .await;

    let elapsed = start_time.elapsed();
    info!("delete_wallet_contact completed in {:?}", elapsed);

    if delete_result.or_internal("Failed to delete contact")? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found(
            "contact_not_found",
            "Contact not found",
        ))
    }
}

/// Update a contact for a wallet
pub async fn update_wallet_contact(
    AuthenticatedUser(user): AuthenticatedUser,
    Path((wallet_checksum, contact_id)): Path<(String, String)>,
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    Json(payload): Json<UpdateContactRequest>,
) -> ApiResult<Json<Contact>> {
    let start_time = std::time::Instant::now();

    require_non_demo(&user)?;
    verify_wallet_access(
        &app_services,
        &user,
        &wallet_checksum,
        "Internal server error",
    )
    .await?;

    // Get existing contact to compare notification methods
    let existing_contact = app_services
        .metadata_db
        .get_single_contact_with_methods(&contact_id, &wallet_checksum)
        .await
        .or_internal("Failed to get existing contact")?
        .ok_or_else(|| ApiError::not_found("contact_not_found", "Contact not found"))?;

    let legacy_transaction_balance = payload
        .include_wallet_balance_in_tx_notifications
        .unwrap_or(existing_contact.include_wallet_balance_in_tx_notifications);
    let processed_methods = process_notification_methods(
        &app_services,
        config.as_ref(),
        &user.user_id,
        &wallet_checksum,
        &payload.notification_methods,
        legacy_transaction_balance,
        Some(&existing_contact),
    )
    .await?;
    reject_duplicate_targets(
        &app_services,
        &wallet_checksum,
        &processed_methods,
        Some(contact_id.as_str()),
    )
    .await?;

    // Omitted content settings preserve an existing method. A genuinely new
    // method receives the documented wallet-name plus event-type default.
    let processed_methods = processed_methods
        .into_iter()
        .map(|(provider, target, is_enabled, requested_content_fields)| {
            let existing_fields = existing_contact
                .notification_methods
                .iter()
                .find(|method| {
                    method.provider_type == provider && method.notification_target == target
                })
                .map(|method| method.content_fields);
            (
                provider,
                target,
                is_enabled,
                requested_content_fields
                    .or(existing_fields)
                    .unwrap_or(NotificationContentFields::standard()),
            )
        })
        .collect();

    // Update contact using transaction
    let update_result = app_services
        .metadata_db
        .update_contact_with_methods(
            &contact_id,
            &wallet_checksum,
            &payload.name,
            processed_methods,
            ContactNotificationSettings {
                notify_sending: payload
                    .notify_sending
                    .unwrap_or(existing_contact.notify_sending),
                notify_sent: payload.notify_sent.unwrap_or(existing_contact.notify_sent),
                notify_receiving: payload
                    .notify_receiving
                    .unwrap_or(existing_contact.notify_receiving),
                notify_received: payload
                    .notify_received
                    .unwrap_or(existing_contact.notify_received),
                notify_cpfp: payload.notify_cpfp.unwrap_or(existing_contact.notify_cpfp),
                notify_rbf: payload.notify_rbf.unwrap_or(existing_contact.notify_rbf),
                include_wallet_balance_in_tx_notifications: legacy_transaction_balance,
            },
        )
        .await;
    if let Err(e) = update_result {
        let elapsed = start_time.elapsed();
        info!("update_wallet_contact failed in {:?}", elapsed);
        return Err(ApiError::uncoded(StatusCode::BAD_REQUEST, e.to_string()));
    }

    // Get the updated contact to return
    let updated_contact = app_services
        .metadata_db
        .get_contacts_with_notification_methods(&wallet_checksum)
        .await
        .or_internal("Failed to fetch updated contact")?
        .into_iter()
        .find(|c| c.id.as_ref() == Some(&contact_id))
        .ok_or_else(|| ApiError::not_found("contact_not_found", "Updated contact not found"))?;

    let elapsed = start_time.elapsed();
    info!("update_wallet_contact completed in {:?}", elapsed);
    Ok(Json(updated_contact))
}

/// Get all contacts for a wallet
pub async fn get_wallet_contacts(
    AuthenticatedUser(user): AuthenticatedUser,
    Path(wallet_checksum): Path<String>,
    State(app_services): State<AppServicesState>,
) -> ApiResult<Json<Vec<Contact>>> {
    let start_time = std::time::Instant::now();

    verify_wallet_read_access(
        &app_services,
        &user,
        &wallet_checksum,
        "Internal server error",
    )
    .await?;

    let contacts_result = app_services
        .metadata_db
        .get_contacts_with_notification_methods_filtered(&wallet_checksum, true)
        .await;

    let elapsed = start_time.elapsed();
    info!("get_wallet_contacts completed in {:?}", elapsed);

    Ok(Json(
        contacts_result.or_internal("Failed to load contacts")?,
    ))
}
