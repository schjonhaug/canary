use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;
use chrono;

use crate::metadata::UserRecord;
use crate::subscription::SubscriptionTier;
use crate::stripe_client_service::StripeClientService;

#[derive(Debug, Clone)]
pub struct WebhookResult {
    pub subscription_updates: Vec<SubscriptionUpdate>,
}

#[derive(Debug, Clone)]
pub struct SubscriptionUpdate {
    pub user_id: String,
    pub subscription_tier: String,
    pub subscription_status: String,
    pub stripe_subscription_id: Option<String>,
    pub subscription_started_at: Option<String>,
    pub trial_ends_at: Option<String>,
}

/// New Stripe billing service using our custom client with 2025 API
pub struct StripeBilling {
    client: StripeClientService,
    webhook_secret: String,
    // Cache pricing info loaded from Stripe on startup
    cached_prices: HashMap<SubscriptionTier, String>, // tier -> monthly price_id
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CheckoutSessionResponse {
    pub url: String,
    pub session_id: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CustomerPortalResponse {
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CheckoutSessionDetails {
    pub session_id: String,
    pub customer_id: Option<String>,
    pub subscription_id: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PricingInfo {
    pub tiers: Vec<TierPricing>,
    pub yearly_discount_percent: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TierPricing {
    pub tier: String,
    pub name: String,
    pub monthly: Option<PriceDetails>,
    pub yearly: Option<PriceDetails>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PriceDetails {
    pub price_id: String,
    pub unit_amount: i64,
    pub currency: String,
    pub formatted_amount: String,
}

impl StripeBilling {
    pub async fn new() -> Result<Self> {
        let secret_key = std::env::var("STRIPE_SECRET_KEY")
            .map_err(|_| anyhow::anyhow!("STRIPE_SECRET_KEY environment variable not set"))?;
        
        let webhook_secret = std::env::var("STRIPE_WEBHOOK_SECRET")
            .map_err(|_| anyhow::anyhow!("STRIPE_WEBHOOK_SECRET environment variable not set"))?;

        let client = StripeClientService::new(secret_key);
        
        let mut billing = Self {
            client,
            webhook_secret,
            cached_prices: HashMap::new(),
        };
        
        // Load products and prices from Stripe on startup
        billing.load_products_and_prices().await?;
        
        Ok(billing)
    }
    
    /// Load products and prices from Stripe based on metadata (like the old implementation)
    async fn load_products_and_prices(&mut self) -> Result<()> {
        tracing::info!("🔍 Loading products and prices from Stripe...");
        
        // Fetch all products from Stripe
        let product_list = self.client.list_products(Some(100)).await?;
        let products = product_list.data.unwrap_or_default();
        
        tracing::info!("📦 Found {} products in Stripe", products.len());
        
        for product in products {
            // Skip archived or inactive products
            if product.active != Some(true) {
                continue;
            }

            // Check if product has tier metadata
            if let Some(metadata) = &product.metadata {
                if let Some(tier_str) = metadata.get("tier") {
                    let tier = SubscriptionTier::from(tier_str.as_str());
                    let product_id = product.id.clone().unwrap_or_default();
                    
                    tracing::info!("🎯 Found product {} for tier {:?}", product_id, tier);
                    
                    // Get all prices for this product
                    let price_list = self.client.list_prices(Some(100), Some(product_id)).await?;
                    let prices = price_list.data.unwrap_or_default();
                    
                    // Store monthly price
                    for price in prices {
                        if let Some(recurring) = &price.recurring {
                            let interval = recurring.interval.as_deref().unwrap_or("");
                            let interval_count = recurring.interval_count.unwrap_or(0);
                            
                            if interval == "month" && interval_count == 1 {
                                let price_id = price.id.clone().unwrap_or_default();
                                let amount = price.unit_amount.unwrap_or(0);
                                let currency = price.currency.as_deref().unwrap_or("USD");
                                
                                self.cached_prices.insert(tier, price_id.clone());
                                tracing::info!("💰 Found monthly price for {:?}: ${:.2}/{} ({})", 
                                    tier, amount as f64 / 100.0, currency.to_uppercase(), price_id);
                                break;
                            }
                        }
                    }
                }
            }
        }

        // Validate we have required prices
        for tier in [SubscriptionTier::Personal, SubscriptionTier::Pro, SubscriptionTier::Business] {
            if !self.cached_prices.contains_key(&tier) {
                tracing::warn!("⚠️ No monthly price found for tier {:?} in Stripe. Please create product with metadata tier={} and a monthly price.", tier, tier.as_str());
            }
        }

        Ok(())
    }

    pub async fn create_checkout_session(
        &self,
        user_id: &str,
        tier: SubscriptionTier,
        billing_cycle: &str, // "monthly" or "yearly"
        success_url: &str,
        cancel_url: &str,
        coupon_id: Option<String>,
    ) -> Result<CheckoutSessionResponse> {
        tracing::info!("🛒 Creating checkout session for user {} with tier {:?}, billing: {}", user_id, tier, billing_cycle);
        
        // Get price ID from cached prices (for now, only monthly)
        let price_id = self.cached_prices.get(&tier)
            .ok_or_else(|| anyhow::anyhow!("No price found for tier {:?}", tier))?
            .clone();
        tracing::info!("💰 Using price ID: {}", price_id);

        // Use user_id as customer identifier for now
        let customer_id = format!("canary_user_{}", user_id);

        // Create subscription metadata
        let mut metadata = HashMap::new();
        metadata.insert("user_id".to_string(), user_id.to_string());
        metadata.insert("tier".to_string(), format!("{:?}", tier));

        let session = self.client.create_checkout_session(
            customer_id,
            price_id,
            success_url.to_string(),
            cancel_url.to_string(),
            metadata,
            coupon_id,
        ).await?;
        
        Ok(CheckoutSessionResponse {
            url: session.url.unwrap_or_default(),
            session_id: session.id.unwrap_or_default(),
        })
    }

    pub async fn create_customer_portal_session(
        &self,
        stripe_customer_id: &str,
        return_url: &str,
    ) -> Result<CustomerPortalResponse> {
        let session = self.client.create_billing_portal_session(
            stripe_customer_id.to_string(),
            return_url.to_string(),
        ).await?;

        Ok(CustomerPortalResponse {
            url: session.url.unwrap_or_default(),
        })
    }

    pub async fn create_trial_subscription(
        &self,
        user: &UserRecord,
        tier: SubscriptionTier,
        metadata_db: &crate::metadata::MetadataDb,
    ) -> Result<()> {
        // Get cached price ID for this tier
        let price_id = self.cached_prices.get(&tier)
            .ok_or_else(|| anyhow::anyhow!("No price found for tier {:?}. Make sure products are loaded from Stripe.", tier))?
            .clone();
        
        tracing::info!("🆕 Creating trial subscription for user {} with tier {:?} (price: {})", user.email, tier, price_id);
        
        // Create Stripe customer first
        let mut customer_metadata = HashMap::new();
        customer_metadata.insert("user_id".to_string(), user.id.clone());
        customer_metadata.insert("tier".to_string(), format!("{:?}", tier));

        let customer = self.client.create_customer(
            user.email.clone(),
            user.name.clone(),
            customer_metadata.clone(),
        ).await?;

        // Create subscription with 30-day trial
        let _subscription = self.client.create_subscription(
            customer.id.clone().unwrap_or_default(),
            price_id,
            Some(30), // 30-day trial
            customer_metadata,
        ).await?;

        // Update user with Stripe customer ID in database
        metadata_db.update_user_stripe_customer(
            &user.id,
            &customer.id.unwrap_or_default(),
        ).await?;

        tracing::info!("✅ Trial subscription created successfully for user {}", user.email);
        Ok(())
    }

    pub async fn handle_webhook(&self, payload: &[u8], signature: &str) -> Result<WebhookResult> {
        // Use stripe library for webhook verification (security critical)
        let payload_str = std::str::from_utf8(payload)?;
        let event = self.client.parse_webhook_event(payload_str, signature, &self.webhook_secret).await?;
        
        let mut updates = Vec::new();

        // Handle different event types with 2025 API structure
        if let Some(event_type) = &event.r#type {
            match event_type.as_str() {
                "customer.subscription.trial_will_end" => {
                    // Fired 3 days before trial ends - we can notify the user
                    tracing::info!("⏰ Trial ending soon for subscription");
                    // TODO: Send notification to user about trial ending in 3 days
                }
                "checkout.session.completed" => {
                    if let Some(data) = &event.data {
                        if let Some(_session_data) = &data.object {
                            // Parse checkout session data
                            tracing::info!("📋 Processing checkout.session.completed");
                            // TODO: Extract subscription info and create update
                        }
                    }
                }
                "customer.subscription.created" => {
                    if let Some(data) = &event.data {
                        if let Some(subscription_obj) = &data.object {
                            tracing::info!("📋 Processing customer.subscription.created");
                            
                            // Parse subscription to extract trial info
                            if let Ok(subscription) = serde_json::from_value::<serde_json::Value>(subscription_obj.clone()) {
                                let status = subscription.get("status").and_then(|s| s.as_str());
                                let trial_end = subscription.get("trial_end").and_then(|t| t.as_i64());
                                let customer_id = subscription.get("customer").and_then(|c| c.as_str());
                                let subscription_id = subscription.get("id").and_then(|s| s.as_str());
                                
                                tracing::info!("🆕 New subscription - Status: {:?}, Trial end: {:?}, Customer: {:?}", 
                                    status, trial_end, customer_id);
                                
                                if let (Some(customer_id), Some(subscription_id)) = (customer_id, subscription_id) {
                                    // Convert trial_end timestamp to ISO string
                                    let trial_ends_at = trial_end.map(|ts| {
                                        chrono::DateTime::from_timestamp(ts, 0)
                                            .map(|dt| dt.to_rfc3339())
                                            .unwrap_or_default()
                                    });
                                    
                                    let update = SubscriptionUpdate {
                                        user_id: self.extract_user_id_from_customer(customer_id),
                                        subscription_tier: "pro".to_string(), // Keep the actual tier (Pro for trials)
                                        subscription_status: status.unwrap_or("unknown").to_string(), // "trialing" from Stripe
                                        stripe_subscription_id: Some(subscription_id.to_string()),
                                        subscription_started_at: Some(chrono::Utc::now().to_rfc3339()),
                                        trial_ends_at,
                                    };
                                    updates.push(update);
                                }
                            }
                        }
                    }
                }
                "customer.subscription.updated" => {
                    // Check if this is a trial ending (status change from trialing -> active/past_due)
                    if let Some(data) = &event.data {
                        if let Some(subscription_obj) = &data.object {
                            // Parse the subscription object to check for trial ending
                            if let Ok(subscription) = serde_json::from_value::<serde_json::Value>(subscription_obj.clone()) {
                                let current_status = subscription.get("status").and_then(|s| s.as_str());
                                
                                // Check previous_attributes to see if status changed from "trialing"  
                                if let Some(previous_attrs) = subscription.get("previous_attributes") {
                                    if let Some(previous_status) = previous_attrs.get("status").and_then(|s| s.as_str()) {
                                        if previous_status == "trialing" && current_status != Some("trialing") {
                                            tracing::info!("🔄 TRIAL ENDED: Status changed from trialing to {:?}", current_status);
                                            
                                            // Extract customer and subscription info
                                            if let Some(customer_id) = subscription.get("customer").and_then(|c| c.as_str()) {
                                                if let Some(subscription_id) = subscription.get("id").and_then(|s| s.as_str()) {
                                                    // Create update to stop wallet syncing for this user
                                                    let update = SubscriptionUpdate {
                                                        user_id: self.extract_user_id_from_customer(customer_id),
                                                        subscription_tier: "trial_ended".to_string(),
                                                        subscription_status: current_status.unwrap_or("unknown").to_string(),
                                                        stripe_subscription_id: Some(subscription_id.to_string()),
                                                        subscription_started_at: None,
                                                        trial_ends_at: None,
                                                    };
                                                    updates.push(update);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                "customer.subscription.deleted" => {
                    // Subscription cancelled/ended completely - stop wallet syncing
                    tracing::info!("🗑️ Subscription cancelled/deleted");
                    // TODO: Create update to stop wallet syncing
                }
                "invoice.payment_succeeded" => {
                    // Payment successful - ensure user has full access
                    tracing::info!("💰 Payment succeeded - subscription is active");
                    // TODO: Ensure user has full access and wallet syncing
                }
                "invoice.payment_failed" => {
                    // Payment failed - subscription may go to past_due
                    tracing::info!("❌ Payment failed - subscription may be suspended");
                    // TODO: Handle payment failure appropriately
                }
                _ => {
                    tracing::debug!("🔄 Ignoring webhook event type: {}", event_type);
                }
            }
        }

        Ok(WebhookResult {
            subscription_updates: updates,
        })
    }


    pub async fn get_checkout_session_details(&self, session_id: &str) -> Result<CheckoutSessionDetails> {
        // TODO: Implement using our client service
        // For now, return a placeholder
        Ok(CheckoutSessionDetails {
            session_id: session_id.to_string(),
            customer_id: None,
            subscription_id: None,
            status: Some("pending".to_string()),
        })
    }

    pub fn get_pricing_for_frontend(&self) -> PricingInfo {
        // TODO: Load pricing dynamically using our client service
        // For now, return a placeholder
        PricingInfo {
            tiers: Vec::new(),
            yearly_discount_percent: Some(20.0),
        }
    }

    // Helper method to extract user_id from Stripe customer_id
    fn extract_user_id_from_customer(&self, customer_id: &str) -> String {
        // If customer_id follows our pattern "canary_user_{user_id}"
        if customer_id.starts_with("canary_user_") {
            customer_id.strip_prefix("canary_user_").unwrap_or(customer_id).to_string()
        } else {
            // For Stripe-generated customer IDs, return with prefix for lookup
            format!("stripe_customer:{}", customer_id)
        }
    }
}