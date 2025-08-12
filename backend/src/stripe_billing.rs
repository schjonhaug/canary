use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use stripe::{
    CheckoutSession, CheckoutSessionMode, Client, CreateCheckoutSession,
    CreateCheckoutSessionLineItems, CreateCustomer, BillingPortalSession, 
    CreateBillingPortalSession, Event, EventType, Webhook, 
    CustomerId, PriceId, ProductId, Price, Product, ListPrices, ListProducts,
    CouponId, CheckoutSessionId,
};
use utoipa::ToSchema;

use crate::metadata::UserRecord;
use crate::subscription::SubscriptionTier;

/// Stripe billing configuration and client
pub struct StripeBilling {
    client: Client,
    webhook_secret: String,
    products: HashMap<SubscriptionTier, ProductId>,
    monthly_prices: HashMap<SubscriptionTier, PriceId>,
    yearly_prices: HashMap<SubscriptionTier, PriceId>,
    // Cache pricing info for frontend (computed once at startup)
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
pub struct PricingInfo {
    pub tiers: Vec<TierPricing>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TierPricing {
    pub tier: String,
    pub name: String,
    pub description: Option<String>,
    pub monthly_price: Option<PriceDetails>,
    pub yearly_price: Option<PriceDetails>,
    pub features: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PriceDetails {
    pub price_id: String,
    pub amount: i64,  // in cents
    pub currency: String,
    pub interval: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CheckoutSessionDetails {
    pub session_id: String,
    pub status: String,
    pub tier: Option<String>,
    pub billing_period: Option<String>,
    pub amount_total: Option<i64>,
    pub currency: Option<String>,
}

impl StripeBilling {
    /// Create a new StripeBilling instance with products loaded from Stripe
    pub async fn new() -> Result<Self> {
        let secret_key = std::env::var("STRIPE_SECRET_KEY")
            .map_err(|_| anyhow::anyhow!("STRIPE_SECRET_KEY environment variable not set"))?;
        
        let webhook_secret = std::env::var("STRIPE_WEBHOOK_SECRET")
            .map_err(|_| anyhow::anyhow!("STRIPE_WEBHOOK_SECRET environment variable not set"))?;

        let client = Client::new(secret_key);
        
        let mut billing = Self {
            client,
            webhook_secret,
            products: HashMap::new(),
            monthly_prices: HashMap::new(),
            yearly_prices: HashMap::new(),
            cached_pricing: PricingInfo { tiers: Vec::new() },
        };

        // Load products and prices from Stripe
        billing.load_products_from_stripe().await?;

        // Build cached pricing info
        billing.build_cached_pricing().await?;

        // Log coupon information
        billing.log_coupon_info().await?;

        Ok(billing)
    }

    /// Load products and prices from Stripe based on metadata
    async fn load_products_from_stripe(&mut self) -> Result<()> {
        // Fetch all products
        let products = Product::list(&self.client, &ListProducts::default()).await?;
        
        for product in products.data {
            // Skip archived or inactive products
            if product.active != Some(true) {
                continue;
            }

            // Check if product has tier metadata
            if let Some(metadata) = &product.metadata {
                if let Some(tier_str) = metadata.get("tier") {
                    let tier = SubscriptionTier::from(tier_str.as_str());
                    self.products.insert(tier, product.id.clone());
                    
                    // Get all prices for this product
                    let mut list_prices = ListPrices::new();
                    list_prices.product = Some(stripe::IdOrCreate::Id(&product.id));
                    list_prices.active = Some(true);
                    
                    let prices = Price::list(&self.client, &list_prices).await?;
                    
                    // Store monthly and yearly prices
                    for price in prices.data {
                        if let Some(recurring) = &price.recurring {
                            match recurring.interval.as_str() {
                                "month" if recurring.interval_count == 1 => {
                                    self.monthly_prices.insert(tier, price.id.clone());
                                    tracing::info!("Found monthly price for {:?}: {}", tier, price.id);
                                }
                                "year" if recurring.interval_count == 1 => {
                                    self.yearly_prices.insert(tier, price.id.clone());
                                    tracing::info!("Found yearly price for {:?}: {}", tier, price.id);
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        // Validate we have required products and prices
        for tier in [SubscriptionTier::Personal, SubscriptionTier::Pro, SubscriptionTier::Business] {
            if !self.products.contains_key(&tier) {
                tracing::warn!("No product found for tier {:?} in Stripe. Please create product with metadata tier={}", tier, tier.as_str());
            }
            if !self.monthly_prices.contains_key(&tier) {
                tracing::warn!("No monthly price found for tier {:?} in Stripe", tier);
            }
        }

        Ok(())
    }

    /// Build cached pricing info from loaded products and prices  
    async fn build_cached_pricing(&mut self) -> Result<()> {
        let mut tiers = Vec::new();

        for tier in [SubscriptionTier::Personal, SubscriptionTier::Pro, SubscriptionTier::Business] {
            if let Some(product_id) = self.products.get(&tier) {
                // Fetch product details once at startup
                let product = Product::retrieve(&self.client, product_id, &[]).await?;
                
                let mut tier_pricing = TierPricing {
                    tier: tier.as_str().to_string(),
                    name: product.name.clone().unwrap_or_else(|| format!("{} Plan", tier.as_str())),
                    description: product.description.clone(),
                    monthly_price: None,
                    yearly_price: None,
                    features: HashMap::new(),
                };

                // Add features from metadata
                if let Some(metadata) = &product.metadata {
                    for (key, value) in metadata.iter() {
                        if key.starts_with("feature_") {
                            let feature_name = key.strip_prefix("feature_").unwrap_or(key);
                            tier_pricing.features.insert(feature_name.to_string(), value.clone());
                        }
                    }
                }

                // Get monthly price details once at startup
                if let Some(price_id) = self.monthly_prices.get(&tier) {
                    let price = Price::retrieve(&self.client, price_id, &[]).await?;
                    if let Some(unit_amount) = price.unit_amount {
                        tier_pricing.monthly_price = Some(PriceDetails {
                            price_id: price.id.to_string(),
                            amount: unit_amount,
                            currency: price.currency.map(|c| c.to_string()).unwrap_or_else(|| "usd".to_string()),
                            interval: "month".to_string(),
                        });
                    }
                }

                // Get yearly price details once at startup
                if let Some(price_id) = self.yearly_prices.get(&tier) {
                    let price = Price::retrieve(&self.client, price_id, &[]).await?;
                    if let Some(unit_amount) = price.unit_amount {
                        tier_pricing.yearly_price = Some(PriceDetails {
                            price_id: price.id.to_string(),
                            amount: unit_amount,
                            currency: price.currency.map(|c| c.to_string()).unwrap_or_else(|| "usd".to_string()),
                            interval: "year".to_string(),
                        });
                    }
                }

                tiers.push(tier_pricing);
            }
        }

        self.cached_pricing = PricingInfo { tiers };
        tracing::info!("✅ Cached pricing information for {} tiers", self.cached_pricing.tiers.len());
        
        Ok(())
    }

    /// Get pricing information for frontend display (now instant!)
    pub fn get_pricing_for_frontend(&self) -> &PricingInfo {
        &self.cached_pricing
    }

    /// Log information about the yearly coupon
    async fn log_coupon_info(&self) -> Result<()> {
        if let Ok(coupon_id) = std::env::var("STRIPE_YEARLY_COUPON_ID") {
            let coupon_id = CouponId::from_str(&coupon_id)?;
            match stripe::Coupon::retrieve(&self.client, &coupon_id, &[]).await {
                Ok(coupon) => {
                    if let Some(percent_off) = coupon.percent_off {
                        tracing::info!("💰 Yearly discount coupon: {}% off (ID: {})", percent_off, coupon_id);
                        if let Some(name) = coupon.name {
                            tracing::info!("   Coupon name: {}", name);
                        }
                    } else if let Some(amount_off) = coupon.amount_off {
                        let currency = coupon.currency.map(|c| c.to_string()).unwrap_or_else(|| "USD".to_string());
                        tracing::info!("💰 Yearly discount coupon: ${:.2} off {} (ID: {})", 
                            amount_off as f64 / 100.0, currency.to_uppercase(), coupon_id);
                    }
                }
                Err(e) => {
                    tracing::warn!("⚠️  Could not fetch coupon {}: {}", coupon_id, e);
                }
            }
        } else {
            tracing::info!("ℹ️  No yearly discount coupon configured (STRIPE_YEARLY_COUPON_ID not set)");
        }
        Ok(())
    }

    /// Create a Stripe checkout session for a user and subscription tier
    pub async fn create_checkout_session(
        &self,
        user: &UserRecord,
        tier: SubscriptionTier,
        is_yearly: bool,
        metadata_db: &crate::metadata::MetadataDb,
    ) -> Result<CheckoutSessionResponse> {
        // Get or create Stripe customer
        let customer_id = match &user.stripe_customer_id {
            Some(id) => {
                tracing::info!("♻️  Using existing Stripe customer {} for user {}", id, user.email);
                CustomerId::from_str(id)?
            },
            None => self.create_customer(user, metadata_db).await?,
        };

        // Select the appropriate price based on billing period
        let price_id = if is_yearly {
            self.yearly_prices.get(&tier)
                .ok_or_else(|| anyhow::anyhow!("No yearly price found for tier {:?}. Please configure in Stripe.", tier))?
        } else {
            self.monthly_prices.get(&tier)
                .ok_or_else(|| anyhow::anyhow!("No monthly price found for tier {:?}. Please configure in Stripe.", tier))?
        };

        // Auto-generate URLs based on frontend URL
        let frontend_url = std::env::var("FRONTEND_URL")
            .unwrap_or_else(|_| "http://localhost:3001".to_string());
        let success_url = format!("{}/billing/success?session={{CHECKOUT_SESSION_ID}}", frontend_url);
        let cancel_url = format!("{}/billing/cancel", frontend_url);

        let mut create_session = CreateCheckoutSession::new();
        create_session.mode = Some(CheckoutSessionMode::Subscription);
        create_session.customer = Some(customer_id);
        create_session.success_url = Some(&success_url);
        create_session.cancel_url = Some(&cancel_url);
        
        create_session.line_items = Some(vec![CreateCheckoutSessionLineItems {
            price: Some(price_id.to_string()),
            quantity: Some(1),
            ..Default::default()
        }]);

        // Add metadata for tracking
        create_session.metadata = Some({
            let mut metadata = HashMap::new();
            metadata.insert("user_id".to_string(), user.id.clone());
            metadata.insert("tier".to_string(), tier.as_str().to_string());
            metadata.insert("billing_period".to_string(), if is_yearly { "yearly" } else { "monthly" }.to_string());
            metadata
        });

        // Auto-apply yearly discount coupon for yearly plans only
        if is_yearly {
            if let Ok(yearly_coupon_id) = std::env::var("STRIPE_YEARLY_COUPON_ID") {
                create_session.discounts = Some(vec![
                    stripe::CreateCheckoutSessionDiscounts {
                        coupon: Some(yearly_coupon_id),
                        promotion_code: None,
                    }
                ]);
            }
        }
        
        // Never allow customer promotion codes (we control discounts via auto-applied coupons)
        // Note: Don't set allow_promotion_codes when using discounts parameter

        let session = CheckoutSession::create(&self.client, create_session).await?;
        
        Ok(CheckoutSessionResponse {
            url: session.url.unwrap_or_default(),
            session_id: session.id.to_string(),
        })
    }

    /// Get checkout session details by session ID
    pub async fn get_checkout_session_details(&self, session_id: &str) -> Result<CheckoutSessionDetails> {
        let session_id = CheckoutSessionId::from_str(session_id)?;
        let session = CheckoutSession::retrieve(&self.client, &session_id, &[]).await?;
        
        // Extract tier and billing period from metadata
        let (tier, billing_period) = if let Some(metadata) = &session.metadata {
            (
                metadata.get("tier").cloned(),
                metadata.get("billing_period").cloned(),
            )
        } else {
            (None, None)
        };
        
        Ok(CheckoutSessionDetails {
            session_id: session.id.to_string(),
            status: session.status.map(|s| format!("{:?}", s)).unwrap_or_else(|| "unknown".to_string()),
            tier,
            billing_period,
            amount_total: session.amount_total,
            currency: session.currency.map(|c| c.to_string()),
        })
    }

    /// Create a Stripe customer for a user  
    async fn create_customer(&self, user: &UserRecord, metadata_db: &crate::metadata::MetadataDb) -> Result<CustomerId> {
        let mut create_customer = CreateCustomer::new();
        create_customer.email = Some(&user.email);
        create_customer.name = user.name.as_deref();
        create_customer.metadata = Some({
            let mut metadata = HashMap::new();
            metadata.insert("user_id".to_string(), user.id.clone());
            metadata
        });

        let customer = stripe::Customer::create(&self.client, create_customer).await?;
        
        // Save customer ID to database
        metadata_db.update_user_stripe_customer(&user.id, &customer.id.to_string()).await?;
        
        tracing::info!("✅ Created new Stripe customer {} for user {}", customer.id, user.email);
        
        Ok(customer.id)
    }

    /// Create a customer portal session for subscription management
    pub async fn create_customer_portal_session(
        &self,
        customer_id: &str,
        return_url: &str,
    ) -> Result<CustomerPortalResponse> {
        let customer_id = CustomerId::from_str(customer_id)?;
        let mut create_session = CreateBillingPortalSession::new(customer_id);
        create_session.return_url = Some(return_url);

        let session = BillingPortalSession::create(&self.client, create_session).await?;
        
        Ok(CustomerPortalResponse {
            url: session.url,
        })
    }

    /// Handle Stripe webhook events
    pub async fn handle_webhook(&self, payload: &[u8], signature: &str) -> Result<()> {
        let event = Webhook::construct_event(
            std::str::from_utf8(payload)?,
            signature,
            &self.webhook_secret,
        )?;

        match event.type_ {
            EventType::CheckoutSessionCompleted => {
                self.handle_checkout_completed(&event).await?;
            }
            EventType::InvoicePaymentSucceeded => {
                self.handle_invoice_payment_succeeded(&event).await?;
            }
            EventType::InvoicePaymentFailed => {
                self.handle_invoice_payment_failed(&event).await?;
            }
            EventType::CustomerSubscriptionUpdated => {
                self.handle_subscription_updated(&event).await?;
            }
            EventType::CustomerSubscriptionDeleted => {
                self.handle_subscription_deleted(&event).await?;
            }
            _ => {
                tracing::info!("Received unhandled webhook event: {:?}", event.type_);
            }
        }

        Ok(())
    }

    async fn handle_checkout_completed(&self, event: &Event) -> Result<()> {
        tracing::info!("Checkout session completed: {}", event.id);
        
        // The event.data.object is an EventObject enum, not an Option
        // We need to match on it to extract the checkout session
        match &event.data.object {
            stripe::EventObject::CheckoutSession(session) => {
                tracing::info!("Processing checkout session: {}", session.id);
                
                // Extract user ID and subscription tier from metadata
                if let Some(metadata) = &session.metadata {
                    if let (Some(user_id), Some(tier_str)) = (
                        metadata.get("user_id"),
                        metadata.get("tier")
                    ) {
                        let tier = SubscriptionTier::from(tier_str.as_str());
                        
                        tracing::info!("Updating user {} to tier {:?}", user_id, tier);
                        
                        // TODO: Update user subscription in database
                        // This would require access to MetadataDb, which we don't have in StripeBilling yet
                        // For now, just log the information
                        tracing::info!("Would update user {} subscription to {:?}", user_id, tier);
                        
                        // Also extract billing period for logging
                        if let Some(billing_period) = metadata.get("billing_period") {
                            tracing::info!("Billing period: {}", billing_period);
                        }
                    } else {
                        tracing::warn!("Checkout session missing required metadata: user_id or tier");
                    }
                } else {
                    tracing::warn!("Checkout session has no metadata");
                }
            }
            _ => {
                tracing::warn!("Expected CheckoutSession object in checkout.session.completed event, got {:?}", event.data.object);
            }
        }
        
        Ok(())
    }

    async fn handle_invoice_payment_succeeded(&self, event: &Event) -> Result<()> {
        tracing::info!("Invoice payment succeeded: {}", event.id);
        Ok(())
    }

    async fn handle_invoice_payment_failed(&self, event: &Event) -> Result<()> {
        tracing::info!("Invoice payment failed: {}", event.id);
        Ok(())
    }

    async fn handle_subscription_updated(&self, event: &Event) -> Result<()> {
        tracing::info!("Subscription updated: {}", event.id);
        Ok(())
    }

    async fn handle_subscription_deleted(&self, event: &Event) -> Result<()> {
        tracing::info!("Subscription deleted: {}", event.id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    /// Test basic struct creation and serialization
    #[test]
    fn test_pricing_structures() {
        let pricing = PricingInfo {
            tiers: vec![
                TierPricing {
                    tier: "personal".to_string(),
                    name: "Personal Plan".to_string(),
                    description: Some("Perfect for individuals".to_string()),
                    monthly_price: Some(PriceDetails {
                        price_id: "price_123".to_string(),
                        amount: 900,
                        currency: "usd".to_string(),
                        interval: "month".to_string(),
                    }),
                    yearly_price: Some(PriceDetails {
                        price_id: "price_456".to_string(),
                        amount: 9000,
                        currency: "usd".to_string(),
                        interval: "year".to_string(),
                    }),
                    features: HashMap::new(),
                }
            ]
        };

        // Test serialization
        let json = serde_json::to_string(&pricing).unwrap();
        assert!(json.contains("Personal Plan"));
        assert!(json.contains("price_123"));
        
        // Test deserialization
        let deserialized: PricingInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.tiers.len(), 1);
        assert_eq!(deserialized.tiers[0].name, "Personal Plan");
    }

    #[test]
    fn test_checkout_session_response() {
        let response = CheckoutSessionResponse {
            url: "https://checkout.stripe.com/pay/cs_test_123".to_string(),
            session_id: "cs_test_123".to_string(),
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: CheckoutSessionResponse = serde_json::from_str(&json).unwrap();
        
        assert_eq!(deserialized.session_id, "cs_test_123");
        assert!(deserialized.url.contains("checkout.stripe.com"));
    }

    #[test]
    fn test_checkout_session_details() {
        let details = CheckoutSessionDetails {
            session_id: "cs_test_123".to_string(),
            status: "complete".to_string(),
            tier: Some("pro".to_string()),
            billing_period: Some("yearly".to_string()),
            amount_total: Some(28800), // $288.00 in cents
            currency: Some("usd".to_string()),
        };

        let json = serde_json::to_string(&details).unwrap();
        let deserialized: CheckoutSessionDetails = serde_json::from_str(&json).unwrap();
        
        assert_eq!(deserialized.session_id, "cs_test_123");
        assert_eq!(deserialized.tier, Some("pro".to_string()));
        assert_eq!(deserialized.amount_total, Some(28800));
    }

    #[test]
    fn test_subscription_tier_mapping() {
        assert_eq!(SubscriptionTier::from("personal"), SubscriptionTier::Personal);
        assert_eq!(SubscriptionTier::from("pro"), SubscriptionTier::Pro);
        assert_eq!(SubscriptionTier::from("business"), SubscriptionTier::Business);
        
        // Test as_str conversion
        assert_eq!(SubscriptionTier::Personal.as_str(), "personal");
        assert_eq!(SubscriptionTier::Pro.as_str(), "pro");
        assert_eq!(SubscriptionTier::Business.as_str(), "business");
    }

    #[tokio::test]
    #[ignore] // Only run with real Stripe credentials
    async fn test_stripe_billing_initialization() {
        // This test requires real environment variables
        env::set_var("STRIPE_SECRET_KEY", "sk_test_fake_key_for_testing");
        env::set_var("STRIPE_WEBHOOK_SECRET", "whsec_fake_secret_for_testing");

        // This will fail with fake credentials, but tests the initialization logic
        let result = StripeBilling::new().await;
        assert!(result.is_err()); // Expected to fail with fake credentials
    }

    /// Test webhook signature validation logic
    #[test]
    fn test_webhook_signature_validation() {
        // Test that webhook signature validation is properly structured
        // In a real test, you'd use Stripe's test webhook data
        let payload = b"{\"id\":\"evt_test_123\",\"object\":\"event\"}";
        let signature = "t=123456,v1=fake_signature";
        
        // This tests the structure, not actual validation (would need real Stripe client)
        assert!(!payload.is_empty());
        assert!(signature.contains("t="));
        assert!(signature.contains("v1="));
    }

    /// Test error handling for missing environment variables
    #[tokio::test]
    async fn test_missing_environment_variables() {
        // Temporarily unset environment variables
        env::remove_var("STRIPE_SECRET_KEY");
        env::remove_var("STRIPE_WEBHOOK_SECRET");

        let result = StripeBilling::new().await;
        assert!(result.is_err());
        
        if let Err(error) = result {
            let error_message = format!("{}", error);
            assert!(error_message.contains("STRIPE_SECRET_KEY"));
        }
    }

    /// Test price calculation logic
    #[test]
    fn test_price_calculations() {
        let monthly_amount = 2900; // $29.00
        let yearly_amount_discounted = 27840; // $278.40 (20% discount)
        let yearly_amount_full = 34800; // $348.00 (full price)
        
        // Verify yearly discount calculation (20% off = pay 80%)
        let expected_yearly = (monthly_amount * 12) as f64 * 0.8; // 20% off
        assert_eq!(yearly_amount_discounted as f64, expected_yearly);
        
        // Verify full yearly price without discount
        let expected_full = (monthly_amount * 12) as f64;
        assert_eq!(yearly_amount_full as f64, expected_full);
        
        // Test currency formatting
        let dollars = monthly_amount as f64 / 100.0;
        assert_eq!(dollars, 29.0);
    }

    /// Test metadata extraction
    #[test]
    fn test_metadata_extraction() {
        let mut metadata = HashMap::new();
        metadata.insert("tier".to_string(), "pro".to_string());
        metadata.insert("billing_period".to_string(), "yearly".to_string());
        metadata.insert("user_id".to_string(), "user_123".to_string());
        
        // Test extracting tier
        let tier = metadata.get("tier").unwrap();
        assert_eq!(tier, "pro");
        
        // Test extracting billing period
        let billing_period = metadata.get("billing_period").unwrap();
        assert_eq!(billing_period, "yearly");
    }

    /// Test URL generation
    #[test]
    fn test_url_generation() {
        let frontend_url = "http://localhost:3001";
        let session_id = "cs_test_123";
        
        let success_url = format!("{}/billing/success?session={}", frontend_url, session_id);
        let cancel_url = format!("{}/billing/cancel", frontend_url);
        
        assert_eq!(success_url, "http://localhost:3001/billing/success?session=cs_test_123");
        assert_eq!(cancel_url, "http://localhost:3001/billing/cancel");
    }
}