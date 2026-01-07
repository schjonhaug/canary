use crate::message_formatter::MessageFormatter;
use crate::metadata::{Contact, Language, NotificationMethod, ProviderType, TransactionNotification};
use crate::notifications::{NotificationProvider, NotificationResult, ProviderInfo};
use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use reqwest::Client;
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Serialize)]
struct TwilioSmsRequest {
    #[serde(rename = "To")]
    to: String,
    #[serde(rename = "From")]
    from: String,
    #[serde(rename = "Body")]
    body: String,
}

pub struct TwilioConfig {
    pub account_sid: String,
    pub auth_token: String,
    pub sender_id: String,
}

impl TwilioConfig {
    pub fn from_env() -> Option<Self> {
        let account_sid = std::env::var("TWILIO_ACCOUNT_SID").ok()?;
        let auth_token = std::env::var("TWILIO_AUTH_TOKEN").ok()?;
        let sender_id = std::env::var("TWILIO_SENDER_ID").ok()?;

        Some(Self {
            account_sid,
            auth_token,
            sender_id,
        })
    }
}

pub struct TwilioProvider {
    client: Client,
    config: TwilioConfig,
}

impl TwilioProvider {
    pub fn new(config: TwilioConfig) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }

    pub fn from_env() -> Option<Self> {
        let config = TwilioConfig::from_env()?;
        Some(Self::new(config))
    }

    async fn send_sms(&self, phone_number: &str, message: &str) -> NotificationResult {
        let url = format!(
            "https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json",
            self.config.account_sid
        );

        let from_number = &self.config.sender_id;

        let sms_request = TwilioSmsRequest {
            to: phone_number.to_string(),
            from: from_number.to_string(),
            body: message.to_string(),
        };

        // Create basic auth header
        let auth_string = format!("{}:{}", self.config.account_sid, self.config.auth_token);
        let auth_header = format!("Basic {}", general_purpose::STANDARD.encode(auth_string));

        match self
            .client
            .post(&url)
            .header("Authorization", auth_header)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&sms_request)
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    // Parse successful response to get Twilio SID
                    match response.text().await {
                        Ok(response_text) => {
                            let twilio_sid = if let Ok(json) =
                                serde_json::from_str::<serde_json::Value>(&response_text)
                            {
                                json.get("sid")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string())
                            } else {
                                None
                            };

                            NotificationResult {
                                success: true,
                                provider_id: twilio_sid,
                                error_message: None,
                            }
                        }
                        Err(e) => NotificationResult {
                            success: false,
                            provider_id: None,
                            error_message: Some(format!("Failed to read response: {}", e)),
                        },
                    }
                } else {
                    let status_code = response.status();
                    match response.text().await {
                        Ok(error_text) => NotificationResult {
                            success: false,
                            provider_id: None,
                            error_message: Some(format!("HTTP {}: {}", status_code, error_text)),
                        },
                        Err(e) => NotificationResult {
                            success: false,
                            provider_id: None,
                            error_message: Some(format!(
                                "HTTP {} and failed to read error: {}",
                                status_code, e
                            )),
                        },
                    }
                }
            }
            Err(e) => NotificationResult {
                success: false,
                provider_id: None,
                error_message: Some(format!("Request failed: {}", e)),
            },
        }
    }
}

#[async_trait]
impl NotificationProvider for TwilioProvider {
    async fn send_notification(
        &self,
        notification: &TransactionNotification,
        wallet_name: &str,
        contacts: &[Contact],
        user_language: &Language,
    ) -> Vec<(NotificationMethod, NotificationResult, String)> {
        let mut results = Vec::new();

        for contact in contacts {
            // Find SMS notification methods for this contact
            let sms_methods: Vec<&NotificationMethod> = contact
                .notification_methods
                .iter()
                .filter(|method| matches!(method.provider_type, ProviderType::Sms))
                .collect();

            for method in sms_methods {
                let message = MessageFormatter::create_localized_message(
                    notification,
                    wallet_name,
                    user_language,
                );
                let result = self.send_sms(&method.notification_target, &message).await;
                results.push((method.clone(), result, message));
            }
        }

        results
    }

    fn provider_info(&self) -> ProviderInfo {
        ProviderInfo {
            name: "twilio".to_string(),
            display_name: "SMS".to_string(),
            config_schema: json!({
                "type": "object",
                "properties": {
                    "account_sid": {
                        "type": "string",
                        "description": "Twilio Account SID"
                    },
                    "auth_token": {
                        "type": "string",
                        "description": "Twilio Auth Token"
                    },
                    "sender_id": {
                        "type": "string",
                        "description": "SMS sender ID (alphanumeric name, phone number, or Messaging Service SID)"
                    }
                },
                "required": ["account_sid", "auth_token", "sender_id"]
            }),
        }
    }

    fn name(&self) -> &'static str {
        "twilio"
    }
}
