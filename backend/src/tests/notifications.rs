use crate::metadata::{
    BalanceAlertNotification, BalanceAlertType, Contact, EventType, Language, NotificationMethod,
    ProviderType, Transaction, TransactionNotification,
};

// Test language constant for all tests
const TEST_LANGUAGE: Language = Language::English;
use crate::email_provider::EmailProvider;
use crate::notifications::{
    contact_allows_notification, notification_methods_for_provider, NotificationProvider,
    NotificationResult, ProviderInfo,
};
use crate::ntfy_provider::NtfyProvider;
use crate::twilio_provider::TwilioProvider;
use async_trait::async_trait;
use once_cell::sync::Lazy;
use std::sync::Arc;
use std::sync::Mutex;

static ENV_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

fn create_test_transaction(
    transaction_type: EventType,
    amount_sats: i64,
    confirmed: bool,
) -> Transaction {
    Transaction {
        txid: "550e8400e29b41d4a716446655440001".to_string(),
        wallet_checksum: "test_wallet".to_string(),
        transaction_type,
        amount_sats,
        fee_sats: None,
        block_height: if confirmed { Some(100) } else { None },
        first_seen_at: 1672574400,
        confirmed_at: if confirmed { Some(1672574400) } else { None },
        transaction_status: "pending".to_string(),
        replaced_by_txid: None,
        replaced_at: None,
        parent_txid: None,
        notification_status: vec![],
    }
}

fn create_test_contact(name: &str) -> Contact {
    Contact {
        id: Some("550e8400-e29b-41d4-a716-446655440001".to_string()),
        wallet_checksum: "test_wallet".to_string(),
        name: name.to_string(),
        notification_methods: vec![],
        created_at: "2023-01-01 12:00:00".to_string(),
        is_active: true,
        notify_sending: true,
        notify_sent: true,
        notify_receiving: true,
        notify_received: true,
        notify_cpfp: true,
        notify_rbf: true,
        include_wallet_balance_in_tx_notifications: false,
    }
}

fn create_notification_method(provider_type: ProviderType, target: &str) -> NotificationMethod {
    NotificationMethod {
        id: Some("550e8400-e29b-41d4-a716-446655440001".to_string()),
        contact_id: "550e8400-e29b-41d4-a716-446655440001".to_string(),
        provider_type,
        notification_target: target.to_string(),
        display_target: None,
        created_at: "2023-01-01 12:00:00".to_string(),
        is_enabled: true,
    }
}

#[test]
fn test_contact_allows_notification_respects_transaction_type_checkboxes() {
    let mut contact = create_test_contact("Test User");

    let sending =
        TransactionNotification::Pending(create_test_transaction(EventType::Send, 100_000, false));
    assert!(contact_allows_notification(&contact, &sending));
    contact.notify_sending = false;
    assert!(!contact_allows_notification(&contact, &sending));

    let sent =
        TransactionNotification::Confirmed(create_test_transaction(EventType::Send, 100_000, true));
    contact = create_test_contact("Test User");
    assert!(contact_allows_notification(&contact, &sent));
    contact.notify_sent = false;
    assert!(!contact_allows_notification(&contact, &sent));

    let receiving = TransactionNotification::Pending(create_test_transaction(
        EventType::Receive,
        100_000,
        false,
    ));
    contact = create_test_contact("Test User");
    assert!(contact_allows_notification(&contact, &receiving));
    contact.notify_receiving = false;
    assert!(!contact_allows_notification(&contact, &receiving));

    let received = TransactionNotification::Confirmed(create_test_transaction(
        EventType::Receive,
        100_000,
        true,
    ));
    contact = create_test_contact("Test User");
    assert!(contact_allows_notification(&contact, &received));
    contact.notify_received = false;
    assert!(!contact_allows_notification(&contact, &received));
}

