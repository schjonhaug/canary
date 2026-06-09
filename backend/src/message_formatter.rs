use crate::metadata::{EventType, Language, TransactionNotification};
use icu::decimal::input::Decimal;
use icu::decimal::DecimalFormatter;
use icu::locale::{locale, Locale};
use rust_i18n::t;
use writeable::Writeable;

pub struct MessageFormatter;

impl MessageFormatter {
    /// Convert Language enum to ICU4X Locale
    fn language_to_locale(language: &Language) -> Locale {
        match language {
            Language::English => locale!("en-US"),
            Language::Norwegian => locale!("nb"),
            Language::Spanish => locale!("es-419"),
            Language::Portuguese => locale!("pt-BR"),
            Language::German => locale!("de-DE"),
            Language::French => locale!("fr-FR"),
            Language::Japanese => locale!("ja"),
            Language::Danish => locale!("da"),
            Language::Swedish => locale!("sv"),
        }
    }

    /// Format Bitcoin amount based on language preference using ICU4X
    /// Note: This stays in Rust as it handles locale-specific number formatting
    pub fn format_btc_amount(amount_sats: i64, language: &Language) -> String {
        let locale = Self::language_to_locale(language);
        let formatter = DecimalFormatter::try_new(locale.into(), Default::default())
            .expect("locale should be valid");

        // Convert satoshis to BTC using integer math to avoid floating-point precision issues
        // Use multiply_pow10(-8) to shift decimal point: 100000000 sats -> 1.00000000 BTC
        let mut decimal = Decimal::from(amount_sats);
        decimal.multiply_pow10(-8);
        decimal.trim_end(); // Remove trailing zeros (e.g., 0.10000000 -> 0.1)

        // Format and normalize spaces (ICU uses various Unicode spaces, convert to regular space)
        formatter
            .format(&decimal)
            .write_to_string()
            .into_owned()
            .replace(['\u{a0}', '\u{202f}'], " ") // non-breaking space, narrow no-break space
    }

    /// Format fiat amount based on language preference using ICU4X
    /// Note: This stays in Rust as it handles locale-specific number formatting
    pub fn format_fiat_amount(amount: f64, currency: &str, language: &Language) -> String {
        let locale = Self::language_to_locale(language);
        let formatter = DecimalFormatter::try_new(locale.into(), Default::default())
            .expect("locale should be valid");

        // Format with 2 decimal places for fiat
        let fiat_string = format!("{:.2}", amount);
        let decimal: Decimal = fiat_string.parse().expect("formatted string should parse");

        // Format and normalize spaces (ICU uses various Unicode spaces, convert to regular space)
        let formatted_amount = formatter
            .format(&decimal)
            .write_to_string()
            .into_owned()
            .replace(['\u{a0}', '\u{202f}'], " "); // non-breaking space, narrow no-break space

        // Add currency symbol/code
        format!("{} {}", formatted_amount, currency)
    }

