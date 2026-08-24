use crate::message_formatter::MessageFormatter;
use crate::metadata::{
    Contact, EventType, Language, NotificationMethod, ProviderType, TransactionNotification,
};
use crate::notifications::{
    notification_methods_for_provider, NotificationProvider, NotificationResult, ProviderInfo,
};
use crate::outbound_target::{client_for_public_url, validate_public_url};
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
    server_url: String,
    auth: NtfyAuth,
    trusted_server: bool,
    trusted_client: Option<reqwest::Client>,
}

impl NtfyProvider {
    pub fn new(server_url: String) -> Self {
        Self::with_auth(server_url, NtfyAuth::None)
    }

    pub fn with_auth(server_url: String, auth: NtfyAuth) -> Self {
        Self {
            server_url: server_url.trim_end_matches('/').to_string(),
            auth,
            trusted_server: false,
            trusted_client: None,
        }
    }

    pub fn with_trusted_auth(server_url: String, auth: NtfyAuth) -> Self {
        Self {
            server_url: server_url.trim_end_matches('/').to_string(),
            auth,
            trusted_server: true,
            trusted_client: Some(
                reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(10))
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                    .expect("failed to build ntfy HTTP client"),
            ),
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
        user_language: &Language,
        wallet_balance_sats: Option<i64>,
    ) -> Vec<(NotificationMethod, NotificationResult, String)> {
        let mut results = Vec::new();

        for (_contact, method) in notification_methods_for_provider(contacts, &ProviderType::Ntfy) {
            let content = MessageFormatter::create_filtered_content(
                notification,
                wallet_name,
                wallet_balance_sats,
                method.content_fields,
            );
            let message =
                MessageFormatter::create_localized_filtered_message(&content, user_language);

            // Extract priority for ntfy headers
            let priority = if content.event.is_none() {
                "default"
            } else {
                match notification {
                    TransactionNotification::Pending(_) => "high",
                    TransactionNotification::Confirmed(_) => "default",
                    TransactionNotification::BalanceAlert(_) => "urgent",
                }
            };

            let topic = &method.notification_target;
            let ntfy_url = format!("{}/{}", self.server_url, topic);
            let client = if self.trusted_server {
                self.trusted_client.clone().expect("trusted ntfy client")
            } else {
                match validate_public_url(&ntfy_url).await {
                    Ok(parsed_url) => match client_for_public_url(&parsed_url).await {
                        Ok(client) => client,
                        Err(_) => {
                            results.push((method.clone(), blocked_server_result(), message));
                            continue;
                        }
                    },
                    Err(_) => {
                        results.push((method.clone(), blocked_server_result(), message));
                        continue;
                    }
                }
            };

            let localized_title =
                MessageFormatter::create_localized_filtered_title(&content, user_language);

            // Build the request with optional authentication
            let mut request = client
                .post(&ntfy_url)
                .header("Content-Type", "text/plain; charset=utf-8")
                .header("Title", localized_title)
                .header("Priority", priority)
                .header(
                    "Tags",
                    if content.event.is_none() {
                        "bell"
                    } else {
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
                        }
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
                            provider_id: Some(format!("ntfy_{}", chrono::Utc::now().timestamp())),
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
                Err(_) => {
                    let error = "Request to ntfy server failed".to_string();
                    NotificationResult {
                        success: false,
                        provider_id: None,
                        error_message: Some(error),
                    }
                }
            };

            results.push((method.clone(), result, message));
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

fn blocked_server_result() -> NotificationResult {
    NotificationResult {
        success: false,
        provider_id: None,
        error_message: Some("ntfy server is not publicly reachable".to_string()),
    }
}

impl Default for NtfyProvider {
    fn default() -> Self {
        Self::new("https://ntfy.sh".to_string())
    }
}
