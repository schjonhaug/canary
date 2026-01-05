use crate::message_formatter::MessageFormatter;
use crate::metadata::{
    Contact, EventType, NotificationMethod, ProviderType, TransactionNotification,
};
use crate::notifications::{NotificationProvider, NotificationResult, ProviderInfo};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde_json::json;

/// Authentication method for ntfy server
#[derive(Clone, Debug)]
pub enum NtfyAuth {
    /// No authentication (public ntfy.sh or open server)
    None,
    /// Bearer token authentication: Authorization: Bearer <token>
    AccessToken(String),
    /// Basic authentication: Authorization: Basic base64(username:password)
    BasicAuth { username: String, password: String },
}

pub struct NtfyProvider {
    client: reqwest::Client,
    server_url: String,
    auth: NtfyAuth,
}

impl NtfyProvider {
    pub fn new(server_url: String) -> Self {
        Self::with_auth(server_url, NtfyAuth::None)
    }

    pub fn with_auth(server_url: String, auth: NtfyAuth) -> Self {
        // Ensure the server URL doesn't have a trailing slash
        let server_url = server_url.trim_end_matches('/').to_string();
        Self {
            client: reqwest::Client::new(),
            server_url,
            auth,
        }
    }

    /// Build the Authorization header value based on auth method
    fn auth_header(&self) -> Option<String> {
        match &self.auth {
            NtfyAuth::None => None,
            NtfyAuth::AccessToken(token) => Some(format!("Bearer {}", token)),
            NtfyAuth::BasicAuth { username, password } => {
                let credentials = format!("{}:{}", username, password);
                let encoded = BASE64.encode(credentials.as_bytes());
                Some(format!("Basic {}", encoded))
            }
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
                use crate::metadata::Language;
                let localized_title = match notification {
                    TransactionNotification::Pending(tx) => match tx.transaction_type {
                        EventType::Receive => match contact.language {
                            Language::English => format!("Receiving Bitcoin - {}", wallet_name),
                            Language::Norwegian => format!("Mottar Bitcoin - {}", wallet_name),
                            Language::Spanish => format!("Recibiendo Bitcoin - {}", wallet_name),
                            Language::Portuguese => format!("Recebendo Bitcoin - {}", wallet_name),
                            Language::German => format!("Bitcoin empfangen - {}", wallet_name),
                            Language::French => format!("Reception de Bitcoin - {}", wallet_name),
                            Language::Japanese => format!("ビットコイン受取中 - {}", wallet_name),
                        },
                        EventType::Send => match contact.language {
                            Language::English => format!("Sending Bitcoin - {}", wallet_name),
                            Language::Norwegian => format!("Sender Bitcoin - {}", wallet_name),
                            Language::Spanish => format!("Enviando Bitcoin - {}", wallet_name),
                            Language::Portuguese => format!("Enviando Bitcoin - {}", wallet_name),
                            Language::German => format!("Bitcoin senden - {}", wallet_name),
                            Language::French => format!("Envoi de Bitcoin - {}", wallet_name),
                            Language::Japanese => format!("ビットコイン送信中 - {}", wallet_name),
                        },
                    },
                    TransactionNotification::Confirmed(tx) => match tx.transaction_type {
                        EventType::Receive => match contact.language {
                            Language::English => format!("Bitcoin Received - {}", wallet_name),
                            Language::Norwegian => format!("Bitcoin mottatt - {}", wallet_name),
                            Language::Spanish => format!("Bitcoin Recibido - {}", wallet_name),
                            Language::Portuguese => format!("Bitcoin Recebido - {}", wallet_name),
                            Language::German => format!("Bitcoin erhalten - {}", wallet_name),
                            Language::French => format!("Bitcoin Recu - {}", wallet_name),
                            Language::Japanese => format!("ビットコイン受取完了 - {}", wallet_name),
                        },
                        EventType::Send => match contact.language {
                            Language::English => format!("Bitcoin Sent - {}", wallet_name),
                            Language::Norwegian => format!("Bitcoin sendt - {}", wallet_name),
                            Language::Spanish => format!("Bitcoin Enviado - {}", wallet_name),
                            Language::Portuguese => format!("Bitcoin Enviado - {}", wallet_name),
                            Language::German => format!("Bitcoin gesendet - {}", wallet_name),
                            Language::French => format!("Bitcoin Envoye - {}", wallet_name),
                            Language::Japanese => format!("ビットコイン送信完了 - {}", wallet_name),
                        },
                    },
                    TransactionNotification::BalanceAlert(_) => match contact.language {
                        Language::English => format!("Balance Alert - {}", wallet_name),
                        Language::Norwegian => format!("Saldovarsel - {}", wallet_name),
                        Language::Spanish => format!("Alerta de Saldo - {}", wallet_name),
                        Language::Portuguese => format!("Alerta de Saldo - {}", wallet_name),
                        Language::German => format!("Kontostandwarnung - {}", wallet_name),
                        Language::French => format!("Alerte de Solde - {}", wallet_name),
                        Language::Japanese => format!("残高アラート - {}", wallet_name),
                    },
                };

                // Build the request with optional authentication
                let mut request = self
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
                    );

                // Add authentication header if configured
                if let Some(auth_value) = self.auth_header() {
                    request = request.header("Authorization", auth_value);
                }

                let result = match request.body(message.clone()).send().await {
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
