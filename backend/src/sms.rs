use crate::metadata::{ContactPerson, EventType, Language, TransactionEvent, TwilioConfig};
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

    /// Format Bitcoin amount based on language preference
    pub fn format_btc_amount(amount_sats: i64, language: &Language) -> String {
        let btc_amount = amount_sats as f64 / 100_000_000.0;

        match language {
            Language::Norwegian => Self::format_btc_amount_norwegian(btc_amount),
            Language::English => Self::format_btc_amount_english(btc_amount),
        }
    }

    /// Format Bitcoin amount in Norwegian style (comma decimal, space thousands)
    fn format_btc_amount_norwegian(btc_amount: f64) -> String {
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

    /// Format Bitcoin amount in English style (period decimal, comma thousands)
    fn format_btc_amount_english(btc_amount: f64) -> String {
        if btc_amount < 1.0 {
            // For small amounts, show all decimal places
            format!("{:.8}", btc_amount)
        } else {
            // For larger amounts, format with thousands separator
            let formatted = format!("{:.8}", btc_amount);
            let parts: Vec<&str> = formatted.split('.').collect();
            let integer_part = parts[0];
            let decimal_part = parts.get(1).unwrap_or(&"00000000");

            // Add comma thousands separator
            let mut result = String::new();
            for (i, c) in integer_part.chars().rev().enumerate() {
                if i > 0 && i % 3 == 0 {
                    result.push(',');
                }
                result.push(c);
            }
            let integer_formatted: String = result.chars().rev().collect();

            format!("{}.{}", integer_formatted, decimal_part)
        }
    }

    /// Generate localized SMS message for transaction event
    pub fn create_localized_message(
        event: &TransactionEvent,
        wallet_name: &str,
        language: &Language,
    ) -> String {
        match language {
            Language::Norwegian => Self::create_norwegian_message(event, wallet_name),
            Language::English => Self::create_english_message(event, wallet_name),
        }
    }

    /// Generate Norwegian SMS message for transaction event
    fn create_norwegian_message(
        event: &TransactionEvent,
        wallet_name: &str,
    ) -> String {
        // Always include total balance if available
        let balance_text = if let Some(balance_sats) = event.balance_total {
            let balance_btc = Self::format_btc_amount(balance_sats, &Language::Norwegian);
            format!(" Total balanse: {} BTC", balance_btc)
        } else {
            String::new()
        };

        match event.event_type {
            EventType::Send => {
                if event.is_confirmed {
                    if event.amount_sats > 0 {
                        let amount_btc = Self::format_btc_amount(event.amount_sats, &Language::Norwegian);
                        format!("✅ Sending bekreftet: {} BTC fra {}.{}", amount_btc, wallet_name, balance_text)
                    } else {
                        format!("✅ Sending bekreftet for {}.{}", wallet_name, balance_text)
                    }
                } else if event.is_rbf {
                    let fee_btc = Self::format_btc_amount(event.amount_sats, &Language::Norwegian);
                    format!("📤 RBF gebyrøkning: +{} BTC for {}.{}", fee_btc, wallet_name, balance_text)
                } else if event.is_cpfp {
                    let fee_btc = Self::format_btc_amount(event.amount_sats, &Language::Norwegian);
                    format!("🚀 CPFP gebyr: {} BTC for {}.{}", fee_btc, wallet_name, balance_text)
                } else {
                    let amount_btc = Self::format_btc_amount(event.amount_sats, &Language::Norwegian);
                    format!("📤 Sender {} BTC fra {}.{}", amount_btc, wallet_name, balance_text)
                }
            }
            EventType::Receive => {
                if event.is_confirmed {
                    let amount_btc = Self::format_btc_amount(event.amount_sats, &Language::Norwegian);
                    format!("✅ Mottak bekreftet: {} BTC til {}.{}", amount_btc, wallet_name, balance_text)
                } else {
                    let amount_btc = Self::format_btc_amount(event.amount_sats, &Language::Norwegian);
                    format!("📥 Mottar {} BTC til {}.{}", amount_btc, wallet_name, balance_text)
                }
            }
        }
    }

    /// Generate English SMS message for transaction event
    fn create_english_message(
        event: &TransactionEvent,
        wallet_name: &str,
    ) -> String {
        // Always include total balance if available
        let balance_text = if let Some(balance_sats) = event.balance_total {
            let balance_btc = Self::format_btc_amount(balance_sats, &Language::English);
            format!(" Total balance: {} BTC", balance_btc)
        } else {
            String::new()
        };

        match event.event_type {
            EventType::Send => {
                if event.is_confirmed {
                    if event.amount_sats > 0 {
                        let amount_btc = Self::format_btc_amount(event.amount_sats, &Language::English);
                        format!("✅ Send confirmed: {} BTC from {}.{}", amount_btc, wallet_name, balance_text)
                    } else {
                        format!("✅ Send confirmed for {}.{}", wallet_name, balance_text)
                    }
                } else if event.is_rbf {
                    let fee_btc = Self::format_btc_amount(event.amount_sats, &Language::English);
                    format!("📤 RBF fee increase: +{} BTC for {}.{}", fee_btc, wallet_name, balance_text)
                } else if event.is_cpfp {
                    let fee_btc = Self::format_btc_amount(event.amount_sats, &Language::English);
                    format!("🚀 CPFP fee: {} BTC for {}.{}", fee_btc, wallet_name, balance_text)
                } else {
                    let amount_btc = Self::format_btc_amount(event.amount_sats, &Language::English);
                    format!("📤 Sending {} BTC from {}.{}", amount_btc, wallet_name, balance_text)
                }
            }
            EventType::Receive => {
                if event.is_confirmed {
                    let amount_btc = Self::format_btc_amount(event.amount_sats, &Language::English);
                    format!("✅ Receive confirmed: {} BTC to {}.{}", amount_btc, wallet_name, balance_text)
                } else {
                    let amount_btc = Self::format_btc_amount(event.amount_sats, &Language::English);
                    format!("📥 Receiving {} BTC to {}.{}", amount_btc, wallet_name, balance_text)
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

        let (from_number, is_test_mode) = if twilio_config.messaging_service_sid == "TEST" {
            ("+15005550006".to_string(), true)
        } else {
            (twilio_config.messaging_service_sid.clone(), false)
        };

        println!(
            "📱 SMS Mode: {} - Sending to {} using from: {}",
            if is_test_mode { "TEST" } else { "LIVE" },
            contact.phone_number,
            from_number
        );

        let sms_request = TwilioSmsRequest {
            to: contact.phone_number.clone(),
            from: from_number,
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
        let mut results = Vec::new();

        for contact in contacts {
            let message = Self::create_localized_message(event, wallet_name, &contact.language);
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
