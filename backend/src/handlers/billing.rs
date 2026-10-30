//! Billing and subscription management handlers

use crate::api::{AppServicesState, BtcPayClientState, ConfigState, StripeBillingState};
use crate::btcpay_client::BtcPayClient;
use crate::config::BillingProvider;
use crate::extractors::AuthenticatedUser;
use crate::handlers::helpers::get_user_or_error;
use crate::handlers::{ApiError, ApiResult, OrInternal};
use crate::metadata::{
    BtcPayEventApplyResult, BtcPaySubscriptionEventParams, MetadataDb, PendingBillingCheckout,
    StripeEventClaim,
};
use crate::models::{
    BillingStatusResponse, BillingTierLimits, CreateCheckoutSessionRequest,
    CreateCustomerPortalRequest,
};
use crate::stripe_billing::{
    CheckoutSessionDetails, CheckoutSessionResponse, CustomerPortalResponse, PricingInfo,
    StripeBilling, SubscriptionUpdate, VerifiedWebhookEvent,
};
use crate::subscription::SubscriptionTier;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use sha2::Sha256;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::info;
use uuid::Uuid;

/// Body Stripe and `BTCPay` expect on a webhook they should not retry.
const WEBHOOK_ACK: &str = "OK";
const WEBHOOK_STATE_UPDATE_FAILED: &str = "Webhook state update failed";

/// A 500 with a fixed message whose cause is not logged here.
///
fn bad_request(message: impl Into<std::borrow::Cow<'static, str>>) -> ApiError {
    ApiError::uncoded(StatusCode::BAD_REQUEST, message)
}

fn service_unavailable(message: &'static str) -> ApiError {
    ApiError::uncoded(StatusCode::SERVICE_UNAVAILABLE, message)
}

fn stripe_client(stripe_billing: &StripeBillingState) -> ApiResult<&StripeBilling> {
    stripe_billing
        .as_deref()
        .ok_or_else(|| ApiError::opaque("Stripe billing not initialized"))
}

fn btcpay_client(btcpay: &BtcPayClientState) -> ApiResult<&BtcPayClient> {
    btcpay
        .as_deref()
        .ok_or_else(|| ApiError::opaque("BTCPay billing not initialized"))
}

async fn current_user_record(
    app_services: &AppServicesState,
    user_id: &str,
) -> ApiResult<crate::metadata::UserRecord> {
    get_user_or_error(
        app_services,
        user_id,
        None,
        "User not found",
        "Database error",
    )
    .await
}

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

fn btcpay_checkout_allowed(subscription_status: &str) -> bool {
    !matches!(subscription_status, "active" | "past_due" | "canceled")
}

