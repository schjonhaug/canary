//! Billing and subscription management handlers

use crate::api::{AppServicesState, BtcPayClientState, ConfigState, StripeBillingState};
use crate::config::BillingProvider;
use crate::extractors::AuthenticatedUser;
use crate::handlers::helpers::{get_user_or_error, DatabaseErrorMessage};
use crate::metadata::{BtcPayEventApplyResult, BtcPaySubscriptionEventParams, StripeEventClaim};
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
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

fn active_billing_provider(
    config: &crate::config::AppConfig,
    stripe_billing: &StripeBillingState,
    btcpay: &BtcPayClientState,
) -> Option<BillingProvider> {
    match config.active_billing_provider() {
        Some(BillingProvider::Stripe) if stripe_billing.is_some() => Some(BillingProvider::Stripe),
        Some(BillingProvider::BtcPay) if btcpay.is_some() => Some(BillingProvider::BtcPay),
        _ => None,
    }
}

/// Create a billing checkout session for subscription
pub async fn create_checkout_session(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(stripe_billing): State<StripeBillingState>,
    State(btcpay): State<BtcPayClientState>,
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

    let result = match active_billing_provider(&config, &stripe_billing, &btcpay) {
        Some(BillingProvider::Stripe) => {
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

            stripe_billing
                .create_checkout_session(
                    &user_record.id,
                    tier,
                    billing_cycle,
                    &success_url,
                    &cancel_url,
                    &app_services.metadata_db,
                )
                .await
        }
        Some(BillingProvider::BtcPay) => {
            if is_yearly {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::coded(
                        "unsupported_billing_period",
                        "BTCPay cloud billing currently supports monthly plans only",
                    )),
                )
                    .into_response();
            }

            let btcpay = match btcpay.as_ref() {
                Some(client) => client.as_ref(),
                None => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse::new("BTCPay billing not initialized")),
                    )
                        .into_response();
                }
            };

            let checkout_token = Uuid::new_v4().to_string();
            if let Err(error) = app_services
                .metadata_db
                .create_pending_billing_checkout(
                    &checkout_token,
                    &user_record.id,
                    "btcpay",
                    tier.as_str(),
                    billing_cycle,
                )
                .await
            {
                tracing::error!(%error, "Failed to persist BTCPay checkout correlation");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("Failed to initialize checkout session")),
                )
                    .into_response();
            }

            let redirect_url = format!(
                "{}/subscription?success=true&provider=btcpay&session={}",
                frontend_url, checkout_token
            );

            btcpay
                .create_cloud_subscription_checkout(
                    tier,
                    &redirect_url,
                    &checkout_token,
                    &user_record.email,
                )
                .await
                .map(|url| crate::stripe_billing::CheckoutSessionResponse {
                    url,
                    session_id: checkout_token,
                })
        }
        None => Err(anyhow::anyhow!("No billing provider configured")),
    };

    let elapsed = start_time.elapsed();
    info!("create_checkout_session completed in {:?}", elapsed);

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

/// Create a customer billing management session when the provider supports it
pub async fn create_customer_portal(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(stripe_billing): State<StripeBillingState>,
    State(_config): State<ConfigState>,
    Json(payload): Json<CreateCustomerPortalRequest>,
) -> Response {
    let start_time = std::time::Instant::now();

    if stripe_billing.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::coded(
                "billing_management_unavailable",
                "The active billing provider does not support in-app subscription management yet",
            )),
        )
            .into_response();
    }

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
    State(stripe_billing): State<StripeBillingState>,
    State(btcpay): State<BtcPayClientState>,
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
        billing_provider: active_billing_provider(&config, &stripe_billing, &btcpay)
            .map(|provider| provider.as_str().to_string()),
        can_manage_billing: stripe_billing.is_some() && user_record.stripe_customer_id.is_some(),
        stripe_customer_id: user_record.stripe_customer_id,
        wallet_count,
        contact_count,
        limits,
    };

    let elapsed = start_time.elapsed();
    info!("get_billing_status completed in {:?}", elapsed);

    (StatusCode::OK, Json(response)).into_response()
}

