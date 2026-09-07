use crate::metadata::{
    Contact, Language, MetadataDb, NotificationContentFields, NotificationMethod, ProviderType,
};
use crate::nostr_provider::{canonicalize_nostr_public_key, NostrDmMode};
use rust_i18n::t;
use serde::{Deserialize, Serialize};

const BULLET: &str = "• ";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedTestIds {
    pub wallet_checksum: String,
    pub contact_id: String,
    pub method_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct SavedTestRequestIds {
    pub wallet_checksum: Option<String>,
    pub contact_id: Option<String>,
    pub method_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IncompleteSavedTestIds;

impl SavedTestRequestIds {
    pub fn parse(self) -> Result<Option<SavedTestIds>, IncompleteSavedTestIds> {
        let wallet_checksum = nonempty(self.wallet_checksum);
        let contact_id = nonempty(self.contact_id);
        let method_id = nonempty(self.method_id);
        match (wallet_checksum, contact_id, method_id) {
            (None, None, None) => Ok(None),
            (Some(wallet_checksum), Some(contact_id), Some(method_id)) => Ok(Some(SavedTestIds {
                wallet_checksum,
                contact_id,
                method_id,
            })),
            _ => Err(IncompleteSavedTestIds),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SavedTestConfigError {
    WalletNotFound,
    AccessDenied,
    ContactNotFound,
    MethodNotFound,
    MethodDisabled,
    ProviderMismatch,
    DestinationMismatch,
    Database(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TestNotificationConfig {
    pub notify_sending: bool,
    pub notify_sent: bool,
    pub notify_receiving: bool,
    pub notify_received: bool,
    pub notify_rbf: bool,
    pub notify_cpfp: bool,
    pub balance_alert_count: usize,
    pub content_fields: NotificationContentFields,
}

impl TestNotificationConfig {
    pub fn from_contact(
        contact: &Contact,
        content_fields: NotificationContentFields,
        balance_alert_count: usize,
    ) -> Self {
        Self {
            notify_sending: contact.notify_sending,
            notify_sent: contact.notify_sent,
            notify_receiving: contact.notify_receiving,
            notify_received: contact.notify_received,
            notify_rbf: contact.notify_rbf,
            notify_cpfp: contact.notify_cpfp,
            balance_alert_count,
            content_fields,
        }
    }

    pub fn enabled_transaction_events(&self) -> Vec<&'static str> {
        let mut events = Vec::new();
        if self.notify_sending {
            events.push("sending");
        }
        if self.notify_receiving {
            events.push("receiving");
        }
        if self.notify_sent {
            events.push("sent");
        }
        if self.notify_received {
            events.push("received");
        }
        if self.notify_rbf {
            events.push("rbf");
        }
        if self.notify_cpfp {
            events.push("cpfp");
        }
        events
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestNotificationCopy {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebhookTestConfig {
    pub transaction_events: Vec<String>,
    pub balance_alert_count: usize,
    pub content_fields: NotificationContentFields,
}

impl From<&TestNotificationConfig> for WebhookTestConfig {
    fn from(config: &TestNotificationConfig) -> Self {
        Self {
            transaction_events: config
                .enabled_transaction_events()
                .into_iter()
                .map(str::to_string)
                .collect(),
            balance_alert_count: config.balance_alert_count,
            content_fields: config.content_fields,
        }
    }
}

pub fn format_generic_test_notification(language: &Language) -> TestNotificationCopy {
    let locale = language.as_str();
    TestNotificationCopy {
        title: t!("test_notification.title", locale = locale).to_string(),
        body: t!("test_notification.message", locale = locale).to_string(),
    }
}

pub fn format_saved_test_notification(
    config: &TestNotificationConfig,
    language: &Language,
) -> TestNotificationCopy {
    let locale = language.as_str();
    TestNotificationCopy {
        title: t!("test_notification.saved_title", locale = locale).to_string(),
        body: saved_test_body(config, locale),
    }
}

pub fn format_generic_webhook_test_notification(language: &Language) -> TestNotificationCopy {
    let locale = language.as_str();
    TestNotificationCopy {
        title: t!("webhook_test_notification.title", locale = locale).to_string(),
        body: t!("webhook_test_notification.message", locale = locale).to_string(),
    }
}

pub fn format_generic_nostr_test_message(language: &Language, dm_mode: NostrDmMode) -> String {
    let locale = language.as_str();
    format!(
        "{}\n\n{}",
        t!("test_notification.nostr_generic", locale = locale),
        nostr_dm_format_line(locale, dm_mode)
    )
}

pub fn format_saved_nostr_test_message(
    config: &TestNotificationConfig,
    language: &Language,
    dm_mode: NostrDmMode,
) -> String {
    let locale = language.as_str();
    let copy = format_saved_test_notification(config, language);
    format!(
        "{}\n\n{}\n\n{}",
        copy.title,
        copy.body,
        nostr_dm_format_line(locale, dm_mode)
    )
}

pub fn notification_destination_matches(
    provider: ProviderType,
    requested: &str,
    method: &NotificationMethod,
) -> bool {
    if provider == ProviderType::Nostr {
        return nostr_destinations_match(requested, &method.notification_target)
            || method
                .display_target
                .as_deref()
                .is_some_and(|display| nostr_destinations_match(requested, display));
    }

    let requested = requested.trim();
    requested == method.notification_target.trim()
        || method
            .display_target
            .as_deref()
            .is_some_and(|display| requested == display.trim())
}

pub async fn load_saved_test_config(
    metadata_db: &MetadataDb,
    user_id: &str,
    is_admin: bool,
    ids: &SavedTestIds,
    expected_provider: ProviderType,
    requested_destination: &str,
) -> Result<TestNotificationConfig, SavedTestConfigError> {
    if metadata_db
        .get_wallet_by_checksum(&ids.wallet_checksum)
        .await
        .map_err(|error| SavedTestConfigError::Database(error.to_string()))?
        .is_none()
    {
        return Err(SavedTestConfigError::WalletNotFound);
    }

    if !is_admin {
        let owns_wallet = metadata_db
            .is_wallet_owned_by_user(&ids.wallet_checksum, user_id)
            .await
            .map_err(|error| SavedTestConfigError::Database(error.to_string()))?;
        if !owns_wallet {
            return Err(SavedTestConfigError::AccessDenied);
        }
    }

    let contact = metadata_db
        .get_single_contact_with_methods(&ids.contact_id, &ids.wallet_checksum)
        .await
        .map_err(|error| SavedTestConfigError::Database(error.to_string()))?
        .ok_or(SavedTestConfigError::ContactNotFound)?;

    let method = contact
        .notification_methods
        .iter()
        .find(|method| method.id.as_deref() == Some(ids.method_id.as_str()))
        .cloned()
        .ok_or(SavedTestConfigError::MethodNotFound)?;

    if !method.is_enabled {
        return Err(SavedTestConfigError::MethodDisabled);
    }
    if method.provider_type != expected_provider {
        return Err(SavedTestConfigError::ProviderMismatch);
    }
    if !notification_destination_matches(expected_provider, requested_destination, &method) {
        return Err(SavedTestConfigError::DestinationMismatch);
    }

    let balance_alert_count = metadata_db
        .get_active_balance_alerts_for_wallet(&ids.wallet_checksum)
        .await
        .map_err(|error| SavedTestConfigError::Database(error.to_string()))?
        .iter()
        .filter(|alert| alert.contact_id.as_deref() == Some(ids.contact_id.as_str()))
        .count();

    Ok(TestNotificationConfig::from_contact(
        &contact,
        method.content_fields,
        balance_alert_count,
    ))
}

fn saved_test_body(config: &TestNotificationConfig, locale: &str) -> String {
    let mut sections = vec![t!("test_notification.delivery_ok", locale = locale).to_string()];

    let notified_about = notified_about_lines(config, locale);
    if !notified_about.is_empty() {
        let mut section = vec![t!("test_notification.notified_about", locale = locale).to_string()];
        section.extend(notified_about);
        sections.push(section.join("\n"));
    }

    let mut content = vec![t!("test_notification.message_content", locale = locale).to_string()];
    content.extend(content_lines(config.content_fields, locale));
    sections.push(content.join("\n"));

    sections.join("\n\n")
}

fn notified_about_lines(config: &TestNotificationConfig, locale: &str) -> Vec<String> {
    let mut lines = Vec::new();

    match (config.notify_sending, config.notify_receiving) {
        (true, true) => lines.push(event_line(t!(
            "test_notification.events.sending_and_receiving",
            locale = locale
        ))),
        (true, false) => lines.push(event_line(t!(
            "test_notification.events.sending",
            locale = locale
        ))),
        (false, true) => lines.push(event_line(t!(
            "test_notification.events.receiving",
            locale = locale
        ))),
        (false, false) => {}
    }

    match (config.notify_sent, config.notify_received) {
        (true, true) => lines.push(event_line(t!(
            "test_notification.events.first_confirmations",
            locale = locale
        ))),
        (true, false) => lines.push(event_line(t!(
            "test_notification.events.sent",
            locale = locale
        ))),
        (false, true) => lines.push(event_line(t!(
            "test_notification.events.received",
            locale = locale
        ))),
        (false, false) => {}
    }

    if config.notify_rbf {
        lines.push(event_line(t!(
            "test_notification.events.rbf",
            locale = locale
        )));
    }
    if config.notify_cpfp {
        lines.push(event_line(t!(
            "test_notification.events.cpfp",
            locale = locale
        )));
    }

    if config.balance_alert_count == 1 {
        lines.push(event_line(t!(
            "test_notification.balance_alert_one",
            locale = locale
        )));
    } else if config.balance_alert_count > 1 {
        lines.push(event_line(t!(
            "test_notification.balance_alerts",
            locale = locale,
            count = config.balance_alert_count
        )));
    }

    lines
}

fn content_lines(fields: NotificationContentFields, locale: &str) -> Vec<String> {
    let financial = has_financial_fields(fields);
    if !fields.wallet_name && !fields.event_type && !financial {
        return vec![event_line(t!(
            "test_notification.content.activity_only",
            locale = locale
        ))];
    }

    let mut lines = Vec::new();
    match (fields.wallet_name, fields.event_type) {
        (true, true) => lines.push(event_line(t!(
            "test_notification.content.wallet_name_and_event",
            locale = locale
        ))),
        (true, false) => lines.push(event_line(t!(
            "test_notification.content.wallet_name",
            locale = locale
        ))),
        (false, true) => lines.push(event_line(t!(
            "test_notification.content.event_type",
            locale = locale
        ))),
        (false, false) => {}
    }

    if financial {
        lines.push(event_line(t!(
            "test_notification.content.financial_included",
            locale = locale
        )));
    } else {
        lines.push(event_line(t!(
            "test_notification.content.financial_hidden",
            locale = locale
        )));
    }

    lines
}

fn has_financial_fields(fields: NotificationContentFields) -> bool {
    fields.transaction_amount
        || fields.transaction_balance
        || fields.balance_alert_condition
        || fields.balance_alert_threshold
        || fields.balance_alert_balance
}

fn event_line(value: impl ToString) -> String {
    format!("{BULLET}{}", value.to_string())
}

fn nostr_dm_format_line(locale: &str, dm_mode: NostrDmMode) -> String {
    let mode = match dm_mode {
        NostrDmMode::Auto => t!("test_notification.nostr_mode.auto", locale = locale),
        NostrDmMode::Nip17 => t!("test_notification.nostr_mode.nip17", locale = locale),
        NostrDmMode::Nip04 => t!("test_notification.nostr_mode.nip04", locale = locale),
    };
    t!(
        "test_notification.nostr_dm_format",
        locale = locale,
        mode = mode
    )
    .to_string()
}

fn nostr_destinations_match(left: &str, right: &str) -> bool {
    match (
        canonicalize_nostr_public_key(left),
        canonicalize_nostr_public_key(right),
    ) {
        (Ok((left_hex, _)), Ok((right_hex, _))) => left_hex == right_hex,
        _ => left.trim() == right.trim(),
    }
}

fn nonempty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_saved_test_ids_requires_all_or_none() {
        assert_eq!(SavedTestRequestIds::default().parse(), Ok(None));
        assert!(SavedTestRequestIds {
            wallet_checksum: Some("wallet".into()),
            contact_id: None,
            method_id: None,
        }
        .parse()
        .is_err());
        assert_eq!(
            SavedTestRequestIds {
                wallet_checksum: Some(" wallet ".into()),
                contact_id: Some("contact".into()),
                method_id: Some("method".into()),
            }
            .parse()
            .unwrap()
            .unwrap(),
            SavedTestIds {
                wallet_checksum: "wallet".into(),
                contact_id: "contact".into(),
                method_id: "method".into(),
            }
        );
    }

    #[test]
    fn saved_summary_lists_enabled_events_and_useful_content() {
        let copy = format_saved_test_notification(
            &TestNotificationConfig {
                notify_sending: true,
                notify_sent: true,
                notify_receiving: true,
                notify_received: true,
                notify_rbf: true,
                balance_alert_count: 2,
                content_fields: NotificationContentFields::standard(),
                ..TestNotificationConfig::default()
            },
            &Language::English,
        );

        assert_eq!(copy.title, "Canary test notification");
        assert!(copy.body.contains("Delivery is working."));
        assert!(copy.body.contains("You'll be notified about:"));
        assert!(copy.body.contains("• Sending and receiving transactions"));
        assert!(copy.body.contains("• First confirmations"));
        assert!(copy.body.contains("• RBF replacements"));
        assert!(!copy.body.contains("CPFP"));
        assert!(copy.body.contains("• 2 balance alerts"));
        assert!(copy.body.contains("• Wallet name and event type included"));
        assert!(copy.body.contains("• Financial values hidden"));
        assert!(!copy.body.contains("sending"));
        assert!(!copy.body.contains("wallet-checksum"));
    }

    #[test]
    fn saved_summary_lists_partial_events_and_omits_zero_alerts() {
        let copy = format_saved_test_notification(
            &TestNotificationConfig {
                notify_sent: true,
                notify_receiving: true,
                content_fields: NotificationContentFields::standard(),
                ..TestNotificationConfig::default()
            },
            &Language::English,
        );

        assert!(copy.body.contains("• Receiving transactions"));
        assert!(copy.body.contains("• Sent confirmations"));
        assert!(!copy.body.contains("Sending and receiving"));
        assert!(!copy.body.contains("First confirmations"));
        assert!(!copy.body.contains("RBF"));
        assert!(!copy.body.contains("balance alert"));
    }

    #[test]
    fn saved_summary_uses_activity_only_for_minimal_privacy() {
        let copy = format_saved_test_notification(
            &TestNotificationConfig {
                notify_sending: true,
                notify_cpfp: true,
                balance_alert_count: 1,
                content_fields: NotificationContentFields::minimal(),
                ..TestNotificationConfig::default()
            },
            &Language::English,
        );

        assert!(copy.body.contains("• Sending transactions"));
        assert!(copy.body.contains("• CPFP fee bumps"));
        assert!(copy.body.contains("• 1 balance alert"));
        assert!(copy
            .body
            .contains("• Only a generic activity status is included"));
        assert!(!copy.body.contains("Wallet name"));
        assert!(!copy.body.contains("Financial values"));
    }

    #[test]
    fn saved_summary_reports_detailed_and_custom_content() {
        let detailed = format_saved_test_notification(
            &TestNotificationConfig {
                content_fields: NotificationContentFields::detailed(true),
                ..TestNotificationConfig::default()
            },
            &Language::English,
        );
        assert!(!detailed.body.contains("You'll be notified about:"));
        assert!(detailed
            .body
            .contains("• Wallet name and event type included"));
        assert!(detailed.body.contains("• Financial values included"));

        let mut custom = NotificationContentFields::minimal();
        custom.event_type = true;
        custom.transaction_amount = true;
        let custom_copy = format_saved_test_notification(
            &TestNotificationConfig {
                content_fields: custom,
                ..TestNotificationConfig::default()
            },
            &Language::English,
        );
        assert!(custom_copy.body.contains("• Event type included"));
        assert!(!custom_copy.body.contains("Wallet name and event type"));
        assert!(custom_copy.body.contains("• Financial values included"));
        assert!(!custom_copy.body.contains("Financial values hidden"));
    }

    #[test]
    fn generic_copy_stays_connectivity_only() {
        let copy = format_generic_test_notification(&Language::English);
        assert_eq!(copy.title, "Test Notification");
        assert!(copy.body.contains("ntfy setup is working correctly"));
        assert!(!copy.body.contains("You'll be notified about:"));

        let norwegian = format_saved_test_notification(
            &TestNotificationConfig {
                notify_sending: true,
                content_fields: NotificationContentFields::standard(),
                ..TestNotificationConfig::default()
            },
            &Language::Norwegian,
        );
        assert_eq!(norwegian.title, "Canary-testvarsel");
        assert!(norwegian.body.contains("Levering fungerer."));
        assert!(!norwegian.body.contains("Delivery is working."));

        let nostr = format_generic_nostr_test_message(&Language::English, NostrDmMode::Nip17);
        assert!(nostr.contains("This is a test Nostr DM from Canary Wallet."));
        assert!(nostr.contains("DM format: Modern NIP-17."));
        assert!(!nostr.contains("You'll be notified about:"));
    }

    #[test]
    fn saved_nostr_copy_appends_localized_dm_format() {
        let message = format_saved_nostr_test_message(
            &TestNotificationConfig {
                notify_sending: true,
                notify_sent: true,
                notify_receiving: true,
                notify_received: true,
                content_fields: NotificationContentFields::standard(),
                ..TestNotificationConfig::default()
            },
            &Language::English,
            NostrDmMode::Nip04,
        );
        assert!(message.starts_with("Canary test notification\n\nDelivery is working."));
        assert!(message.contains("DM format: Legacy NIP-04."));
    }

    #[test]
    fn webhook_config_lists_individual_enabled_events() {
        let config = WebhookTestConfig::from(&TestNotificationConfig {
            notify_sending: true,
            notify_receiving: true,
            notify_rbf: true,
            balance_alert_count: 3,
            content_fields: NotificationContentFields::minimal(),
            ..TestNotificationConfig::default()
        });
        assert_eq!(
            config.transaction_events,
            vec!["sending", "receiving", "rbf"]
        );
        assert_eq!(config.balance_alert_count, 3);
        assert!(!config.content_fields.wallet_name);
    }
}