#[test]
fn test_contact_allows_notification_respects_replacement_and_fee_bump_checkboxes() {
    let mut rbf_event = create_test_transaction(EventType::Send, 100_000, false);
    rbf_event.transaction_status = "replaced".to_string();
    rbf_event.replaced_by_txid = Some("replacement-txid".to_string());
    let rbf = TransactionNotification::Pending(rbf_event);

    let mut contact = create_test_contact("Test User");
    assert!(contact_allows_notification(&contact, &rbf));
    contact.notify_rbf = false;
    assert!(!contact_allows_notification(&contact, &rbf));

    let mut cpfp_event = create_test_transaction(EventType::Send, 100_000, false);
    cpfp_event.parent_txid = Some("parent-txid".to_string());
    let cpfp = TransactionNotification::Pending(cpfp_event);

    contact = create_test_contact("Test User");
    assert!(contact_allows_notification(&contact, &cpfp));
    contact.notify_cpfp = false;
    assert!(!contact_allows_notification(&contact, &cpfp));
}

#[test]
fn test_contact_allows_notification_rejects_contact_specific_balance_alert_without_contact_id() {
    let mut contact = create_test_contact("Test User");
    contact.id = None;
    let notification = TransactionNotification::BalanceAlert(BalanceAlertNotification {
        id: "notification-id".to_string(),
        balance_alert_id: "alert-id".to_string(),
        wallet_checksum: "test_wallet".to_string(),
        contact_id: Some("contact-id".to_string()),
        threshold_sats: 100_000_000,
        current_balance_sats: 150_000_000,
        alert_type: BalanceAlertType::Above,
        notification_sent_at: 1_672_574_400,
        created_at: "2023-01-01 12:00:00".to_string(),
        threshold_currency: None,
        threshold_fiat_amount: None,
        exchange_rate_snapshot: None,
    });

    assert!(!contact_allows_notification(&contact, &notification));
}

#[test]
fn test_create_contact_request_defaults_wallet_balance_notifications_off() {
    let request: crate::models::CreateContactWithMethodsRequest =
        serde_json::from_value(serde_json::json!({
            "name": "Test User",
            "notification_methods": []
        }))
        .unwrap();

    assert!(request.notify_sending);
    assert!(request.notify_sent);
    assert!(request.notify_receiving);
    assert!(request.notify_received);
    assert!(request.notify_cpfp);
    assert!(request.notify_rbf);
    assert!(!request.include_wallet_balance_in_tx_notifications);
}

#[tokio::test]
async fn test_ntfy_provider_info() {
    let provider = NtfyProvider::new("https://ntfy.sh".to_string());
    let info = provider.provider_info();

    assert_eq!(info.name, "ntfy");
    assert_eq!(info.display_name, "ntfy.sh Notifications");
    assert!(info.config_schema.is_object());
}

#[test]
fn test_notification_methods_for_provider_filters_and_preserves_contacts() {
    let mut alice = create_test_contact("Alice");
    alice.notification_methods = vec![
        create_notification_method(ProviderType::Email, "alice@example.com"),
        create_notification_method(ProviderType::Sms, "+4711111111"),
    ];

    let mut bob = create_test_contact("Bob");
    bob.notification_methods = vec![
        create_notification_method(ProviderType::Email, "bob@example.com"),
        create_notification_method(ProviderType::Ntfy, "bob-topic"),
    ];

    let contacts = vec![alice, bob];
    let email_targets: Vec<(String, String)> =
        notification_methods_for_provider(&contacts, &ProviderType::Email)
            .map(|(contact, method)| (contact.name.clone(), method.notification_target.clone()))
            .collect();

    assert_eq!(
        email_targets,
        vec![
            ("Alice".to_string(), "alice@example.com".to_string()),
            ("Bob".to_string(), "bob@example.com".to_string()),
        ]
    );
}

