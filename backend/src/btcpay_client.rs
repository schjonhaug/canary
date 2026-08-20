use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;

use crate::config::BtcPayPlanConfig;
use crate::stripe_billing::{FrontendPriceInfo, FrontendTierPricing, PricingInfo};
use crate::subscription::SubscriptionTier;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InvoiceResponse {
    checkout_link: String,
}

#[derive(Debug, Deserialize)]
struct PlanCheckoutResponse {
    url: String,
}

#[derive(Clone)]
pub struct BtcPayClient {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    store_id: String,
    offering_id: Option<String>,
    plan_id: Option<String>,
    cloud_plan_config: Option<BtcPayPlanConfig>,
}

impl std::fmt::Debug for BtcPayClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BtcPayClient")
            .field("base_url", &self.base_url)
            .field("api_key", &"[REDACTED]")
            .field("store_id", &self.store_id)
            .field("offering_id", &self.offering_id)
            .field("plan_id", &self.plan_id)
            .field("cloud_plan_config", &self.cloud_plan_config)
            .finish()
    }
}

impl BtcPayClient {
    pub fn new(
        base_url: String,
        api_key: String,
        store_id: String,
        offering_id: Option<String>,
        plan_id: Option<String>,
        cloud_plan_config: Option<BtcPayPlanConfig>,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            store_id,
            offering_id,
            plan_id,
            cloud_plan_config,
        }
    }

    /// Create a top-up invoice (user chooses amount) and return the checkout link.
    pub async fn create_invoice(&self, redirect_url: &str) -> Result<String> {
        let url = format!("{}/api/v1/stores/{}/invoices", self.base_url, self.store_id);

        let body = serde_json::json!({
            "checkout": {
                "redirectURL": redirect_url,
                "redirectAutomatically": true
            }
        });

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("token {}", self.api_key))
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            tracing::warn!(%status, "BTCPay invoice creation failed");
            return Err(anyhow::anyhow!(
                "BTCPay invoice creation failed ({})",
                status
            ));
        }

        let invoice: InvoiceResponse = response.json().await?;
        Ok(invoice.checkout_link)
    }

    /// Create a recurring plan checkout and return the checkout URL.
    pub async fn create_plan_checkout(&self, redirect_url: &str) -> Result<String> {
        let offering_id = self
            .offering_id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("BTCPAY_OFFERING_ID not configured"))?;
        let plan_id = self
            .plan_id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("BTCPAY_PLAN_ID not configured"))?;

        let url = format!("{}/api/v1/plan-checkout", self.base_url);

        let body = serde_json::json!({
            "storeId": self.store_id,
            "offeringId": offering_id,
            "planId": plan_id,
            "successRedirectLink": redirect_url
        });

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("token {}", self.api_key))
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            tracing::warn!(%status, "BTCPay plan checkout creation failed");
            return Err(anyhow::anyhow!(
                "BTCPay plan checkout creation failed ({})",
                status
            ));
        }

        let checkout: PlanCheckoutResponse = response.json().await?;
        Ok(checkout.url)
    }

    pub async fn create_cloud_subscription_checkout(
        &self,
        tier: SubscriptionTier,
        redirect_url: &str,
        checkout_token: &str,
        email: &str,
    ) -> Result<String> {
        let cloud_plan_config = self
            .cloud_plan_config
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("BTCPAY_CLOUD_* plan configuration is missing"))?;

        let plan_id = match tier {
            SubscriptionTier::Personal => &cloud_plan_config.personal_plan_id,
            SubscriptionTier::Team => &cloud_plan_config.team_plan_id,
        };

        let url = format!("{}/api/v1/plan-checkout", self.base_url);
        let body = serde_json::json!({
            "storeId": self.store_id,
            "offeringId": cloud_plan_config.offering_id,
            "planId": plan_id,
            "successRedirectLink": redirect_url,
            "newSubscriberEmail": email,
            "newSubscriberMetadata": {
                "canaryCheckoutToken": checkout_token
            }
        });

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("token {}", self.api_key))
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            tracing::warn!(%status, "BTCPay cloud checkout creation failed");
            return Err(anyhow::anyhow!(
                "BTCPay cloud checkout creation failed ({})",
                status
            ));
        }

        let checkout: PlanCheckoutResponse = response.json().await?;
        Ok(checkout.url)
    }

    pub fn cloud_plan_tier_from_plan_id(&self, plan_id: &str) -> Option<SubscriptionTier> {
        let cloud_plan_config = self.cloud_plan_config.as_ref()?;
        if plan_id == cloud_plan_config.personal_plan_id {
            Some(SubscriptionTier::Personal)
        } else if plan_id == cloud_plan_config.team_plan_id {
            Some(SubscriptionTier::Team)
        } else {
            None
        }
    }

    pub fn get_cloud_pricing_for_frontend(&self) -> Result<PricingInfo> {
        let cloud_plan_config = self
            .cloud_plan_config
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("BTCPAY_CLOUD_* plan configuration is missing"))?;

        Ok(PricingInfo {
            tiers: vec![
                FrontendTierPricing {
                    tier: "personal".to_string(),
                    name: "Personal".to_string(),
                    description: Some("Individual monitoring for one wallet".to_string()),
                    monthly_price: Some(FrontendPriceInfo {
                        price_id: cloud_plan_config.personal_plan_id.clone(),
                        amount: cloud_plan_config.personal_monthly_price,
                        currency: cloud_plan_config.currency.clone(),
                        interval: "month".to_string(),
                    }),
                    yearly_price: None,
                    features: Self::tier_features("personal"),
                },
                FrontendTierPricing {
                    tier: "team".to_string(),
                    name: "Team".to_string(),
                    description: Some("Faster sync and higher limits for teams".to_string()),
                    monthly_price: Some(FrontendPriceInfo {
                        price_id: cloud_plan_config.team_plan_id.clone(),
                        amount: cloud_plan_config.team_monthly_price,
                        currency: cloud_plan_config.currency.clone(),
                        interval: "month".to_string(),
                    }),
                    yearly_price: None,
                    features: Self::tier_features("team"),
                },
            ],
            yearly_discount_percent: None,
        })
    }

    fn tier_features(tier: &str) -> HashMap<String, String> {
        let mut features = HashMap::new();
        match tier {
            "personal" => {
                features.insert("wallets".to_string(), "1".to_string());
                features.insert("contacts".to_string(), "1".to_string());
                features.insert("sync".to_string(), "10m".to_string());
            }
            "team" => {
                features.insert("wallets".to_string(), "5".to_string());
                features.insert("contacts".to_string(), "5".to_string());
                features.insert("sync".to_string(), "2m".to_string());
            }
            _ => {}
        }
        features
    }
}