/// Get pricing information from the active billing provider (no auth required)
pub async fn get_billing_pricing(
    State(stripe_billing): State<StripeBillingState>,
    State(btcpay): State<BtcPayClientState>,
    State(config): State<ConfigState>,
) -> Response {
    let pricing = match active_billing_provider(&config, &stripe_billing, &btcpay) {
        Some(BillingProvider::Stripe) => {
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
            stripe_billing.get_pricing_for_frontend()
        }
        Some(BillingProvider::BtcPay) => {
            let btcpay = match btcpay.as_ref() {
                Some(client) => client.as_ref(),
                None => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse::new("BTCPay billing not initialized")),
                    )
                        .into_response();
                }
            };

            match btcpay.get_cloud_pricing_for_frontend() {
                Ok(pricing) => pricing,
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse::new(format!(
                            "Failed to load BTCPay pricing: {}",
                            e
                        ))),
                    )
                        .into_response();
                }
            }
        }
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse::new("No billing provider configured")),
            )
                .into_response();
        }
    };
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
    let verified_event = match stripe_billing
        .verify_webhook_event(body.as_bytes(), signature)
        .await
    {
        Ok(event_id) => event_id,
        Err(e) => {
            tracing::error!("❌ Webhook verification failed: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(format!(
                    "Webhook processing failed: {}",
                    e
                ))),
            )
                .into_response();
        }
    };

    let event_id = &verified_event.event_id;
    let claim_token = match app_services.metadata_db.claim_stripe_event(event_id).await {
        Ok(StripeEventClaim::Claimed(claim_token)) => claim_token,
        Ok(StripeEventClaim::Processed) => {
            tracing::info!(event_id = %event_id, "Ignoring duplicate Stripe webhook");
            if let Err(e) = stripe_billing
                .deliver_pending_trial_ending_notifications(event_id)
                .await
            {
                tracing::error!("Failed to deliver pending trial ending notification: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("Webhook notification delivery failed")),
                )
                    .into_response();
            }
            return (StatusCode::OK, "OK").into_response();
        }
        Ok(StripeEventClaim::Active) => {
            tracing::info!(event_id = %event_id, "Stripe webhook is already being processed");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Webhook is already being processed")),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("Failed to claim Stripe webhook event: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Webhook state update failed")),
            )
                .into_response();
        }
    };

    // Stripe API work can outlive the claim lease, so renew it until that work finishes.
    let lease_db = app_services.metadata_db.clone();
    let lease_event_id = event_id.clone();
    let lease_claim_token = claim_token.clone();
    let lease_refresh = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        interval.tick().await;
        loop {
            interval.tick().await;
            match lease_db
                .refresh_stripe_event_claim(&lease_event_id, &lease_claim_token)
                .await
            {
                Ok(true) => {}
                Ok(false) => {
                    tracing::warn!(event_id = %lease_event_id, "Stripe webhook claim was lost during processing");
                    break;
                }
                Err(error) => {
                    tracing::error!(event_id = %lease_event_id, "Failed to refresh Stripe webhook claim: {error}");
                }
            }
        }
    });

    let webhook_result = match stripe_billing
        .handle_webhook(body.as_bytes(), signature)
        .await
    {
        Err(e) => {
            lease_refresh.abort();
            if let Err(release_error) = app_services
                .metadata_db
                .release_stripe_event(event_id, &claim_token)
                .await
            {
                tracing::error!(
                    "Failed to release Stripe webhook event for retry: {}",
                    release_error
                );
            }
            tracing::error!("❌ Webhook processing failed: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(format!(
                    "Webhook processing failed: {}",
                    e
                ))),
            )
                .into_response();
        }
        Ok(webhook_result) => webhook_result,
    };
    let mut subscription_updates = Vec::with_capacity(webhook_result.subscription_updates.len());
    for mut update in webhook_result.subscription_updates {
        if update.stripe_subscription_id.is_some() {
            match stripe_billing.reconcile_subscription_update(&update).await {
                Ok(reconciled) => update = reconciled,
                Err(error) => {
                    lease_refresh.abort();
                    tracing::error!("Failed to reconcile Stripe subscription state: {error}");
                    let _ = app_services
                        .metadata_db
                        .release_stripe_event(event_id, &claim_token)
                        .await;
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse::new("Webhook state update failed")),
                    )
                        .into_response();
                }
            }
        }
        let deleted_subscription_id = update
            .user_id
            .strip_prefix("stripe_customer:")
            .and_then(|customer_id| customer_id.split_once(':'))
            .map(|(_, subscription_id)| subscription_id.to_string());
        let user = if let Some(customer_id) = update.user_id.strip_prefix("stripe_customer:") {
            let mut parts = customer_id.split(':');
            let customer_id = parts.next().expect("prefix leaves customer ID");
            let deleted_subscription_id = parts.next();
            match app_services
                .metadata_db
                .get_user_by_stripe_customer_id(customer_id)
                .await
            {
                Ok(Some(user)) if deleted_subscription_id.is_none() => user,
                Ok(Some(user))
                    if user.stripe_subscription_id.as_deref() == deleted_subscription_id =>
                {
                    user
                }
                Ok(Some(_)) | Ok(None) => continue,
                Err(e) => {
                    lease_refresh.abort();
                    tracing::error!("Failed to look up Stripe customer {}: {}", customer_id, e);
                    let _ = app_services
                        .metadata_db
                        .release_stripe_event(event_id, &claim_token)
                        .await;
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse::new("Webhook state update failed")),
                    )
                        .into_response();
                }
            }
        } else {
            match app_services
                .metadata_db
                .get_user_by_id(&update.user_id)
                .await
            {
                Ok(Some(user)) => user,
                Ok(None) => continue,
                Err(e) => {
                    lease_refresh.abort();
                    tracing::error!("Failed to look up user {}: {}", update.user_id, e);
                    let _ = app_services
                        .metadata_db
                        .release_stripe_event(event_id, &claim_token)
                        .await;
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse::new("Webhook state update failed")),
                    )
                        .into_response();
                }
            }
        };

        if let Some(deleted_subscription_id) = deleted_subscription_id {
            update.stripe_subscription_id = Some(deleted_subscription_id);
        }
        update.user_id = user.id;
        subscription_updates.push(update);
    }

    match app_services
        .metadata_db
        .complete_stripe_event_with_subscription_updates(
            event_id,
            &claim_token,
            verified_event.event_created,
            &subscription_updates,
            &webhook_result.trial_ending_notifications,
        )
        .await
    {
        Ok(true) => {
            lease_refresh.abort();
            if let Err(e) = stripe_billing
                .deliver_pending_trial_ending_notifications(event_id)
                .await
            {
                tracing::error!("Failed to deliver trial ending notification: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("Webhook notification delivery failed")),
                )
                    .into_response();
            }
            tracing::info!("✅ Webhook processed successfully");
            (StatusCode::OK, "OK").into_response()
        }
        Ok(false) => {
            lease_refresh.abort();
            tracing::error!("Stripe webhook claim was lost before completion");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Webhook state update failed")),
            )
                .into_response()
        }
        Err(e) => {
            lease_refresh.abort();
            tracing::error!("Failed to atomically persist Stripe webhook state: {}", e);
            let _ = app_services
                .metadata_db
                .release_stripe_event(event_id, &claim_token)
                .await;
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Webhook state update failed")),
            )
                .into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BtcPayWebhookPayload {
    delivery_id: String,
    timestamp: i64,
    #[serde(rename = "type")]
    event_type: String,
    store_id: String,
    subscriber: Option<BtcPayWebhookSubscriber>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BtcPayWebhookSubscriber {
    is_active: Option<bool>,
    period_end: Option<i64>,
    metadata: Option<serde_json::Value>,
    customer: Option<BtcPayWebhookCustomer>,
    plan: Option<BtcPayWebhookPlan>,
}

#[derive(Debug, Deserialize)]
struct BtcPayWebhookCustomer {
    id: String,
}

#[derive(Debug, Deserialize)]
struct BtcPayWebhookPlan {
    id: String,
}

fn verify_btcpay_webhook_signature(secret: &str, body: &[u8], signature: &str) -> bool {
    let Some(encoded_signature) = signature.strip_prefix("sha256=") else {
        return false;
    };
    let Ok(signature_bytes) = hex::decode(encoded_signature) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    mac.verify_slice(&signature_bytes).is_ok()
}

fn btcpay_subscription_status(event_type: &str, is_active: Option<bool>) -> Option<&'static str> {
    match event_type {
        "PlanStarted" | "SubscriberActivated" | "SubscriberCharged" => Some("active"),
        "SubscriberPhaseChanged" if is_active == Some(true) => Some("active"),
        "SubscriberNeedUpgrade" => Some("past_due"),
        "SubscriberDisabled" => Some("expired"),
        _ => None,
    }
}

