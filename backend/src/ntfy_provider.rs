use crate::message_formatter::MessageFormatter;
use crate::metadata::{
    Contact, EventType, NotificationMethod, ProviderType, TransactionNotification,
};
use crate::notifications::{NotificationProvider, NotificationResult, ProviderInfo};
use async_trait::async_trait;
use serde_json::json;

pub struct NtfyProvider {
    client: reqwest::Client,
}

impl NtfyProvider {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl NotificationProvider for NtfyProvider {
    async fn send_notification(
        &self,
        notification: &TransactionNotification,
        wallet_name: &str,
        contacts: &[Contact],
    ) -> Vec<(NotificationMethod, NotificationResult, String)> {
        let mut results = Vec::new();

        for contact in contacts {
            // Find ntfy notification methods for this contact
            let ntfy_methods: Vec<&NotificationMethod> = contact
                .notification_methods
                .iter()
                .filter(|method| matches!(method.provider_type, ProviderType::Ntfy))
                .collect();

            for method in ntfy_methods {
                let message = MessageFormatter::create_localized_message(
                    notification,
                    wallet_name,
                    &contact.language,
                );

                // Extract notification details for ntfy headers
                let (priority, notification_type, txid) = match notification {
                    TransactionNotification::Pending(tx) => ("high", "pending", &tx.txid),
                    TransactionNotification::Confirmed(tx) => ("default", "confirmed", &tx.txid),
                    TransactionNotification::BalanceAlert(alert) => ("urgent", "balance_alert", &alert.id),
                };

                let topic = &method.notification_target;
                let ntfy_url = format!("https://ntfy.sh/{}", topic);

                let result = match self
                    .client
                    .post(&ntfy_url)
                    .header("Content-Type", "text/plain; charset=utf-8")
                    .header("Title", format!("Canary - {}", wallet_name))
                    .header("Priority", priority)
                    .header(
                        "Tags",
                        match notification {
                            TransactionNotification::Pending(tx) | TransactionNotification::Confirmed(tx) => {
                                if tx.transaction_type == EventType::Receive {
                                    "money_with_wings"
                                } else {
                                    "arrow_right"
                                }
                            }
                            TransactionNotification::BalanceAlert(_) => "chart_with_upwards_trend",
                        },
                    )
                    .body(message.clone())
                    .send()
                    .await
                {
                    Ok(response) => {
                        if response.status().is_success() {
                            NotificationResult {
                                success: true,
                                provider_id: Some(format!(
                                    "ntfy_{}",
                                    chrono::Utc::now().timestamp()
                                )),
                                error_message: None,
                            }
                        } else {
                            let error = format!(
                                "HTTP {}: {}",
                                response.status(),
                                response.status().canonical_reason().unwrap_or("Unknown")
                            );
                            NotificationResult {
                                success: false,
                                provider_id: None,
                                error_message: Some(error),
                            }
                        }
                    }
                    Err(e) => {
                        let error = format!("Request failed: {}", e);
                        NotificationResult {
                            success: false,
                            provider_id: None,
                            error_message: Some(error),
                        }
                    }
                };

                results.push((method.clone(), result, message));
            }
        }

        results
    }

    fn provider_info(&self) -> ProviderInfo {
        ProviderInfo {
            name: "ntfy".to_string(),
            display_name: "ntfy.sh Notifications".to_string(),
            config_schema: json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "title": "ntfy Topic",
                        "description": "The ntfy.sh topic name to send notifications to (e.g., 'my-bitcoin-wallet')"
                    }
                },
                "required": ["topic"]
            }),
        }
    }

    fn name(&self) -> &'static str {
        "ntfy"
    }
}

impl Default for NtfyProvider {
    fn default() -> Self {
        Self::new()
    }
}
