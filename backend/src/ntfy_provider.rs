use crate::message_formatter::MessageFormatter;
use crate::metadata::{
    Contact, EventType, NotificationMethod, ProviderType, TransactionNotification,
};
use crate::notifications::{NotificationProvider, NotificationResult, ProviderInfo};
use async_trait::async_trait;
use serde_json::json;

pub struct NtfyProvider {
    client: reqwest::Client,
    server_url: String,
}

impl NtfyProvider {
    pub fn new(server_url: String) -> Self {
        // Ensure the server URL doesn't have a trailing slash
        let server_url = server_url.trim_end_matches('/').to_string();
        Self {
            client: reqwest::Client::new(),
            server_url,
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

                // Extract priority for ntfy headers
                let priority = match notification {
                    TransactionNotification::Pending(_) => "high",
                    TransactionNotification::Confirmed(_) => "default",
                    TransactionNotification::BalanceAlert(_) => "urgent",
                };

                let topic = &method.notification_target;
                let ntfy_url = format!("{}/{}", self.server_url, topic);

                // Create localized title for push notification
                let localized_title = match notification {
                    TransactionNotification::Pending(tx) => {
                        match tx.transaction_type {
                            EventType::Receive => match contact.language {
                                crate::metadata::Language::Norwegian => format!("Mottar Bitcoin - {}", wallet_name),
                                crate::metadata::Language::English => format!("Receiving Bitcoin - {}", wallet_name),
                            },
                            EventType::Send => match contact.language {
                                crate::metadata::Language::Norwegian => format!("Sender Bitcoin - {}", wallet_name),
                                crate::metadata::Language::English => format!("Sending Bitcoin - {}", wallet_name),
                            },
                        }
                    }
                    TransactionNotification::Confirmed(tx) => {
                        match tx.transaction_type {
                            EventType::Receive => match contact.language {
                                crate::metadata::Language::Norwegian => format!("Bitcoin mottatt - {}", wallet_name),
                                crate::metadata::Language::English => format!("Bitcoin Received - {}", wallet_name),
                            },
                            EventType::Send => match contact.language {
                                crate::metadata::Language::Norwegian => format!("Bitcoin sendt - {}", wallet_name),
                                crate::metadata::Language::English => format!("Bitcoin Sent - {}", wallet_name),
                            },
                        }
                    }
                    TransactionNotification::BalanceAlert(_) => {
                        match contact.language {
                            crate::metadata::Language::Norwegian => format!("Saldovarsel - {}", wallet_name),
                            crate::metadata::Language::English => format!("Balance Alert - {}", wallet_name),
                        }
                    }
                };

                let result = match self
                    .client
                    .post(&ntfy_url)
                    .header("Content-Type", "text/plain; charset=utf-8")
                    .header("Title", localized_title)
                    .header("Priority", priority)
                    .header(
                        "Tags",
                        match notification {
                            TransactionNotification::Pending(tx)
                            | TransactionNotification::Confirmed(tx) => {
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
        // Extract the server name for display (e.g., "ntfy.sh" from "https://ntfy.sh")
        let server_display = self
            .server_url
            .trim_start_matches("https://")
            .trim_start_matches("http://");

        ProviderInfo {
            name: "ntfy".to_string(),
            display_name: format!("{} Notifications", server_display),
            config_schema: json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "title": "ntfy Topic",
                        "description": format!("The {} topic name to send notifications to (e.g., 'my-bitcoin-wallet')", server_display)
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
        Self::new("https://ntfy.sh".to_string())
    }
}
