use crate::metadata::{ContactPerson, TransactionEvent, EventType, Language};
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

    fn format_btc_amount(amount_sats: i64, language: &Language) -> String {
        let btc_amount = amount_sats as f64 / 100_000_000.0;
        
        match language {
            Language::Norwegian => {
                if btc_amount.fract() == 0.0 {
                    format!("{:.0}", btc_amount).replace('.', ",")
                } else {
                    format!("{:.8}", btc_amount).trim_end_matches('0').replace('.', ",").to_string()
                }
            }
            Language::English => {
                if btc_amount.fract() == 0.0 {
                    format!("{:.0}", btc_amount)
                } else {
                    format!("{:.8}", btc_amount).trim_end_matches('0').to_string()
                }
            }
        }
    }

    fn create_localized_message(
        event: &TransactionEvent,
        wallet_name: &str,
        language: &Language,
    ) -> String {
        let balance_text = if let Some(balance_sats) = event.balance_total {
            let balance_btc = Self::format_btc_amount(balance_sats, language);
            match language {
                Language::Norwegian => format!(" Total balanse: {} BTC", balance_btc),
                Language::English => format!(" Total balance: {} BTC", balance_btc),
            }
        } else {
            String::new()
        };

        match event.event_type {
            EventType::Send => {
                if event.is_confirmed {
                    if event.amount_sats > 0 {
                        let amount_btc = Self::format_btc_amount(event.amount_sats, language);
                        match language {
                            Language::Norwegian => format!("✅ Sending bekreftet: {} BTC fra {}.{}", amount_btc, wallet_name, balance_text),
                            Language::English => format!("✅ Send confirmed: {} BTC from {}.{}", amount_btc, wallet_name, balance_text),
                        }
                    } else {
                        match language {
                            Language::Norwegian => format!("✅ Sending bekreftet for {}.{}", wallet_name, balance_text),
                            Language::English => format!("✅ Send confirmed for {}.{}", wallet_name, balance_text),
                        }
                    }
                } else if event.is_rbf {
                    let fee_btc = Self::format_btc_amount(event.amount_sats, language);
                    match language {
                        Language::Norwegian => format!("📤 RBF gebyrøkning: +{} BTC for {}.{}", fee_btc, wallet_name, balance_text),
                        Language::English => format!("📤 RBF fee increase: +{} BTC for {}.{}", fee_btc, wallet_name, balance_text),
                    }
                } else if event.is_cpfp {
                    let fee_btc = Self::format_btc_amount(event.amount_sats, language);
                    match language {
                        Language::Norwegian => format!("🚀 CPFP-gebyr: {} BTC for {}.{}", fee_btc, wallet_name, balance_text),
                        Language::English => format!("🚀 CPFP fee: {} BTC for {}.{}", fee_btc, wallet_name, balance_text),
                    }
                } else {
                    let amount_btc = Self::format_btc_amount(event.amount_sats, language);
                    match language {
                        Language::Norwegian => format!("📤 Sender {} BTC fra {}.{}", amount_btc, wallet_name, balance_text),
                        Language::English => format!("📤 Sending {} BTC from {}.{}", amount_btc, wallet_name, balance_text),
                    }
                }
            }
            EventType::Receive => {
                if event.is_confirmed {
                    let amount_btc = Self::format_btc_amount(event.amount_sats, language);
                    match language {
                        Language::Norwegian => format!("✅ Mottak bekreftet: {} BTC til {}.{}", amount_btc, wallet_name, balance_text),
                        Language::English => format!("✅ Receive confirmed: {} BTC to {}.{}", amount_btc, wallet_name, balance_text),
                    }
                } else {
                    let amount_btc = Self::format_btc_amount(event.amount_sats, language);
                    match language {
                        Language::Norwegian => format!("📥 Mottar {} BTC til {}.{}", amount_btc, wallet_name, balance_text),
                        Language::English => format!("📥 Receiving {} BTC to {}.{}", amount_btc, wallet_name, balance_text),
                    }
                }
            }
        }
    }

}

#[async_trait]
impl NotificationProvider for NtfyProvider {
    async fn send_notification(
        &self,
        event: &TransactionEvent,
        wallet_name: &str,
        contacts: &[ContactPerson],
    ) -> Vec<(ContactPerson, NotificationResult, String)> {
        let mut results = Vec::new();
        
        for contact in contacts {
            let message = Self::create_localized_message(event, wallet_name, &contact.language);
            
            // For ntfy.sh, we expect the contact's contact_address field to contain the ntfy topic name
            let topic = &contact.contact_address;
            let ntfy_url = format!("https://ntfy.sh/{}", topic);
            
            println!("📱 Sending ntfy notification to topic '{}' for {}", topic, contact.name);
            println!("   Message: {}", message);
            
            let result = match self.client
                .post(&ntfy_url)
                .header("Content-Type", "text/plain; charset=utf-8")
                .header("Title", format!("Canary - {}", wallet_name))
                .header("Priority", if event.is_confirmed { "default" } else { "high" })
                .header("Tags", if event.event_type == EventType::Receive { "money_with_wings" } else { "arrow_right" })
                .body(message.clone())
                .send()
                .await
            {
                Ok(response) => {
                    if response.status().is_success() {
                        println!("✅ Successfully sent ntfy notification to {}", contact.name);
                        NotificationResult {
                            success: true,
                            provider_id: Some(format!("ntfy_{}", chrono::Utc::now().timestamp())),
                            error_message: None,
                        }
                    } else {
                        let error = format!("HTTP {}: {}", response.status(), response.status().canonical_reason().unwrap_or("Unknown"));
                        println!("❌ Failed to send ntfy notification to {}: {}", contact.name, error);
                        NotificationResult {
                            success: false,
                            provider_id: None,
                            error_message: Some(error),
                        }
                    }
                }
                Err(e) => {
                    let error = format!("Request failed: {}", e);
                    println!("❌ Failed to send ntfy notification to {}: {}", contact.name, error);
                    NotificationResult {
                        success: false,
                        provider_id: None,
                        error_message: Some(error),
                    }
                }
            };
            
            results.push((contact.clone(), result, message));
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