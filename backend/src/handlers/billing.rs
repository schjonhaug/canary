//! Stripe billing and subscription management handlers

use crate::api::{AppServicesState, ConfigState, StripeBillingState};
use crate::extractors::AuthenticatedUser;
use crate::handlers::helpers::{get_user_or_error, DatabaseErrorMessage};
use crate::metadata::SubscriptionUpdateParams;
use crate::models::{
    BillingStatusResponse, BillingTierLimits, CreateCheckoutSessionRequest,
    CreateCustomerPortalRequest, ErrorResponse,
};
use crate::subscription::SubscriptionTier;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use std::sync::Arc;
use tracing::info;

/// Create a Stripe checkout session for subscription
pub async fn create_stripe_checkout_session(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(stripe_billing): State<StripeBillingState>,
    State(config): State<ConfigState>,
    Json(payload): Json<CreateCheckoutSessionRequest>,
) -> Response {
    let start_time = std::time::Instant::now();

    // Parse subscription tier
    let tier = match payload.tier.as_str() {
        "personal" => SubscriptionTier::Personal,
        "team" => SubscriptionTier::Team,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::coded(
                    "invalid_subscription_tier",
                    "Invalid subscription tier",
                )),
            )
                .into_response();
        }
    };

    // Get user record
    let user_record = match get_user_or_error(
        &app_services,
        &user.user_id,
        None,
        "User not found",
        DatabaseErrorMessage::Prefix("Database error"),
    )
    .await
    {
        Ok(user_record) => user_record,
        Err(response) => return response,
    };

    // Get Stripe billing from state
    let stripe_billing = match stripe_billing.as_ref() {
        Some(billing) => billing.as_ref(),
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Stripe billing not initialized")),
            )
                .into_response();
        }
    };

    // Create checkout session with configurable URLs
    let is_yearly = payload.is_yearly.unwrap_or(false);
    let billing_cycle = if is_yearly { "yearly" } else { "monthly" };
    let frontend_url = match config.frontend_url() {
        Some(url) => url,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("FRONTEND_URL not configured")),
            )
                .into_response();
        }
    };
    let success_url = format!(
        "{}/subscription?success=true&session={{CHECKOUT_SESSION_ID}}",
        frontend_url
    );
    let cancel_url = format!("{}/subscription?cancelled=true", frontend_url);

    let result = stripe_billing
        .create_checkout_session(
            &user_record.id,
            tier,
            billing_cycle,
            &success_url,
            &cancel_url,
            &app_services.metadata_db,
        )
        .await;

    let elapsed = start_time.elapsed();
    info!("create_stripe_checkout_session completed in {:?}", elapsed);

    match result {
        Ok(session) => (StatusCode::OK, Json(session)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(format!(
                "Failed to create checkout session: {}",
                e
            ))),
        )
            .into_response(),
    }
}

