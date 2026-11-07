//! Stripe billing: the [`StripeBilling`] service and the subscription
//! updates it produces.
//!
//! The service loads Canary's prices from Stripe at startup and is shared by
//! the HTTP layer. Its methods are split by responsibility:
//!
//! - `pricing`: the price list served to the frontend, and mapping a Stripe
//!   price back to a tier
//! - `checkout`: customers, trial subscriptions, checkout sessions and the
//!   customer portal
//! - `webhook`: verifying webhook events and turning them into
//!   [`SubscriptionUpdate`]s
//! - `trial_notifications`: trial ending emails, sent once an event is stored

mod checkout;
mod pricing;
mod trial_notifications;
mod webhook;

pub use checkout::{CheckoutSessionDetails, CheckoutSessionResponse, CustomerPortalResponse};
pub use pricing::{FrontendPriceInfo, FrontendTierPricing, PricingInfo};

use anyhow::Result;
use std::sync::Arc;

use crate::config::AppConfig;
use crate::email_service::EmailService;
use crate::metadata::{MetadataDb, SubscriptionStatus};
use crate::stripe_client_service::StripeClientService;
use crate::subscription::SubscriptionTier;

#[derive(Debug, Clone)]
pub struct WebhookResult {
    pub subscription_updates: Vec<SubscriptionUpdate>,
    pub trial_ending_notifications: Vec<TrialEndingNotification>,
}

#[derive(Debug, Clone)]
pub struct SubscriptionUpdate {
    pub user_id: String,
    /// A [`SubscriptionTier`] name, or `keep_current` to leave the stored
    /// tier as it is.
    pub subscription_tier: String,
    /// The Stripe status mapped onto Canary's statuses; `incomplete` and
    /// `unpaid` become `past_due`, `incomplete_expired` and `paused` become
    /// `expired`.
    pub subscription_status: SubscriptionStatus,
    pub stripe_subscription_id: Option<String>,
    pub subscription_started_at: Option<String>,
    pub subscription_ends_at: Option<String>,
    pub trial_ends_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TrialEndingNotification {
    pub(crate) customer_id: String,
    pub(crate) trial_end_timestamp: i64,
}

/// The Canary status for a subscription status reported by Stripe, or `None`
/// for a status Stripe has not documented.
///
/// Stripe's set is wider than Canary's, so some statuses are folded:
///
/// - `incomplete` (the first payment has not succeeded yet) and `unpaid`
///   (renewal retries are exhausted) become `past_due`: a payment is owed
///   and monitoring stops until it is paid, but the subscription can still
///   recover.
/// - `incomplete_expired` (the first payment never succeeded) and `paused`
///   (the trial ended without a payment method) become `expired`: there is
///   no paid period to honour. Stripe reports the change when a paused
///   subscription resumes, which moves it back to `active`.
fn known_status(status: &str) -> Option<SubscriptionStatus> {
    match status {
        "incomplete" | "unpaid" => Some(SubscriptionStatus::PastDue),
        "incomplete_expired" | "paused" => Some(SubscriptionStatus::Expired),
        // `pending` is Canary's own pre-trial state, never a Stripe status.
        "pending" => None,
        status => status.parse().ok(),
    }
}

/// The stored name of a tier from Stripe product or subscription metadata,
/// or `None` when it names no [`SubscriptionTier`]. Matching ignores case,
/// because the metadata is typed by hand in the Stripe dashboard.
fn known_tier(tier: &str) -> Option<&'static str> {
    [SubscriptionTier::Personal, SubscriptionTier::Team]
        .into_iter()
        .find(|known| known.as_str().eq_ignore_ascii_case(tier))
        .map(SubscriptionTier::as_str)
}

pub struct VerifiedWebhookEvent {
    pub event_id: String,
    pub event_created: i64,
}

/// New Stripe billing service using our custom client with 2025 API
pub struct StripeBilling {
    client: StripeClientService,
    webhook_secret: String,
    // Cache full pricing info loaded from Stripe on startup
    cached_pricing: PricingInfo,
    // Metadata database for user lookups during webhook processing
    metadata_db: Arc<MetadataDb>,
    /// Sends trial ending emails; `None` when Resend is not configured.
    email_service: Option<Arc<EmailService>>,
}

impl StripeBilling {
    pub async fn new(
        metadata_db: Arc<MetadataDb>,
        config: &AppConfig,
        email_service: Option<Arc<EmailService>>,
    ) -> Result<Self> {
        let stripe = config.stripe();
        let secret_key = stripe
            .secret_key
            .as_ref()
            .map(|key| key.expose().to_string())
            .ok_or_else(|| anyhow::anyhow!("STRIPE_SECRET_KEY environment variable not set"))?;

        let webhook_secret = stripe
            .webhook_secret
            .as_ref()
            .map(|secret| secret.expose().to_string())
            .ok_or_else(|| anyhow::anyhow!("STRIPE_WEBHOOK_SECRET environment variable not set"))?;

        let client = StripeClientService::new(secret_key);

        let mut billing = Self {
            client,
            webhook_secret,
            cached_pricing: PricingInfo {
                tiers: Vec::new(),
                yearly_discount_percent: None,
            },
            metadata_db,
            email_service,
        };

        // Load products and prices from Stripe on startup
        billing.load_products_and_prices().await?;

        Ok(billing)
    }

    // Helper method to extract user_id from Stripe customer_id
    fn extract_user_id_from_customer(customer_id: &str) -> String {
        // If customer_id follows our pattern "canary_user_{user_id}"
        if customer_id.starts_with("canary_user_") {
            customer_id
                .strip_prefix("canary_user_")
                .unwrap_or(customer_id)
                .to_string()
        } else {
            // For Stripe-generated customer IDs, return with prefix for lookup
            format!("stripe_customer:{customer_id}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stripe_statuses_map_onto_storable_statuses() {
        for (stripe, canary) in [
            ("trialing", SubscriptionStatus::Trialing),
            ("active", SubscriptionStatus::Active),
            ("past_due", SubscriptionStatus::PastDue),
            ("canceled", SubscriptionStatus::Canceled),
            ("incomplete", SubscriptionStatus::PastDue),
            ("unpaid", SubscriptionStatus::PastDue),
            ("incomplete_expired", SubscriptionStatus::Expired),
            ("paused", SubscriptionStatus::Expired),
        ] {
            assert_eq!(known_status(stripe), Some(canary), "{stripe}");
        }
        for unknown in ["pending", "", "Active", "something_new"] {
            assert_eq!(known_status(unknown), None, "{unknown}");
        }
    }

    #[test]
    fn tiers_match_case_insensitively() {
        assert_eq!(known_tier("team"), Some("team"));
        assert_eq!(known_tier("Team"), Some("team"));
        assert_eq!(known_tier("PERSONAL"), Some("personal"));
        assert_eq!(known_tier("pro"), None);
        assert_eq!(known_tier("keep_current"), None);
    }
}
