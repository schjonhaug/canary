use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use stripe::{
    CheckoutSession, CheckoutSessionMode, Client, CreateCheckoutSession,
    CreateCheckoutSessionLineItems, CreateCustomer, CreatePrice, CreateProduct,
    Currency, Price, Product, BillingPortalSession, CreateBillingPortalSession,
    Event, EventType, Webhook, CreatePriceRecurring, CreatePriceRecurringInterval,
    CustomerId, PriceId, ProductId,
};
use utoipa::ToSchema;

use crate::metadata::UserRecord;
use crate::subscription::SubscriptionTier;

/// Stripe billing configuration and client
pub struct StripeBilling {
    client: Client,
    webhook_secret: String,
    products: HashMap<SubscriptionTier, ProductId>,
    prices: HashMap<SubscriptionTier, PriceId>,
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

impl StripeBilling {
    /// Create a new StripeBilling instance
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
            prices: HashMap::new(),
        };

        // Initialize products and prices
        billing.setup_products_and_prices().await?;

        Ok(billing)
    }

    /// Setup Stripe products and prices for all subscription tiers
    async fn setup_products_and_prices(&mut self) -> Result<()> {
        for tier in [SubscriptionTier::Personal, SubscriptionTier::Pro, SubscriptionTier::Business] {
            let (product_id, price_id) = self.create_product_and_price(&tier).await?;
            self.products.insert(tier, product_id);
            self.prices.insert(tier, price_id);
        }
        Ok(())
    }

    /// Get tier pricing details
    fn get_tier_pricing(&self, tier: &SubscriptionTier) -> (i64, &'static str, &'static str) {
        match tier {
            SubscriptionTier::Personal => (
                900, // $9.00 in cents
                "Canary Personal",
                "Individual Bitcoin wallet monitoring with email notifications",
            ),
            SubscriptionTier::Pro => (
                2900, // $29.00 in cents
                "Canary Pro", 
                "Professional Bitcoin monitoring with SMS, email, and push notifications",
            ),
            SubscriptionTier::Business => (
                9900, // $99.00 in cents
                "Canary Business",
                "Enterprise Bitcoin monitoring with API access and priority support", 
            ),
        }
    }

    /// Create product and price for a specific subscription tier
    async fn create_product_and_price(&self, tier: &SubscriptionTier) -> Result<(ProductId, PriceId)> {
        let (amount, name, description) = self.get_tier_pricing(tier);

        // Create product
        let mut create_product = CreateProduct::new(name);
        create_product.description = Some(description);
        create_product.metadata = Some({
            let mut metadata = HashMap::new();
            metadata.insert("tier".to_string(), tier.as_str().to_string());
            metadata
        });

        let product = Product::create(&self.client, create_product).await?;

        // Create price
        let mut create_price = CreatePrice::new(Currency::USD);
        create_price.product = Some(stripe::IdOrCreate::Id(&product.id));
        create_price.unit_amount = Some(amount);
        create_price.recurring = Some(CreatePriceRecurring {
            interval: CreatePriceRecurringInterval::Month,
            interval_count: Some(1),
            ..Default::default()
        });
        create_price.metadata = Some({
            let mut metadata = HashMap::new();
            metadata.insert("tier".to_string(), tier.as_str().to_string());
            metadata
        });

        let price = Price::create(&self.client, create_price).await?;

        Ok((product.id, price.id))
    }

    /// Create a Stripe checkout session for a user and subscription tier
    pub async fn create_checkout_session(
        &self,
        user: &UserRecord,
        tier: SubscriptionTier,
        success_url: &str,
        cancel_url: &str,
    ) -> Result<CheckoutSessionResponse> {
        // Get or create Stripe customer
        let customer_id = match &user.stripe_customer_id {
            Some(id) => CustomerId::from_str(id)?,
            None => self.create_customer(user).await?,
        };

        let price_id = self.prices.get(&tier)
            .ok_or_else(|| anyhow::anyhow!("Price not found for tier: {:?}", tier))?;

        let mut create_session = CreateCheckoutSession::new();
        create_session.mode = Some(CheckoutSessionMode::Subscription);
        create_session.customer = Some(customer_id);
        create_session.success_url = Some(success_url);
        create_session.cancel_url = Some(cancel_url);
        
        create_session.line_items = Some(vec![CreateCheckoutSessionLineItems {
            price: Some(price_id.to_string()),
            quantity: Some(1),
            ..Default::default()
        }]);

        create_session.metadata = Some({
            let mut metadata = HashMap::new();
            metadata.insert("user_id".to_string(), user.id.clone());
            metadata.insert("tier".to_string(), tier.as_str().to_string());
            metadata
        });

        let session = CheckoutSession::create(&self.client, create_session).await?;
        
        Ok(CheckoutSessionResponse {
            url: session.url.unwrap_or_default(),
            session_id: session.id.to_string(),
        })
    }

    /// Create a Stripe customer for a user
    async fn create_customer(&self, user: &UserRecord) -> Result<CustomerId> {
        let mut create_customer = CreateCustomer::new();
        create_customer.email = Some(&user.email);
        create_customer.name = user.name.as_deref();
        create_customer.metadata = Some({
            let mut metadata = HashMap::new();
            metadata.insert("user_id".to_string(), user.id.clone());
            metadata
        });

        let customer = stripe::Customer::create(&self.client, create_customer).await?;
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
        
        // TODO: Implement checkout completion logic
        // Extract session from event data and update user subscription
        
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

    // TODO: Add unit tests for StripeBilling methods
    // Would need to mock Stripe API calls for proper testing
}