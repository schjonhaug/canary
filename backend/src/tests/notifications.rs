use crate::metadata::{
    Contact, EventType, Language, NotificationMethod, ProviderType, Transaction,
    TransactionNotification,
};
use crate::notifications::{NotificationProvider, NotificationResult, ProviderInfo};
use crate::ntfy_provider::NtfyProvider;
use crate::twilio_provider::TwilioProvider;
use async_trait::async_trait;
use std::sync::Arc;

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
        dropped_at: None,
        parent_txid: None,
        notification_status: vec![],
    }
}

fn create_test_contact(name: &str, language: Language) -> Contact {
    Contact {
        id: Some("550e8400-e29b-41d4-a716-446655440001".to_string()),
        wallet_checksum: "test_wallet".to_string(),
        name: name.to_string(),
        language,
        notification_methods: vec![],
        created_at: "2023-01-01 12:00:00".to_string(),
        is_active: true,
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
    }
}

#[tokio::test]
async fn test_ntfy_provider_info() {
    let provider = NtfyProvider::new("https://ntfy.sh".to_string());
    let info = provider.provider_info();

    assert_eq!(info.name, "ntfy");
    assert_eq!(info.display_name, "ntfy.sh Notifications");
    assert!(info.config_schema.is_object());
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

    let mut contact = create_test_contact("Test User", Language::English);
    contact.notification_methods =
        vec![create_notification_method(ProviderType::Ntfy, "test-topic")];

    let results = provider
        .send_notification(&notification, "Test Wallet", &[contact])
        .await;

    assert_eq!(results.len(), 1);
    let (method, _result, message) = &results[0];
    assert_eq!(method.notification_target, "test-topic");
    assert_eq!(message, "✅ Received: 1.00000000 BTC to Test Wallet");
    // Note: Actual result.success depends on ntfy.sh availability
}

#[tokio::test]
async fn test_ntfy_filters_only_ntfy_methods() {
    let provider = NtfyProvider::new("https://ntfy.sh".to_string());
    let event = create_test_transaction(EventType::Send, 50_000_000, false);
    let notification = TransactionNotification::Pending(event);

    let mut contact = create_test_contact("Test User", Language::Norwegian);
    contact.notification_methods = vec![
        create_notification_method(ProviderType::Ntfy, "my-topic"),
        create_notification_method(ProviderType::Sms, "+4712345678"),
    ];

    let results = provider
        .send_notification(&notification, "Test Wallet", &[contact])
        .await;

    // Should only process the ntfy method
    assert_eq!(results.len(), 1);
    let (method, _, message) = &results[0];
    assert_eq!(method.provider_type, ProviderType::Ntfy);
    assert_eq!(method.notification_target, "my-topic");
    assert!(message.contains("0,50000000 BTC")); // Norwegian formatting
}

#[test]
fn test_twilio_provider_creation() {
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
    // Ensure env vars are not set
    std::env::remove_var("TWILIO_ACCOUNT_SID");
    std::env::remove_var("TWILIO_AUTH_TOKEN");
    std::env::remove_var("TWILIO_SENDER_ID");

    let provider = TwilioProvider::from_env();
    assert!(provider.is_none());
}

#[tokio::test]
async fn test_twilio_send_notification() {
    // Test with test credentials
    std::env::set_var("TWILIO_ACCOUNT_SID", "ACtest");
    std::env::set_var("TWILIO_AUTH_TOKEN", "test");
    std::env::set_var("TWILIO_SENDER_ID", "+15551234567");

    let provider = TwilioProvider::from_env().unwrap();
    let event = create_test_transaction(EventType::Receive, 100_000_000, true);
    let notification = TransactionNotification::Confirmed(event);

    let mut contact = create_test_contact("Test User", Language::English);
    contact.notification_methods = vec![create_notification_method(
        ProviderType::Sms,
        "+15551234567",
    )];

    let results = provider
        .send_notification(&notification, "Test Wallet", &[contact])
        .await;

    assert_eq!(results.len(), 1);
    let (method, _result, message) = &results[0];
    assert_eq!(method.notification_target, "+15551234567");
    assert_eq!(message, "✅ Received: 1.00000000 BTC to Test Wallet");

    // Clean up
    std::env::remove_var("TWILIO_ACCOUNT_SID");
    std::env::remove_var("TWILIO_AUTH_TOKEN");
    std::env::remove_var("TWILIO_SENDER_ID");
}

#[tokio::test]
async fn test_twilio_filters_only_sms_methods() {
    std::env::set_var("TWILIO_ACCOUNT_SID", "ACtest");
    std::env::set_var("TWILIO_AUTH_TOKEN", "test");
    std::env::set_var("TWILIO_SENDER_ID", "+15551234567");

    let provider = TwilioProvider::from_env().unwrap();
    let event = create_test_transaction(EventType::Send, 50_000_000, false);
    let notification = TransactionNotification::Pending(event);

    let mut contact = create_test_contact("Test User", Language::Norwegian);
    contact.notification_methods = vec![
        create_notification_method(ProviderType::Ntfy, "my-topic"),
        create_notification_method(ProviderType::Sms, "+4712345678"),
    ];

    let results = provider
        .send_notification(&notification, "Test Wallet", &[contact])
        .await;

    // Should only process the SMS method
    assert_eq!(results.len(), 1);
    let (method, _, message) = &results[0];
    assert_eq!(method.provider_type, ProviderType::Sms);
    assert_eq!(method.notification_target, "+4712345678");
    assert!(message.contains("0,50000000 BTC")); // Norwegian formatting

    // Clean up
    std::env::remove_var("TWILIO_ACCOUNT_SID");
    std::env::remove_var("TWILIO_AUTH_TOKEN");
    std::env::remove_var("TWILIO_SENDER_ID");
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
    let mut contact = create_test_contact("Test User", Language::English);
    contact.notification_methods =
        vec![create_notification_method(ProviderType::Ntfy, "test-topic")];

    let results = manager
        .send_notifications("success", &notification, "Test Wallet", &[contact.clone()])
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].1.success);

    let results = manager
        .send_notifications("failure", &notification, "Test Wallet", &[contact])
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
    let contact = create_test_contact("Test User", Language::English);

    let result = manager
        .send_notifications("unknown", &notification, "Test Wallet", &[contact])
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}