/// Create a billing checkout session for subscription
pub async fn create_checkout_session(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(stripe_billing): State<StripeBillingState>,
    State(btcpay): State<BtcPayClientState>,
    State(config): State<ConfigState>,
    Json(payload): Json<CreateCheckoutSessionRequest>,
) -> ApiResult<Json<CheckoutSessionResponse>> {
    let start_time = std::time::Instant::now();

    let tier = match payload.tier.as_str() {
        "personal" => SubscriptionTier::Personal,
        "team" => SubscriptionTier::Team,
        _ => {
            return Err(ApiError::bad_request(
                "invalid_subscription_tier",
                "Invalid subscription tier",
            ));
        }
    };

    let user_record = current_user_record(&app_services, &user.user_id).await?;

    let is_yearly = payload.is_yearly.unwrap_or(false);
    let billing_cycle = if is_yearly { "yearly" } else { "monthly" };
    let frontend_url = config
        .frontend_url()
        .ok_or_else(|| ApiError::opaque("FRONTEND_URL not configured"))?;
    let success_url =
        format!("{frontend_url}/subscription?success=true&session={{CHECKOUT_SESSION_ID}}");
    let cancel_url = format!("{frontend_url}/subscription?cancelled=true");

    let result = match active_billing_provider(&config, &stripe_billing, &btcpay) {
        Some(BillingProvider::Stripe) => {
            stripe_client(&stripe_billing)?
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
            if !btcpay_checkout_allowed(&user_record.subscription_status) {
                return Err(ApiError::conflict(
                    "billing_management_unavailable",
                    "Manage the existing BTCPay subscription before starting a new checkout",
                ));
            }
            if is_yearly {
                return Err(ApiError::bad_request(
                    "unsupported_billing_period",
                    "BTCPay cloud billing currently supports monthly plans only",
                ));
            }
            let btcpay = btcpay_client(&btcpay)?;

            let checkout_token = Uuid::new_v4().to_string();
            app_services
                .metadata_db
                .create_pending_billing_checkout(
                    &checkout_token,
                    &user_record.id,
                    "btcpay",
                    tier.as_str(),
                    billing_cycle,
                )
                .await
                .map_err(|_| {
                    tracing::error!("Failed to persist checkout correlation");
                    ApiError::opaque("Failed to initialize checkout session")
                })?;

            let redirect_url = format!(
                "{frontend_url}/subscription?success=true&provider=btcpay&session={checkout_token}"
            );

            btcpay
                .create_cloud_subscription_checkout(
                    tier,
                    &redirect_url,
                    &checkout_token,
                    &user_record.email,
                )
                .await
                .map(|url| CheckoutSessionResponse {
                    url,
                    session_id: checkout_token,
                })
        }
        None => Err(anyhow::anyhow!("No billing provider configured")),
    };

    info!(
        "create_checkout_session completed in {:?}",
        start_time.elapsed()
    );

    result.map(Json).map_err(|_| {
        tracing::error!("Checkout session creation failed");
        ApiError::opaque("Failed to create checkout session")
    })
}

/// Create a customer billing management session when the provider supports it
pub async fn create_customer_portal(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(stripe_billing): State<StripeBillingState>,
    State(config): State<ConfigState>,
    Json(payload): Json<CreateCustomerPortalRequest>,
) -> ApiResult<Json<CustomerPortalResponse>> {
    let start_time = std::time::Instant::now();

    if config.active_billing_provider() != Some(BillingProvider::Stripe) || stripe_billing.is_none()
    {
        return Err(ApiError::bad_request(
            "billing_management_unavailable",
            "The active billing provider does not support in-app subscription management yet",
        ));
    }

    let user_record = current_user_record(&app_services, &user.user_id).await?;

    let Some(customer_id) = &user_record.stripe_customer_id else {
        return Err(ApiError::bad_request(
            "no_stripe_customer",
            "No Stripe customer found. Please create a subscription first.",
        ));
    };

    let stripe_billing = stripe_client(&stripe_billing)?;

    tracing::info!("Creating customer portal session");
    let result = stripe_billing
        .create_customer_portal_session(customer_id, &payload.return_url)
        .await;
    if result.is_ok() {
        tracing::info!("Customer portal session created");
    } else {
        tracing::error!("Customer portal session creation failed");
    }
    info!(
        "create_stripe_customer_portal completed in {:?}",
        start_time.elapsed()
    );

    result
        .map(Json)
        .map_err(|_| ApiError::opaque("Failed to create customer portal session"))
}

