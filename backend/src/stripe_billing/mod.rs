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

#[derive(Debug, Clone)]
pub struct WebhookResult {
    pub subscription_updates: Vec<SubscriptionUpdate>,
    pub trial_ending_notifications: Vec<TrialEndingNotification>,
}

#[derive(Debug, Clone)]
pub struct SubscriptionUpdate {
    pub user_id: String,
    pub subscription_tier: String,
    /// The status as Stripe reported it, not a [`SubscriptionStatus`].
    ///
    /// Stripe's set is open-ended and wider than Canary's (`incomplete`,
    /// `unpaid`, `paused`, ...). A value Canary does not store is rejected by
    /// the `users.subscription_status` CHECK constraint when the update is
    /// written, which rolls back the event so Stripe retries it. Parsing it
    /// into the enum here would move that failure earlier and change how such
    /// events are handled.
    pub subscription_status: String,
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

/// Parses a status reported by Stripe, returning `None` for values outside
/// [`SubscriptionStatus`].
fn known_status(status: &str) -> Option<SubscriptionStatus> {
    status.parse().ok()
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
