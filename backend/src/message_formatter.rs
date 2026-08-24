use crate::metadata::{
    BalanceAlertType, EventType, Language, NotificationContentFields, TransactionNotification,
};
use icu::decimal::input::Decimal;
use icu::decimal::DecimalFormatter;
use icu::locale::{locale, Locale};
use rust_i18n::t;
use writeable::Writeable;

pub struct MessageFormatter;

/// Provider-neutral notification data after applying one delivery method's
/// content policy. Providers must format only this representation.
#[derive(Debug, Clone, PartialEq)]
pub struct FilteredNotificationContent {
    pub confirmed: bool,
    pub event: Option<&'static str>,
    pub wallet_name: Option<String>,
    pub transaction_amount_sats: Option<i64>,
    pub transaction_balance_sats: Option<i64>,
    pub balance_alert_condition: Option<BalanceAlertType>,
    pub balance_alert_threshold: Option<FilteredBalanceAlertThreshold>,
    pub balance_alert_balance_sats: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FilteredBalanceAlertThreshold {
    pub threshold_sats: i64,
    pub threshold_currency: Option<String>,
    pub threshold_fiat_amount: Option<f64>,
}

impl FilteredNotificationContent {
    pub fn from_notification(
        notification: &TransactionNotification,
        wallet_name: &str,
        wallet_balance_sats: Option<i64>,
        fields: NotificationContentFields,
    ) -> Self {
        let confirmed = matches!(notification, TransactionNotification::Confirmed(_));
        let event = fields.event_type.then(|| event_name(notification));
        let selected_wallet_name = fields.wallet_name.then(|| wallet_name.to_string());

        match notification {
            TransactionNotification::Pending(transaction)
            | TransactionNotification::Confirmed(transaction) => Self {
                confirmed,
                event,
                wallet_name: selected_wallet_name,
                transaction_amount_sats: fields
                    .transaction_amount
                    .then_some(transaction.amount_sats),
                transaction_balance_sats: fields
                    .transaction_balance
                    .then_some(wallet_balance_sats)
                    .flatten(),
                balance_alert_condition: None,
                balance_alert_threshold: None,
                balance_alert_balance_sats: None,
            },
            TransactionNotification::BalanceAlert(alert) => Self {
                confirmed,
                event,
                wallet_name: selected_wallet_name,
                transaction_amount_sats: None,
                transaction_balance_sats: None,
                balance_alert_condition: fields.balance_alert_condition.then_some(alert.alert_type),
                balance_alert_threshold: fields.balance_alert_threshold.then(|| {
                    FilteredBalanceAlertThreshold {
                        threshold_sats: alert.threshold_sats,
                        threshold_currency: alert.threshold_currency.clone(),
                        threshold_fiat_amount: alert.threshold_fiat_amount,
                    }
                }),
                balance_alert_balance_sats: fields
                    .balance_alert_balance
                    .then_some(alert.current_balance_sats),
            },
        }
    }

    pub fn webhook_event(&self) -> &'static str {
        self.event.unwrap_or(if self.confirmed {
            "activity_confirmed"
        } else {
            "activity_detected"
        })
    }
}

fn event_name(notification: &TransactionNotification) -> &'static str {
    match notification {
        TransactionNotification::Pending(tx) if tx.transaction_status == "replaced" => "rbf",
        TransactionNotification::Pending(tx) if tx.parent_txid.is_some() => "cpfp",
        TransactionNotification::Pending(tx) => match tx.transaction_type {
            EventType::Receive => "receiving",
            EventType::Send => "sending",
        },
        TransactionNotification::Confirmed(tx) => match tx.transaction_type {
            EventType::Receive => "received",
            EventType::Send => "sent",
        },
        TransactionNotification::BalanceAlert(_) => "balance_alert",
    }
}

impl MessageFormatter {
    pub fn create_filtered_content(
        notification: &TransactionNotification,
        wallet_name: &str,
        wallet_balance_sats: Option<i64>,
        fields: NotificationContentFields,
    ) -> FilteredNotificationContent {
        FilteredNotificationContent::from_notification(
            notification,
            wallet_name,
            wallet_balance_sats,
            fields,
        )
    }

