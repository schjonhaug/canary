use crate::metadata::{EventType, Language, TransactionNotification};
use num_format::{Locale, ToFormattedString};
use rust_i18n::t;

pub struct MessageFormatter;

impl MessageFormatter {
    /// Format Bitcoin amount based on language preference
    /// Note: This stays in Rust as it handles locale-specific number formatting
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
            Language::Norwegian | Language::German | Language::French | Language::Spanish | Language::Portuguese => {
                // These languages use space as thousands separator
                // Replace non-breaking space with regular space
                integer_value
                    .to_formatted_string(&Locale::nb)
                    .replace('\u{a0}', " ")
            }
            Language::English | Language::Japanese => {
                // English and Japanese use comma as thousands separator
                integer_value.to_formatted_string(&Locale::en)
            }
        };

        // Combine with decimal separator based on language
        match language {
            Language::Norwegian | Language::German | Language::French | Language::Spanish | Language::Portuguese => {
                format!("{},{}", formatted_integer, decimal_part)
            }
            Language::English | Language::Japanese => {
                format!("{}.{}", formatted_integer, decimal_part)
            }
        }
    }

    /// Format fiat amount based on language preference
    /// Note: This stays in Rust as it handles locale-specific number formatting
    pub fn format_fiat_amount(amount: f64, currency: &str, language: &Language) -> String {
        // Format with 2 decimal places for fiat
        let integer_part = amount.floor() as i64;
        let decimal_part = ((amount - amount.floor()) * 100.0).round() as i64;

        // Format integer part with locale-specific thousands separators
        let formatted_integer = match language {
            Language::Norwegian | Language::German | Language::French | Language::Spanish | Language::Portuguese => {
                // These languages use space as thousands separator
                integer_part
                    .to_formatted_string(&Locale::nb)
                    .replace('\u{a0}', " ")
            }
            Language::English | Language::Japanese => {
                // English and Japanese use comma as thousands separator
                integer_part.to_formatted_string(&Locale::en)
            }
        };

        // Combine with decimal separator based on language
        let formatted_amount = match language {
            Language::Norwegian | Language::German | Language::French | Language::Spanish | Language::Portuguese => {
                format!("{},{:02}", formatted_integer, decimal_part)
            }
            Language::English | Language::Japanese => {
                format!("{}.{:02}", formatted_integer, decimal_part)
            }
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

    /// Generate localized email subject for transaction notification
    pub fn create_localized_email_subject(
        notification: &TransactionNotification,
        wallet_name: &str,
        language: &Language,
    ) -> String {
        let locale = language.as_str();

        match notification {
            TransactionNotification::Pending(tx) => {
                let (title_key, emoji) = match tx.transaction_type {
                    EventType::Receive => ("titles.receive.pending", "💸"),
                    EventType::Send => ("titles.send.pending", "📤"),
                };
                let title_text = t!(title_key, locale = locale).to_string();
                format!("{} {} - {}", emoji, title_text, wallet_name)
            }
            TransactionNotification::Confirmed(tx) => {
                let (title_key, emoji) = match tx.transaction_type {
                    EventType::Receive => ("titles.receive.confirmed", "✅"),
                    EventType::Send => ("titles.send.confirmed", "✅"),
                };
                let title_text = t!(title_key, locale = locale).to_string();
                format!("{} {} - {}", emoji, title_text, wallet_name)
            }
            TransactionNotification::BalanceAlert(_) => {
                let title_text = t!("titles.balance_alert", locale = locale).to_string();
                format!("📊 {} - {}", title_text, wallet_name)
            }
        }
    }

    /// Generate localized message for balance alert notification
    fn create_balance_alert_message(
        alert: &crate::metadata::BalanceAlertNotification,
        wallet_name: &str,
        language: &Language,
    ) -> String {
        let locale = language.as_str();

        // Check if this is a fiat threshold alert
        let threshold_display = if let (Some(ref currency), Some(fiat_amount), Some(rate)) = (
            &alert.threshold_currency,
            alert.threshold_fiat_amount,
            alert.exchange_rate_snapshot,
        ) {
            // Fiat threshold: show fiat amount with BTC equivalent
            let fiat_str = Self::format_fiat_amount(fiat_amount, currency, language);
            let btc_str = Self::format_btc_amount(alert.threshold_sats, language);
            format!(
                "{} (≈ {} BTC at {:.0} {}/BTC)",
                fiat_str, btc_str, rate, currency
            )
        } else {
            // BTC threshold: show only BTC
            format!(
                "{} BTC",
                Self::format_btc_amount(alert.threshold_sats, language)
            )
        };

        let current_display = if let (Some(ref currency), Some(rate)) =
            (&alert.threshold_currency, alert.exchange_rate_snapshot)
        {
            // Calculate current balance in fiat
            let current_btc = alert.current_balance_sats as f64 / 100_000_000.0;
            let current_fiat = current_btc * rate;
            let fiat_str = Self::format_fiat_amount(current_fiat, currency, language);
            let btc_str = Self::format_btc_amount(alert.current_balance_sats, language);
            format!("{} (≈ {} BTC)", fiat_str, btc_str)
        } else {
            format!(
                "{} BTC",
                Self::format_btc_amount(alert.current_balance_sats, language)
            )
        };

        match alert.alert_type {
            crate::metadata::BalanceAlertType::Equals => {
                if alert.threshold_sats == 0 {
                    // Special wallet drain alert
                    t!("balance_alert.drain", locale = locale, wallet_name = wallet_name).to_string()
                } else {
                    t!("balance_alert.equals", locale = locale, wallet_name = wallet_name, current_display = current_display).to_string()
                }
            }
            crate::metadata::BalanceAlertType::Above => {
                t!("balance_alert.above", locale = locale, wallet_name = wallet_name, threshold_display = threshold_display, current_display = current_display).to_string()
            }
            crate::metadata::BalanceAlertType::Below => {
                t!("balance_alert.below", locale = locale, wallet_name = wallet_name, threshold_display = threshold_display, current_display = current_display).to_string()
            }
        }
    }

    /// Generate localized message for transaction notification
    fn create_transaction_message(
        transaction: &crate::metadata::Transaction,
        wallet_name: &str,
        language: &Language,
        is_confirmed: bool,
    ) -> String {
        let locale = language.as_str();
        let amount_btc = Self::format_btc_amount(transaction.amount_sats, language);

        // No balance display in notifications for privacy reasons
        match transaction.transaction_type {
            EventType::Send => {
                if is_confirmed {
                    if transaction.amount_sats > 0 {
                        t!("transaction.send.confirmed_with_amount", locale = locale, amount_btc = amount_btc, wallet_name = wallet_name).to_string()
                    } else {
                        t!("transaction.send.confirmed_no_amount", locale = locale, wallet_name = wallet_name).to_string()
                    }
                } else if transaction.transaction_status == "replaced" {
                    // RBF replacement notification
                    let replaced_by = transaction.replaced_by_txid.as_deref().unwrap_or("unknown");
                    let short_txid = &replaced_by[..8.min(replaced_by.len())];
                    t!("transaction.send.replaced", locale = locale, amount_btc = amount_btc, wallet_name = wallet_name, short_txid = short_txid).to_string()
                } else {
                    t!("transaction.send.pending", locale = locale, amount_btc = amount_btc, wallet_name = wallet_name).to_string()
                }
            }
            EventType::Receive => {
                if transaction.transaction_status == "replaced" {
                    // RBF replacement notification for receive transaction
                    let replaced_by = transaction.replaced_by_txid.as_deref().unwrap_or("unknown");
                    let short_txid = &replaced_by[..8.min(replaced_by.len())];
                    t!("transaction.receive.replaced", locale = locale, amount_btc = amount_btc, wallet_name = wallet_name, short_txid = short_txid).to_string()
                } else if is_confirmed {
                    t!("transaction.receive.confirmed", locale = locale, amount_btc = amount_btc, wallet_name = wallet_name).to_string()
                } else {
                    t!("transaction.receive.pending", locale = locale, amount_btc = amount_btc, wallet_name = wallet_name).to_string()
                }
            }
        }
    }
}
