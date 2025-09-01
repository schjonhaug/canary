use crate::metadata::{EventType, Language, TransactionEvent};
use num_format::{Locale, ToFormattedString};

pub struct MessageFormatter;

impl MessageFormatter {
    /// Format Bitcoin amount based on language preference
    pub fn format_btc_amount(amount_sats: i64, language: &Language) -> String {
        let btc_amount = amount_sats as f64 / 100_000_000.0;

        // Always format with 8 decimal places
        let formatted_with_decimals = format!("{:.8}", btc_amount);

        // Split into integer and decimal parts
        let parts: Vec<&str> = formatted_with_decimals.split('.').collect();
        let integer_part = parts[0];
        let decimal_part = parts.get(1).unwrap_or(&"00000000");

        // Parse integer part for locale formatting
        let integer_value: i64 = integer_part.parse().unwrap_or(0);

        // Format integer part with locale-specific thousands separators
        let formatted_integer = match language {
            Language::Norwegian => {
                // Norwegian uses space as thousands separator
                // Replace non-breaking space with regular space
                integer_value
                    .to_formatted_string(&Locale::nb)
                    .replace('\u{a0}', " ")
            }
            Language::English => {
                // English uses comma as thousands separator
                integer_value.to_formatted_string(&Locale::en)
            }
        };

        // Combine with decimal separator based on language
        match language {
            Language::Norwegian => format!("{},{}", formatted_integer, decimal_part),
            Language::English => format!("{}.{}", formatted_integer, decimal_part),
        }
    }

    /// Generate localized message for transaction event
    pub fn create_localized_message(
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
                // Check if wallet is completely drained (balance = 0)
                let is_drain = event.balance_total.map_or(false, |balance| balance == 0);
                
                if is_drain {
                    let amount_btc = Self::format_btc_amount(event.amount_sats, language);
                    if event.is_confirmed {
                        match language {
                            Language::Norwegian => format!(
                                "🚨 LOMMEBOK TØMT: {} BTC sendt fra {}. Lommeboken er nå tom!",
                                amount_btc, wallet_name
                            ),
                            Language::English => format!(
                                "🚨 WALLET DRAINED: {} BTC sent from {}. Wallet is now empty!",
                                amount_btc, wallet_name
                            ),
                        }
                    } else {
                        match language {
                            Language::Norwegian => format!(
                                "⚠️ LOMMEBOK TØMMES: Hele saldoen på {} BTC sender fra {}!",
                                amount_btc, wallet_name
                            ),
                            Language::English => format!(
                                "⚠️ WALLET DRAINING: Entire balance of {} BTC sending from {}!",
                                amount_btc, wallet_name
                            ),
                        }
                    }
                } else if event.is_confirmed {
                    if event.amount_sats > 0 {
                        let amount_btc = Self::format_btc_amount(event.amount_sats, language);
                        match language {
                            Language::Norwegian => format!(
                                "✅ Sendt: {} BTC fra {}.{}",
                                amount_btc, wallet_name, balance_text
                            ),
                            Language::English => format!(
                                "✅ Sent: {} BTC from {}.{}",
                                amount_btc, wallet_name, balance_text
                            ),
                        }
                    } else {
                        match language {
                            Language::Norwegian => {
                                format!("✅ Sendt fra {}.{}", wallet_name, balance_text)
                            }
                            Language::English => {
                                format!("✅ Sent from {}.{}", wallet_name, balance_text)
                            }
                        }
                    }
                } else if event.is_rbf {
                    let fee_btc = Self::format_btc_amount(event.amount_sats, language);
                    match language {
                        Language::Norwegian => format!(
                            "📤 RBF gebyrøkning: +{} BTC for {}.{}",
                            fee_btc, wallet_name, balance_text
                        ),
                        Language::English => format!(
                            "📤 RBF fee increase: +{} BTC for {}.{}",
                            fee_btc, wallet_name, balance_text
                        ),
                    }
                } else if event.is_cpfp {
                    let fee_btc = Self::format_btc_amount(event.amount_sats, language);
                    match language {
                        Language::Norwegian => format!(
                            "🚀 CPFP-gebyr: {} BTC for {}.{}",
                            fee_btc, wallet_name, balance_text
                        ),
                        Language::English => format!(
                            "🚀 CPFP fee: {} BTC for {}.{}",
                            fee_btc, wallet_name, balance_text
                        ),
                    }
                } else {
                    let amount_btc = Self::format_btc_amount(event.amount_sats, language);
                    match language {
                        Language::Norwegian => format!(
                            "📤 Sender: {} BTC fra {}.{}",
                            amount_btc, wallet_name, balance_text
                        ),
                        Language::English => format!(
                            "📤 Sending: {} BTC from {}.{}",
                            amount_btc, wallet_name, balance_text
                        ),
                    }
                }
            }
            EventType::Receive => {
                if event.is_confirmed {
                    let amount_btc = Self::format_btc_amount(event.amount_sats, language);
                    match language {
                        Language::Norwegian => format!(
                            "✅ Mottatt: {} BTC til {}.{}",
                            amount_btc, wallet_name, balance_text
                        ),
                        Language::English => format!(
                            "✅ Received: {} BTC to {}.{}",
                            amount_btc, wallet_name, balance_text
                        ),
                    }
                } else {
                    let amount_btc = Self::format_btc_amount(event.amount_sats, language);
                    match language {
                        Language::Norwegian => format!(
                            "💸 Mottar: {} BTC til {} (ubekreftet).{}",
                            amount_btc, wallet_name, balance_text
                        ),
                        Language::English => format!(
                            "💸 Receiving: {} BTC to {} (unconfirmed).{}",
                            amount_btc, wallet_name, balance_text
                        ),
                    }
                }
            }
        }
    }
}
