//! Stripe webhook events: signature verification, turning each event into
//! [`SubscriptionUpdate`]s, and reconciling an update with the subscription
//! as Stripe currently reports it.

use anyhow::Result;

use super::{
    known_status, StripeBilling, SubscriptionUpdate, TrialEndingNotification, VerifiedWebhookEvent,
    WebhookResult,
};
use crate::metadata::SubscriptionStatus;
use crate::stripe_client_service::{Event, StripeClientService};

impl StripeBilling {
    pub fn verify_webhook_event(
        &self,
        payload: &[u8],
        signature: &str,
    ) -> Result<VerifiedWebhookEvent> {
        let payload_str = std::str::from_utf8(payload)?;
        StripeClientService::parse_webhook_event(payload_str, signature, &self.webhook_secret)?;
        let event = serde_json::from_str::<serde_json::Value>(payload_str)?;
        Ok(VerifiedWebhookEvent {
            event_id: event
                .get("id")
                .and_then(|id| id.as_str())
                .map(str::to_owned)
                .ok_or_else(|| anyhow::anyhow!("Stripe webhook event is missing an ID"))?,
            event_created: event
                .get("created")
                .and_then(serde_json::Value::as_i64)
                .ok_or_else(|| {
                    anyhow::anyhow!("Stripe webhook event is missing a creation time")
                })?,
        })
    }

    pub async fn reconcile_subscription_update(
        &self,
        update: &SubscriptionUpdate,
    ) -> Result<SubscriptionUpdate> {
        let subscription_id = update
            .stripe_subscription_id
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Cannot reconcile a deleted subscription"))?;
        let subscription = self.client.get_subscription_json(subscription_id).await?;
        let timestamp = |field: &str| {
            subscription
                .get(field)
                .and_then(serde_json::Value::as_i64)
                .and_then(|value| chrono::DateTime::from_timestamp(value, 0))
                .map(|value| value.to_rfc3339())
        };
        let subscription_status = if subscription
            .get("cancel_at_period_end")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
        {
            SubscriptionStatus::Canceled.to_string()
        } else {
            subscription
                .get("status")
                .and_then(|status| status.as_str())
                .map(str::to_string)
                .ok_or_else(|| anyhow::anyhow!("Stripe subscription is missing a status"))?
        };
        let period_end = current_period_end(&subscription)
            .and_then(|value| chrono::DateTime::from_timestamp(value, 0))
            .map(|value| value.to_rfc3339());

        Ok(SubscriptionUpdate {
            user_id: update.user_id.clone(),
            subscription_tier: self.determine_tier_from_subscription_items(&subscription),
            subscription_status,
            stripe_subscription_id: Some(subscription_id.to_string()),
            subscription_started_at: timestamp("created"),
            subscription_ends_at: timestamp("cancel_at").or(period_end),
            trial_ends_at: timestamp("trial_end"),
        })
    }

    pub async fn handle_webhook(&self, payload: &[u8], signature: &str) -> Result<WebhookResult> {
        // Use stripe library for webhook verification (security critical)
        let payload_str = std::str::from_utf8(payload)?;
        let event =
            StripeClientService::parse_webhook_event(payload_str, signature, &self.webhook_secret)?;
        let mut updates = Vec::new();
        let mut trial_ending_notifications = Vec::new();

        // Handle different event types with 2025 API structure
        if let Some(event_type) = &event.r#type {
            match event_type.as_str() {
                "customer.subscription.trial_will_end" => {
                    // Fired 3 days before trial ends - send notification to user
                    tracing::info!("⏰ Trial ending soon for subscription");
                    trial_ending_notifications.extend(trial_ending_notification(&event));
                }
                "checkout.session.completed" => {
                    updates.extend(self.checkout_completed_update(&event).await?);
                }
                "customer.subscription.created" => {
                    updates.extend(self.subscription_created_update(&event));
                }
                "customer.subscription.updated" => {
                    // Handle ALL subscription updates: tier changes, trial endings, etc.
                    updates.extend(self.subscription_updated_update(&event));
                }
                "customer.subscription.deleted" => {
                    // Subscription cancelled/ended - need to check if it's current subscription in API layer
                    updates.extend(subscription_deleted_update(&event));
                }
                "invoice.payment_succeeded" => {
                    // Parse invoice data to provide clearer logging for $0 trial invoices vs actual payments
                    updates.extend(payment_succeeded_update(&event));
                }
                "invoice.payment_failed" => {
                    tracing::info!("Payment failed; subscription may be suspended");
                }
                _ => {
                    tracing::debug!("🔄 Ignoring webhook event type: {}", event_type);
                }
            }
        }

