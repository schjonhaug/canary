use crate::metadata::{EventType, Language, TransactionNotification};
use num_format::{Locale, ToFormattedString};

pub struct MessageFormatter;

impl MessageFormatter {
    /// Create localized email subject for transaction notification
    pub fn create_localized_email_subject(
        notification: &TransactionNotification,
        wallet_name: &str,
        language: &Language,
    ) -> String {
        match notification {
            TransactionNotification::Pending(tx) | TransactionNotification::Confirmed(tx) => {
                let is_confirmed = matches!(notification, TransactionNotification::Confirmed(_));
                
                let (subject_prefix, emoji) = match (tx.transaction_type, is_confirmed) {
                    (EventType::Receive, true) => match language {
                        Language::Norwegian => ("Bitcoin Mottatt", "✅"),
                        Language::English => ("Bitcoin Received", "✅"),
                    },
                    (EventType::Receive, false) => match language {
                        Language::Norwegian => ("Bitcoin Mottar", "💸"),
                        Language::English => ("Bitcoin Receiving", "💸"),
                    },
                    (EventType::Send, true) => match language {
                        Language::Norwegian => ("Bitcoin Sendt", "✅"),
                        Language::English => ("Bitcoin Sent", "✅"),
                    },
                    (EventType::Send, false) => match language {
                        Language::Norwegian => ("Bitcoin Sender", "📤"),
                        Language::English => ("Bitcoin Sending", "📤"),
                    },
                };
                
                format!("{} {} - {}", emoji, subject_prefix, wallet_name)
            }
            TransactionNotification::BalanceAlert(_) => match language {
                Language::Norwegian => format!("📊 Saldo Varsel - {}", wallet_name),
                Language::English => format!("📊 Balance Alert - {}", wallet_name),
            },
        }
    }

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

    /// Format fiat amount based on language preference
    pub fn format_fiat_amount(amount: f64, currency: &str, language: &Language) -> String {
        // Format with 2 decimal places for fiat
        let integer_part = amount.floor() as i64;
        let decimal_part = ((amount - amount.floor()) * 100.0).round() as i64;

        // Format integer part with locale-specific thousands separators
        let formatted_integer = match language {
            Language::Norwegian => {
                // Norwegian uses space as thousands separator
                integer_part
                    .to_formatted_string(&Locale::nb)
                    .replace('\u{a0}', " ")
            }
            Language::English => {
                // English uses comma as thousands separator
                integer_part.to_formatted_string(&Locale::en)
            }
        };

        // Combine with decimal separator based on language
        let formatted_amount = match language {
            Language::Norwegian => format!("{},{:02}", formatted_integer, decimal_part),
            Language::English => format!("{}.{:02}", formatted_integer, decimal_part),
        };

        // Add currency symbol/code
        format!("{} {}", formatted_amount, currency)
    }

    /// Generate localized message for transaction notification
    pub fn create_localized_message(
        notification: &TransactionNotification,
        wallet_name: &str,
        language: &Language,
    ) -> String {
        // Handle different notification types
        match notification {
            TransactionNotification::Pending(tx) => {
                Self::create_transaction_message(tx, wallet_name, language, false)
            }
            TransactionNotification::Confirmed(tx) => {
                Self::create_transaction_message(tx, wallet_name, language, true)
            }
            TransactionNotification::BalanceAlert(alert) => {
                Self::create_balance_alert_message(alert, wallet_name, language)
            }
        }
    }

    /// Generate localized message for balance alert notification
    fn create_balance_alert_message(
        alert: &crate::metadata::BalanceAlertNotification,
        wallet_name: &str,
        language: &Language,
    ) -> String {
        // Check if this is a fiat threshold alert
        let threshold_display = if let (Some(ref currency), Some(fiat_amount), Some(rate)) = (&alert.threshold_currency, alert.threshold_fiat_amount, alert.exchange_rate_snapshot) {
            // Fiat threshold: show fiat amount with BTC equivalent
            let fiat_str = Self::format_fiat_amount(fiat_amount, currency, language);
            let btc_str = Self::format_btc_amount(alert.threshold_sats, language);
            format!("{} (≈ {} BTC at {:.0} {}/BTC)", fiat_str, btc_str, rate, currency)
        } else {
            // BTC threshold: show only BTC
            format!("{} BTC", Self::format_btc_amount(alert.threshold_sats, language))
        };

        let current_display = if let (Some(ref currency), Some(rate)) = (&alert.threshold_currency, alert.exchange_rate_snapshot) {
            // Calculate current balance in fiat
            let current_btc = alert.current_balance_sats as f64 / 100_000_000.0;
            let current_fiat = current_btc * rate;
            let fiat_str = Self::format_fiat_amount(current_fiat, currency, language);
            let btc_str = Self::format_btc_amount(alert.current_balance_sats, language);
            format!("{} (≈ {} BTC)", fiat_str, btc_str)
        } else {
            format!("{} BTC", Self::format_btc_amount(alert.current_balance_sats, language))
        };

        match alert.alert_type {
            crate::metadata::BalanceAlertType::Equals => {
                if alert.threshold_sats == 0 {
                    // Special wallet drain alert
                    match language {
                        Language::Norwegian => {
                            format!("🚨 Lommebok tømt: {} saldo er nå 0 BTC", wallet_name)
                        }
                        Language::English => format!(
                            "🚨 Wallet Drain Alert: {} balance is now 0 BTC",
                            wallet_name
                        ),
                    }
                } else {
                    match language {
                        Language::Norwegian => format!(
                            "📊 Saldo varsel: {} saldo er nå {}",
                            wallet_name, current_display
                        ),
                        Language::English => format!(
                            "📊 Balance Alert: {} balance is now {}",
                            wallet_name, current_display
                        ),
                    }
                }
            }
            crate::metadata::BalanceAlertType::Above => match language {
                Language::Norwegian => format!(
                    "📊 Saldo varsel: {} saldo er nå over {} (nåværende: {})",
                    wallet_name, threshold_display, current_display
                ),
                Language::English => format!(
                    "📊 Balance Alert: {} balance is now above {} (current: {})",
                    wallet_name, threshold_display, current_display
                ),
            },
            crate::metadata::BalanceAlertType::Below => match language {
                Language::Norwegian => format!(
                    "📊 Saldo varsel: {} saldo er nå under {} (nåværende: {})",
                    wallet_name, threshold_display, current_display
                ),
                Language::English => format!(
                    "📊 Balance Alert: {} balance is now below {} (current: {})",
                    wallet_name, threshold_display, current_display
                ),
            },
        }
    }

