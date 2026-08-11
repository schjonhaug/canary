use crate::message_formatter::MessageFormatter;
use crate::metadata::{
    BalanceAlertNotification, Contact, EventType, Language, NotificationMethod, ProviderType,
    Transaction, TransactionNotification,
};
use crate::notifications::{
    notification_log_type, notification_methods_for_provider, NotificationProvider,
    NotificationResult, ProviderInfo,
};
use async_trait::async_trait;
use chrono::Utc;
use futures::{stream, StreamExt};
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::OnceLock;
use std::time::Duration;
use url::Url;

pub const WEBHOOK_SCHEMA_VERSION: u8 = 1;
pub const WEBHOOK_MAX_URL_LENGTH: usize = 2_048;
pub const WEBHOOK_MAX_CONCURRENT_DELIVERIES: usize = 4;
const WEBHOOK_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
static WEBHOOK_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebhookWallet {
    pub checksum: String,
    pub name: String,
    pub balance_sats: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebhookContact {
    pub id: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebhookTransaction {
    pub txid: String,
    pub direction: String,
    pub amount_sats: i64,
    pub fee_sats: Option<i64>,
    pub block_height: Option<u32>,
    pub first_seen_at: u64,
    pub confirmed_at: Option<u64>,
    pub status: String,
    pub parent_txid: Option<String>,
    pub replaced_by_txid: Option<String>,
    pub replaced_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebhookBalanceAlert {
    pub id: String,
    pub alert_id: String,
    pub alert_type: String,
    pub threshold_sats: i64,
    pub current_balance_sats: i64,
    pub threshold_currency: Option<String>,
    pub threshold_fiat_amount: Option<f64>,
    pub exchange_rate_snapshot: Option<f64>,
    pub current_fiat_amount: Option<f64>,
}

impl From<&Transaction> for WebhookTransaction {
    fn from(transaction: &Transaction) -> Self {
        Self {
            txid: transaction.txid.clone(),
            direction: transaction.transaction_type.as_str().to_string(),
            amount_sats: transaction.amount_sats,
            fee_sats: transaction.fee_sats,
            block_height: transaction.block_height,
            first_seen_at: transaction.first_seen_at,
            confirmed_at: transaction.confirmed_at,
            status: transaction.transaction_status.clone(),
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
            id: alert.id.clone(),
            alert_id: alert.balance_alert_id.clone(),
            alert_type: alert.alert_type.as_str().to_string(),
            threshold_sats: alert.threshold_sats,
            current_balance_sats: alert.current_balance_sats,
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
    ) -> Self {
        let message = MessageFormatter::create_localized_message(
            notification,
            wallet_name,
            language,
            contact.include_wallet_balance_in_tx_notifications,
            wallet_balance_sats,
        );
        let title = localized_title(notification, wallet_name, language);
        let wallet_checksum = match notification {
            TransactionNotification::Pending(transaction)
            | TransactionNotification::Confirmed(transaction) => &transaction.wallet_checksum,
            TransactionNotification::BalanceAlert(alert) => &alert.wallet_checksum,
        };
        let transaction = match notification {
            TransactionNotification::Pending(transaction)
            | TransactionNotification::Confirmed(transaction) => Some(transaction.into()),
            TransactionNotification::BalanceAlert(_) => None,
        };
        let balance_alert = match notification {
            TransactionNotification::BalanceAlert(alert) => Some(alert.into()),
            _ => None,
        };

        Self {
            schema_version: WEBHOOK_SCHEMA_VERSION,
            event: notification_log_type(notification).to_string(),
            title,
            message,
            sent_at: Utc::now().to_rfc3339(),
            wallet: Some(WebhookWallet {
                checksum: wallet_checksum.clone(),
                name: wallet_name.to_string(),
                balance_sats: wallet_balance_sats,
            }),
            contact: Some(WebhookContact {
                id: contact.id.clone(),
                name: contact.name.clone(),
            }),
            transaction,
            balance_alert,
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
        }
    }
}

fn localized_title(
    notification: &TransactionNotification,
    wallet_name: &str,
    language: &Language,
) -> String {
    let locale = language.as_str();
    let title = match notification {
        TransactionNotification::Pending(transaction)
            if transaction.transaction_status == "replaced" =>
        {
            t!("titles.rbf", locale = locale).to_string()
        }
        TransactionNotification::Pending(transaction) => match transaction.transaction_type {
            EventType::Receive => t!("titles.receive.pending", locale = locale).to_string(),
            EventType::Send if transaction.parent_txid.is_some() => {
                t!("titles.send.cpfp", locale = locale).to_string()
            }
            EventType::Send => t!("titles.send.pending", locale = locale).to_string(),
        },
        TransactionNotification::Confirmed(transaction) => match transaction.transaction_type {
            EventType::Receive => t!("titles.receive.confirmed", locale = locale).to_string(),
            EventType::Send => t!("titles.send.confirmed", locale = locale).to_string(),
        },
        TransactionNotification::BalanceAlert(_) => {
            t!("titles.balance_alert", locale = locale).to_string()
        }
    };
    format!("{} - {}", title, wallet_name)
}

pub fn validate_webhook_url(input: &str) -> Result<String, String> {
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
    client: reqwest::Client,
}

impl WebhookProvider {
    pub fn new() -> Self {
        Self::with_client(Self::default_client())
    }

    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }

    pub fn default_client() -> reqwest::Client {
        WEBHOOK_CLIENT
            .get_or_init(|| {
                reqwest::Client::builder()
                    .timeout(WEBHOOK_REQUEST_TIMEOUT)
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                    .expect("failed to build webhook HTTP client")
            })
            .clone()
    }

    pub async fn send_payload(&self, url: &str, payload: &WebhookPayload) -> NotificationResult {
        match self.client.post(url).json(payload).send().await {
            Ok(response) if response.status().is_success() => NotificationResult {
                success: true,
                provider_id: Some(format!("webhook_{}", Utc::now().timestamp_millis())),
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
