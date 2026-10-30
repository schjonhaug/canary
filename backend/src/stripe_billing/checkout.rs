//! Stripe customers and the sessions that start or manage a subscription:
//! customer creation, trial subscriptions, checkout sessions and the customer
//! portal.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{StripeBilling, known_status};
use crate::metadata::{SubscriptionStatus, SubscriptionUpdateParams, UserRecord};
use crate::subscription::SubscriptionTier;

#[derive(Debug, Serialize, Deserialize)]
pub struct CheckoutSessionResponse {
    pub url: String,
    pub session_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CustomerPortalResponse {
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CheckoutSessionDetails {
    pub session_id: String,
    pub customer_id: Option<String>,
    pub subscription_id: Option<String>,
    pub status: Option<String>,
    pub tier: Option<String>,
    pub billing_period: Option<String>,
    pub amount_total: Option<i64>,
    pub currency: Option<String>,
}

impl StripeBilling {
    pub async fn create_checkout_session(
        &self,
        user_id: &str,
        tier: SubscriptionTier,
        billing_cycle: &str, // "monthly" or "yearly"
        success_url: &str,
        cancel_url: &str,
        metadata_db: &crate::metadata::MetadataDb,
    ) -> Result<CheckoutSessionResponse> {
        tracing::info!("Creating checkout session");

        // Get price ID from cached pricing data
        let tier_str = tier.as_str();
        let tier_pricing = self
            .cached_pricing
            .tiers
            .iter()
            .find(|t| t.tier == tier_str)
            .ok_or_else(|| anyhow::anyhow!("No pricing found for tier {tier:?}"))?;

        let price_info = match billing_cycle {
            "yearly" => tier_pricing
                .yearly_price
                .as_ref()
                .or(tier_pricing.monthly_price.as_ref()),
            _ => tier_pricing
                .monthly_price
                .as_ref()
                .or(tier_pricing.yearly_price.as_ref()),
        }
        .ok_or_else(|| {
            anyhow::anyhow!("No price found for tier {tier:?} with billing cycle {billing_cycle}")
        })?;

        let price_id = price_info.price_id.clone();
        tracing::info!("💰 Using price ID: {}", price_id);

        // Upsells are now configured directly in Stripe Dashboard on the monthly price
        if billing_cycle != "yearly" {
            tracing::info!("🎯 Using monthly price with Stripe Dashboard upsell configuration");
        }

        // Look up user's Stripe customer ID from database
        let user = metadata_db
            .get_user_by_id(user_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User not found: {user_id}"))?;

        let customer_id = user.stripe_customer_id
            .ok_or_else(|| anyhow::anyhow!("User {user_id} does not have a Stripe customer ID. Trial may not have been created properly."))?;

        tracing::info!("Using existing billing customer");

        // Check if user is trying to upgrade/downgrade their subscription
        let subscription_list = self.client.list_subscriptions(&customer_id).await?;
        let subscriptions = subscription_list.data.unwrap_or_default();

        let mut has_active_subscription = false;
        let mut current_tier: Option<String> = None;

        for subscription in &subscriptions {
            if let Some(status) = &subscription.status
                && matches!(
                    known_status(status),
                    Some(SubscriptionStatus::Active | SubscriptionStatus::PastDue)
                )
            {
                has_active_subscription = true;

                // Extract current tier from subscription metadata
                if let Some(metadata) = &subscription.metadata
                    && let Some(tier_from_metadata) = metadata.get("tier")
                {
                    current_tier = Some(tier_from_metadata.to_lowercase());
                }
                break;
            }
        }

        if has_active_subscription {
            let target_tier = tier.as_str().to_lowercase();
            if let Some(ref current) = current_tier {
                if current == &target_tier {
                    tracing::warn!("Checkout rejected: subscription tier already active");
                    return Err(anyhow::anyhow!("You already have a {current} subscription"));
                }
                tracing::info!("Processing subscription tier upgrade");
            } else {
                tracing::info!("Processing subscription tier change");
            }
        } else {
            tracing::info!("✅ User has no active paid subscription, proceeding with checkout");
        }

        // Create subscription metadata
        let mut metadata = HashMap::new();
        metadata.insert("user_id".to_string(), user_id.to_string());
        metadata.insert("tier".to_string(), tier.as_str().to_string());
        metadata.insert("billing_period".to_string(), billing_cycle.to_string());

        let session = self
            .client
            .create_checkout_session(
                customer_id,
                price_id,
                success_url.to_string(),
                cancel_url.to_string(),
                metadata,
            )
            .await?;

        let checkout_url = session.url.unwrap_or_default();
        let session_id = session.id.unwrap_or_default();

        tracing::info!("Checkout session created");

        Ok(CheckoutSessionResponse {
            url: checkout_url,
            session_id,
        })
    }

    pub async fn create_customer_portal_session(
        &self,
        stripe_customer_id: &str,
        return_url: &str,
    ) -> Result<CustomerPortalResponse> {
        let session = self
            .client
            .create_billing_portal_session(stripe_customer_id.to_string(), return_url.to_string())
            .await?;

        Ok(CustomerPortalResponse {
            url: session.url.unwrap_or_default(),
        })
    }

    /// Create only a Stripe customer (no subscription) during registration
    pub async fn create_stripe_customer_only(
        &self,
        user: &UserRecord,
        metadata_db: &crate::metadata::MetadataDb,
    ) -> Result<String> {
        let mut customer_metadata = HashMap::new();
        customer_metadata.insert("user_id".to_string(), user.id.clone());

        tracing::info!("Creating billing customer");

        let customer = self
            .client
            .create_customer(user.email.clone(), user.name.clone(), customer_metadata)
            .await?;

        let customer_id = customer.id.unwrap_or_default();

        // Update user with Stripe customer ID in database
        metadata_db
            .update_user_stripe_customer(&user.id, &customer_id)
            .await?;

        tracing::info!("Billing customer created");

        Ok(customer_id)
    }

    /// Create a trial subscription for a user (called when they add their first wallet)
    pub async fn create_trial_subscription(
        &self,
        user: &UserRecord,
        tier: SubscriptionTier,
        metadata_db: &crate::metadata::MetadataDb,
    ) -> Result<()> {
        // Ensure pricing is loaded
        if self.cached_pricing.tiers.is_empty() {
            return Err(anyhow::anyhow!(
                "No pricing data available. Please ensure Stripe products are configured."
            ));
        }

        // Find the tier pricing
        let tier_pricing = self
            .cached_pricing
            .tiers
            .iter()
            .find(|t| t.tier.to_lowercase() == format!("{tier:?}").to_lowercase())
            .ok_or_else(|| anyhow::anyhow!("No pricing found for tier {tier:?}"))?;

        // Get the monthly price ID for the tier (trials should use monthly pricing)
        let price_info = tier_pricing
            .monthly_price
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No monthly price found for tier {tier:?}"))?;

        let price_id = price_info.price_id.clone();

        // Get or create Stripe customer
        let customer_id = match &user.stripe_customer_id {
            Some(id) => id.clone(),
            None => {
                // Create customer first if they don't have one
                self.create_stripe_customer_only(user, metadata_db).await?
            }
        };

        tracing::info!("Creating trial subscription");

        // Create subscription metadata
        let mut metadata = HashMap::new();
        metadata.insert("user_id".to_string(), user.id.clone());
        metadata.insert("tier".to_string(), format!("{tier:?}"));

        // Create subscription with 30-day trial
        let subscription = self
            .client
            .create_subscription(
                customer_id.clone(),
                price_id.clone(),
                Some(30), // 30-day trial
                metadata,
            )
            .await?;

        let subscription_id = subscription.id.unwrap_or_default();

        // Update user's subscription info in database
        let now = chrono::Utc::now();
        let trial_end = now + chrono::Duration::days(30);
        let now_str = now.to_rfc3339();
        let trial_end_str = trial_end.to_rfc3339();
        let tier_str = format!("{tier:?}").to_lowercase();

        let sub_params = SubscriptionUpdateParams {
            subscription_tier: &tier_str,
            subscription_status: SubscriptionStatus::Trialing,
            stripe_subscription_id: Some(&subscription_id),
            subscription_started_at: Some(&now_str),
            subscription_ends_at: None, // subscription_ends_at - not set for trials
            trial_ends_at: Some(&trial_end_str),
        };
        metadata_db
            .update_user_subscription(&user.id, &sub_params)
            .await?;

        tracing::info!("Trial subscription created");

        Ok(())
    }

    pub async fn get_checkout_session_details(
        &self,
        session_id: &str,
    ) -> Result<CheckoutSessionDetails> {
        // Get the checkout session from Stripe
        let session = self.client.get_checkout_session(session_id).await?;

        // Map payment_status to simpler status for frontend
        let status = session.payment_status.as_deref().map(|s| {
            match s {
                "paid" => "complete",
                "unpaid" => "pending",
                _ => s,
            }
            .to_string()
        });

        // Extract tier and billing period from metadata
        let mut tier: Option<String> = None;
        let mut billing_period: Option<String> = None;

        if let Some(metadata) = &session.metadata {
            tier = metadata.get("tier").cloned();
            billing_period = metadata.get("billing_period").cloned();
        }

        Ok(CheckoutSessionDetails {
            session_id: session.id.unwrap_or_else(|| session_id.to_string()),
            customer_id: session.customer,
            subscription_id: session.subscription,
            status,
            tier,
            billing_period,
            amount_total: session.amount_total,
            currency: session.currency,
        })
    }
}