    /// Generate localized message for transaction notification
    pub fn create_localized_message(
        notification: &TransactionNotification,
        wallet_name: &str,
        language: &Language,
        include_wallet_balance: bool,
        wallet_balance_sats: Option<i64>,
    ) -> String {
        // Handle different notification types
        match notification {
            TransactionNotification::Pending(tx) => Self::create_transaction_message(
                tx,
                wallet_name,
                language,
                false,
                include_wallet_balance,
                wallet_balance_sats,
            ),
            TransactionNotification::Confirmed(tx) => Self::create_transaction_message(
                tx,
                wallet_name,
                language,
                true,
                include_wallet_balance,
                wallet_balance_sats,
            ),
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

        // Note: t!() macro requires string literals, not variables, for compile-time translation lookup
        match notification {
            TransactionNotification::Pending(tx) => match tx.transaction_type {
                EventType::Receive => {
                    let title_text = t!("titles.receive.pending", locale = locale).to_string();
                    format!("💸 {} - {}", title_text, wallet_name)
                }
                EventType::Send => {
                    if tx.parent_txid.is_some() {
                        let title_text = t!("titles.send.cpfp", locale = locale).to_string();
                        format!("⚡ {} - {}", title_text, wallet_name)
                    } else {
                        let title_text = t!("titles.send.pending", locale = locale).to_string();
                        format!("📤 {} - {}", title_text, wallet_name)
                    }
                }
            },
            TransactionNotification::Confirmed(tx) => match tx.transaction_type {
                EventType::Receive => {
                    let title_text = t!("titles.receive.confirmed", locale = locale).to_string();
                    format!("✅ {} - {}", title_text, wallet_name)
                }
                EventType::Send => {
                    let title_text = t!("titles.send.confirmed", locale = locale).to_string();
                    format!("✅ {} - {}", title_text, wallet_name)
                }
            },
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
                    t!(
                        "balance_alert.drain",
                        locale = locale,
                        wallet_name = wallet_name
                    )
                    .to_string()
                } else {
                    t!(
                        "balance_alert.equals",
                        locale = locale,
                        wallet_name = wallet_name,
                        current_display = current_display
                    )
                    .to_string()
                }
            }
            crate::metadata::BalanceAlertType::Above => t!(
                "balance_alert.above",
                locale = locale,
                wallet_name = wallet_name,
                threshold_display = threshold_display,
                current_display = current_display
            )
            .to_string(),
            crate::metadata::BalanceAlertType::Below => t!(
                "balance_alert.below",
                locale = locale,
                wallet_name = wallet_name,
                threshold_display = threshold_display,
                current_display = current_display
            )
            .to_string(),
        }
    }

    /// Generate localized message for transaction notification
    fn create_transaction_message(
        transaction: &crate::metadata::Transaction,
        wallet_name: &str,
        language: &Language,
        is_confirmed: bool,
        include_wallet_balance: bool,
        wallet_balance_sats: Option<i64>,
    ) -> String {
        let locale = language.as_str();
        let amount_btc = Self::format_btc_amount(transaction.amount_sats, language);

        let message = match transaction.transaction_type {
            EventType::Send => {
                if is_confirmed {
                    if transaction.amount_sats > 0 {
                        t!(
                            "transaction.send.confirmed_with_amount",
                            locale = locale,
                            amount_btc = amount_btc,
                            wallet_name = wallet_name
                        )
                        .to_string()
                    } else {
                        t!(
                            "transaction.send.confirmed_no_amount",
                            locale = locale,
                            wallet_name = wallet_name
                        )
                        .to_string()
                    }
                } else if transaction.transaction_status == "replaced" {
                    // RBF replacement notification
                    let replaced_by = transaction.replaced_by_txid.as_deref().unwrap_or("unknown");
                    let short_txid = &replaced_by[..8.min(replaced_by.len())];
                    t!(
                        "transaction.send.replaced",
                        locale = locale,
                        amount_btc = amount_btc,
                        wallet_name = wallet_name,
                        short_txid = short_txid
                    )
                    .to_string()
                } else if transaction.parent_txid.is_some() {
                    t!(
                        "transaction.send.cpfp",
                        locale = locale,
                        amount_btc = amount_btc,
                        wallet_name = wallet_name
                    )
                    .to_string()
                } else {
                    t!(
                        "transaction.send.pending",
                        locale = locale,
                        amount_btc = amount_btc,
                        wallet_name = wallet_name
                    )
                    .to_string()
                }
            }
            EventType::Receive => {
                if transaction.transaction_status == "replaced" {
                    // RBF replacement notification for receive transaction
                    let replaced_by = transaction.replaced_by_txid.as_deref().unwrap_or("unknown");
                    let short_txid = &replaced_by[..8.min(replaced_by.len())];
                    t!(
                        "transaction.receive.replaced",
                        locale = locale,
                        amount_btc = amount_btc,
                        wallet_name = wallet_name,
                        short_txid = short_txid
                    )
                    .to_string()
                } else if is_confirmed {
                    t!(
                        "transaction.receive.confirmed",
                        locale = locale,
                        amount_btc = amount_btc,
                        wallet_name = wallet_name
                    )
                    .to_string()
                } else {
                    t!(
                        "transaction.receive.pending",
                        locale = locale,
                        amount_btc = amount_btc,
                        wallet_name = wallet_name
                    )
                    .to_string()
                }
            }
        };

        if include_wallet_balance {
            if let Some(balance_sats) = wallet_balance_sats {
                let balance_btc = Self::format_btc_amount(balance_sats, language);
                return format!(
                    "{}\n{}",
                    message,
                    t!(
                        "transaction.wallet_balance",
                        locale = locale,
                        balance_btc = balance_btc
                    )
                );
            }
        }

        message
    }
}