        Ok(WebhookResult {
            subscription_updates: updates,
            trial_ending_notifications,
        })
    }

    /// The update for a `checkout.session.completed` event, after cancelling
    /// the customer's other billable subscriptions.
    async fn checkout_completed_update(&self, event: &Event) -> Result<Option<SubscriptionUpdate>> {
        let Some(session_data) = event_object(event) else {
            return Ok(None);
        };
        tracing::info!("📋 Processing checkout.session.completed");

        // Parse checkout session data
        let Ok(session) = serde_json::from_value::<serde_json::Value>(session_data.clone()) else {
            return Ok(None);
        };
        let customer_id = session.get("customer").and_then(|c| c.as_str());
        let new_subscription_id = session.get("subscription").and_then(|s| s.as_str());

        tracing::info!("Processing completed checkout");

        let (Some(customer_id), Some(new_subscription_id)) = (customer_id, new_subscription_id)
        else {
            return Ok(None);
        };
        // Handle duplicate subscription cleanup
        let cleanup_update = self
            .handle_checkout_completion(customer_id, new_subscription_id)
            .await?;
        Ok(Some(cleanup_update))
    }

    /// The update for a `customer.subscription.created` event.
    fn subscription_created_update(&self, event: &Event) -> Option<SubscriptionUpdate> {
        let subscription_obj = event_object(event)?;
        tracing::info!("📋 Processing customer.subscription.created");

        // Parse subscription to extract trial info and tier
        let subscription =
            serde_json::from_value::<serde_json::Value>(subscription_obj.clone()).ok()?;
        let status = subscription.get("status").and_then(|s| s.as_str());
        let trial_end = subscription
            .get("trial_end")
            .and_then(serde_json::Value::as_i64);

        // Extract current_period_end from subscription items (more reliable than top level)
        let current_period_end = current_period_end(&subscription);

        let customer_id = subscription.get("customer").and_then(|c| c.as_str());
        let subscription_id = subscription.get("id").and_then(|s| s.as_str());

        // Determine tier from subscription items (more reliable than metadata)
        let tier = self.determine_tier_from_subscription_items(&subscription);

        tracing::info!("Processing created subscription");

        let (Some(customer_id), Some(subscription_id)) = (customer_id, subscription_id) else {
            return None;
        };
        // Convert trial_end timestamp to ISO string
        let trial_ends_at = trial_end.map(rfc3339_or_empty);

        // Convert current_period_end timestamp to ISO string
        let subscription_ends_at = current_period_end.map(rfc3339_or_empty);

        Some(SubscriptionUpdate {
            user_id: StripeBilling::extract_user_id_from_customer(customer_id),
            subscription_tier: tier, // Use actual tier from subscription items
            subscription_status: status.unwrap_or("unknown").to_string(), // "trialing" or "active" from Stripe
            stripe_subscription_id: Some(subscription_id.to_string()),
            subscription_started_at: Some(chrono::Utc::now().to_rfc3339()),
            subscription_ends_at,
            trial_ends_at,
        })
    }

    /// The update for a `customer.subscription.updated` event, or `None` when
    /// the change is not one Canary stores.
    fn subscription_updated_update(&self, event: &Event) -> Option<SubscriptionUpdate> {
        let subscription_obj = event_object(event)?;
        let subscription =
            serde_json::from_value::<serde_json::Value>(subscription_obj.clone()).ok()?;
        let current_status = subscription.get("status").and_then(|s| s.as_str());
        let current_known_status = current_status.and_then(known_status);
        let customer_id = subscription.get("customer").and_then(|c| c.as_str());
        let subscription_id = subscription.get("id").and_then(|s| s.as_str());

        // Extract current_period_end from subscription items (more reliable than top level)
        let current_period_end = current_period_end(&subscription);

        let cancel_at_period_end = subscription
            .get("cancel_at_period_end")
            .and_then(serde_json::Value::as_bool);
        let cancel_at = subscription
            .get("cancel_at")
            .and_then(serde_json::Value::as_i64);

        // Determine current tier from subscription items (more reliable than metadata)
        let current_tier = self.determine_tier_from_subscription_items(&subscription);

        tracing::info!("Processing updated subscription");

        let (Some(customer_id), Some(subscription_id)) = (customer_id, subscription_id) else {
            return None;
        };
        let reason = subscription_update_reason(
            &subscription,
            current_known_status,
            &current_tier,
            cancel_at_period_end,
        )?;
        tracing::info!("✅ Processing subscription update: {}", reason);

        // Convert current_period_end timestamp to ISO string
        // Use cancel_at if available (set when cancel_at_period_end is true)
        let subscription_ends_at = cancel_at.or(current_period_end).map(rfc3339_or_empty);

        // Extract trial_end for trialing subscriptions
        let trial_ends_at = if current_known_status == Some(SubscriptionStatus::Trialing) {
            subscription
                .get("trial_end")
                .and_then(serde_json::Value::as_i64)
                .map(rfc3339_or_empty)
        } else {
            None
        };

        // Override status to "canceled" if cancel_at_period_end is true
        let final_status = if cancel_at_period_end == Some(true) {
            SubscriptionStatus::Canceled.to_string()
        } else {
            current_status.unwrap_or("unknown").to_string()
        };

        Some(SubscriptionUpdate {
            user_id: Self::extract_user_id_from_customer(customer_id),
            subscription_tier: current_tier,
            subscription_status: final_status,
            stripe_subscription_id: Some(subscription_id.to_string()),
            subscription_started_at: Some(chrono::Utc::now().to_rfc3339()),
            subscription_ends_at,
            trial_ends_at,
        })
    }

    async fn handle_checkout_completion(
        &self,
        customer_id: &str,
        new_subscription_id: &str,
    ) -> Result<SubscriptionUpdate> {
        tracing::info!("Reconciling checkout completion");

        // List all subscriptions for this customer
        let subscription_list = self.client.list_subscriptions(customer_id).await?;
        let subscriptions = subscription_list.data.unwrap_or_default();

        tracing::info!("Loaded customer subscriptions for reconciliation");

        // Extract tier from the new subscription's metadata
        let mut tier = "personal".to_string(); // Default to personal if no metadata found

        // Find the new subscription and extract its tier
        for subscription in &subscriptions {
            if let Some(sub_id) = &subscription.id {
                if sub_id == new_subscription_id {
                    // Extract tier from subscription metadata
                    if let Some(metadata) = &subscription.metadata {
                        if let Some(tier_from_metadata) = metadata.get("tier") {
                            tier = tier_from_metadata.to_lowercase();
                            tracing::info!(
                                "📋 Extracted tier '{}' from new subscription metadata",
                                tier
                            );
                        }
                    }
                } else {
                    // A replacement checkout must retire any billable prior subscription.
                    if matches!(
                        subscription.status.as_deref().and_then(known_status),
                        Some(SubscriptionStatus::Trialing | SubscriptionStatus::Active)
                    ) {
                        tracing::info!("Cancelling previous subscription");
                        match self.client.cancel_subscription(sub_id).await {
                            Ok(_) => {
                                tracing::info!("Previous subscription cancelled");
                            }
                            Err(e) => {
                                tracing::error!("Previous subscription cancellation failed");
                                return Err(e);
                            }
                        }
                    }
                }
            }
        }

        // Create update for the new paid subscription
        let user_id = StripeBilling::extract_user_id_from_customer(customer_id);

        tracing::info!("Preparing subscription update");

        Ok(SubscriptionUpdate {
            user_id,
            subscription_tier: tier, // Use actual tier from subscription metadata
            subscription_status: SubscriptionStatus::Active.to_string(), // Paid subscription is active
            stripe_subscription_id: Some(new_subscription_id.to_string()),
            subscription_started_at: Some(chrono::Utc::now().to_rfc3339()),
            subscription_ends_at: None,
            trial_ends_at: None, // No longer in trial
        })
    }
}

