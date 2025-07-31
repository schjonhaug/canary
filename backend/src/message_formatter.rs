use crate::metadata::{TransactionEvent, EventType, Language};

pub struct MessageFormatter;

impl MessageFormatter {
    /// Format Bitcoin amount based on language preference
    pub fn format_btc_amount(amount_sats: i64, language: &Language) -> String {
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