/// Get billing status for the authenticated user
pub async fn get_billing_status(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(stripe_billing): State<StripeBillingState>,
    State(btcpay): State<BtcPayClientState>,
    State(config): State<Arc<crate::config::AppConfig>>,
) -> ApiResult<Json<BillingStatusResponse>> {
    let start_time = std::time::Instant::now();

    let user_record = current_user_record(&app_services, &user.user_id).await?;

    let wallet_count = app_services
        .metadata_db
        .count_wallets_for_user(&user.user_id)
        .await
        .or_internal("Database error")?;

    let user_wallets = app_services
        .metadata_db
        .get_wallets_for_user(Some(&user.user_id))
        .await
        .or_internal("Database error")?;
    let mut contact_count = 0;
    for wallet in &user_wallets {
        contact_count += app_services
            .metadata_db
            .count_contacts_for_wallet(&wallet.checksum)
            .await
            .or_internal("Database error")?;
    }

    // Report the network-specific sync interval rather than the tier default.
    let tier_limits = user_record.subscription_tier.limits_for_api();
    let (personal_sync, team_sync) = SubscriptionTier::get_sync_intervals(&config.network);
    let actual_sync_interval = match user_record.subscription_tier {
        SubscriptionTier::Personal => personal_sync,
        SubscriptionTier::Team => team_sync,
    };

    let limits = BillingTierLimits {
        max_wallets: tier_limits
            .max_wallets
            .map_or(-1, |n| i32::try_from(n).unwrap_or(i32::MAX)),
        max_contacts_per_wallet: tier_limits
            .max_contacts_per_wallet
            .map_or(-1, |n| i32::try_from(n).unwrap_or(i32::MAX)),
        sync_interval_seconds: actual_sync_interval,
    };

    let effective_subscription_status = crate::subscription::get_effective_subscription_status(
        &user_record.subscription_status,
        user_record.trial_ends_at.as_deref(),
    );

    let billing_provider = active_billing_provider(&config, &stripe_billing, &btcpay);
    let response = BillingStatusResponse {
        user_id: user.user_id.clone(),
        subscription_tier: user_record.subscription_tier.as_str().to_string(),
        subscription_status: effective_subscription_status,
        trial_ends_at: user_record.trial_ends_at,
        subscription_started_at: user_record.subscription_started_at,
        subscription_ends_at: user_record.subscription_ends_at,
        billing_provider: billing_provider.map(|provider| provider.as_str().to_string()),
        can_manage_billing: billing_provider == Some(BillingProvider::Stripe)
            && user_record.stripe_customer_id.is_some(),
        stripe_customer_id: user_record.stripe_customer_id,
        wallet_count,
        contact_count,
        limits,
    };

    info!("get_billing_status completed in {:?}", start_time.elapsed());

    Ok(Json(response))
}

/// Get pricing information from the active billing provider (no auth required)
pub async fn get_billing_pricing(
    State(stripe_billing): State<StripeBillingState>,
    State(btcpay): State<BtcPayClientState>,
    State(config): State<ConfigState>,
) -> ApiResult<Json<PricingInfo>> {
    let pricing = match active_billing_provider(&config, &stripe_billing, &btcpay) {
        Some(BillingProvider::Stripe) => stripe_client(&stripe_billing)?.get_pricing_for_frontend(),
        Some(BillingProvider::BtcPay) => btcpay_client(&btcpay)?
            .get_cloud_pricing_for_frontend()
            .or_internal("Failed to load BTCPay pricing")?,
        None => return Err(service_unavailable("No billing provider configured")),
    };
    Ok(Json(pricing))
}

fn stripe_signature(headers: &HeaderMap) -> ApiResult<&str> {
    headers
        .get("stripe-signature")
        .ok_or_else(|| bad_request("Missing stripe-signature header"))?
        .to_str()
        .map_err(|_| bad_request("Invalid stripe-signature header"))
}

/// The 400 Stripe sees when it sent an event we cannot verify or parse.
fn stripe_webhook_rejected(error: &anyhow::Error) -> ApiError {
    bad_request(format!("Webhook processing failed: {error}"))
}

async fn deliver_trial_ending_notifications(
    stripe_billing: &StripeBilling,
    event_id: &str,
) -> ApiResult<()> {
    stripe_billing
        .deliver_pending_trial_ending_notifications(event_id)
        .await
        .map_err(|_| {
            tracing::error!("Trial notification delivery failed");
            ApiError::opaque("Webhook notification delivery failed")
        })
}

/// Claims a Stripe event for processing.
///
/// Returns `None` when the event was already processed; any notifications it
/// left pending are delivered before acknowledging it again.
async fn claim_stripe_event(
    app_services: &AppServicesState,
    stripe_billing: &StripeBilling,
    event_id: &str,
) -> ApiResult<Option<String>> {
    match app_services.metadata_db.claim_stripe_event(event_id).await {
        Ok(StripeEventClaim::Claimed(claim_token)) => Ok(Some(claim_token)),
        Ok(StripeEventClaim::Processed) => {
            tracing::info!(event_id = %event_id, "Ignoring duplicate Stripe webhook");
            deliver_trial_ending_notifications(stripe_billing, event_id).await?;
            Ok(None)
        }
        Ok(StripeEventClaim::Active) => {
            tracing::info!(event_id = %event_id, "Stripe webhook is already being processed");
            Err(ApiError::opaque("Webhook is already being processed"))
        }
        Err(_) => {
            tracing::error!("Webhook event claim failed");
            Err(ApiError::opaque(WEBHOOK_STATE_UPDATE_FAILED))
        }
    }
}

