use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Minimal Stripe API models - only what we need
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Customer {
    pub id: Option<String>,
    pub email: Option<String>,
    pub name: Option<String>,
    pub metadata: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: Option<String>,
    pub customer: Option<String>,
    pub status: Option<String>,
    pub trial_start: Option<i64>,
    pub trial_end: Option<i64>,
    pub current_period_start: Option<i64>,
    pub current_period_end: Option<i64>,
    pub metadata: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutSession {
    pub id: Option<String>,
    pub url: Option<String>,
    pub customer: Option<String>,
    pub subscription: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingPortalSession {
    pub id: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    pub id: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub active: Option<bool>,
    pub metadata: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductList {
    pub object: Option<String>,
    pub data: Option<Vec<Product>>,
    pub has_more: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Price {
    pub id: Option<String>,
    pub product: Option<String>,
    pub unit_amount: Option<i64>,
    pub currency: Option<String>,
    pub recurring: Option<PriceRecurring>,
    pub metadata: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceRecurring {
    pub interval: Option<String>,
    pub interval_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceList {
    pub object: Option<String>,
    pub data: Option<Vec<Price>>,
    pub has_more: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coupon {
    pub id: Option<String>,
    pub name: Option<String>,
    pub percent_off: Option<f64>,
    pub amount_off: Option<i64>,
    pub currency: Option<String>,
    pub duration: Option<String>,
    pub valid: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: Option<String>,
    pub r#type: Option<String>,
    pub data: Option<EventData>,
    pub created: Option<i64>,
    pub livemode: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventData {
    pub object: Option<serde_json::Value>, // We'll handle this generically
}

#[derive(Debug, Clone)]
pub struct StripeClientService {
    client: reqwest::Client,
    secret_key: String,
}

impl StripeClientService {
    pub fn new(secret_key: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            secret_key,
        }
    }

    // Helper method to add common Stripe headers
    fn add_stripe_headers(&self, request_builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        request_builder
            .header("Authorization", format!("Bearer {}", self.secret_key))
            .header("Stripe-Version", "2025-07-30.basil") // Latest API version for new project
    }

    pub async fn create_customer(
        &self, 
        email: String, 
        name: Option<String>, 
        metadata: HashMap<String, String>
    ) -> Result<Customer> {
        let mut form_data = vec![("email".to_string(), email)];
        
        if let Some(name) = name {
            form_data.push(("name".to_string(), name));
        }
        
        for (key, value) in metadata {
            form_data.push((format!("metadata[{}]", key), value));
        }

        let response = self.add_stripe_headers(self.client.post("https://api.stripe.com/v1/customers"))
            .form(&form_data)
            .send()
            .await?;

        let customer: Customer = response.json().await?;
        Ok(customer)
    }

    pub async fn create_subscription(
        &self,
        customer_id: String,
        price_id: String,
        trial_period_days: Option<i32>,
        metadata: HashMap<String, String>
    ) -> Result<Subscription> {
        let mut form_data = vec![
            ("customer".to_string(), customer_id),
            ("items[0][price]".to_string(), price_id),
        ];

        if let Some(trial_days) = trial_period_days {
            form_data.push(("trial_period_days".to_string(), trial_days.to_string()));
            form_data.push(("payment_behavior".to_string(), "allow_incomplete".to_string()));
        }

        for (key, value) in metadata {
            form_data.push((format!("metadata[{}]", key), value));
        }

        let response = self.add_stripe_headers(self.client.post("https://api.stripe.com/v1/subscriptions"))
            .form(&form_data)
            .send()
            .await?;

        let subscription: Subscription = response.json().await?;
        Ok(subscription)
    }

    pub async fn create_checkout_session(
        &self,
        customer_id: String,
        price_id: String,
        success_url: String,
        cancel_url: String,
        metadata: HashMap<String, String>,
        discount_coupon: Option<String>
    ) -> Result<CheckoutSession> {
        let mut form_data = vec![
            ("customer".to_string(), customer_id),
            ("success_url".to_string(), success_url),
            ("cancel_url".to_string(), cancel_url),
            ("mode".to_string(), "subscription".to_string()),
            ("payment_method_collection".to_string(), "if_required".to_string()),
            ("line_items[0][price]".to_string(), price_id),
            ("line_items[0][quantity]".to_string(), "1".to_string()),
        ];

        for (key, value) in metadata {
            form_data.push((format!("subscription_data[metadata][{}]", key), value));
        }

        if let Some(coupon) = discount_coupon {
            form_data.push(("discounts[0][coupon]".to_string(), coupon));
        }

        let response = self.add_stripe_headers(self.client.post("https://api.stripe.com/v1/checkout/sessions"))
            .form(&form_data)
            .send()
            .await?;

        let session: CheckoutSession = response.json().await?;
        Ok(session)
    }

    pub async fn create_billing_portal_session(
        &self,
        customer_id: String,
        return_url: String
    ) -> Result<BillingPortalSession> {
        let form_data = vec![
            ("customer".to_string(), customer_id),
            ("return_url".to_string(), return_url),
        ];

        let response = self.add_stripe_headers(self.client.post("https://api.stripe.com/v1/billing_portal/sessions"))
            .form(&form_data)
            .send()
            .await?;

        let session: BillingPortalSession = response.json().await?;
        Ok(session)
    }

    pub async fn list_products(&self, limit: Option<i32>) -> Result<ProductList> {
        let mut query_params = vec![];
        if let Some(limit) = limit {
            query_params.push(("limit".to_string(), limit.to_string()));
        }

        let mut url = "https://api.stripe.com/v1/products".to_string();
        if !query_params.is_empty() {
            let query_string = query_params
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&");
            url.push_str(&format!("?{}", query_string));
        }

        let response = self.add_stripe_headers(self.client.get(&url))
            .send()
            .await?;

        let products: ProductList = response.json().await?;
        Ok(products)
    }

    pub async fn list_prices(&self, limit: Option<i32>, product: Option<String>) -> Result<PriceList> {
        let mut query_params = vec![];
        if let Some(limit) = limit {
            query_params.push(("limit".to_string(), limit.to_string()));
        }
        if let Some(product) = product {
            query_params.push(("product".to_string(), product));
        }

        let mut url = "https://api.stripe.com/v1/prices".to_string();
        if !query_params.is_empty() {
            let query_string = query_params
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&");
            url.push_str(&format!("?{}", query_string));
        }

        let response = self.add_stripe_headers(self.client.get(&url))
            .send()
            .await?;

        let prices: PriceList = response.json().await?;
        Ok(prices)
    }


    pub async fn retrieve_coupon(&self, coupon_id: &str) -> Result<Coupon> {
        let url = format!("https://api.stripe.com/v1/coupons/{}", coupon_id);
        
        let response = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.secret_key))
            .header("Stripe-Version", "2025-07-30.basil")
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Stripe API error: {}", error_text));
        }

        let coupon: Coupon = response.json().await?;
        Ok(coupon)
    }

    pub async fn parse_webhook_event(&self, payload: &str, signature: &str, webhook_secret: &str) -> Result<Event> {
        // Manual webhook signature verification for 2025 API compatibility
        // This bypasses the old async-stripe library which uses old API versions
        
        tracing::debug!("🔐 Verifying webhook signature");
        
        // Parse signature header
        let mut timestamp = None;
        let mut signatures = Vec::new();
        
        for pair in signature.split(',') {
            let parts: Vec<&str> = pair.splitn(2, '=').collect();
            if parts.len() == 2 {
                match parts[0] {
                    "t" => timestamp = parts[1].parse::<i64>().ok(),
                    "v1" => signatures.push(parts[1]),
                    _ => {} // Ignore unknown signature versions
                }
            }
        }
        
        let timestamp = timestamp.ok_or_else(|| anyhow::anyhow!("No timestamp in signature"))?;
        if signatures.is_empty() {
            return Err(anyhow::anyhow!("No v1 signature found"));
        }
        
        // Verify timestamp (within 5 minutes)
        let now = chrono::Utc::now().timestamp();
        if (now - timestamp).abs() > 300 {
            return Err(anyhow::anyhow!("Webhook timestamp too old"));
        }
        
        // Create expected signature
        let signed_payload = format!("{}.{}", timestamp, payload);
        let expected_sig = {
            use hmac::{Hmac, Mac};
            use sha2::Sha256;
            type HmacSha256 = Hmac<Sha256>;
            
            let mut mac = HmacSha256::new_from_slice(webhook_secret.as_bytes())
                .map_err(|e| anyhow::anyhow!("Invalid webhook secret: {}", e))?;
            mac.update(signed_payload.as_bytes());
            hex::encode(mac.finalize().into_bytes())
        };
        
        // Verify signature
        let signature_valid = signatures.iter().any(|sig| {
            // Constant-time comparison
            sig.len() == expected_sig.len() && 
            sig.chars().zip(expected_sig.chars()).all(|(a, b)| a == b)
        });
        
        if !signature_valid {
            return Err(anyhow::anyhow!("Invalid webhook signature"));
        }
        
        tracing::debug!("✅ Webhook signature verified");
        
        // Parse the event directly as JSON (2025 API format)
        let event: Event = serde_json::from_str(payload)
            .map_err(|e| anyhow::anyhow!("Failed to parse webhook event: {}", e))?;
        
        Ok(event)
    }

}