fn btcpay_event_priority(event_type: &str) -> i64 {
    match event_type {
        "SubscriberDisabled" => 3,
        "SubscriberNeedUpgrade" => 2,
        _ => 1,
    }
}

/// Process signed BTCPay subscription events and reconcile them with Canary users.
pub async fn handle_btcpay_webhook(
    State(app_services): State<AppServicesState>,
    State(btcpay): State<BtcPayClientState>,
    State(config): State<ConfigState>,
    headers: HeaderMap,
    body: String,
) -> Response {
    let Some(secret) = config.btcpay_webhook_secret() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("BTCPay webhook is not configured")),
        )
            .into_response();
    };
    let Some(signature) = headers
        .get("btcpay-sig")
        .and_then(|value| value.to_str().ok())
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Missing BTCPay webhook signature")),
        )
            .into_response();
    };
    if !verify_btcpay_webhook_signature(&secret, body.as_bytes(), signature) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Invalid BTCPay webhook signature")),
        )
            .into_response();
    }

    let payload: BtcPayWebhookPayload = match serde_json::from_str(&body) {
        Ok(payload) => payload,
        Err(error) => {
            tracing::warn!(%error, "Invalid BTCPay webhook payload");
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new("Invalid BTCPay webhook payload")),
            )
                .into_response();
        }
    };
    if config.btcpay_store_id() != Some(payload.store_id.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("BTCPay webhook store does not match")),
        )
            .into_response();
    }

    let new_status = btcpay_subscription_status(
        &payload.event_type,
        payload
            .subscriber
            .as_ref()
            .and_then(|subscriber| subscriber.is_active),
    );
    let Some(new_status) = new_status else {
        tracing::debug!(
            event_type = %payload.event_type,
            delivery_id = %payload.delivery_id,
            "Ignoring non-authoritative BTCPay subscription event"
        );
        return (StatusCode::OK, "OK").into_response();
    };

    let Some(subscriber) = payload.subscriber else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("BTCPay webhook subscriber is missing")),
        )
            .into_response();
    };
    let checkout_token = subscriber
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("canaryCheckoutToken"))
        .and_then(serde_json::Value::as_str);
    let Some(checkout_token) = checkout_token else {
        tracing::debug!(
            delivery_id = %payload.delivery_id,
            "Ignoring BTCPay subscriber not created by Canary"
        );
        return (StatusCode::OK, "OK").into_response();
    };

    let checkout = match app_services
        .metadata_db
        .get_billing_checkout_by_token(checkout_token)
        .await
    {
        Ok(Some(checkout)) if checkout.provider == "btcpay" => checkout,
        Ok(_) => {
            tracing::warn!(
                delivery_id = %payload.delivery_id,
                "BTCPay webhook referenced an unknown checkout"
            );
            return (StatusCode::OK, "OK").into_response();
        }
        Err(error) => {
            tracing::error!(%error, "Failed to look up BTCPay checkout");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("BTCPay webhook state lookup failed")),
            )
                .into_response();
        }
    };

    let Some(client) = btcpay.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("BTCPay billing is not initialized")),
        )
            .into_response();
    };
    let Some(plan_id) = subscriber.plan.as_ref().map(|plan| plan.id.as_str()) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("BTCPay webhook plan is missing")),
        )
            .into_response();
    };
    let Some(customer_id) = subscriber
        .customer
        .as_ref()
        .map(|customer| customer.id.as_str())
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("BTCPay webhook customer is missing")),
        )
            .into_response();
    };
    let Some(tier) = client.cloud_plan_tier_from_plan_id(plan_id) else {
        tracing::warn!(plan_id, "Ignoring BTCPay webhook for an unknown cloud plan");
        return (StatusCode::OK, "OK").into_response();
    };
    if checkout.subscription_tier != tier.as_str() {
        tracing::warn!(
            delivery_id = %payload.delivery_id,
            "BTCPay webhook plan does not match its checkout"
        );
        return (StatusCode::OK, "OK").into_response();
    }

    let Some(event_time) = chrono::DateTime::from_timestamp(payload.timestamp, 0) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("BTCPay webhook timestamp is invalid")),
        )
            .into_response();
    };
    let started_at = event_time.to_rfc3339();
    let subscription_ends_at = match subscriber.period_end {
        Some(timestamp) => match chrono::DateTime::from_timestamp(timestamp, 0) {
            Some(date) => Some(date.to_rfc3339()),
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new("BTCPay subscription period is invalid")),
                )
                    .into_response();
            }
        },
        None => None,
    };
    let apply_result = match app_services
        .metadata_db
        .apply_btcpay_subscription_event(&BtcPaySubscriptionEventParams {
            delivery_id: &payload.delivery_id,
            event_type: &payload.event_type,
            event_timestamp: payload.timestamp,
            event_priority: btcpay_event_priority(&payload.event_type),
            checkout_token,
            customer_id,
            plan_id,
            subscription_tier: tier.as_str(),
            subscription_status: new_status,
            subscription_started_at: Some(&started_at),
            subscription_ends_at: subscription_ends_at.as_deref(),
        })
        .await
    {
        Ok(result) => result,
        Err(error) => {
            tracing::error!(%error, "Failed to atomically apply BTCPay subscription update");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("BTCPay subscription update failed")),
            )
                .into_response();
        }
    };

    match apply_result {
        BtcPayEventApplyResult::Stale
        | BtcPayEventApplyResult::Superseded
        | BtcPayEventApplyResult::CheckoutNotFound => {
            tracing::info!(
                delivery_id = %payload.delivery_id,
                result = ?apply_result,
                "Ignored non-current BTCPay subscription webhook"
            );
            return (StatusCode::OK, "OK").into_response();
        }
        BtcPayEventApplyResult::Applied | BtcPayEventApplyResult::Duplicate => {}
    }

    let updated_user = match app_services
        .metadata_db
        .get_user_by_id(&checkout.user_id)
        .await
    {
        Ok(Some(user)) => user,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("BTCPay checkout user not found")),
            )
                .into_response();
        }
        Err(error) => {
            tracing::error!(%error, "Failed to reload BTCPay checkout user");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("BTCPay subscription user lookup failed")),
            )
                .into_response();
        }
    };
    if let Err(error) = app_services
        .apply_subscription_limits(
            &checkout.user_id,
            updated_user.subscription_tier.as_str(),
            &updated_user.subscription_status,
            updated_user.is_admin,
            updated_user.trial_ends_at,
            updated_user.subscription_ends_at,
        )
        .await
    {
        tracing::error!(%error, "Failed to apply BTCPay subscription limits");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(
                "BTCPay subscription limit update failed",
            )),
        )
            .into_response();
    }

    tracing::info!(
        delivery_id = %payload.delivery_id,
        user_id = %checkout.user_id,
        status = new_status,
        result = ?apply_result,
        "Processed BTCPay subscription webhook"
    );
    (StatusCode::OK, "OK").into_response()
}