/// The `data.object` of a webhook event, if it has one.
fn event_object(event: &Event) -> Option<&serde_json::Value> {
    event.data.as_ref()?.object.as_ref()
}

/// A subscription's `current_period_end`, read from its first item and else
/// from the subscription itself.
fn current_period_end(subscription: &serde_json::Value) -> Option<i64> {
    subscription
        .get("items")
        .and_then(|items| items.get("data"))
        .and_then(|data| data.as_array())
        .and_then(|items| items.first())
        .and_then(|item| item.get("current_period_end"))
        .and_then(serde_json::Value::as_i64)
        .or_else(|| {
            subscription
                .get("current_period_end")
                .and_then(serde_json::Value::as_i64)
        })
}

/// A Unix timestamp as RFC 3339, or an empty string when it is out of range.
fn rfc3339_or_empty(timestamp: i64) -> String {
    chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default()
}

/// The trial ending email a `customer.subscription.trial_will_end` event
/// asks for.
fn trial_ending_notification(event: &Event) -> Option<TrialEndingNotification> {
    let subscription_obj = event_object(event)?;
    let subscription =
        serde_json::from_value::<serde_json::Value>(subscription_obj.clone()).ok()?;
    let customer_id = subscription.get("customer").and_then(|c| c.as_str());
    let trial_end = subscription
        .get("trial_end")
        .and_then(serde_json::Value::as_i64);

    tracing::info!("Processing trial ending event");

    let (Some(customer_id), Some(trial_end_timestamp)) = (customer_id, trial_end) else {
        return None;
    };
    // Send only after the event is durably completed, so retries
    // caused by a database failure cannot duplicate this email.
    Some(TrialEndingNotification {
        customer_id: customer_id.to_string(),
        trial_end_timestamp,
    })
}

