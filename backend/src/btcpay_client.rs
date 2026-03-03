use anyhow::Result;
use serde::Deserialize;
use std::time::Duration;

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
}

impl std::fmt::Debug for BtcPayClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BtcPayClient")
            .field("base_url", &self.base_url)
            .field("api_key", &"[REDACTED]")
            .field("store_id", &self.store_id)
            .field("offering_id", &self.offering_id)
            .field("plan_id", &self.plan_id)
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
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "BTCPay invoice creation failed ({}): {}",
                status,
                error_text
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
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "BTCPay plan checkout creation failed ({}): {}",
                status,
                error_text
            ));
        }

        let checkout: PlanCheckoutResponse = response.json().await?;
        Ok(checkout.url)
    }
}