/// Renews the claim on a Stripe event every minute, because Stripe API work
/// can outlive the claim lease. Abort the task once processing finishes.
fn spawn_claim_lease_refresh(
    metadata_db: MetadataDb,
    event_id: String,
    claim_token: String,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_mins(1));
        interval.tick().await;
        loop {
            interval.tick().await;
            match metadata_db
                .refresh_stripe_event_claim(&event_id, &claim_token)
                .await
            {
                Ok(true) => {}
                Ok(false) => {
                    tracing::warn!(event_id = %event_id, "Stripe webhook claim was lost during processing");
                    break;
                }
                Err(_error) => {
                    tracing::error!("Webhook lease refresh failed");
                }
            }
        }
    })
}

/// Reconciles one webhook update with Stripe and resolves the Canary user it
/// applies to. Returns `None` when no current user matches.
async fn resolve_stripe_subscription_update(
    app_services: &AppServicesState,
    stripe_billing: &StripeBilling,
    mut update: SubscriptionUpdate,
) -> ApiResult<Option<SubscriptionUpdate>> {
    if update.stripe_subscription_id.is_some() {
        update = stripe_billing
            .reconcile_subscription_update(&update)
            .await
            .map_err(|_| {
                tracing::error!("Subscription reconciliation failed");
                ApiError::opaque(WEBHOOK_STATE_UPDATE_FAILED)
            })?;
    }

    // Deletion events carry `stripe_customer:<customer>[:<subscription>]`
    // instead of a user ID.
    let user = if let Some(customer_ref) = update.user_id.strip_prefix("stripe_customer:") {
        let mut parts = customer_ref.split(':');
        let customer_id = parts.next().expect("prefix leaves customer ID");
        let deleted_subscription_id = parts.next();
        let user = app_services
            .metadata_db
            .get_user_by_stripe_customer_id(customer_id)
            .await
            .map_err(|_| {
                tracing::error!("Billing customer lookup failed");
                ApiError::opaque(WEBHOOK_STATE_UPDATE_FAILED)
            })?;
        let Some(user) = user.filter(|user| {
            deleted_subscription_id.is_none()
                || user.stripe_subscription_id.as_deref() == deleted_subscription_id
        }) else {
            return Ok(None);
        };
        if let Some((_, deleted_subscription_id)) = customer_ref.split_once(':') {
            update.stripe_subscription_id = Some(deleted_subscription_id.to_string());
        }
        user
    } else {
        let user = app_services
            .metadata_db
            .get_user_by_id(&update.user_id)
            .await
            .map_err(|_| {
                tracing::error!("Billing user lookup failed");
                ApiError::opaque(WEBHOOK_STATE_UPDATE_FAILED)
            })?;
        let Some(user) = user else {
            return Ok(None);
        };
        user
    };

    update.user_id = user.id;
    Ok(Some(update))
}

/// Applies a claimed Stripe event. Returns whether the claim was still held
/// when the result was stored.
async fn apply_claimed_stripe_event(
    app_services: &AppServicesState,
    stripe_billing: &StripeBilling,
    verified_event: &VerifiedWebhookEvent,
    claim_token: &str,
    body: &str,
    signature: &str,
) -> ApiResult<bool> {
    let webhook_result = stripe_billing
        .handle_webhook(body.as_bytes(), signature)
        .await
        .map_err(|error| {
            tracing::error!("Webhook processing failed");
            stripe_webhook_rejected(&error)
        })?;

    let mut subscription_updates = Vec::with_capacity(webhook_result.subscription_updates.len());
    for update in webhook_result.subscription_updates {
        if let Some(update) =
            resolve_stripe_subscription_update(app_services, stripe_billing, update).await?
        {
            subscription_updates.push(update);
        }
    }

    app_services
        .metadata_db
        .complete_stripe_event_with_subscription_updates(
            &verified_event.event_id,
            claim_token,
            verified_event.event_created,
            &subscription_updates,
            &webhook_result.trial_ending_notifications,
        )
        .await
        .map_err(|_| {
            tracing::error!("Webhook persistence failed");
            ApiError::opaque(WEBHOOK_STATE_UPDATE_FAILED)
        })
}

