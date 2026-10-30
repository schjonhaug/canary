//! Stripe webhook events: verifying the signature, claiming the event so it
//! is applied once, reconciling its updates with Canary users, and sending
//! the trial notifications it produced.

use super::{WEBHOOK_ACK, WEBHOOK_STATE_UPDATE_FAILED, bad_request, stripe_client};
use crate::api::{AppServicesState, StripeBillingState};
use crate::handlers::{ApiError, ApiResult};
use crate::metadata::{MetadataDb, StripeEventClaim};
use crate::stripe_billing::{StripeBilling, SubscriptionUpdate, VerifiedWebhookEvent};
use axum::{extract::State, http::HeaderMap};
use tokio::task::JoinHandle;

fn stripe_signature(headers: &HeaderMap) -> ApiResult<&str> {
    headers
        .get("stripe-signature")
        .ok_or_else(|| bad_request("Missing stripe-signature header"))?
        .to_str()
        .map_err(|_| bad_request("Invalid stripe-signature header"))
}

/// The 400 Stripe sees when it sent an event we cannot verify or parse.
fn stripe_webhook_rejected() -> ApiError {
    bad_request("Webhook processing failed")
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
        .map_err(|_| {
            tracing::error!("Webhook processing failed");
            stripe_webhook_rejected()
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
        .map_err(|_| {
            tracing::error!("Webhook verification failed");
            stripe_webhook_rejected()
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
