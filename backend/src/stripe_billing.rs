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
    // Cache full pricing info loaded from Stripe on startup
    cached_pricing: PricingInfo,
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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PricingInfo {
    pub tiers: Vec<FrontendTierPricing>,
    pub yearly_discount_percent: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FrontendTierPricing {
    pub tier: String,
    pub name: String,
    pub description: Option<String>,
    pub monthly_price: Option<FrontendPriceInfo>,
    pub yearly_price: Option<FrontendPriceInfo>,
    pub features: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FrontendPriceInfo {
    pub price_id: String,
    pub amount: i64, // amount in cents
    pub currency: String,
    pub interval: String,
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
            cached_pricing: PricingInfo {
                tiers: Vec::new(),
                yearly_discount_percent: None,
            },
        };
        
        // Load products and prices from Stripe on startup
        billing.load_products_and_prices().await?;
        
        Ok(billing)
    }
    
    /// Load products and prices from Stripe and build frontend pricing structure
    async fn load_products_and_prices(&mut self) -> Result<()> {
        tracing::info!("🔍 Loading products and prices from Stripe...");
        
        // Fetch all products from Stripe
        let product_list = self.client.list_products(Some(100)).await?;
        let products = product_list.data.unwrap_or_default();
        
        tracing::info!("📦 Found {} products in Stripe", products.len());
        
        let mut tiers: HashMap<String, FrontendTierPricing> = HashMap::new();
        let mut monthly_amounts: Vec<i64> = Vec::new();
        let mut yearly_amounts: Vec<i64> = Vec::new();
        
        for product in products {
            // Skip archived or inactive products
            if product.active != Some(true) {
                continue;
            }

            // Check if product has tier metadata
            if let Some(metadata) = &product.metadata {
                if let Some(tier_str) = metadata.get("tier") {
                    let product_id = product.id.clone().unwrap_or_default();
                    
                    tracing::info!("🎯 Found product {} for tier {}", product_id, tier_str);
                    
                    // Get all prices for this product
                    let price_list = self.client.list_prices(Some(100), Some(product_id)).await?;
                    let prices = price_list.data.unwrap_or_default();
                    
                    // Initialize tier pricing if not exists
                    if !tiers.contains_key(tier_str) {
                        tiers.insert(tier_str.clone(), FrontendTierPricing {
                            tier: tier_str.clone(),
                            name: self.get_tier_display_name(tier_str),
                            description: self.get_tier_description(tier_str),
                            monthly_price: None,
                            yearly_price: None,
                            features: self.get_tier_features(tier_str),
                        });
                    }
                    
                    let tier_pricing = tiers.get_mut(tier_str).unwrap();
                    
                    // Process all active prices for this product
                    for price in prices {
                        // Skip archived prices
                        if price.active != Some(true) {
                            continue;
                        }
                        
                        if let Some(recurring) = &price.recurring {
                            let interval = recurring.interval.as_deref().unwrap_or("");
                            let interval_count = recurring.interval_count.unwrap_or(0);
                            let price_id = price.id.clone().unwrap_or_default();
                            let amount = price.unit_amount.unwrap_or(0);
                            let currency = price.currency.as_deref().unwrap_or("USD").to_uppercase();
                            
                            if interval == "month" && interval_count == 1 {
                                // Prefer the most recent or lowest monthly price
                                if tier_pricing.monthly_price.is_none() || amount < tier_pricing.monthly_price.as_ref().unwrap().amount {
                                    tier_pricing.monthly_price = Some(FrontendPriceInfo {
                                        price_id: price_id.clone(),
                                        amount,
                                        currency: currency.clone(),
                                        interval: "month".to_string(),
                                    });
                                    tracing::info!("💰 Found monthly price for {}: ${:.2}/{} ({})", 
                                        tier_str, amount as f64 / 100.0, currency, price_id);
                                }
                                monthly_amounts.push(amount);
                            } else if interval == "year" && interval_count == 1 {
                                // Prefer the lowest yearly price (discounted price)
                                if tier_pricing.yearly_price.is_none() || amount < tier_pricing.yearly_price.as_ref().unwrap().amount {
                                    tier_pricing.yearly_price = Some(FrontendPriceInfo {
                                        price_id: price_id.clone(),
                                        amount,
                                        currency: currency.clone(),
                                        interval: "year".to_string(),
                                    });
                                    tracing::info!("💰 Found yearly price for {}: ${:.2}/{} ({})", 
                                        tier_str, amount as f64 / 100.0, currency, price_id);
                                }
                                yearly_amounts.push(amount);
                            }
                        }
                    }
                }
            }
        }

        // Calculate discount percentage from actual price differences (now that yearly prices have discount built-in)
        let discount_percent = if !monthly_amounts.is_empty() && !yearly_amounts.is_empty() {
            let mut total_discount = 0.0;
            let mut count = 0;
            
            // Calculate discount for each tier by comparing monthly*12 vs yearly price
            for tier_pricing in tiers.values() {
                if let (Some(monthly), Some(yearly)) = (&tier_pricing.monthly_price, &tier_pricing.yearly_price) {
                    let monthly_total = (monthly.amount * 12) as f64;
                    let yearly_price = yearly.amount as f64;
                    
                    if yearly_price < monthly_total {
                        let discount = (monthly_total - yearly_price) / monthly_total * 100.0;
                        total_discount += discount;
                        count += 1;
                        tracing::info!("📊 {} tier: {}% yearly discount (${:.2}/year vs ${:.2}/year)", 
                            tier_pricing.tier, discount, yearly_price / 100.0, monthly_total / 100.0);
                    }
                }
            }
            
            if count > 0 {
                Some((total_discount / count as f64).round())
            } else {
                None
            }
        } else {
            None
        };

        // Store the collected pricing data
        self.cached_pricing = PricingInfo {
            tiers: tiers.into_values().collect(),
            yearly_discount_percent: discount_percent,
        };

        // Sort tiers by display order (Personal -> Pro -> Business)
        self.cached_pricing.tiers.sort_by(|a, b| {
            let order_a = match a.tier.to_lowercase().as_str() {
                "personal" => 1,
                "pro" => 2,
                "business" => 3,
                _ => 99,
            };
            let order_b = match b.tier.to_lowercase().as_str() {
                "personal" => 1,
                "pro" => 2,
                "business" => 3,
                _ => 99,
            };
            order_a.cmp(&order_b)
        });

        tracing::info!("✅ Loaded {} pricing tiers with {}% yearly discount", 
            self.cached_pricing.tiers.len(), 
            discount_percent.unwrap_or(0.0));

        Ok(())
    }

    fn get_tier_display_name(&self, tier: &str) -> String {
        match tier.to_lowercase().as_str() {
            "personal" => "Personal".to_string(),
            "pro" => "Pro".to_string(),
            "business" => "Business".to_string(),
            _ => tier.to_string(),
        }
    }

    fn get_tier_description(&self, tier: &str) -> Option<String> {
        match tier.to_lowercase().as_str() {
            "personal" => Some("For individual Bitcoin holders".to_string()),
            "pro" => Some("For Uncle Jims & family guardians".to_string()),
            "business" => Some("For businesses & services".to_string()),
            _ => None,
        }
    }


    fn get_tier_features(&self, tier: &str) -> HashMap<String, String> {
        let mut features = HashMap::new();
        
        // Base features for all tiers
        features.insert("trial".to_string(), "30-day free trial".to_string());
        features.insert("email".to_string(), "Email notifications".to_string());

        match tier.to_lowercase().as_str() {
            "personal" => {
                features.insert("wallets".to_string(), "1 wallet".to_string());
                features.insert("contacts".to_string(), "1 contact".to_string());
                features.insert("sync".to_string(), "5 minute sync time".to_string());
            },
            "pro" => {
                features.insert("wallets".to_string(), "15 wallets".to_string());
                features.insert("contacts".to_string(), "10 contacts per wallet".to_string());
                features.insert("sync".to_string(), "1 minute sync time".to_string());
                features.insert("sms".to_string(), "SMS notifications".to_string());
                features.insert("push".to_string(), "Push notifications".to_string());
                features.insert("analysis".to_string(), "Transaction analysis (RBF/CPFP)".to_string());
            },
            "business" => {
                features.insert("wallets".to_string(), "Unlimited wallets".to_string());
                features.insert("contacts".to_string(), "Unlimited contacts".to_string());
                features.insert("sync".to_string(), "5 second sync time".to_string());
                features.insert("sms".to_string(), "SMS notifications".to_string());
                features.insert("push".to_string(), "Push notifications".to_string());
                features.insert("analysis".to_string(), "Transaction analysis (RBF/CPFP)".to_string());
                features.insert("api".to_string(), "REST API access".to_string());
                features.insert("webhooks".to_string(), "Custom webhooks".to_string());
            },
            _ => {},
        }

        features
    }

    pub async fn create_checkout_session(
        &self,
        user_id: &str,
        tier: SubscriptionTier,
        billing_cycle: &str, // "monthly" or "yearly"
        success_url: &str,
        cancel_url: &str,
        coupon_id: Option<String>,
        metadata_db: &crate::metadata::MetadataDb,
    ) -> Result<CheckoutSessionResponse> {
        tracing::info!("🛒 Creating checkout session for user {} with tier {:?}, billing: {}", user_id, tier, billing_cycle);
        
        // Get price ID from cached pricing data
        let tier_str = tier.as_str();
        let tier_pricing = self.cached_pricing.tiers.iter()
            .find(|t| t.tier == tier_str)
            .ok_or_else(|| anyhow::anyhow!("No pricing found for tier {:?}", tier))?;
        
        let price_info = match billing_cycle {
            "yearly" => tier_pricing.yearly_price.as_ref()
                .or(tier_pricing.monthly_price.as_ref()),
            _ => tier_pricing.monthly_price.as_ref()
                .or(tier_pricing.yearly_price.as_ref()),
        }.ok_or_else(|| anyhow::anyhow!("No price found for tier {:?} with billing cycle {}", tier, billing_cycle))?;
        
        let price_id = price_info.price_id.clone();
        tracing::info!("💰 Using price ID: {}", price_id);

        // Upsells are now configured directly in Stripe Dashboard on the monthly price
        if billing_cycle != "yearly" {
            tracing::info!("🎯 Using monthly price with Stripe Dashboard upsell configuration");
        }

        // Look up user's Stripe customer ID from database
        let user = metadata_db.get_user_by_id(user_id).await?
            .ok_or_else(|| anyhow::anyhow!("User not found: {}", user_id))?;
        
        let customer_id = user.stripe_customer_id
            .ok_or_else(|| anyhow::anyhow!("User {} does not have a Stripe customer ID. Trial may not have been created properly.", user_id))?;
        
        tracing::info!("🔍 Using Stripe customer ID: {}", customer_id);

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
        
        let checkout_url = session.url.unwrap_or_default();
        let session_id = session.id.unwrap_or_default();
        
        tracing::info!("✅ Checkout session created: {} (URL: {})", session_id, checkout_url);
        
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
        // Get cached price ID for this tier (default to monthly)
        let tier_str = tier.as_str();
        let tier_pricing = self.cached_pricing.tiers.iter()
            .find(|t| t.tier == tier_str)
            .ok_or_else(|| anyhow::anyhow!("No pricing found for tier {:?}. Make sure products are loaded from Stripe.", tier))?;
        
        let price_info = tier_pricing.monthly_price.as_ref()
            .or(tier_pricing.yearly_price.as_ref())
            .ok_or_else(|| anyhow::anyhow!("No price found for tier {:?}. Make sure products have prices in Stripe.", tier))?;
        
        let price_id = price_info.price_id.clone();
        
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
        self.cached_pricing.clone()
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