/// Handle Stripe webhook events (no auth - uses Stripe signature)
///
/// Stripe retries any non-2xx response, so failures after the event is
/// claimed release the claim to let the retry process it.
pub async fn handle_stripe_webhook(
    State(app_services): State<AppServicesState>,
    State(stripe_billing): State<StripeBillingState>,
    headers: HeaderMap,
    body: String,
) -> ApiResult<&'static str> {
    let signature = stripe_signature(&headers)?;
    let stripe_billing = stripe_client(&stripe_billing)?;

    tracing::info!("Processing Stripe webhook");
    let verified_event = stripe_billing
        .verify_webhook_event(body.as_bytes(), signature)
        .map_err(|error| {
            tracing::error!("Webhook verification failed");
            stripe_webhook_rejected(&error)
        })?;
    let event_id = &verified_event.event_id;

    let Some(claim_token) = claim_stripe_event(&app_services, stripe_billing, event_id).await?
    else {
        return Ok(WEBHOOK_ACK);
    };

    let lease_refresh = spawn_claim_lease_refresh(
        app_services.metadata_db.clone(),
        event_id.clone(),
        claim_token.clone(),
    );
    let applied = apply_claimed_stripe_event(
        &app_services,
        stripe_billing,
        &verified_event,
        &claim_token,
        &body,
        signature,
    )
    .await;
    lease_refresh.abort();

    match applied {
        Ok(true) => {
            deliver_trial_ending_notifications(stripe_billing, event_id).await?;
            tracing::info!("Webhook processed successfully");
            Ok(WEBHOOK_ACK)
        }
        Ok(false) => {
            tracing::error!("Stripe webhook claim was lost before completion");
            Err(ApiError::opaque(WEBHOOK_STATE_UPDATE_FAILED))
        }
        Err(error) => {
            if app_services
                .metadata_db
                .release_stripe_event(event_id, &claim_token)
                .await
                .is_err()
            {
                tracing::error!("Webhook retry release failed");
            }
            Err(error)
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

/// Checks the `BTCPay` signature and store, then parses the payload.
fn verified_btcpay_payload(
    config: &crate::config::AppConfig,
    headers: &HeaderMap,
    body: &str,
) -> ApiResult<BtcPayWebhookPayload> {
    let secret = config
        .btcpay_webhook_secret()
        .ok_or_else(|| service_unavailable("BTCPay webhook is not configured"))?;
    let signature = headers
        .get("btcpay-sig")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| bad_request("Missing BTCPay webhook signature"))?;
    if !verify_btcpay_webhook_signature(&secret, body.as_bytes(), signature) {
        return Err(bad_request("Invalid BTCPay webhook signature"));
    }

    let payload: BtcPayWebhookPayload = serde_json::from_str(body).map_err(|_| {
        tracing::warn!("Invalid billing webhook payload");
        bad_request("Invalid BTCPay webhook payload")
    })?;
    if config.btcpay_store_id() != Some(payload.store_id.as_str()) {
        return Err(bad_request("BTCPay webhook store does not match"));
    }
    Ok(payload)
}

/// Loads the Canary checkout a `BTCPay` subscriber was created for, or `None`
/// when it is not a known `BTCPay` checkout.
async fn find_btcpay_checkout(
    app_services: &AppServicesState,
    checkout_token: &str,
) -> ApiResult<Option<PendingBillingCheckout>> {
    let checkout = app_services
        .metadata_db
        .get_billing_checkout_by_token(checkout_token)
        .await
        .map_err(|_| {
            tracing::error!("Checkout lookup failed");
            ApiError::opaque("BTCPay webhook state lookup failed")
        })?;
    Ok(checkout.filter(|checkout| checkout.provider == "btcpay"))
}

fn rfc3339_from_timestamp(timestamp: i64, invalid_message: &'static str) -> ApiResult<String> {
    chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|date| date.to_rfc3339())
        .ok_or_else(|| bad_request(invalid_message))
}