#[tokio::test]
async fn test_ntfy_provider_custom_server() {
    let provider = NtfyProvider::new("https://ntfy.example.com".to_string());
    let info = provider.provider_info();

    assert_eq!(info.name, "ntfy");
    assert_eq!(info.display_name, "ntfy.example.com Notifications");
    assert!(info.config_schema.is_object());
}

#[tokio::test]
async fn test_ntfy_send_notification() {
    let provider = NtfyProvider::new("https://ntfy.sh".to_string());
    let event = create_test_transaction(EventType::Receive, 100_000_000, true);
    let notification = TransactionNotification::Confirmed(event);

    let mut contact = create_test_contact("Test User");
    contact.notification_methods =
        vec![create_notification_method(ProviderType::Ntfy, "test-topic")];

    let results = provider
        .send_notification(
            &notification,
            "Test Wallet",
            &[contact],
            &TEST_LANGUAGE,
            None,
        )
        .await;

    assert_eq!(results.len(), 1);
    let (method, _result, message) = &results[0];
    assert_eq!(method.notification_target, "test-topic");
    assert_eq!(message, "✅ Received: 1 BTC to Test Wallet");
    // Note: Actual result.success depends on ntfy.sh availability
}

#[tokio::test]
async fn test_ntfy_filters_only_ntfy_methods() {
    let provider = NtfyProvider::new("https://ntfy.sh".to_string());
    let event = create_test_transaction(EventType::Send, 50_000_000, false);
    let notification = TransactionNotification::Pending(event);

    let mut first_contact = create_test_contact("Test User");
    first_contact.notification_methods = vec![
        create_notification_method(ProviderType::Ntfy, "my-topic"),
        create_notification_method(ProviderType::Sms, "+4712345678"),
    ];

    let mut second_contact = create_test_contact("Second User");
    second_contact.notification_methods = vec![
        create_notification_method(ProviderType::Email, "second@example.com"),
        create_notification_method(ProviderType::Ntfy, "second-topic"),
    ];

    let norwegian = Language::Norwegian;
    let results = provider
        .send_notification(
            &notification,
            "Test Wallet",
            &[first_contact, second_contact],
            &norwegian,
            None,
        )
        .await;

    // Should only process ntfy methods across both contacts
    assert_eq!(results.len(), 2);
    let (method, _, message) = &results[0];
    assert_eq!(method.provider_type, ProviderType::Ntfy);
    assert_eq!(method.notification_target, "my-topic");
    assert!(message.contains("0,5 BTC")); // Norwegian formatting
    assert_eq!(results[1].0.notification_target, "second-topic");
}

#[test]
fn test_twilio_provider_creation() {
    let _env_lock = ENV_LOCK.lock().unwrap();

    // Test with test credentials
    std::env::set_var("TWILIO_ACCOUNT_SID", "ACtest");
    std::env::set_var("TWILIO_AUTH_TOKEN", "test");
    std::env::set_var("TWILIO_SENDER_ID", "+15551234567");

    let provider = TwilioProvider::from_env();
    assert!(provider.is_some());

    let provider = provider.unwrap();
    let info = provider.provider_info();
    assert_eq!(info.name, "twilio");
    assert_eq!(info.display_name, "SMS");

    // Clean up
    std::env::remove_var("TWILIO_ACCOUNT_SID");
    std::env::remove_var("TWILIO_AUTH_TOKEN");
    std::env::remove_var("TWILIO_SENDER_ID");
}

#[test]
fn test_twilio_provider_missing_env() {
    let _env_lock = ENV_LOCK.lock().unwrap();

    // Ensure env vars are not set
    std::env::remove_var("TWILIO_ACCOUNT_SID");
    std::env::remove_var("TWILIO_AUTH_TOKEN");
    std::env::remove_var("TWILIO_SENDER_ID");

    let provider = TwilioProvider::from_env();
    assert!(provider.is_none());
}