/// Create a Stripe customer portal session
pub async fn create_stripe_customer_portal(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(stripe_billing): State<StripeBillingState>,
    Json(payload): Json<CreateCustomerPortalRequest>,
) -> Response {
    let start_time = std::time::Instant::now();

    // Get user record
    let user_record = match get_user_or_error(
        &app_services,
        &user.user_id,
        None,
        "User not found",
        DatabaseErrorMessage::Prefix("Database error"),
    )
    .await
    {
        Ok(user_record) => user_record,
        Err(response) => return response,
    };

    // User must have a Stripe customer ID
    let customer_id = match &user_record.stripe_customer_id {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::coded(
                    "no_stripe_customer",
                    "No Stripe customer found. Please create a subscription first.",
                )),
            )
                .into_response();
        }
    };

    // Get Stripe billing from state
    let stripe_billing = match stripe_billing.as_ref() {
        Some(billing) => billing.as_ref(),
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Stripe billing not initialized")),
            )
                .into_response();
        }
    };

    // Create customer portal session
    tracing::info!(
        "Creating customer portal session for customer_id: {}, return_url: {}",
        customer_id,
        payload.return_url
    );
    match stripe_billing
        .create_customer_portal_session(customer_id, &payload.return_url)
        .await
    {
        Ok(session) => {
            tracing::info!(
                "✅ Customer portal session created successfully: {}",
                session.url
            );
            let elapsed = start_time.elapsed();
            info!("create_stripe_customer_portal completed in {:?}", elapsed);
            (StatusCode::OK, Json(session)).into_response()
        }
        Err(e) => {
            tracing::error!("❌ Failed to create customer portal session: {}", e);
            let elapsed = start_time.elapsed();
            info!("create_stripe_customer_portal completed in {:?}", elapsed);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to create customer portal session: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}

/// Get billing status for the authenticated user
pub async fn get_billing_status(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<crate::config::AppConfig>>,
) -> Response {
    let start_time = std::time::Instant::now();

    // Direct metadata access - no mutex blocking!
    let user_record = match get_user_or_error(
        &app_services,
        &user.user_id,
        None,
        "User not found",
        DatabaseErrorMessage::Prefix("Database error"),
    )
    .await
    {
        Ok(user_record) => user_record,
        Err(response) => return response,
    };

    // Get wallet count for this user
    let wallet_count = match app_services
        .metadata_db
        .count_wallets_for_user(&user.user_id)
        .await
    {
        Ok(count) => count,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!("Database error: {}", e))),
            )
                .into_response();
        }
    };

    // Get contact count across all wallets for this user
    let user_wallets = match app_services
        .metadata_db
        .get_wallets_for_user(Some(&user.user_id))
        .await
    {
        Ok(wallets) => wallets,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!("Database error: {}", e))),
            )
                .into_response();
        }
    };
    let contact_count = {
        let mut total = 0;
        for wallet in &user_wallets {
            let count = match app_services
                .metadata_db
                .count_contacts_for_wallet(&wallet.checksum)
                .await
            {
                Ok(count) => count,
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse::new(format!("Database error: {}", e))),
                    )
                        .into_response();
                }
            };
            total += count;
        }
        total
    };

    // Get tier limits with actual network-specific sync intervals
    let tier_limits = user_record.subscription_tier.limits_for_api();
    let (personal_sync, team_sync) = user_record
        .subscription_tier
        .get_sync_intervals(&config.network);
    let actual_sync_interval = match user_record.subscription_tier {
        crate::subscription::SubscriptionTier::Personal => personal_sync,
        crate::subscription::SubscriptionTier::Team => team_sync,
    };

    let limits = BillingTierLimits {
        max_wallets: tier_limits.max_wallets.map(|n| n as i32).unwrap_or(-1),
        max_contacts_per_wallet: tier_limits
            .max_contacts_per_wallet
            .map(|n| n as i32)
            .unwrap_or(-1),
        sync_interval_seconds: actual_sync_interval,
    };

    // Check if trial has expired and update status accordingly
    let effective_subscription_status = crate::subscription::get_effective_subscription_status(
        &user_record.subscription_status,
        user_record.trial_ends_at.as_deref(),
    );

    let response = BillingStatusResponse {
        user_id: user.user_id.clone(),
        subscription_tier: user_record.subscription_tier.as_str().to_string(),
        subscription_status: effective_subscription_status,
        trial_ends_at: user_record.trial_ends_at,
        subscription_started_at: user_record.subscription_started_at,
        subscription_ends_at: user_record.subscription_ends_at,
        stripe_customer_id: user_record.stripe_customer_id,
        wallet_count,
        contact_count,
        limits,
    };

    let elapsed = start_time.elapsed();
    info!("get_billing_status completed in {:?}", elapsed);

    (StatusCode::OK, Json(response)).into_response()
}

/// Get pricing information from Stripe (no auth required)
pub async fn get_billing_pricing(State(stripe_billing): State<StripeBillingState>) -> Response {
    // Get Stripe billing from state
    let stripe_billing = match stripe_billing.as_ref() {
        Some(billing) => billing.as_ref(),
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Stripe billing not initialized")),
            )
                .into_response();
        }
    };

    // Get cached pricing information (instant!)
    let pricing = stripe_billing.get_pricing_for_frontend();
    (StatusCode::OK, Json(pricing)).into_response()
}