/// Re-applies tier limits for the checkout's user from their stored subscription.
async fn refresh_subscription_limits(
    app_services: &AppServicesState,
    user_id: &str,
) -> ApiResult<()> {
    let updated_user = app_services
        .metadata_db
        .get_user_by_id(user_id)
        .await
        .map_err(|_| {
            tracing::error!("Checkout user reload failed");
            ApiError::opaque("BTCPay subscription user lookup failed")
        })?
        .ok_or_else(|| {
            ApiError::uncoded(StatusCode::NOT_FOUND, "BTCPay checkout user not found")
        })?;
    app_services
        .apply_subscription_limits(
            user_id,
            updated_user.subscription_tier.as_str(),
            &updated_user.subscription_status,
            updated_user.is_admin,
            updated_user.trial_ends_at,
            updated_user.subscription_ends_at,
        )
        .await
        .map_err(|_| {
            tracing::error!("Subscription limit update failed");
            ApiError::opaque("BTCPay subscription limit update failed")
        })
}

/// Process signed `BTCPay` subscription events and reconcile them with Canary users.
///
/// `BTCPay` retries any non-2xx response, so events Canary deliberately ignores
/// are acknowledged with 200.
pub async fn handle_btcpay_webhook(
    State(app_services): State<AppServicesState>,
    State(btcpay): State<BtcPayClientState>,
    State(config): State<ConfigState>,
    headers: HeaderMap,
    body: String,
) -> ApiResult<&'static str> {
    let payload = verified_btcpay_payload(&config, &headers, &body)?;

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
        return Ok(WEBHOOK_ACK);
    };

    let subscriber = payload
        .subscriber
        .ok_or_else(|| bad_request("BTCPay webhook subscriber is missing"))?;
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
        return Ok(WEBHOOK_ACK);
    };

    let Some(checkout) = find_btcpay_checkout(&app_services, checkout_token).await? else {
        tracing::warn!(
            delivery_id = %payload.delivery_id,
            "BTCPay webhook referenced an unknown checkout"
        );
        return Ok(WEBHOOK_ACK);
    };

    let client = btcpay
        .as_deref()
        .ok_or_else(|| service_unavailable("BTCPay billing is not initialized"))?;
    let plan_id = subscriber
        .plan
        .as_ref()
        .map(|plan| plan.id.as_str())
        .ok_or_else(|| bad_request("BTCPay webhook plan is missing"))?;
    let customer_id = subscriber
        .customer
        .as_ref()
        .map(|customer| customer.id.as_str())
        .ok_or_else(|| bad_request("BTCPay webhook customer is missing"))?;
    let Some(tier) = client.cloud_plan_tier_from_plan_id(plan_id) else {
        tracing::warn!(plan_id, "Ignoring BTCPay webhook for an unknown cloud plan");
        return Ok(WEBHOOK_ACK);
    };
    if checkout.subscription_tier != tier.as_str() {
        tracing::warn!(
            delivery_id = %payload.delivery_id,
            "BTCPay webhook plan does not match its checkout"
        );
        return Ok(WEBHOOK_ACK);
    }

    let started_at =
        rfc3339_from_timestamp(payload.timestamp, "BTCPay webhook timestamp is invalid")?;
    let subscription_ends_at = subscriber
        .period_end
        .map(|timestamp| rfc3339_from_timestamp(timestamp, "BTCPay subscription period is invalid"))
        .transpose()?;
    let apply_result = app_services
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
        .map_err(|_| {
            tracing::error!("Subscription persistence failed");
            ApiError::opaque("BTCPay subscription update failed")
        })?;

    match apply_result {
        BtcPayEventApplyResult::Stale
        | BtcPayEventApplyResult::Superseded
        | BtcPayEventApplyResult::CheckoutNotFound => {
            tracing::info!(
                delivery_id = %payload.delivery_id,
                result = ?apply_result,
                "Ignored non-current BTCPay subscription webhook"
            );
            return Ok(WEBHOOK_ACK);
        }
        BtcPayEventApplyResult::Applied | BtcPayEventApplyResult::Duplicate => {}
    }

    refresh_subscription_limits(&app_services, &checkout.user_id).await?;

    tracing::info!("Subscription webhook processed");
    Ok(WEBHOOK_ACK)
}

