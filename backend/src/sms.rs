use crate::metadata::{ContactPerson, EventType, Language, TransactionEvent, TwilioConfig};
use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use reqwest::Client;
use serde::Serialize;
use num_format::{Locale, ToFormattedString};

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

    /// Format Bitcoin amount based on language preference using num-format
    pub fn format_btc_amount(amount_sats: i64, language: &Language) -> String {
        let btc_amount = amount_sats as f64 / 100_000_000.0;

        // Get the appropriate locale for thousands separator
        // num-format uses different locale codes, let's try some common ones
        let locale = match language {
            Language::Norwegian => Locale::fr, // French uses space as thousands separator like Norwegian
            Language::English => Locale::en,
        };

        // For Bitcoin amounts, we always want to show 8 decimal places
        // but num-format doesn't directly support custom decimal places for floats
        // So we'll format the integer part and manually add the decimal part
        let integer_part = btc_amount.trunc() as i64;
        let decimal_part = ((btc_amount - integer_part as f64) * 100_000_000.0).round() as i64;

        if integer_part == 0 {
            // For amounts less than 1 BTC, show as 0.xxxxxxxx
            let decimal_str = format!("{:08}", decimal_part);
            match language {
                Language::Norwegian => format!("0,{}", decimal_str),
                Language::English => format!("0.{}", decimal_str),
            }
        } else {
            // For amounts >= 1 BTC, format the integer part with locale-specific thousands separator
            let formatted_integer = integer_part.to_formatted_string(&locale);
            let decimal_str = format!("{:08}", decimal_part);
            match language {
                Language::Norwegian => {
                    // Replace any non-breaking space or other space characters with regular space
                    let normalized = formatted_integer.replace('\u{202f}', " ").replace('\u{00a0}', " ");
                    format!("{},{}", normalized, decimal_str)
                },
                Language::English => format!("{}.{}", formatted_integer, decimal_str),
            }
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
        println!("   📄 Message Content: {}", message);

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
    ) -> Vec<(ContactPerson, SmsResponse, String)> {
        let mut results = Vec::new();

        println!("🔔 SMS Notifications for Transaction Event:");
        println!("   Event Type: {:?}", event.event_type);
        println!("   Amount: {} sats", event.amount_sats);
        println!("   Confirmed: {}", event.is_confirmed);
        println!("   RBF: {}, CPFP: {}", event.is_rbf, event.is_cpfp);
        println!("   Wallet: {}", wallet_name);
        println!("   Contacts: {}", contacts.len());

        for contact in contacts {
            let message = Self::create_localized_message(event, wallet_name, &contact.language);
            
            println!("📱 SMS to {} ({}): Language: {:?}", 
                contact.name, contact.phone_number, contact.language);
            println!("   Message: {}", message);
            
            let response = match self.send_sms(twilio_config, &contact, &message).await {
                Ok(response) => {
                    println!("   ✅ SMS sent successfully");
                    response
                },
                Err(e) => {
                    println!("   ❌ SMS failed: {}", e);
                    SmsResponse {
                        success: false,
                        twilio_sid: None,
                        error_message: Some(e.to_string()),
                    }
                },
            };

            results.push((contact, response, message));
        }

        results
    }
}