    /// Generate localized message for transaction notification
    fn create_transaction_message(
        transaction: &crate::metadata::Transaction,
        wallet_name: &str,
        language: &Language,
        is_confirmed: bool,
    ) -> String {
        // No balance display in notifications for privacy reasons
        match transaction.transaction_type {
            EventType::Send => {
                if is_confirmed {
                    if transaction.amount_sats > 0 {
                        let amount_btc = Self::format_btc_amount(transaction.amount_sats, language);
                        match language {
                            Language::Norwegian => {
                                format!("✅ Sendt: {} BTC fra {}", amount_btc, wallet_name)
                            }
                            Language::English => {
                                format!("✅ Sent: {} BTC from {}", amount_btc, wallet_name)
                            }
                        }
                    } else {
                        match language {
                            Language::Norwegian => format!("✅ Sendt fra {}", wallet_name),
                            Language::English => format!("✅ Sent from {}", wallet_name),
                        }
                    }
                } else if transaction.transaction_status == "replaced" {
                    // RBF replacement notification
                    let amount_btc = Self::format_btc_amount(transaction.amount_sats, language);
                    let replaced_by = transaction.replaced_by_txid.as_deref().unwrap_or("unknown");
                    match language {
                        Language::Norwegian => format!(
                            "🔄 Erstatttet: {} BTC fra {} (erstattet av {})",
                            amount_btc,
                            wallet_name,
                            &replaced_by[..8.min(replaced_by.len())]
                        ),
                        Language::English => format!(
                            "🔄 Replaced: {} BTC from {} (replaced by {})",
                            amount_btc,
                            wallet_name,
                            &replaced_by[..8.min(replaced_by.len())]
                        ),
                    }
                } else {
                    let amount_btc = Self::format_btc_amount(transaction.amount_sats, language);
                    match language {
                        Language::Norwegian => {
                            format!("📤 Sender: {} BTC fra {}", amount_btc, wallet_name)
                        }
                        Language::English => {
                            format!("📤 Sending: {} BTC from {}", amount_btc, wallet_name)
                        }
                    }
                }
            }
            EventType::Receive => {
                if transaction.transaction_status == "replaced" {
                    // RBF replacement notification for receive transaction
                    let amount_btc = Self::format_btc_amount(transaction.amount_sats, language);
                    let replaced_by = transaction.replaced_by_txid.as_deref().unwrap_or("unknown");
                    match language {
                        Language::Norwegian => format!(
                            "🔄 Erstatttet: {} BTC til {} (erstattet av {})",
                            amount_btc,
                            wallet_name,
                            &replaced_by[..8.min(replaced_by.len())]
                        ),
                        Language::English => format!(
                            "🔄 Replaced: {} BTC to {} (replaced by {})",
                            amount_btc,
                            wallet_name,
                            &replaced_by[..8.min(replaced_by.len())]
                        ),
                    }
                } else if is_confirmed {
                    let amount_btc = Self::format_btc_amount(transaction.amount_sats, language);
                    match language {
                        Language::Norwegian => {
                            format!("✅ Mottatt: {} BTC til {}", amount_btc, wallet_name)
                        }
                        Language::English => {
                            format!("✅ Received: {} BTC to {}", amount_btc, wallet_name)
                        }
                    }
                } else {
                    let amount_btc = Self::format_btc_amount(transaction.amount_sats, language);
                    match language {
                        Language::Norwegian => format!(
                            "💸 Mottar: {} BTC til {} (ubekreftet)",
                            amount_btc, wallet_name
                        ),
                        Language::English => format!(
                            "💸 Receiving: {} BTC to {} (unconfirmed)",
                            amount_btc, wallet_name
                        ),
                    }
                }
            }
        }
    }
}
