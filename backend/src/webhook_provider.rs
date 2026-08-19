use crate::message_formatter::MessageFormatter;
use crate::metadata::{
    BalanceAlertNotification, Contact, ContentPrivacyLevel, Language, NotificationMethod,
    ProviderType, Transaction, TransactionNotification,
};
use crate::notifications::{
    notification_log_type, notification_methods_for_provider, NotificationProvider,
    NotificationResult, ProviderInfo,
};
use crate::outbound_target::{client_for_public_url, validate_public_url};
use async_trait::async_trait;
use chrono::Utc;
use futures::{stream, StreamExt};
use rust_i18n::t;
use serde::{ser::SerializeMap, Deserialize, Serialize, Serializer};
use serde_json::{json, Map, Value};
use url::Url;

pub const WEBHOOK_SCHEMA_VERSION: u8 = 1;
pub const WEBHOOK_MAX_URL_LENGTH: usize = 2_048;
pub const WEBHOOK_MAX_CONCURRENT_DELIVERIES: usize = 4;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct WebhookPayload {
    pub schema_version: u8,
    pub event: String,
    pub title: String,
    pub message: String,
    pub sent_at: String,
    pub wallet: Option<WebhookWallet>,
    pub contact: Option<WebhookContact>,
    pub transaction: Option<WebhookTransaction>,
    pub balance_alert: Option<WebhookBalanceAlert>,
    #[serde(skip)]
    content_privacy_level: ContentPrivacyLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebhookWallet {
    pub checksum: Option<String>,
    pub name: Option<String>,
    pub balance_sats: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebhookContact {
    pub id: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebhookTransaction {
    pub txid: Option<String>,
    pub direction: Option<String>,
    pub amount_sats: Option<i64>,
    pub fee_sats: Option<i64>,
    pub block_height: Option<u32>,
    pub first_seen_at: Option<u64>,
    pub confirmed_at: Option<u64>,
    pub status: Option<String>,
    pub parent_txid: Option<String>,
    pub replaced_by_txid: Option<String>,
    pub replaced_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebhookBalanceAlert {
    pub id: Option<String>,
    pub alert_id: Option<String>,
    pub alert_type: Option<String>,
    pub threshold_sats: Option<i64>,
    pub current_balance_sats: Option<i64>,
    pub threshold_currency: Option<String>,
    pub threshold_fiat_amount: Option<f64>,
    pub exchange_rate_snapshot: Option<f64>,
    pub current_fiat_amount: Option<f64>,
}

impl Serialize for WebhookPayload {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut payload = serializer.serialize_map(None)?;
        payload.serialize_entry("schema_version", &self.schema_version)?;
        payload.serialize_entry("event", &self.event)?;
        payload.serialize_entry("title", &self.title)?;
        payload.serialize_entry("message", &self.message)?;
        payload.serialize_entry("sent_at", &self.sent_at)?;

        match self.content_privacy_level {
            ContentPrivacyLevel::Minimal => {}
            ContentPrivacyLevel::Standard => {
                if let Some(wallet) = &self.wallet {
                    let mut fields = Map::new();
                    if let Some(name) = &wallet.name {
                        fields.insert("name".to_string(), Value::String(name.clone()));
                    }
                    payload.serialize_entry("wallet", &fields)?;
                }
                if let Some(transaction) = &self.transaction {
                    let mut fields = Map::new();
                    if let Some(direction) = &transaction.direction {
                        fields.insert("direction".to_string(), Value::String(direction.clone()));
                    }
                    if let Some(status) = &transaction.status {
                        fields.insert("status".to_string(), Value::String(status.clone()));
                    }
                    payload.serialize_entry("transaction", &fields)?;
                }
                if let Some(balance_alert) = &self.balance_alert {
                    let mut fields = Map::new();
                    if let Some(alert_type) = &balance_alert.alert_type {
                        fields.insert("alert_type".to_string(), Value::String(alert_type.clone()));
                    }
                    payload.serialize_entry("balance_alert", &fields)?;
                }
            }
            ContentPrivacyLevel::Detailed => {
                // Detailed is the pre-privacy v1 contract: all top-level and
                // nested fields remain present, including explicit nulls.
                payload.serialize_entry("wallet", &self.wallet)?;
                payload.serialize_entry("contact", &self.contact)?;
                payload.serialize_entry("transaction", &self.transaction)?;
                payload.serialize_entry("balance_alert", &self.balance_alert)?;
            }
        }

        payload.end()
    }
}

impl From<&Transaction> for WebhookTransaction {
    fn from(transaction: &Transaction) -> Self {
        Self {
            txid: Some(transaction.txid.clone()),
            direction: Some(transaction.transaction_type.as_str().to_string()),
            amount_sats: Some(transaction.amount_sats),
            fee_sats: transaction.fee_sats,
            block_height: transaction.block_height,
            first_seen_at: Some(transaction.first_seen_at),
            confirmed_at: transaction.confirmed_at,
            status: Some(transaction.transaction_status.clone()),
            parent_txid: transaction.parent_txid.clone(),
            replaced_by_txid: transaction.replaced_by_txid.clone(),
            replaced_at: transaction.replaced_at,
        }
    }
}

impl From<&BalanceAlertNotification> for WebhookBalanceAlert {
    fn from(alert: &BalanceAlertNotification) -> Self {
        let current_fiat_amount = alert
            .exchange_rate_snapshot
            .map(|rate| alert.current_balance_sats as f64 / 100_000_000.0 * rate);

        Self {
            id: Some(alert.id.clone()),
            alert_id: Some(alert.balance_alert_id.clone()),
            alert_type: Some(alert.alert_type.as_str().to_string()),
            threshold_sats: Some(alert.threshold_sats),
            current_balance_sats: Some(alert.current_balance_sats),
            threshold_currency: alert.threshold_currency.clone(),
            threshold_fiat_amount: alert.threshold_fiat_amount,
            exchange_rate_snapshot: alert.exchange_rate_snapshot,
            current_fiat_amount,
        }
    }
}

impl WebhookPayload {
    pub fn for_notification(
        notification: &TransactionNotification,
        wallet_name: &str,
        contact: &Contact,
        language: &Language,
        wallet_balance_sats: Option<i64>,
        content_privacy_level: ContentPrivacyLevel,
    ) -> Self {
        let message = MessageFormatter::create_localized_message_for_level(
            notification,
            wallet_name,
            language,
            contact.include_wallet_balance_in_tx_notifications,
            wallet_balance_sats,
            content_privacy_level,
        );
        let event = if content_privacy_level == ContentPrivacyLevel::Minimal {
            match notification {
                TransactionNotification::Confirmed(_) => "activity_confirmed",
                _ => "activity_detected",
            }
        } else {
            notification_log_type(notification)
        };
        let title = MessageFormatter::create_localized_title(
            notification,
            wallet_name,
            language,
            content_privacy_level,
        );
        let wallet_checksum = match notification {
            TransactionNotification::Pending(transaction)
            | TransactionNotification::Confirmed(transaction) => &transaction.wallet_checksum,
            TransactionNotification::BalanceAlert(alert) => &alert.wallet_checksum,
        };
        let detailed_transaction: Option<WebhookTransaction> = match notification {
            TransactionNotification::Pending(transaction)
            | TransactionNotification::Confirmed(transaction) => Some(transaction.into()),
            TransactionNotification::BalanceAlert(_) => None,
        };
        let detailed_balance_alert: Option<WebhookBalanceAlert> = match notification {
            TransactionNotification::BalanceAlert(alert) => Some(alert.into()),
            _ => None,
        };

        Self {
            schema_version: WEBHOOK_SCHEMA_VERSION,
            event: event.to_string(),
            title,
            message,
            sent_at: Utc::now().to_rfc3339(),
            wallet: match content_privacy_level {
                ContentPrivacyLevel::Minimal => None,
                ContentPrivacyLevel::Standard => Some(WebhookWallet {
                    checksum: None,
                    name: Some(wallet_name.to_string()),
                    balance_sats: None,
                }),
                ContentPrivacyLevel::Detailed => Some(WebhookWallet {
                    checksum: Some(wallet_checksum.clone()),
                    name: Some(wallet_name.to_string()),
                    balance_sats: wallet_balance_sats,
                }),
            },
            contact: (content_privacy_level == ContentPrivacyLevel::Detailed).then(|| {
                WebhookContact {
                    id: contact.id.clone(),
                    name: contact.name.clone(),
                }
            }),
            transaction: match content_privacy_level {
                ContentPrivacyLevel::Minimal => None,
                ContentPrivacyLevel::Standard => {
                    detailed_transaction.map(|transaction| WebhookTransaction {
                        direction: transaction.direction,
                        status: transaction.status,
                        txid: None,
                        amount_sats: None,
                        fee_sats: None,
                        block_height: None,
                        first_seen_at: None,
                        confirmed_at: None,
                        parent_txid: None,
                        replaced_by_txid: None,
                        replaced_at: None,
                    })
                }
                ContentPrivacyLevel::Detailed => detailed_transaction,
            },
            balance_alert: match content_privacy_level {
                ContentPrivacyLevel::Minimal => None,
                ContentPrivacyLevel::Standard => {
                    detailed_balance_alert.map(|alert| WebhookBalanceAlert {
                        alert_type: alert.alert_type,
                        id: None,
                        alert_id: None,
                        threshold_sats: None,
                        current_balance_sats: None,
                        threshold_currency: None,
                        threshold_fiat_amount: None,
                        exchange_rate_snapshot: None,
                        current_fiat_amount: None,
                    })
                }
                ContentPrivacyLevel::Detailed => detailed_balance_alert,
            },
            content_privacy_level,
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
            contact: None,
            transaction: None,
            balance_alert: None,
            content_privacy_level: ContentPrivacyLevel::Detailed,
        }
    }
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
                    method.content_privacy_level,
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