/// Get checkout session details
pub async fn get_checkout_session_details(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(stripe_billing): State<StripeBillingState>,
    State(btcpay): State<BtcPayClientState>,
    State(config): State<ConfigState>,
    Path(session_id): Path<String>,
) -> Response {
    if active_billing_provider(&config, &stripe_billing, &btcpay) == Some(BillingProvider::BtcPay) {
        let checkout = match app_services
            .metadata_db
            .get_pending_billing_checkout(&session_id)
            .await
        {
            Ok(Some(checkout))
                if checkout.user_id == user.user_id && checkout.provider == "btcpay" =>
            {
                checkout
            }
            Ok(_) => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse::new("Session not found")),
                )
                    .into_response();
            }
            Err(error) => {
                tracing::error!(%error, "Failed to load BTCPay checkout session");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("Failed to load checkout session")),
                )
                    .into_response();
            }
        };

        let details = crate::stripe_billing::CheckoutSessionDetails {
            session_id,
            customer_id: None,
            subscription_id: None,
            status: Some(if checkout.completed_at.is_some() {
                "complete".to_string()
            } else {
                "pending".to_string()
            }),
            tier: Some(checkout.subscription_tier),
            billing_period: Some(checkout.billing_period),
            amount_total: None,
            currency: None,
        };
        return (StatusCode::OK, Json(details)).into_response();
    }

    if active_billing_provider(&config, &stripe_billing, &btcpay) != Some(BillingProvider::Stripe) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::coded(
                "checkout_session_unsupported",
                "Checkout session lookups are only available for Stripe",
            )),
        )
            .into_response();
    }

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