#[tokio::test]
async fn test_twilio_send_notification() {
    let _env_lock = ENV_LOCK.lock().unwrap();

    // Test with test credentials
    std::env::set_var("TWILIO_ACCOUNT_SID", "ACtest");
    std::env::set_var("TWILIO_AUTH_TOKEN", "test");
    std::env::set_var("TWILIO_SENDER_ID", "+15551234567");

    let provider = TwilioProvider::from_env().unwrap();
    let event = create_test_transaction(EventType::Receive, 100_000_000, true);
    let notification = TransactionNotification::Confirmed(event);

    let mut contact = create_test_contact("Test User");
    contact.notification_methods = vec![create_notification_method(
        ProviderType::Sms,
        "+15551234567",
    )];

    let results = provider
        .send_notification(
            &notification,
            "Test Wallet",
            &[contact],
            &TEST_LANGUAGE,
            None,
        )
        .await;

    assert_eq!(results.len(), 1);
    let (method, _result, message) = &results[0];
    assert_eq!(method.notification_target, "+15551234567");
    assert_eq!(message, "✅ Received: 1 BTC to Test Wallet");

    // Clean up
    std::env::remove_var("TWILIO_ACCOUNT_SID");
    std::env::remove_var("TWILIO_AUTH_TOKEN");
    std::env::remove_var("TWILIO_SENDER_ID");
}

#[tokio::test]
async fn test_twilio_filters_only_sms_methods() {
    let _env_lock = ENV_LOCK.lock().unwrap();

    std::env::set_var("TWILIO_ACCOUNT_SID", "ACtest");
    std::env::set_var("TWILIO_AUTH_TOKEN", "test");
    std::env::set_var("TWILIO_SENDER_ID", "+15551234567");

    let provider = TwilioProvider::from_env().unwrap();
    let event = create_test_transaction(EventType::Send, 50_000_000, false);
    let notification = TransactionNotification::Pending(event);

    let mut first_contact = create_test_contact("Test User");
    first_contact.notification_methods = vec![
        create_notification_method(ProviderType::Ntfy, "my-topic"),
        create_notification_method(ProviderType::Sms, "+4712345678"),
    ];

    let mut second_contact = create_test_contact("Second User");
    second_contact.notification_methods = vec![
        create_notification_method(ProviderType::Sms, "+4798765432"),
        create_notification_method(ProviderType::Email, "second@example.com"),
    ];

    let norwegian = Language::Norwegian;
    let results = provider
        .send_notification(
            &notification,
            "Test Wallet",
            &[first_contact, second_contact],
            &norwegian,
            None,
        )
        .await;

    // Should only process SMS methods across both contacts
    assert_eq!(results.len(), 2);
    let (method, _, message) = &results[0];
    assert_eq!(method.provider_type, ProviderType::Sms);
    assert_eq!(method.notification_target, "+4712345678");
    assert!(message.contains("0,5 BTC")); // Norwegian formatting
    assert_eq!(results[1].0.notification_target, "+4798765432");

    // Clean up
    std::env::remove_var("TWILIO_ACCOUNT_SID");
    std::env::remove_var("TWILIO_AUTH_TOKEN");
    std::env::remove_var("TWILIO_SENDER_ID");
}

