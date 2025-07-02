use crate::metadata::{ContactPerson, EventType, TransactionEvent, TwilioConfig};
use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use reqwest::Client;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct TwilioSmsRequest {
    #[serde(rename = "To")]
    to: String,
    #[serde(rename = "From")]
    from: String,
    #[serde(rename = "Body")]
    body: String,
}

#[derive(Debug)]
pub struct SmsResponse {
    pub success: bool,
    pub twilio_sid: Option<String>,
    pub error_message: Option<String>,
}

pub struct SmsService {
    pub client: Client,
}

impl SmsService {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    /// Format Bitcoin amount in Norwegian style
    pub fn format_btc_amount(amount_sats: i64) -> String {
        let btc_amount = amount_sats as f64 / 100_000_000.0;

        // Format manually for Norwegian locale (comma as decimal separator, space as thousands separator)
        if btc_amount < 1.0 {
            // For small amounts, show all decimal places
            format!("{:.8}", btc_amount).replace('.', ",")
        } else {
            // For larger amounts, format with thousands separator
            let formatted = format!("{:.8}", btc_amount);
            let parts: Vec<&str> = formatted.split('.').collect();
            let integer_part = parts[0];
            let decimal_part = parts.get(1).unwrap_or(&"00000000");

            // Add space thousands separator
            let mut result = String::new();
            for (i, c) in integer_part.chars().rev().enumerate() {
                if i > 0 && i % 3 == 0 {
                    result.push(' ');
                }
                result.push(c);
            }
            let integer_formatted: String = result.chars().rev().collect();

            format!("{},{}", integer_formatted, decimal_part)
        }
    }

    /// Generate Norwegian SMS message for transaction event
    pub fn create_norwegian_message(event: &TransactionEvent, wallet_name: &str) -> String {
        match event.event_type {
            EventType::Send => {
                if event.is_confirmed {
                    if event.amount_sats > 0 {
                        let amount_btc = Self::format_btc_amount(event.amount_sats);
                        format!(
                            "✅ Sending bekreftet: {} BTC fra {}",
                            amount_btc, wallet_name
                        )
                    } else {
                        format!("✅ Sending bekreftet for {}", wallet_name)
                    }
                } else if event.is_rbf {
                    let fee_btc = Self::format_btc_amount(event.amount_sats);
                    format!("📤 RBF gebyr økning: +{} BTC for {}", fee_btc, wallet_name)
                } else if event.is_cpfp {
                    let fee_btc = Self::format_btc_amount(event.amount_sats);
                    format!("🚀 CPFP gebyr: {} BTC for {}", fee_btc, wallet_name)
                } else {
                    let amount_btc = Self::format_btc_amount(event.amount_sats);
                    format!("📤 Sender {} BTC fra {}", amount_btc, wallet_name)
                }
            }
            EventType::Receive => {
                if event.is_confirmed {
                    let amount_btc = Self::format_btc_amount(event.amount_sats);
                    format!(
                        "✅ Mottak bekreftet: {} BTC til {}",
                        amount_btc, wallet_name
                    )
                } else {
                    let amount_btc = Self::format_btc_amount(event.amount_sats);
                    format!("📥 Mottar {} BTC til {}", amount_btc, wallet_name)
                }
            }
        }
    }

    /// Send SMS via Twilio API
    pub async fn send_sms(
        &self,
        twilio_config: &TwilioConfig,
        contact: &ContactPerson,
        message: &str,
    ) -> Result<SmsResponse> {
        let url = format!(
            "https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json",
            twilio_config.account_sid
        );

        let sms_request = TwilioSmsRequest {
            to: contact.phone_number.clone(),
            from: twilio_config.messaging_service_sid.clone(),
            body: message.to_string(),
        };

        // Create basic auth header
        let auth_string = format!("{}:{}", twilio_config.account_sid, twilio_config.auth_token);
        let auth_header = format!("Basic {}", general_purpose::STANDARD.encode(auth_string));

        let response = self
            .client
            .post(&url)
            .header("Authorization", auth_header)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&sms_request)
            .send()
            .await?;

        if response.status().is_success() {
            // Parse successful response to get Twilio SID
            let response_text = response.text().await?;

            // Try to extract SID from JSON response
            let twilio_sid =
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&response_text) {
                    json.get("sid")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                } else {
                    None
                };

            Ok(SmsResponse {
                success: true,
                twilio_sid,
                error_message: None,
            })
        } else {
            let status_code = response.status();
            let error_text = response.text().await?;
            Ok(SmsResponse {
                success: false,
                twilio_sid: None,
                error_message: Some(format!("HTTP {}: {}", status_code, error_text)),
            })
        }
    }

    /// Send SMS notifications for a transaction event
    pub async fn send_event_notifications(
        &self,
        event: &TransactionEvent,
        wallet_name: &str,
        contacts: Vec<ContactPerson>,
        twilio_config: &TwilioConfig,
    ) -> Vec<(ContactPerson, SmsResponse)> {
        let message = Self::create_norwegian_message(event, wallet_name);
        let mut results = Vec::new();

        for contact in contacts {
            let response = match self.send_sms(twilio_config, &contact, &message).await {
                Ok(response) => response,
                Err(e) => SmsResponse {
                    success: false,
                    twilio_sid: None,
                    error_message: Some(e.to_string()),
                },
            };

            results.push((contact, response));
        }

        results
    }
}