pub async fn create_stripe_checkout_session(
    authenticated_user: AuthenticatedUser,
    app_services: State<AppServicesState>,
    stripe_billing: State<StripeBillingState>,
    btcpay: State<BtcPayClientState>,
    config: State<ConfigState>,
    payload: Json<CreateCheckoutSessionRequest>,
) -> Response {
    create_checkout_session(
        authenticated_user,
        app_services,
        stripe_billing,
        btcpay,
        config,
        payload,
    )
    .await
}

pub async fn create_stripe_customer_portal(
    authenticated_user: AuthenticatedUser,
    app_services: State<AppServicesState>,
    stripe_billing: State<StripeBillingState>,
    config: State<ConfigState>,
    payload: Json<CreateCustomerPortalRequest>,
) -> Response {
    create_customer_portal(
        authenticated_user,
        app_services,
        stripe_billing,
        config,
        payload,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_btcpay_signature_using_the_raw_body() {
        let signature = "sha256=f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8";
        assert!(verify_btcpay_webhook_signature(
            "key",
            b"The quick brown fox jumps over the lazy dog",
            signature
        ));
        assert!(!verify_btcpay_webhook_signature(
            "key",
            b"The quick brown fox jumps over the lazy cat",
            signature
        ));
        assert!(!verify_btcpay_webhook_signature(
            "key",
            b"The quick brown fox jumps over the lazy dog",
            "f7bc83f4"
        ));
    }

    #[test]
    fn maps_only_authoritative_btcpay_subscription_events() {
        assert_eq!(
            btcpay_subscription_status("PlanStarted", Some(true)),
            Some("active")
        );
        assert_eq!(
            btcpay_subscription_status("SubscriberPhaseChanged", Some(true)),
            Some("active")
        );
        assert_eq!(
            btcpay_subscription_status("SubscriberPhaseChanged", Some(false)),
            None
        );
        assert_eq!(
            btcpay_subscription_status("SubscriberNeedUpgrade", Some(true)),
            Some("past_due")
        );
        assert_eq!(
            btcpay_subscription_status("SubscriberDisabled", Some(false)),
            Some("expired")
        );
        assert_eq!(
            btcpay_subscription_status("SubscriberCreated", Some(false)),
            None
        );
    }
}