#[tokio::test]
async fn test_email_provider_unconfigured_filters_only_email_methods() {
    let _env_lock = ENV_LOCK.lock().unwrap();

    std::env::remove_var("RESEND_API_KEY");
    std::env::remove_var("RESEND_FROM_EMAIL");
    std::env::remove_var("RESEND_FROM_NAME");

    let provider = EmailProvider::new();
    let event = create_test_transaction(EventType::Receive, 100_000_000, true);
    let notification = TransactionNotification::Confirmed(event);

    let mut first_contact = create_test_contact("Email User");
    first_contact.notification_methods = vec![
        create_notification_method(ProviderType::Email, "email@example.com"),
        create_notification_method(ProviderType::Sms, "+4712345678"),
    ];

    let mut second_contact = create_test_contact("Second Email User");
    second_contact.notification_methods = vec![
        create_notification_method(ProviderType::Ntfy, "second-topic"),
        create_notification_method(ProviderType::Email, "second@example.com"),
    ];

    let results = provider
        .send_notification(
            &notification,
            "Test Wallet",
            &[first_contact, second_contact],
            &TEST_LANGUAGE,
            None,
        )
        .await;

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].0.provider_type, ProviderType::Email);
    assert_eq!(results[0].0.notification_target, "email@example.com");
    assert_eq!(
        results[0].1.error_message,
        Some("Resend not configured".to_string())
    );
    assert_eq!(results[1].0.notification_target, "second@example.com");
    assert_eq!(
        results[1].1.error_message,
        Some("Resend not configured".to_string())
    );
}

// Mock provider for testing the notification manager
struct MockProvider {
    name: String,
    should_succeed: bool,
}

#[async_trait]
impl NotificationProvider for MockProvider {
    async fn send_notification(
        &self,
        _notification: &TransactionNotification,
        _wallet_name: &str,
        contacts: &[Contact],
        _user_language: &Language,
        _wallet_balance_sats: Option<i64>,
    ) -> Vec<(NotificationMethod, NotificationResult, String)> {
        contacts
            .iter()
            .flat_map(|contact| &contact.notification_methods)
            .map(|method| {
                (
                    method.clone(),
                    NotificationResult {
                        success: self.should_succeed,
                        provider_id: Some("mock-123".to_string()),
                        error_message: if self.should_succeed {
                            None
                        } else {
                            Some("Mock error".to_string())
                        },
                    },
                    "Mock message".to_string(),
                )
            })
            .collect()
    }

    fn provider_info(&self) -> ProviderInfo {
        ProviderInfo {
            name: self.name.clone(),
            display_name: format!("{} Provider", self.name),
            config_schema: serde_json::json!({}),
        }
    }

    fn name(&self) -> &'static str {
        Box::leak(self.name.clone().into_boxed_str())
    }
}

#[tokio::test]
async fn test_notification_manager() {
    use crate::notifications::NotificationManager;

    let mut manager = NotificationManager::new();

    // Register mock providers
    let success_provider = Arc::new(MockProvider {
        name: "success".to_string(),
        should_succeed: true,
    });
    let failure_provider = Arc::new(MockProvider {
        name: "failure".to_string(),
        should_succeed: false,
    });

    manager.register_provider(success_provider);
    manager.register_provider(failure_provider);

    // List providers
    let providers = manager.list_providers();
    assert_eq!(providers.len(), 2);

    // Send notifications
    let event = create_test_transaction(EventType::Receive, 100_000_000, true);
    let notification = TransactionNotification::Confirmed(event);
    let mut contact = create_test_contact("Test User");
    contact.notification_methods =
        vec![create_notification_method(ProviderType::Ntfy, "test-topic")];

    let results = manager
        .send_notifications(
            "success",
            &notification,
            "Test Wallet",
            &[contact.clone()],
            &TEST_LANGUAGE,
            None,
        )
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].1.success);

    let results = manager
        .send_notifications(
            "failure",
            &notification,
            "Test Wallet",
            &[contact],
            &TEST_LANGUAGE,
            None,
        )
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert!(!results[0].1.success);
    assert_eq!(results[0].1.error_message, Some("Mock error".to_string()));
}

#[tokio::test]
async fn test_notification_manager_unknown_provider() {
    use crate::notifications::NotificationManager;

    let manager = NotificationManager::new();
    let event = create_test_transaction(EventType::Receive, 100_000_000, true);
    let notification = TransactionNotification::Confirmed(event);
    let contact = create_test_contact("Test User");

    let result = manager
        .send_notifications(
            "unknown",
            &notification,
            "Test Wallet",
            &[contact],
            &TEST_LANGUAGE,
            None,
        )
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}