/// Why a `customer.subscription.updated` event changes what Canary stores, or
/// `None` when it does not. The reason is only logged, and can be empty.
fn subscription_update_reason(
    subscription: &serde_json::Value,
    current_known_status: Option<SubscriptionStatus>,
    current_tier: &str,
    cancel_at_period_end: Option<bool>,
) -> Option<String> {
    let mut should_update = false;
    let mut reason = String::new();

    // Check for trial ending
    if let Some(previous_attrs) = subscription.get("previous_attributes") {
        if let Some(previous_status) = previous_attrs.get("status").and_then(|s| s.as_str()) {
            if known_status(previous_status) == Some(SubscriptionStatus::Trialing)
                && current_known_status != Some(SubscriptionStatus::Trialing)
            {
                should_update = true;
                reason = "Trial ended".to_string();
            }
        }
    }

    // Check for tier changes (items changed)
    if let Some(previous_attrs) = subscription.get("previous_attributes") {
        if previous_attrs.get("items").is_some() {
            should_update = true;
            reason = format!("Tier changed to {current_tier}");
        }
    }

    // Check for trial-related changes (trial extension, shortening, etc.)
    // Always update if subscription is trialing to ensure trial_ends_at is kept in sync
    if current_known_status == Some(SubscriptionStatus::Trialing) {
        should_update = true;
        if reason.is_empty() {
            if let Some(current_trial_end) = subscription
                .get("trial_end")
                .and_then(serde_json::Value::as_i64)
            {
                let new_date = chrono::DateTime::from_timestamp(current_trial_end, 0).map_or_else(
                    || "unknown".to_string(),
                    |dt| dt.format("%B %d, %Y").to_string(),
                );
                reason = format!("Trial active until {new_date}");
            }
        }
    }

    // Catch-all: update when subscription transitions to active
    // Only fires when previous_attributes confirms status changed from non-active
    if !should_update && current_known_status == Some(SubscriptionStatus::Active) {
        let status_changed_to_active = subscription
            .get("previous_attributes")
            .and_then(|p| p.get("status"))
            .and_then(|s| s.as_str())
            .is_some_and(|s| known_status(s) != Some(SubscriptionStatus::Active));
        if status_changed_to_active {
            should_update = true;
            reason = "Subscription activated".to_string();
        }
    }

    // Check for subscription cancellation
    // If cancel_at_period_end is true, always update to set the end date
    if cancel_at_period_end == Some(true) {
        should_update = true;
        reason = "Subscription cancelled (retains access until period end)".to_string();
    }

    should_update.then_some(reason)
}

