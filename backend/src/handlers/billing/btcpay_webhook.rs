//! `BTCPay` subscription webhook events: verifying the signature and store,
//! matching the subscriber to a Canary checkout, and applying the new
//! subscription status.

use super::{bad_request, service_unavailable, WEBHOOK_ACK};
use crate::api::{AppServicesState, BtcPayClientState, ConfigState};
use crate::handlers::{ApiError, ApiResult};
use crate::metadata::{
    BtcPayEventApplyResult, BtcPaySubscriptionEventParams, PendingBillingCheckout,
    SubscriptionStatus,
};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
};
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use sha2::Sha256;

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

fn btcpay_subscription_status(
    event_type: &str,
    is_active: Option<bool>,
) -> Option<SubscriptionStatus> {
    match event_type {
        "PlanStarted" | "SubscriberActivated" | "SubscriberCharged" => {
            Some(SubscriptionStatus::Active)
        }
        "SubscriberPhaseChanged" if is_active == Some(true) => Some(SubscriptionStatus::Active),
        "SubscriberNeedUpgrade" => Some(SubscriptionStatus::PastDue),
        "SubscriberDisabled" => Some(SubscriptionStatus::Expired),
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
    if !verify_btcpay_webhook_signature(secret, body.as_bytes(), signature) {
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
            updated_user.subscription_status,
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
            Some(SubscriptionStatus::Active)
        );
        assert_eq!(
            btcpay_subscription_status("SubscriberPhaseChanged", Some(true)),
            Some(SubscriptionStatus::Active)
        );
        assert_eq!(
            btcpay_subscription_status("SubscriberPhaseChanged", Some(false)),
            None
        );
        assert_eq!(
            btcpay_subscription_status("SubscriberNeedUpgrade", Some(true)),
            Some(SubscriptionStatus::PastDue)
        );
        assert_eq!(
            btcpay_subscription_status("SubscriberDisabled", Some(false)),
            Some(SubscriptionStatus::Expired)
        );
        assert_eq!(
            btcpay_subscription_status("SubscriberCreated", Some(false)),
            None
        );
    }
}