/// Handle Stripe webhook events (no auth - uses Stripe signature)
pub async fn handle_stripe_webhook(
    State(app_services): State<AppServicesState>,
    State(stripe_billing): State<StripeBillingState>,
    headers: HeaderMap,
    body: String,
) -> Response {
    // Get Stripe signature from headers (case-insensitive lookup)
    let signature = match headers.get("stripe-signature") {
        Some(sig) => match sig.to_str() {
            Ok(s) => s,
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new("Invalid stripe-signature header")),
                )
                    .into_response();
            }
        },
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new("Missing stripe-signature header")),
            )
                .into_response();
        }
    };

    // Get Stripe billing from state
    let stripe_billing = match stripe_billing.as_ref() {
        Some(billing) => billing.as_ref(),
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Stripe billing not initialized")),
            )
                .into_response();
        }
    };

    // Handle the webhook
    tracing::info!("🎣 Processing Stripe webhook");
    match stripe_billing
        .handle_webhook(body.as_bytes(), signature)
        .await
    {
        Ok(webhook_result) => {
            match app_services
                .metadata_db
                .claim_stripe_webhook_event(
                    &webhook_result.event_id,
                    webhook_result.event_created,
                    &webhook_result.event_type,
                )
                .await
            {
                Ok(false) => match app_services
                    .metadata_db
                    .is_stripe_webhook_event_complete(&webhook_result.event_id)
                    .await
                {
                    Ok(true) => {
                        tracing::info!(event_id = %webhook_result.event_id, "Ignoring duplicate Stripe webhook");
                        return (StatusCode::OK, "OK").into_response();
                    }
                    Ok(false) => {
                        tracing::info!(event_id = %webhook_result.event_id, "Stripe webhook is already being processed");
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse::new("Webhook state update in progress")),
                        )
                            .into_response();
                    }
                    Err(e) => {
                        tracing::error!("Failed to inspect Stripe webhook claim: {}", e);
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse::new("Webhook state update failed")),
                        )
                            .into_response();
                    }
                },
                Ok(true) => {}
                Err(e) => {
                    tracing::error!("Failed to check Stripe webhook idempotency: {}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse::new("Webhook state update failed")),
                    )
                        .into_response();
                }
            }

            // Process any subscription updates - no mutex blocking!
            let mut persistence_failed = false;
            for update in webhook_result.subscription_updates {
                tracing::info!(
                    "Processing subscription update for user {}: {} -> {}",
                    update.user_id,
                    update.subscription_tier,
                    update.subscription_status
                );

                // Check if this is a customer ID lookup (from subscription cancellation)
                let actual_user_id = if update.user_id.starts_with("stripe_customer:") {
                    let parts: Vec<&str> = update
                        .user_id
                        .strip_prefix("stripe_customer:")
                        .unwrap()
                        .split(':')
                        .collect();

                    if parts.len() == 2 {
                        // Special case: subscription deletion with customer_id:subscription_id
                        let customer_id = parts[0];
                        let deleted_subscription_id = parts[1];

                        tracing::info!(
                            "Checking subscription deletion for customer {} - subscription {}",
                            customer_id,
                            deleted_subscription_id
                        );

                        // Find user and check if deleted subscription matches current subscription
                        match app_services
                            .metadata_db
                            .get_user_by_stripe_customer_id(customer_id)
                            .await
                        {
                            Ok(Some(user)) => {
                                if let Some(current_subscription_id) = &user.stripe_subscription_id
                                {
                                    if current_subscription_id == deleted_subscription_id {
                                        tracing::info!("📉 Marked user {} as expired (current subscription {} deleted)", user.id, deleted_subscription_id);
                                        user.id
                                    } else {
                                        tracing::info!("🚮 Ignoring deletion of old subscription {} for user {} (current: {})", deleted_subscription_id, user.id, current_subscription_id);
                                        continue; // Skip this update - it's an old subscription
                                    }
                                } else if user.subscription_status == "expired" {
                                    // Retry an earlier deletion webhook whose database expiry succeeded
                                    // but applying wallet/contact limits failed.
                                    tracing::info!(
                                        "🔄 Retrying expiry limits for subscription {} and user {}",
                                        deleted_subscription_id,
                                        user.id
                                    );
                                    user.id
                                } else {
                                    tracing::info!("🚮 Ignoring deletion of subscription {} for user {} (no current subscription)", deleted_subscription_id, user.id);
                                    continue; // Skip this update - user has no current subscription
                                }
                            }
                            Ok(None) => {
                                tracing::warn!(
                                    "No user found for Stripe customer ID: {}",
                                    customer_id
                                );
                                continue; // Skip this update
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Failed to lookup user by customer ID {}: {}",
                                    customer_id,
                                    e
                                );
                                persistence_failed = true;
                                continue; // Skip this update
                            }
                        }
                    } else {
                        // Regular customer ID lookup
                        let customer_id = parts[0];
                        tracing::info!("Looking up user by Stripe customer ID: {}", customer_id);

                        // Find user by Stripe customer ID - no mutex blocking!
                        match app_services
                            .metadata_db
                            .get_user_by_stripe_customer_id(customer_id)
                            .await
                        {
                            Ok(Some(user)) => {
                                tracing::info!(
                                    "Found user {} for customer {}",
                                    user.id,
                                    customer_id
                                );
                                user.id
                            }
                            Ok(None) => {
                                tracing::warn!(
                                    "No user found for Stripe customer ID: {}",
                                    customer_id
                                );
                                continue; // Skip this update
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Failed to lookup user by customer ID {}: {}",
                                    customer_id,
                                    e
                                );
                                persistence_failed = true;
                                continue; // Skip this update
                            }
                        }
                    }
                } else {
                    update.user_id.clone()
                };

                // Handle special "keep_current" tier for cancellations
                if update.subscription_tier == "keep_current" {
                    let update_result = if update.subscription_status == "expired"
                        && update.stripe_subscription_id.is_none()
                    {
                        app_services
                            .metadata_db
                            .expire_user_subscription_for_stripe_event(
                                &actual_user_id,
                                webhook_result.event_created,
                            )
                            .await
                    } else {
                        app_services
                            .metadata_db
                            .update_user_subscription_status_for_stripe_event(
                                &actual_user_id,
                                &update.subscription_status,
                                update.stripe_subscription_id.as_deref(),
                                webhook_result.event_created,
                            )
                            .await
                    };
                    match update_result {
                        Ok(false) => {
                            tracing::info!(
                                user_id = %actual_user_id,
                                event_id = %webhook_result.event_id,
                                "Ignoring stale Stripe webhook"
                            );
                            continue;
                        }
                        Err(e) => {
                            tracing::error!(
                                "Failed to update user {} subscription status: {}",
                                actual_user_id,
                                e
                            );
                            persistence_failed = true;
                        }
                        Ok(true) => {
                            tracing::info!(
                            "✅ Updated user {} subscription status to {} (keeping current tier)",
                            actual_user_id,
                            update.subscription_status
                        );

                            match app_services
                                .metadata_db
                                .get_user_by_id(&actual_user_id)
                                .await
                            {
                                Ok(Some(user_record)) => {
                                    if let Err(e) = app_services
                                        .apply_subscription_limits(
                                            &actual_user_id,
                                            user_record.subscription_tier.as_str(),
                                            &update.subscription_status,
                                            user_record.is_admin,
                                            user_record.trial_ends_at.clone(),
                                            user_record.subscription_ends_at.clone(),
                                        )
                                        .await
                                    {
                                        tracing::error!(
                                            "Failed to apply subscription limits for user {}: {}",
                                            actual_user_id,
                                            e
                                        );
                                        persistence_failed = true;
                                    }
                                }
                                Ok(None) => {
                                    tracing::error!(
                                        "User {} not found when applying limits",
                                        actual_user_id
                                    );
                                    persistence_failed = true;
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Failed to fetch user {} when applying limits: {}",
                                        actual_user_id,
                                        e
                                    );
                                    persistence_failed = true;
                                }
                            }
                        }
                    }
                } else {
                    // Regular subscription update (tier + status) - no mutex blocking!
                    let sub_params = SubscriptionUpdateParams {
                        subscription_tier: &update.subscription_tier,
                        subscription_status: &update.subscription_status,
                        stripe_subscription_id: update.stripe_subscription_id.as_deref(),
                        subscription_started_at: update.subscription_started_at.as_deref(),
                        subscription_ends_at: update.subscription_ends_at.as_deref(),
                        trial_ends_at: update.trial_ends_at.as_deref(),
                    };
                    match app_services
                        .metadata_db
                        .update_user_subscription_for_stripe_event(
                            &actual_user_id,
                            &sub_params,
                            webhook_result.event_created,
                        )
                        .await
                    {
                        Ok(false) => {
                            tracing::info!(
                                user_id = %actual_user_id,
                                event_id = %webhook_result.event_id,
                                "Ignoring stale Stripe webhook"
                            );
                            continue;
                        }
                        Err(e) => {
                            tracing::error!(
                                "Failed to update user {} subscription: {}",
                                actual_user_id,
                                e
                            );
                            persistence_failed = true;
                        }
                        Ok(true) => {
                            tracing::info!(
                                "✅ Updated user {} subscription to {} ({})",
                                actual_user_id,
                                update.subscription_tier,
                                update.subscription_status
                            );

                            // Apply subscription tier limits to wallets and contacts
                            // First, get user record to check admin status - no mutex blocking!
                            match app_services
                                .metadata_db
                                .get_user_by_id(&actual_user_id)
                                .await
                            {
                                Ok(Some(user_record)) => {
                                    if let Err(e) = app_services
                                        .apply_subscription_limits(
                                            &actual_user_id,
                                            &update.subscription_tier,
                                            &update.subscription_status,
                                            user_record.is_admin,
                                            user_record.trial_ends_at.clone(),
                                            user_record.subscription_ends_at.clone(),
                                        )
                                        .await
                                    {
                                        tracing::error!(
                                            "Failed to apply subscription limits for user {}: {}",
                                            actual_user_id,
                                            e
                                        );
                                        persistence_failed = true;
                                    } else if user_record.is_admin {
                                        tracing::info!(
                                            "✅ Applied unlimited limits for admin user {}",
                                            actual_user_id
                                        );
                                    } else {
                                        tracing::info!(
                                            "✅ Applied {} tier limits for user {}",
                                            update.subscription_tier,
                                            actual_user_id
                                        );
                                    }
                                }
                                Ok(None) => {
                                    tracing::error!(
                                        "User {} not found when applying limits",
                                        actual_user_id
                                    );
                                    persistence_failed = true;
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Failed to fetch user {} when applying limits: {}",
                                        actual_user_id,
                                        e
                                    );
                                    persistence_failed = true;
                                }
                            }
                        }
                    }
                }
            }

            if persistence_failed {
                tracing::error!("Webhook state update failed; returning an error for Stripe retry");
                if let Err(e) = app_services
                    .metadata_db
                    .fail_stripe_webhook_event(&webhook_result.event_id)
                    .await
                {
                    tracing::error!("Failed to release Stripe webhook claim: {}", e);
                }
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("Webhook state update failed")),
                )
                    .into_response();
            }

            if let Err(e) = app_services
                .metadata_db
                .complete_stripe_webhook_event(&webhook_result.event_id)
                .await
            {
                tracing::error!("Failed to record processed Stripe webhook: {}", e);
                if let Err(e) = app_services
                    .metadata_db
                    .fail_stripe_webhook_event(&webhook_result.event_id)
                    .await
                {
                    tracing::error!("Failed to release Stripe webhook claim: {}", e);
                }
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("Webhook state update failed")),
                )
                    .into_response();
            }

            tracing::info!("✅ Webhook processed successfully");
            (StatusCode::OK, "OK").into_response()
        }
        Err(e) => {
            tracing::error!("❌ Webhook processing failed: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(format!(
                    "Webhook processing failed: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}

/// Get checkout session details
pub async fn get_checkout_session_details(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(stripe_billing): State<StripeBillingState>,
    Path(session_id): Path<String>,
) -> Response {
    // Get Stripe billing from state
    let stripe_billing = match stripe_billing.as_ref() {
        Some(billing) => billing.as_ref(),
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Stripe billing not initialized")),
            )
                .into_response();
        }
    };

    // Get session details from Stripe
    match stripe_billing
        .get_checkout_session_details(&session_id)
        .await
    {
        Ok(details) => {
            let owns_session = match details.customer_id.as_deref() {
                Some(customer_id) => {
                    match app_services.metadata_db.get_user_by_id(&user.user_id).await {
                        Ok(Some(current_user)) => {
                            current_user.stripe_customer_id.as_deref() == Some(customer_id)
                        }
                        _ => false,
                    }
                }
                None => false,
            };
            if owns_session {
                (StatusCode::OK, Json(details)).into_response()
            } else {
                (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse::new("Session not found")),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(format!("Session not found: {}", e))),
        )
            .into_response(),
    }
}