/// The update for a `customer.subscription.deleted` event. It names the
/// customer and subscription instead of a user, so the handler can check that
/// the deleted subscription is still the user's current one.
fn subscription_deleted_update(event: &Event) -> Option<SubscriptionUpdate> {
    let subscription_obj = event_object(event)?;
    let subscription =
        serde_json::from_value::<serde_json::Value>(subscription_obj.clone()).ok()?;
    let customer_id = subscription.get("customer").and_then(|c| c.as_str());
    let deleted_subscription_id = subscription.get("id").and_then(|s| s.as_str());

    tracing::info!("Processing deleted subscription");

    let (Some(customer_id), Some(deleted_subscription_id)) = (customer_id, deleted_subscription_id)
    else {
        return None;
    };
    // Create a conditional update that the API layer will verify
    Some(SubscriptionUpdate {
        user_id: format!("stripe_customer:{customer_id}:{deleted_subscription_id}"),
        subscription_tier: "keep_current".to_string(),
        subscription_status: SubscriptionStatus::Expired.to_string(),
        stripe_subscription_id: None, // Clear subscription ID
        subscription_started_at: None,
        subscription_ends_at: Some(chrono::Utc::now().to_rfc3339()),
        trial_ends_at: None,
    })
}

/// The update for an `invoice.payment_succeeded` event: a subscription
/// invoice that collected a payment marks the subscription active.
fn payment_succeeded_update(event: &Event) -> Option<SubscriptionUpdate> {
    let invoice_obj = event_object(event)?;
    let Ok(invoice) = serde_json::from_value::<serde_json::Value>(invoice_obj.clone()) else {
        // Fallback if we can't parse the invoice
        tracing::info!("💰 Payment succeeded - subscription is active");
        return None;
    };
    let amount_paid = invoice
        .get("amount_paid")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    let amount_due = invoice
        .get("amount_due")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    let billing_reason = invoice.get("billing_reason").and_then(|r| r.as_str());
    let customer_id = invoice.get("customer").and_then(|c| c.as_str());

    if amount_paid == 0 && amount_due == 0 {
        // This is a $0 invoice (likely a trial start)
        if billing_reason == Some("subscription_create") {
            tracing::info!("Trial invoice processed");
        } else {
            tracing::info!("Zero-value invoice processed");
        }
        return None;
    }

    // Actual payment was collected
    tracing::info!("Payment succeeded");

    // Safety net: ensure subscription status is "active" after successful payment.
    // This handles race conditions where subscription.deleted (for old trial)
    // may overwrite the active status set by checkout.session.completed.
    // Only applies to subscription-related invoices.
    let subscription_id = invoice
        .get("subscription")
        .and_then(|s| s.as_str())
        .map(std::string::ToString::to_string);

    let (Some(cid), Some(sub_id)) = (customer_id, subscription_id) else {
        return None;
    };
    tracing::debug!("Reconciling active subscription status");

    Some(SubscriptionUpdate {
        user_id: StripeBilling::extract_user_id_from_customer(cid),
        subscription_tier: "keep_current".to_string(),
        subscription_status: SubscriptionStatus::Active.to_string(),
        stripe_subscription_id: Some(sub_id),
        subscription_started_at: None,
        subscription_ends_at: None,
        trial_ends_at: None,
    })
}