    pub fn create_localized_filtered_message(
        content: &FilteredNotificationContent,
        language: &Language,
    ) -> String {
        let locale = language.as_str();
        let mut lines = vec![if content.confirmed {
            t!("privacy.activity_confirmed", locale = locale).to_string()
        } else {
            t!("privacy.activity_detected", locale = locale).to_string()
        }];

        if let Some(wallet_name) = &content.wallet_name {
            lines.push(
                t!(
                    "content_fields.wallet",
                    locale = locale,
                    value = wallet_name
                )
                .to_string(),
            );
        }
        if let Some(event) = content.event {
            lines.push(
                t!(
                    "content_fields.event",
                    locale = locale,
                    value = Self::localized_event_name(event, language)
                )
                .to_string(),
            );
        }
        if let Some(amount_sats) = content.transaction_amount_sats {
            lines.push(
                t!(
                    "content_fields.amount",
                    locale = locale,
                    value = format!("{} BTC", Self::format_btc_amount(amount_sats, language))
                )
                .to_string(),
            );
        }
        if let Some(balance_sats) = content.transaction_balance_sats {
            lines.push(
                t!(
                    "content_fields.current_balance",
                    locale = locale,
                    value = format!("{} BTC", Self::format_btc_amount(balance_sats, language))
                )
                .to_string(),
            );
        }
        if let Some(condition) = content.balance_alert_condition {
            let value = match condition {
                BalanceAlertType::Above => {
                    t!("content_fields.conditions.above", locale = locale).to_string()
                }
                BalanceAlertType::Below => {
                    t!("content_fields.conditions.below", locale = locale).to_string()
                }
                BalanceAlertType::Equals => {
                    t!("content_fields.conditions.equals", locale = locale).to_string()
                }
            };
            lines.push(t!("content_fields.condition", locale = locale, value = value).to_string());
        }
        if let Some(threshold) = &content.balance_alert_threshold {
            let value = match (
                &threshold.threshold_currency,
                threshold.threshold_fiat_amount,
            ) {
                (Some(currency), Some(amount)) => {
                    Self::format_fiat_amount(amount, currency, language)
                }
                _ => format!(
                    "{} BTC",
                    Self::format_btc_amount(threshold.threshold_sats, language)
                ),
            };
            lines.push(t!("content_fields.threshold", locale = locale, value = value).to_string());
        }
        if let Some(balance_sats) = content.balance_alert_balance_sats {
            lines.push(
                t!(
                    "content_fields.current_balance",
                    locale = locale,
                    value = format!("{} BTC", Self::format_btc_amount(balance_sats, language))
                )
                .to_string(),
            );
        }

        lines.join("\n")
    }

    pub fn create_localized_filtered_title(
        content: &FilteredNotificationContent,
        language: &Language,
    ) -> String {
        let locale = language.as_str();
        let base = content.event.map_or_else(
            || {
                if content.confirmed {
                    t!("privacy.activity_confirmed", locale = locale).to_string()
                } else {
                    t!("privacy.activity_detected", locale = locale).to_string()
                }
            },
            |event| Self::localized_event_name(event, language),
        );
        match &content.wallet_name {
            Some(wallet_name) => format!("{} - {}", base, wallet_name),
            None => base,
        }
    }

    pub fn localized_event_name(event: &str, language: &Language) -> String {
        let locale = language.as_str();
        match event {
            "sending" => t!("titles.send.pending", locale = locale).to_string(),
            "sent" => t!("titles.send.confirmed", locale = locale).to_string(),
            "receiving" => t!("titles.receive.pending", locale = locale).to_string(),
            "received" => t!("titles.receive.confirmed", locale = locale).to_string(),
            "rbf" => t!("titles.rbf", locale = locale).to_string(),
            "cpfp" => t!("titles.send.cpfp", locale = locale).to_string(),
            _ => t!("titles.balance_alert", locale = locale).to_string(),
        }
    }

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
}
