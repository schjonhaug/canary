use crate::message_formatter::{FilteredNotificationContent, MessageFormatter};
use crate::metadata::{
    Contact, Language, NotificationContentFields, NotificationMethod, ProviderType,
    TransactionNotification,
};
use crate::notifications::{
    notification_methods_for_provider, NotificationProvider, NotificationResult, ProviderInfo,
};
use crate::outbound_target::{client_for_public_url, validate_public_url};
use async_trait::async_trait;
use chrono::Utc;
use futures::{stream, StreamExt};
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use serde_json::json;
use url::Url;

pub const WEBHOOK_SCHEMA_VERSION: u8 = 1;
pub const WEBHOOK_MAX_URL_LENGTH: usize = 2_048;
pub const WEBHOOK_MAX_CONCURRENT_DELIVERIES: usize = 4;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebhookPayload {
    pub schema_version: u8,
    pub event: String,
    pub title: String,
    pub message: String,
    pub sent_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wallet: Option<WebhookWallet>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction: Option<WebhookTransaction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance_alert: Option<WebhookBalanceAlert>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebhookWallet {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebhookTransaction {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_sats: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_balance_sats: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebhookBalanceAlert {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold_sats: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold_currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold_fiat_amount: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_balance_sats: Option<i64>,
}

impl WebhookPayload {
    pub fn for_notification(
        notification: &TransactionNotification,
        wallet_name: &str,
        contact: &Contact,
        language: &Language,
        wallet_balance_sats: Option<i64>,
        content_fields: NotificationContentFields,
    ) -> Self {
        let _ = contact;
        let content = MessageFormatter::create_filtered_content(
            notification,
            wallet_name,
            wallet_balance_sats,
            content_fields,
        );
        let message = MessageFormatter::create_localized_filtered_message(&content, language);
        let title = MessageFormatter::create_localized_filtered_title(&content, language);

        Self {
            schema_version: WEBHOOK_SCHEMA_VERSION,
            event: content.webhook_event().to_string(),
            title,
            message,
            sent_at: Utc::now().to_rfc3339(),
            wallet: content
                .wallet_name
                .clone()
                .map(|name| WebhookWallet { name }),
            transaction: webhook_transaction(&content),
            balance_alert: webhook_balance_alert(&content),
        }
    }

    pub fn test(language: &Language) -> Self {
        let locale = language.as_str();
        Self {
            schema_version: WEBHOOK_SCHEMA_VERSION,
            event: "test".to_string(),
            title: t!("webhook_test_notification.title", locale = locale).to_string(),
            message: t!("webhook_test_notification.message", locale = locale).to_string(),
            sent_at: Utc::now().to_rfc3339(),
            wallet: None,
            transaction: None,
            balance_alert: None,
        }
    }
}

fn webhook_transaction(content: &FilteredNotificationContent) -> Option<WebhookTransaction> {
    if content.transaction_amount_sats.is_none() && content.transaction_balance_sats.is_none() {
        return None;
    }
    Some(WebhookTransaction {
        amount_sats: content.transaction_amount_sats,
        current_balance_sats: content.transaction_balance_sats,
    })
}

fn webhook_balance_alert(content: &FilteredNotificationContent) -> Option<WebhookBalanceAlert> {
    if content.balance_alert_condition.is_none()
        && content.balance_alert_threshold.is_none()
        && content.balance_alert_balance_sats.is_none()
    {
        return None;
    }
    let threshold = content.balance_alert_threshold.as_ref();
    Some(WebhookBalanceAlert {
        condition: content
            .balance_alert_condition
            .map(|condition| condition.as_str().to_string()),
        threshold_sats: threshold.map(|value| value.threshold_sats),
        threshold_currency: threshold.and_then(|value| value.threshold_currency.clone()),
        threshold_fiat_amount: threshold.and_then(|value| value.threshold_fiat_amount),
        current_balance_sats: content.balance_alert_balance_sats,
    })
}

pub async fn validate_webhook_url(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Webhook URL cannot be empty".to_string());
    }
    if trimmed.len() > WEBHOOK_MAX_URL_LENGTH {
        return Err(format!(
            "Webhook URL must be at most {} characters",
            WEBHOOK_MAX_URL_LENGTH
        ));
    }
    let authority = trimmed
        .split_once("://")
        .map(|(_, authority)| authority)
        .ok_or_else(|| "Webhook URL must be an absolute URL".to_string())?;
    if authority.is_empty() || authority.starts_with('/') {
        return Err("Webhook URL must include a host".to_string());
    }

    let mut url =
        Url::parse(trimmed).map_err(|_| "Webhook URL must be an absolute URL".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Webhook URL must use http or https".to_string());
    }
    if url.host().is_none() {
        return Err("Webhook URL must include a host".to_string());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Webhook URL must not include user information".to_string());
    }
    if url.fragment().is_some() {
        return Err("Webhook URL must not include a fragment".to_string());
    }

    url.set_fragment(None);
    let canonical = url.to_string();
    if canonical.len() > WEBHOOK_MAX_URL_LENGTH {
        return Err(format!(
            "Webhook URL must be at most {} characters",
            WEBHOOK_MAX_URL_LENGTH
        ));
    }
    validate_public_url(&canonical).await?;
    Ok(canonical)
}

pub fn redact_webhook_url(input: &str) -> String {
    Url::parse(input)
        .ok()
        .map(|url| url.origin().ascii_serialization())
        .filter(|origin| origin != "null")
        .unwrap_or_else(|| "invalid webhook URL".to_string())
}

pub fn redact_notification_target(provider_type: &str, target: &str) -> String {
    if provider_type == "webhook" {
        redact_webhook_url(target)
    } else {
        target.to_string()
    }
}

fn sanitized_request_error(error: &reqwest::Error, webhook_url: &str) -> String {
    let origin = redact_webhook_url(webhook_url);
    if error.is_timeout() {
        format!("Request to {origin} timed out")
    } else if error.is_connect() {
        format!("Could not connect to {origin}")
    } else if error.is_redirect() {
        format!("Redirect from {origin} was not followed")
    } else {
        format!("Request to {origin} failed")
    }
}

pub struct WebhookProvider {
    test_client: Option<reqwest::Client>,
}

impl WebhookProvider {
    pub fn new() -> Self {
        Self { test_client: None }
    }

    #[allow(dead_code)] // Used by unit tests with local disposable receivers.
    pub fn with_client(client: reqwest::Client) -> Self {
        Self {
            test_client: Some(client),
        }
    }

    pub async fn send_payload(&self, url: &str, payload: &WebhookPayload) -> NotificationResult {
        let client = match &self.test_client {
            Some(client) => client.clone(),
            None => match validate_public_url(url).await {
                Ok(parsed_url) => match client_for_public_url(&parsed_url).await {
                    Ok(client) => client,
                    Err(_) => return blocked_target_result(),
                },
                Err(_) => return blocked_target_result(),
            },
        };
        match client.post(url).json(payload).send().await {
            Ok(response) if response.status().is_success() => NotificationResult {
                success: true,
                provider_id: Some(format!("webhook_{}", uuid::Uuid::new_v4())),
                error_message: None,
            },
            Ok(response) => NotificationResult {
                success: false,
                provider_id: None,
                error_message: Some(format!("HTTP {}", response.status().as_u16())),
            },
            Err(error) => NotificationResult {
                success: false,
                provider_id: None,
                error_message: Some(sanitized_request_error(&error, url)),
            },
        }
    }
}

fn blocked_target_result() -> NotificationResult {
    NotificationResult {
        success: false,
        provider_id: None,
        error_message: Some("Webhook target is not publicly reachable".to_string()),
    }
}

impl Default for WebhookProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NotificationProvider for WebhookProvider {
    async fn send_notification(
        &self,
        notification: &TransactionNotification,
        wallet_name: &str,
        contacts: &[Contact],
        user_language: &Language,
        wallet_balance_sats: Option<i64>,
    ) -> Vec<(NotificationMethod, NotificationResult, String)> {
        let deliveries: Vec<_> =
            notification_methods_for_provider(contacts, &ProviderType::Webhook)
                .map(|(contact, method)| (contact.clone(), method.clone()))
                .collect();

        stream::iter(deliveries)
            .map(|(contact, method)| async move {
                let payload = WebhookPayload::for_notification(
                    notification,
                    wallet_name,
                    &contact,
                    user_language,
                    wallet_balance_sats,
                    method.content_fields,
                );
                let result = self
                    .send_payload(&method.notification_target, &payload)
                    .await;
                (method, result, payload.message)
            })
            .buffer_unordered(WEBHOOK_MAX_CONCURRENT_DELIVERIES)
            .collect()
            .await
    }

    fn provider_info(&self) -> ProviderInfo {
        ProviderInfo {
            name: "webhook".to_string(),
            display_name: "JSON Webhook".to_string(),
            config_schema: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "format": "uri",
                        "maxLength": WEBHOOK_MAX_URL_LENGTH,
                        "title": "Webhook URL",
                        "description": "Absolute HTTP or HTTPS endpoint that receives Canary JSON events"
                    }
                },
                "required": ["url"]
            }),
        }
    }

    fn name(&self) -> &'static str {
        "webhook"
    }
}
