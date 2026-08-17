use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::metadata::{MetadataDb, SubscriptionUpdateParams, UserRecord};
use crate::stripe_client_service::StripeClientService;
use crate::subscription::SubscriptionTier;

#[derive(Debug, Clone)]
pub struct WebhookResult {
    pub event_id: String,
    pub event_created: i64,
    pub event_type: String,
    pub subscription_updates: Vec<SubscriptionUpdate>,
}

#[derive(Debug, Clone)]
pub struct VerifiedWebhookEvent {
    pub event_id: String,
    pub event_created: i64,
    pub event_type: String,
}

#[derive(Debug, Clone)]
pub struct SubscriptionUpdate {
    pub user_id: String,
    pub subscription_tier: String,
    pub subscription_status: String,
    pub stripe_subscription_id: Option<String>,
    pub subscription_started_at: Option<String>,
    pub subscription_ends_at: Option<String>,
    pub trial_ends_at: Option<String>,
}

/// New Stripe billing service using our custom client with 2025 API
pub struct StripeBilling {
    client: StripeClientService,
    webhook_secret: String,
    // Cache full pricing info loaded from Stripe on startup
    cached_pricing: PricingInfo,
    // Metadata database for user lookups during webhook processing
    metadata_db: Arc<MetadataDb>,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingInfo {
    pub tiers: Vec<FrontendTierPricing>,
    pub yearly_discount_percent: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontendTierPricing {
    pub tier: String,
    pub name: String,
    pub description: Option<String>,
    pub monthly_price: Option<FrontendPriceInfo>,
    pub yearly_price: Option<FrontendPriceInfo>,
    pub features: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontendPriceInfo {
    pub price_id: String,
    pub amount: i64, // amount in cents
    pub currency: String,
    pub interval: String,
}

impl StripeBilling {
    pub async fn verify_webhook_event(
        &self,
        payload: &[u8],
        signature: &str,
    ) -> Result<VerifiedWebhookEvent> {
        let payload_str = std::str::from_utf8(payload)?;
        self.client
            .parse_webhook_event(payload_str, signature, &self.webhook_secret)
            .await?;
        let event_metadata: serde_json::Value = serde_json::from_str(payload_str)?;
        Ok(VerifiedWebhookEvent {
            event_id: event_metadata
                .get("id")
                .and_then(|id| id.as_str())
                .ok_or_else(|| anyhow::anyhow!("Stripe webhook event is missing an ID"))?
                .to_string(),
            event_created: event_metadata
                .get("created")
                .and_then(|created| created.as_i64())
                .ok_or_else(|| {
                    anyhow::anyhow!("Stripe webhook event is missing a creation time")
                })?,
            event_type: event_metadata
                .get("type")
                .and_then(|event_type| event_type.as_str())
                .ok_or_else(|| anyhow::anyhow!("Stripe webhook event is missing a type"))?
                .to_string(),
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
        let status = subscription
            .get("status")
            .and_then(|status| status.as_str())
            .ok_or_else(|| anyhow::anyhow!("Stripe subscription is missing a status"))?;
        let timestamp = |field: &str| {
            subscription
                .get(field)
                .and_then(|value| value.as_i64())
                .and_then(|value| chrono::DateTime::from_timestamp(value, 0))
                .map(|value| value.to_rfc3339())
        };
        let period_end = subscription
            .get("items")
            .and_then(|items| items.get("data"))
            .and_then(|data| data.as_array())
            .and_then(|items| items.first())
            .and_then(|item| item.get("current_period_end"))
            .and_then(|value| value.as_i64())
            .and_then(|value| chrono::DateTime::from_timestamp(value, 0))
            .map(|value| value.to_rfc3339());

        Ok(SubscriptionUpdate {
            user_id: update.user_id.clone(),
            subscription_tier: self.determine_tier_from_subscription_items(&subscription),
            subscription_status: status.to_string(),
            stripe_subscription_id: Some(subscription_id.to_string()),
            subscription_started_at: timestamp("created"),
            subscription_ends_at: timestamp("cancel_at").or(period_end),
            trial_ends_at: timestamp("trial_end"),
        })
    }
    pub async fn new(metadata_db: Arc<MetadataDb>) -> Result<Self> {
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
            metadata_db,
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
                        tiers.insert(
                            tier_str.clone(),
                            FrontendTierPricing {
                                tier: tier_str.clone(),
                                name: self.get_tier_display_name(tier_str),
                                description: self.get_tier_description(tier_str),
                                monthly_price: None,
                                yearly_price: None,
                                features: self.get_tier_features(tier_str),
                            },
                        );
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
                            let currency =
                                price.currency.as_deref().unwrap_or("USD").to_uppercase();

                            if interval == "month" && interval_count == 1 {
                                // Prefer the most recent or lowest monthly price
                                if tier_pricing.monthly_price.is_none()
                                    || amount < tier_pricing.monthly_price.as_ref().unwrap().amount
                                {
                                    tier_pricing.monthly_price = Some(FrontendPriceInfo {
                                        price_id: price_id.clone(),
                                        amount,
                                        currency: currency.clone(),
                                        interval: "month".to_string(),
                                    });
                                    tracing::info!(
                                        "💰 Found monthly price for {}: ${:.2}/{} ({})",
                                        tier_str,
                                        amount as f64 / 100.0,
                                        currency,
                                        price_id
                                    );
                                }
                                monthly_amounts.push(amount);
                            } else if interval == "year" && interval_count == 1 {
                                // Prefer the lowest yearly price (discounted price)
                                if tier_pricing.yearly_price.is_none()
                                    || amount < tier_pricing.yearly_price.as_ref().unwrap().amount
                                {
                                    tier_pricing.yearly_price = Some(FrontendPriceInfo {
                                        price_id: price_id.clone(),
                                        amount,
                                        currency: currency.clone(),
                                        interval: "year".to_string(),
                                    });
                                    tracing::info!(
                                        "💰 Found yearly price for {}: ${:.2}/{} ({})",
                                        tier_str,
                                        amount as f64 / 100.0,
                                        currency,
                                        price_id
                                    );
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
                if let (Some(monthly), Some(yearly)) =
                    (&tier_pricing.monthly_price, &tier_pricing.yearly_price)
                {
                    let monthly_total = (monthly.amount * 12) as f64;
                    let yearly_price = yearly.amount as f64;

                    if yearly_price < monthly_total {
                        let discount = (monthly_total - yearly_price) / monthly_total * 100.0;
                        total_discount += discount;
                        count += 1;
                        tracing::info!(
                            "📊 {} tier: {}% yearly discount (${:.2}/year vs ${:.2}/year)",
                            tier_pricing.tier,
                            discount,
                            yearly_price / 100.0,
                            monthly_total / 100.0
                        );
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

        // Sort tiers by display order (Personal -> Team)
        self.cached_pricing.tiers.sort_by(|a, b| {
            let order_a = match a.tier.to_lowercase().as_str() {
                "personal" => 1,
                "team" => 2,
                _ => 99,
            };
            let order_b = match b.tier.to_lowercase().as_str() {
                "personal" => 1,
                "team" => 2,
                _ => 99,
            };
            order_a.cmp(&order_b)
        });

        tracing::info!(
            "✅ Loaded {} pricing tiers with {}% yearly discount",
            self.cached_pricing.tiers.len(),
            discount_percent.unwrap_or(0.0)
        );

        Ok(())
    }

    fn get_tier_display_name(&self, tier: &str) -> String {
        match tier.to_lowercase().as_str() {
            "personal" => "Personal".to_string(),
            "team" => "Team".to_string(),
            _ => tier.to_string(),
        }
    }

    fn get_tier_description(&self, tier: &str) -> Option<String> {
        match tier.to_lowercase().as_str() {
            "personal" => Some("For individual Bitcoin holders".to_string()),
            "team" => Some("For Uncle Jims & family guardians".to_string()),
            _ => None,
        }
    }

    fn get_tier_features(&self, tier: &str) -> HashMap<String, String> {
        let mut features = HashMap::new();

        // All features available on all tiers - consistent with frontend
        features.insert("trial".to_string(), "30-day free trial".to_string());
        features.insert("email".to_string(), "Email notifications".to_string());
        features.insert("sms".to_string(), "SMS notifications".to_string());
        features.insert("push".to_string(), "Push notifications".to_string());
        features.insert(
            "analysis".to_string(),
            "Transaction analysis (RBF/CPFP coming soon)".to_string(),
        );

        // Tier-specific capacity limits
        match tier.to_lowercase().as_str() {
            "personal" => {
                features.insert("wallets".to_string(), "1 wallet".to_string());
                features.insert("contacts".to_string(), "1 contact".to_string());
                features.insert("sync".to_string(), "10 minute sync time".to_string());
            }
            "team" => {
                features.insert("wallets".to_string(), "5 wallets".to_string());
                features.insert("contacts".to_string(), "5 contacts per wallet".to_string());
                features.insert("sync".to_string(), "2 minute sync time".to_string());
            }
            _ => {}
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
        metadata_db: &crate::metadata::MetadataDb,
    ) -> Result<CheckoutSessionResponse> {
        tracing::info!(
            "🛒 Creating checkout session for user {} with tier {:?}, billing: {}",
            user_id,
            tier,
            billing_cycle
        );

        // Get price ID from cached pricing data
        let tier_str = tier.as_str();
        let tier_pricing = self
            .cached_pricing
            .tiers
            .iter()
            .find(|t| t.tier == tier_str)
            .ok_or_else(|| anyhow::anyhow!("No pricing found for tier {:?}", tier))?;

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
            anyhow::anyhow!(
                "No price found for tier {:?} with billing cycle {}",
                tier,
                billing_cycle
            )
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
            .ok_or_else(|| anyhow::anyhow!("User not found: {}", user_id))?;

        let customer_id = user.stripe_customer_id
            .ok_or_else(|| anyhow::anyhow!("User {} does not have a Stripe customer ID. Trial may not have been created properly.", user_id))?;

        tracing::info!("🔍 Using Stripe customer ID: {}", customer_id);

        // Check if user is trying to upgrade/downgrade their subscription
        let subscription_list = self.client.list_subscriptions(&customer_id).await?;
        let subscriptions = subscription_list.data.unwrap_or_default();

        let mut has_active_subscription = false;
        let mut current_tier: Option<String> = None;

        for subscription in &subscriptions {
            if let Some(status) = &subscription.status {
                if status == "active" || status == "past_due" {
                    has_active_subscription = true;

                    // Extract current tier from subscription metadata
                    if let Some(metadata) = &subscription.metadata {
                        if let Some(tier_from_metadata) = metadata.get("tier") {
                            current_tier = Some(tier_from_metadata.to_lowercase());
                        }
                    }
                    break;
                }
            }
        }

        if has_active_subscription {
            let target_tier = tier.as_str().to_lowercase();
            if let Some(ref current) = current_tier {
                if current == &target_tier {
                    tracing::warn!("🚫 User {} already has {} tier subscription. Cannot checkout for same tier.", user_id, current);
                    return Err(anyhow::anyhow!(
                        "You already have a {} subscription",
                        current
                    ));
                } else {
                    tracing::info!(
                        "🔄 User {} upgrading from {} to {} tier",
                        user_id,
                        current,
                        target_tier
                    );
                }
            } else {
                tracing::info!(
                    "🔄 User {} has active subscription, proceeding with tier change to {}",
                    user_id,
                    target_tier
                );
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

        tracing::info!(
            "✅ Checkout session created: {} (URL: {})",
            session_id,
            checkout_url
        );

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

        tracing::info!(
            "🆕 Creating Stripe customer (no subscription yet) for user {}",
            user.email
        );

        let customer = self
            .client
            .create_customer(user.email.clone(), user.name.clone(), customer_metadata)
            .await?;

        let customer_id = customer.id.unwrap_or_default();

        // Update user with Stripe customer ID in database
        metadata_db
            .update_user_stripe_customer(&user.id, &customer_id)
            .await?;

        tracing::info!(
            "✅ Stripe customer created successfully for user {} (ID: {})",
            user.email,
            customer_id
        );

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
            .find(|t| t.tier.to_lowercase() == format!("{:?}", tier).to_lowercase())
            .ok_or_else(|| anyhow::anyhow!("No pricing found for tier {:?}", tier))?;

        // Get the monthly price ID for the tier (trials should use monthly pricing)
        let price_info = tier_pricing
            .monthly_price
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No monthly price found for tier {:?}", tier))?;

        let price_id = price_info.price_id.clone();

        // Get or create Stripe customer
        let customer_id = match &user.stripe_customer_id {
            Some(id) => id.clone(),
            None => {
                // Create customer first if they don't have one
                self.create_stripe_customer_only(user, metadata_db).await?
            }
        };

        tracing::info!(
            "🎉 Creating 30-day trial subscription for user {} (customer: {}) with tier {:?}",
            user.email,
            customer_id,
            tier
        );

        // Create subscription metadata
        let mut metadata = HashMap::new();
        metadata.insert("user_id".to_string(), user.id.clone());
        metadata.insert("tier".to_string(), format!("{:?}", tier));

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
        let tier_str = format!("{:?}", tier).to_lowercase();

        let sub_params = SubscriptionUpdateParams {
            subscription_tier: &tier_str,
            subscription_status: "trialing",
            stripe_subscription_id: Some(&subscription_id),
            subscription_started_at: Some(&now_str),
            subscription_ends_at: None, // subscription_ends_at - not set for trials
            trial_ends_at: Some(&trial_end_str),
        };
        metadata_db
            .update_user_subscription(&user.id, &sub_params)
            .await?;

        tracing::info!(
            "✅ Trial subscription created successfully for user {} (ID: {})",
            user.email,
            subscription_id
        );

        Ok(())
    }

    pub async fn handle_webhook(&self, payload: &[u8], signature: &str) -> Result<WebhookResult> {
        // Use stripe library for webhook verification (security critical)
        let payload_str = std::str::from_utf8(payload)?;
        let event = self
            .client
            .parse_webhook_event(payload_str, signature, &self.webhook_secret)
            .await?;
        let event_metadata: serde_json::Value = serde_json::from_str(payload_str)?;
        let event_id = event_metadata
            .get("id")
            .and_then(|id| id.as_str())
            .ok_or_else(|| anyhow::anyhow!("Stripe webhook event is missing an ID"))?
            .to_string();
        let event_created = event_metadata
            .get("created")
            .and_then(|created| created.as_i64())
            .ok_or_else(|| anyhow::anyhow!("Stripe webhook event is missing a creation time"))?;
        let event_type = event_metadata
            .get("type")
            .and_then(|event_type| event_type.as_str())
            .ok_or_else(|| anyhow::anyhow!("Stripe webhook event is missing a type"))?
            .to_string();

        let mut updates = Vec::new();

        // Handle different event types with 2025 API structure
        if let Some(event_type) = &event.r#type {
            match event_type.as_str() {
                "customer.subscription.trial_will_end" => {
                    // Fired 3 days before trial ends - send notification to user
                    tracing::info!("⏰ Trial ending soon for subscription");

                    if let Some(data) = &event.data {
                        if let Some(subscription_obj) = &data.object {
                            if let Ok(subscription) = serde_json::from_value::<serde_json::Value>(
                                subscription_obj.clone(),
                            ) {
                                let customer_id =
                                    subscription.get("customer").and_then(|c| c.as_str());
                                let trial_end =
                                    subscription.get("trial_end").and_then(|t| t.as_i64());

                                tracing::info!("📧 Processing trial_will_end - Customer: {:?}, Trial end: {:?}",
                                    customer_id, trial_end);

                                if let (Some(customer_id), Some(trial_end_timestamp)) =
                                    (customer_id, trial_end)
                                {
                                    // Look up user by Stripe customer ID
                                    match self
                                        .metadata_db
                                        .get_user_by_stripe_customer_id(customer_id)
                                        .await
                                    {
                                        Ok(Some(user)) if user.email_verified => {
                                            // Format trial end date for email
                                            let trial_ends_at = chrono::DateTime::from_timestamp(
                                                trial_end_timestamp,
                                                0,
                                            )
                                            .map(|dt| dt.format("%B %d, %Y").to_string())
                                            .unwrap_or_else(|| "soon".to_string());

                                            let user_name =
                                                user.name.as_deref().unwrap_or(&user.email);

                                            if self
                                                .metadata_db
                                                .trial_ending_email_was_sent(&event_id)
                                                .await?
                                            {
                                                tracing::info!(
                                                    "Trial ending notification already sent for event {}",
                                                    event_id
                                                );
                                            } else {
                                                // Resend deduplicates this key if a previous attempt sent
                                                // the email before its database completion marker was written.
                                                use crate::email_service::EmailService;
                                                let user_language = user
                                                    .preferred_language
                                                    .as_deref()
                                                    .unwrap_or("en-US");
                                                match EmailService::from_env() {
                                                    Ok(email_service) => {
                                                        email_service
                                                            .send_trial_ending_notification(
                                                                &user.email,
                                                                user_name,
                                                                &trial_ends_at,
                                                                user_language,
                                                                &format!(
                                                                    "stripe-trial-will-end-{}",
                                                                    event_id
                                                                ),
                                                            )
                                                            .await?;
                                                        self.metadata_db
                                                            .mark_trial_ending_email_sent(&event_id)
                                                            .await?;
                                                        tracing::info!(
                                                            "✅ Trial ending notification sent to {}",
                                                            user.email
                                                        );
                                                    }
                                                    Err(e) => {
                                                        tracing::warn!(
                                                            "⚠️  Email service not configured, skipping trial ending notification: {}",
                                                            e
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                        Ok(Some(user)) => {
                                            tracing::info!(
                                                "⏭️  User {} email not verified, skipping trial ending notification",
                                                user.email
                                            );
                                        }
                                        Ok(None) => {
                                            tracing::warn!(
                                                "⚠️  No user found for Stripe customer {}",
                                                customer_id
                                            );
                                        }
                                        Err(e) => {
                                            tracing::error!(
                                                "❌ Error looking up user by Stripe customer {}: {}",
                                                customer_id,
                                                e
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                "checkout.session.completed" => {
                    if let Some(data) = &event.data {
                        if let Some(session_data) = &data.object {
                            tracing::info!("📋 Processing checkout.session.completed");

                            // Parse checkout session data
                            if let Ok(session) =
                                serde_json::from_value::<serde_json::Value>(session_data.clone())
                            {
                                let customer_id = session.get("customer").and_then(|c| c.as_str());
                                let new_subscription_id =
                                    session.get("subscription").and_then(|s| s.as_str());

                                tracing::info!("🛒 Checkout completed - Customer: {:?}, New Subscription: {:?}", 
                                    customer_id, new_subscription_id);

                                if let (Some(customer_id), Some(new_subscription_id)) =
                                    (customer_id, new_subscription_id)
                                {
                                    // Handle duplicate subscription cleanup
                                    let cleanup_update = self
                                        .handle_checkout_completion(
                                            customer_id,
                                            new_subscription_id,
                                        )
                                        .await?;
                                    updates.push(cleanup_update);
                                }
                            }
                        }
                    }
                }
                "customer.subscription.created" => {
                    if let Some(data) = &event.data {
                        if let Some(subscription_obj) = &data.object {
                            tracing::info!("📋 Processing customer.subscription.created");

                            // Parse subscription to extract trial info and tier
                            if let Ok(subscription) = serde_json::from_value::<serde_json::Value>(
                                subscription_obj.clone(),
                            ) {
                                let status = subscription.get("status").and_then(|s| s.as_str());
                                let trial_end =
                                    subscription.get("trial_end").and_then(|t| t.as_i64());

                                // Extract current_period_end from subscription items (more reliable than top level)
                                let current_period_end = subscription
                                    .get("items")
                                    .and_then(|items| items.get("data"))
                                    .and_then(|data| data.as_array())
                                    .and_then(|arr| arr.first())
                                    .and_then(|item| item.get("current_period_end"))
                                    .and_then(|t| t.as_i64())
                                    .or_else(|| {
                                        subscription
                                            .get("current_period_end")
                                            .and_then(|t| t.as_i64())
                                    });

                                let customer_id =
                                    subscription.get("customer").and_then(|c| c.as_str());
                                let subscription_id =
                                    subscription.get("id").and_then(|s| s.as_str());

                                // Determine tier from subscription items (more reliable than metadata)
                                let tier =
                                    self.determine_tier_from_subscription_items(&subscription);

                                tracing::info!("🆕 New subscription - Status: {:?}, Trial end: {:?}, Period end: {:?}, Customer: {:?}, Tier: {}",
                                    status, trial_end, current_period_end, customer_id, tier);

                                if let (Some(customer_id), Some(subscription_id)) =
                                    (customer_id, subscription_id)
                                {
                                    // Convert trial_end timestamp to ISO string
                                    let trial_ends_at = trial_end.map(|ts| {
                                        chrono::DateTime::from_timestamp(ts, 0)
                                            .map(|dt| dt.to_rfc3339())
                                            .unwrap_or_default()
                                    });

                                    // Convert current_period_end timestamp to ISO string
                                    let subscription_ends_at = current_period_end.map(|ts| {
                                        chrono::DateTime::from_timestamp(ts, 0)
                                            .map(|dt| dt.to_rfc3339())
                                            .unwrap_or_default()
                                    });

                                    let update = SubscriptionUpdate {
                                        user_id: self.extract_user_id_from_customer(customer_id),
                                        subscription_tier: tier, // Use actual tier from subscription items
                                        subscription_status: status
                                            .unwrap_or("unknown")
                                            .to_string(), // "trialing" or "active" from Stripe
                                        stripe_subscription_id: Some(subscription_id.to_string()),
                                        subscription_started_at: Some(
                                            chrono::Utc::now().to_rfc3339(),
                                        ),
                                        subscription_ends_at,
                                        trial_ends_at,
                                    };
                                    updates.push(update);
                                }
                            }
                        }
                    }
                }
                "customer.subscription.updated" => {
                    // Handle ALL subscription updates: tier changes, trial endings, etc.
                    if let Some(data) = &event.data {
                        if let Some(subscription_obj) = &data.object {
                            if let Ok(subscription) = serde_json::from_value::<serde_json::Value>(
                                subscription_obj.clone(),
                            ) {
                                let current_status =
                                    subscription.get("status").and_then(|s| s.as_str());
                                let customer_id =
                                    subscription.get("customer").and_then(|c| c.as_str());
                                let subscription_id =
                                    subscription.get("id").and_then(|s| s.as_str());

                                // Extract current_period_end from subscription items (more reliable than top level)
                                let current_period_end = subscription
                                    .get("items")
                                    .and_then(|items| items.get("data"))
                                    .and_then(|data| data.as_array())
                                    .and_then(|arr| arr.first())
                                    .and_then(|item| item.get("current_period_end"))
                                    .and_then(|t| t.as_i64())
                                    .or_else(|| {
                                        subscription
                                            .get("current_period_end")
                                            .and_then(|t| t.as_i64())
                                    });

                                let cancel_at_period_end = subscription
                                    .get("cancel_at_period_end")
                                    .and_then(|b| b.as_bool());
                                let cancel_at =
                                    subscription.get("cancel_at").and_then(|t| t.as_i64());

                                // Determine current tier from subscription items (more reliable than metadata)
                                let current_tier =
                                    self.determine_tier_from_subscription_items(&subscription);

                                tracing::info!("🔄 Subscription updated - Customer: {:?}, Status: {:?}, Tier: {}, Period end: {:?}, Cancel at period end: {:?}, Cancel at: {:?}",
                                    customer_id, current_status, current_tier, current_period_end, cancel_at_period_end, cancel_at);

                                if let (Some(customer_id), Some(subscription_id)) =
                                    (customer_id, subscription_id)
                                {
                                    let mut should_update = false;
                                    let mut reason = String::new();

                                    // Check for trial ending
                                    if let Some(previous_attrs) =
                                        subscription.get("previous_attributes")
                                    {
                                        if let Some(previous_status) =
                                            previous_attrs.get("status").and_then(|s| s.as_str())
                                        {
                                            if previous_status == "trialing"
                                                && current_status != Some("trialing")
                                            {
                                                should_update = true;
                                                reason = "Trial ended".to_string();
                                            }
                                        }
                                    }

                                    // Check for tier changes (items changed)
                                    if let Some(previous_attrs) =
                                        subscription.get("previous_attributes")
                                    {
                                        if previous_attrs.get("items").is_some() {
                                            should_update = true;
                                            reason = format!("Tier changed to {}", current_tier);
                                        }
                                    }

                                    // Check for trial-related changes (trial extension, shortening, etc.)
                                    // Always update if subscription is trialing to ensure trial_ends_at is kept in sync
                                    if current_status == Some("trialing") {
                                        should_update = true;
                                        if reason.is_empty() {
                                            if let Some(current_trial_end) = subscription
                                                .get("trial_end")
                                                .and_then(|t| t.as_i64())
                                            {
                                                let new_date = chrono::DateTime::from_timestamp(
                                                    current_trial_end,
                                                    0,
                                                )
                                                .map(|dt| dt.format("%B %d, %Y").to_string())
                                                .unwrap_or_else(|| "unknown".to_string());
                                                reason = format!("Trial active until {}", new_date);
                                            }
                                        }
                                    }

                                    // Catch-all: update when subscription transitions to active
                                    // Only fires when previous_attributes confirms status changed from non-active
                                    if !should_update && current_status == Some("active") {
                                        let status_changed_to_active = subscription
                                            .get("previous_attributes")
                                            .and_then(|p| p.get("status"))
                                            .and_then(|s| s.as_str())
                                            .map(|s| s != "active")
                                            .unwrap_or(false);
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

                                    if should_update {
                                        tracing::info!(
                                            "✅ Processing subscription update: {}",
                                            reason
                                        );

                                        // Convert current_period_end timestamp to ISO string
                                        // Use cancel_at if available (set when cancel_at_period_end is true)
                                        let subscription_ends_at =
                                            cancel_at.or(current_period_end).map(|ts| {
                                                chrono::DateTime::from_timestamp(ts, 0)
                                                    .map(|dt| dt.to_rfc3339())
                                                    .unwrap_or_default()
                                            });

                                        // Extract trial_end for trialing subscriptions
                                        let trial_ends_at = if current_status == Some("trialing") {
                                            subscription
                                                .get("trial_end")
                                                .and_then(|t| t.as_i64())
                                                .map(|ts| {
                                                    chrono::DateTime::from_timestamp(ts, 0)
                                                        .map(|dt| dt.to_rfc3339())
                                                        .unwrap_or_default()
                                                })
                                        } else {
                                            None
                                        };

                                        // Override status to "canceled" if cancel_at_period_end is true
                                        let final_status = if cancel_at_period_end == Some(true) {
                                            "canceled".to_string()
                                        } else {
                                            current_status.unwrap_or("unknown").to_string()
                                        };

                                        let update = SubscriptionUpdate {
                                            user_id: self
                                                .extract_user_id_from_customer(customer_id),
                                            subscription_tier: current_tier,
                                            subscription_status: final_status,
                                            stripe_subscription_id: Some(
                                                subscription_id.to_string(),
                                            ),
                                            subscription_started_at: Some(
                                                chrono::Utc::now().to_rfc3339(),
                                            ),
                                            subscription_ends_at,
                                            trial_ends_at,
                                        };
                                        updates.push(update);
                                    }
                                }
                            }
                        }
                    }
                }
                "customer.subscription.deleted" => {
                    // Subscription cancelled/ended - need to check if it's current subscription in API layer
                    if let Some(data) = &event.data {
                        if let Some(subscription_obj) = &data.object {
                            if let Ok(subscription) = serde_json::from_value::<serde_json::Value>(
                                subscription_obj.clone(),
                            ) {
                                let customer_id =
                                    subscription.get("customer").and_then(|c| c.as_str());
                                let deleted_subscription_id =
                                    subscription.get("id").and_then(|s| s.as_str());
                                let status = subscription.get("status").and_then(|s| s.as_str());

                                tracing::info!("🗑️ Subscription deleted - Customer: {:?}, Subscription: {:?}, Status: {:?}",
                                    customer_id, deleted_subscription_id, status);

                                if let (Some(customer_id), Some(deleted_subscription_id)) =
                                    (customer_id, deleted_subscription_id)
                                {
                                    // The API layer conditionally expires only the subscription
                                    // currently attached to this customer.
                                    let update = SubscriptionUpdate {
                                        user_id: format!(
                                            "stripe_customer:{}:{}",
                                            customer_id, deleted_subscription_id
                                        ),
                                        subscription_tier: "keep_current".to_string(),
                                        subscription_status: "expired".to_string(),
                                        stripe_subscription_id: None,
                                        subscription_started_at: None,
                                        subscription_ends_at: Some(chrono::Utc::now().to_rfc3339()),
                                        trial_ends_at: None,
                                    };
                                    updates.push(update);
                                }
                            }
                        }
                    }
                }
                "invoice.payment_succeeded" => {
                    // Parse invoice data to provide clearer logging for $0 trial invoices vs actual payments
                    if let Some(data) = &event.data {
                        if let Some(invoice_obj) = &data.object {
                            if let Ok(invoice) =
                                serde_json::from_value::<serde_json::Value>(invoice_obj.clone())
                            {
                                let amount_paid = invoice
                                    .get("amount_paid")
                                    .and_then(|a| a.as_i64())
                                    .unwrap_or(0);
                                let amount_due = invoice
                                    .get("amount_due")
                                    .and_then(|a| a.as_i64())
                                    .unwrap_or(0);
                                let billing_reason =
                                    invoice.get("billing_reason").and_then(|r| r.as_str());
                                let customer_id = invoice.get("customer").and_then(|c| c.as_str());

                                if amount_paid == 0 && amount_due == 0 {
                                    // This is a $0 invoice (likely a trial start)
                                    if billing_reason == Some("subscription_create") {
                                        tracing::info!("✅ Trial started - $0 invoice processed for customer {}", 
                                            customer_id.unwrap_or("unknown"));
                                    } else {
                                        tracing::info!(
                                            "✅ $0 invoice processed for customer {}",
                                            customer_id.unwrap_or("unknown")
                                        );
                                    }
                                } else {
                                    // Actual payment was collected
                                    let amount_dollars = amount_paid as f64 / 100.0;
                                    tracing::info!(
                                        "💰 Payment succeeded - ${:.2} collected from customer {}",
                                        amount_dollars,
                                        customer_id.unwrap_or("unknown")
                                    );

                                    // Safety net: ensure subscription status is "active" after successful payment.
                                    // This handles race conditions where subscription.deleted (for old trial)
                                    // may overwrite the active status set by checkout.session.completed.
                                    // Only applies to subscription-related invoices.
                                    let subscription_id = invoice
                                        .get("subscription")
                                        .and_then(|s| s.as_str())
                                        .map(|s| s.to_string());

                                    if let (Some(cid), Some(sub_id)) =
                                        (customer_id, subscription_id)
                                    {
                                        tracing::debug!(
                                            "🔒 Safety net: ensuring active status for customer {} (subscription: {})",
                                            cid,
                                            sub_id
                                        );

                                        let update = SubscriptionUpdate {
                                            user_id: self.extract_user_id_from_customer(cid),
                                            subscription_tier: "keep_current".to_string(),
                                            subscription_status: "active".to_string(),
                                            stripe_subscription_id: Some(sub_id),
                                            subscription_started_at: None,
                                            subscription_ends_at: None,
                                            trial_ends_at: None,
                                        };
                                        updates.push(update);
                                    }
                                }
                            } else {
                                // Fallback if we can't parse the invoice
                                tracing::info!("💰 Payment succeeded - subscription is active");
                            }
                        }
                    }
                }
                "invoice.payment_failed" => {
                    // Parse invoice data to show the failed amount
                    if let Some(data) = &event.data {
                        if let Some(invoice_obj) = &data.object {
                            if let Ok(invoice) =
                                serde_json::from_value::<serde_json::Value>(invoice_obj.clone())
                            {
                                let amount_due = invoice
                                    .get("amount_due")
                                    .and_then(|a| a.as_i64())
                                    .unwrap_or(0);
                                let customer_id = invoice.get("customer").and_then(|c| c.as_str());
                                let amount_dollars = amount_due as f64 / 100.0;

                                tracing::info!(
                                    "❌ Payment failed - ${:.2} payment failed for customer {}",
                                    amount_dollars,
                                    customer_id.unwrap_or("unknown")
                                );
                            } else {
                                // Fallback if we can't parse the invoice
                                tracing::info!("❌ Payment failed - subscription may be suspended");
                            }
                        }
                    }
                }
                _ => {
                    tracing::debug!("🔄 Ignoring webhook event type: {}", event_type);
                }
            }
        }

        Ok(WebhookResult {
            event_id,
            event_created,
            event_type,
            subscription_updates: updates,
        })
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

    pub async fn handle_checkout_completion(
        &self,
        customer_id: &str,
        new_subscription_id: &str,
    ) -> Result<SubscriptionUpdate> {
        tracing::info!(
            "🧹 Handling checkout completion for customer: {}, new subscription: {}",
            customer_id,
            new_subscription_id
        );

        // List all subscriptions for this customer
        let subscription_list = self.client.list_subscriptions(customer_id).await?;
        let subscriptions = subscription_list.data.unwrap_or_default();

        tracing::info!(
            "📊 Found {} subscriptions for customer {}",
            subscriptions.len(),
            customer_id
        );

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
                    if matches!(subscription.status.as_deref(), Some("trialing" | "active")) {
                        tracing::info!("🗑️ Cancelling prior subscription: {}", sub_id);
                        match self.client.cancel_subscription(sub_id).await {
                            Ok(_) => {
                                tracing::info!(
                                    "✅ Successfully cancelled prior subscription: {}",
                                    sub_id
                                );
                            }
                            Err(e) => {
                                tracing::error!(
                                    "❌ Failed to cancel prior subscription {}: {}",
                                    sub_id,
                                    e
                                );
                                return Err(e);
                            }
                        }
                    }
                }
            }
        }

        // Create update for the new paid subscription
        let user_id = self.extract_user_id_from_customer(customer_id);

        tracing::info!(
            "✅ Creating subscription update for user {} with tier {}",
            user_id,
            tier
        );

        Ok(SubscriptionUpdate {
            user_id,
            subscription_tier: tier, // Use actual tier from subscription metadata
            subscription_status: "active".to_string(), // Paid subscription is active
            stripe_subscription_id: Some(new_subscription_id.to_string()),
            subscription_started_at: Some(chrono::Utc::now().to_rfc3339()),
            subscription_ends_at: None,
            trial_ends_at: None, // No longer in trial
        })
    }

    pub fn get_pricing_for_frontend(&self) -> PricingInfo {
        self.cached_pricing.clone()
    }

    // Helper method to extract user_id from Stripe customer_id
    fn extract_user_id_from_customer(&self, customer_id: &str) -> String {
        // If customer_id follows our pattern "canary_user_{user_id}"
        if customer_id.starts_with("canary_user_") {
            customer_id
                .strip_prefix("canary_user_")
                .unwrap_or(customer_id)
                .to_string()
        } else {
            // For Stripe-generated customer IDs, return with prefix for lookup
            format!("stripe_customer:{}", customer_id)
        }
    }

    // Helper method to determine tier from subscription items (products/prices)
    fn determine_tier_from_subscription_items(&self, subscription: &serde_json::Value) -> String {
        if let Some(items) = subscription.get("items") {
            if let Some(data) = items.get("data") {
                if let Some(items_array) = data.as_array() {
                    for item in items_array {
                        if let Some(price) = item.get("price") {
                            if let Some(product_id) = price.get("product").and_then(|p| p.as_str())
                            {
                                // Check our cached pricing to find the tier for this product
                                for tier_info in &self.cached_pricing.tiers {
                                    if let Some(monthly_price) = &tier_info.monthly_price {
                                        // Get product from our pricing cache (we need to check against the product)
                                        // For now, use the price_id to determine tier
                                        if let Some(price_id) =
                                            price.get("id").and_then(|id| id.as_str())
                                        {
                                            if monthly_price.price_id == price_id {
                                                tracing::info!(
                                                    "🎯 Determined tier from price_id {}: {}",
                                                    price_id,
                                                    tier_info.tier
                                                );
                                                return tier_info.tier.clone();
                                            }
                                        }
                                        // Also check yearly price
                                        if let Some(yearly_price) = &tier_info.yearly_price {
                                            if let Some(price_id) =
                                                price.get("id").and_then(|id| id.as_str())
                                            {
                                                if yearly_price.price_id == price_id {
                                                    tracing::info!("🎯 Determined tier from yearly price_id {}: {}", price_id, tier_info.tier);
                                                    return tier_info.tier.clone();
                                                }
                                            }
                                        }
                                    }
                                }
                                tracing::warn!(
                                    "❓ Could not determine tier for product_id: {}",
                                    product_id
                                );
                            }
                        }
                    }
                }
            }
        }

        // Fallback to metadata-based tier detection
        if let Some(tier) = subscription
            .get("metadata")
            .and_then(|m| m.get("tier"))
            .and_then(|t| t.as_str())
        {
            tracing::info!("📋 Using tier from metadata: {}", tier);
            return tier.to_lowercase();
        }

        let sub_id = subscription
            .get("id")
            .and_then(|id| id.as_str())
            .unwrap_or("unknown");
        let price_ids: Vec<&str> = subscription
            .get("items")
            .and_then(|i| i.get("data"))
            .and_then(|d| d.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        item.get("price")
                            .and_then(|p| p.get("id"))
                            .and_then(|id| id.as_str())
                    })
                    .collect()
            })
            .unwrap_or_default();
        tracing::warn!(
            "⚠️ Could not determine tier from subscription {}, price_ids: {:?}, defaulting to personal",
            sub_id,
            price_ids
        );
        "personal".to_string()
    }
}