fn session_not_found() -> ApiError {
    ApiError::uncoded(StatusCode::NOT_FOUND, "Session not found")
}

/// Get checkout session details
pub async fn get_checkout_session_details(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(stripe_billing): State<StripeBillingState>,
    State(btcpay): State<BtcPayClientState>,
    State(config): State<ConfigState>,
    Path(session_id): Path<String>,
) -> ApiResult<Json<CheckoutSessionDetails>> {
    match active_billing_provider(&config, &stripe_billing, &btcpay) {
        Some(BillingProvider::BtcPay) => {
            btcpay_checkout_session_details(&app_services, &user.user_id, session_id).await
        }
        Some(BillingProvider::Stripe) => {
            stripe_checkout_session_details(
                &app_services,
                stripe_client(&stripe_billing)?,
                &user.user_id,
                &session_id,
            )
            .await
        }
        None => Err(ApiError::bad_request(
            "checkout_session_unsupported",
            "Checkout session lookups are only available for Stripe",
        )),
    }
    .map(Json)
}

async fn btcpay_checkout_session_details(
    app_services: &AppServicesState,
    user_id: &str,
    session_id: String,
) -> ApiResult<CheckoutSessionDetails> {
    let checkout = app_services
        .metadata_db
        .get_pending_billing_checkout(&session_id)
        .await
        .map_err(|_| {
            tracing::error!("Checkout session lookup failed");
            ApiError::opaque("Failed to load checkout session")
        })?
        .filter(|checkout| checkout.user_id == user_id && checkout.provider == "btcpay")
        .ok_or_else(session_not_found)?;

    let status = if checkout.completed_at.is_some() {
        "complete"
    } else {
        "pending"
    };
    Ok(CheckoutSessionDetails {
        session_id,
        customer_id: None,
        subscription_id: None,
        status: Some(status.to_string()),
        tier: Some(checkout.subscription_tier),
        billing_period: Some(checkout.billing_period),
        amount_total: None,
        currency: None,
    })
}

async fn stripe_checkout_session_details(
    app_services: &AppServicesState,
    stripe_billing: &StripeBilling,
    user_id: &str,
    session_id: &str,
) -> ApiResult<CheckoutSessionDetails> {
    let details = stripe_billing
        .get_checkout_session_details(session_id)
        .await
        .map_err(|error| {
            ApiError::uncoded(StatusCode::NOT_FOUND, format!("Session not found: {error}"))
        })?;
    let Some(customer_id) = details.customer_id.as_deref() else {
        return Err(session_not_found());
    };
    let owns_session = matches!(
        app_services.metadata_db.get_user_by_id(user_id).await,
        Ok(Some(current_user)) if current_user.stripe_customer_id.as_deref() == Some(customer_id)
    );
    if owns_session {
        Ok(details)
    } else {
        Err(session_not_found())
    }
}

pub async fn create_stripe_checkout_session(
    authenticated_user: AuthenticatedUser,
    app_services: State<AppServicesState>,
    stripe_billing: State<StripeBillingState>,
    btcpay: State<BtcPayClientState>,
    config: State<ConfigState>,
    payload: Json<CreateCheckoutSessionRequest>,
) -> ApiResult<Json<CheckoutSessionResponse>> {
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
) -> ApiResult<Json<CustomerPortalResponse>> {
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

    #[test]
    fn btcpay_checkout_rejects_non_terminal_subscriptions() {
        assert!(!btcpay_checkout_allowed("active"));
        assert!(!btcpay_checkout_allowed("past_due"));
        assert!(!btcpay_checkout_allowed("canceled"));
        assert!(btcpay_checkout_allowed("expired"));
        assert!(btcpay_checkout_allowed("trialing"));
        assert!(btcpay_checkout_allowed("pending"));
    }
}