#[cfg(test)]
mod privacy_tests {
    use super::*;
    use crate::config::{AppConfig, NetworkConfig, OperatingMode};
    use crate::metadata::MetadataDb;
    use crate::stripe_billing::PricingInfo;
    use hmac::{Hmac, KeyInit, Mac};
    use sha2::Sha256;
    use std::io::Write;
    use std::sync::Arc;
    use std::sync::Mutex;

    #[derive(Clone)]
    struct Capture(Arc<Mutex<Vec<u8>>>);
    impl Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn billing_events_and_token_lookups_do_not_log_sensitive_fixtures() {
        let directory = tempfile::tempdir().unwrap();
        let config = AppConfig::new_for_test(
            NetworkConfig::Regtest,
            Some("tcp://127.0.0.1:50001".to_string()),
            "127.0.0.1:3000".to_string(),
            directory.path().to_string_lossy().to_string(),
            OperatingMode::SelfHosted,
            None,
            None,
        );
        let metadata_db = Arc::new(
            MetadataDb::new(
                directory.path().join("test.sqlite").to_str().unwrap(),
                &config,
            )
            .await
            .unwrap(),
        );
        let billing = StripeBilling {
            client: StripeClientService::new("synthetic-key".to_string()),
            webhook_secret: "synthetic-webhook-secret".to_string(),
            cached_pricing: PricingInfo {
                tiers: vec![],
                yearly_discount_percent: None,
            },
            metadata_db: metadata_db.clone(),
            email_service: None,
        };
        let captured = Arc::new(Mutex::new(Vec::new()));
        let writer = Capture(captured.clone());
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_max_level(tracing::Level::TRACE)
            .with_writer(move || writer.clone())
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);
        for event_type in [
            "invoice.payment_failed",
            "customer.subscription.trial_will_end",
        ] {
            let payload = serde_json::json!({
                "id": "evt_synthetic_private", "type": event_type,
                "data": {"object": {
                    "id": "sub_synthetic_private", "customer": "cus_synthetic_private",
                    "customer_email": "synthetic-private@example.invalid",
                    "amount_due": 123_456_789, "trial_end": 2_000_000_000
                }}
            })
            .to_string();
            let timestamp = chrono::Utc::now().timestamp();
            let mut mac = Hmac::<Sha256>::new_from_slice(b"synthetic-webhook-secret").unwrap();
            mac.update(format!("{timestamp}.{payload}").as_bytes());
            let digest = hex::encode(mac.finalize().into_bytes());
            billing
                .handle_webhook(payload.as_bytes(), &format!("t={timestamp},v1={digest}"))
                .await
                .unwrap();
        }
        metadata_db
            .verify_email_token("synthetic-private-verification-token")
            .await
            .unwrap();
        let logs = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
        assert!(logs.contains("Payment failed"));
        assert!(logs.contains("Processing trial ending event"));
        for sensitive in [
            "cus_synthetic_private",
            "sub_synthetic_private",
            "synthetic-private@example.invalid",
            "123456789",
            "1234567.89",
            "synthetic-private-verification-token",
        ] {
            assert!(
                !logs.contains(sensitive),
                "Sensitive fixture appeared in captured logs"
            );
        }
    }
}
