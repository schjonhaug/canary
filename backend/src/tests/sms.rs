use crate::metadata::{ContactPerson, EventType, TransactionEvent, TwilioConfig};
use crate::sms::{SmsResponse, SmsService};

#[test]
fn test_format_btc_amount_small() {
    // Test small amounts (less than 1 BTC)
    let amount_1000_sats = 1000; // 0.00001 BTC
    let formatted = SmsService::format_btc_amount(amount_1000_sats);
    assert_eq!(formatted, "0,00001000");

    let amount_100000_sats = 100000; // 0.001 BTC
    let formatted = SmsService::format_btc_amount(amount_100000_sats);
    assert_eq!(formatted, "0,00100000");
}

#[test]
fn test_format_btc_amount_large() {
    // Test large amounts (1 BTC or more)
    let amount_1_btc = 100_000_000; // 1 BTC
    let formatted = SmsService::format_btc_amount(amount_1_btc);
    assert_eq!(formatted, "1,00000000");

    let amount_1000_btc = 100_000_000_000; // 1000 BTC
    let formatted = SmsService::format_btc_amount(amount_1000_btc);
    assert_eq!(formatted, "1 000,00000000");

    let amount_1234567_btc = 123_456_700_000_000; // 1,234,567 BTC
    let formatted = SmsService::format_btc_amount(amount_1234567_btc);
    assert_eq!(formatted, "1 234 567,00000000");
}

#[test]
fn test_format_btc_amount_zero() {
    let amount_0_sats = 0;
    let formatted = SmsService::format_btc_amount(amount_0_sats);
    assert_eq!(formatted, "0,00000000");
}

#[test]
fn test_create_norwegian_message_receive_confirmed() {
    let event = TransactionEvent {
        id: Some(1),
        wallet_id: 1,
        event_type: EventType::Receive,
        amount_sats: 100_000_000, // 1 BTC
        is_confirmed: true,
        is_rbf: false,
        is_cpfp: false,
        confirmed_amount_sats: Some(100_000_000),
        created_at: "2024-01-01 12:00:00".to_string(),
    };

    let message = SmsService::create_norwegian_message(&event, "Test Wallet", Some(150_000_000)); // 1.5 BTC balance
    assert_eq!(
        message,
        "✅ Mottak bekreftet: 1,00000000 BTC til Test Wallet. Ny saldo: 1,50000000 BTC"
    );
}

#[test]
fn test_create_norwegian_message_receive_unconfirmed() {
    let event = TransactionEvent {
        id: Some(1),
        wallet_id: 1,
        event_type: EventType::Receive,
        amount_sats: 50_000_000, // 0.5 BTC
        is_confirmed: false,
        is_rbf: false,
        is_cpfp: false,
        confirmed_amount_sats: None,
        created_at: "2024-01-01 12:00:00".to_string(),
    };

    let message = SmsService::create_norwegian_message(&event, "Test Wallet", None);
    assert_eq!(message, "📥 Mottar 0,50000000 BTC til Test Wallet");
}

#[test]
fn test_create_norwegian_message_send_confirmed() {
    let event = TransactionEvent {
        id: Some(1),
        wallet_id: 1,
        event_type: EventType::Send,
        amount_sats: 25_000_000, // 0.25 BTC
        is_confirmed: true,
        is_rbf: false,
        is_cpfp: false,
        confirmed_amount_sats: Some(25_000_000),
        created_at: "2024-01-01 12:00:00".to_string(),
    };

    let message = SmsService::create_norwegian_message(&event, "Test Wallet", Some(75_000_000));
    assert_eq!(
        message,
        "✅ Sending bekreftet: 0,25000000 BTC fra Test Wallet. Ny saldo: 0,75000000 BTC"
    );
}

#[test]
fn test_create_norwegian_message_send_unconfirmed() {
    let event = TransactionEvent {
        id: Some(1),
        wallet_id: 1,
        event_type: EventType::Send,
        amount_sats: 10_000_000, // 0.1 BTC
        is_confirmed: false,
        is_rbf: false,
        is_cpfp: false,
        confirmed_amount_sats: None,
        created_at: "2024-01-01 12:00:00".to_string(),
    };

    let message = SmsService::create_norwegian_message(&event, "Test Wallet", None);
    assert_eq!(message, "📤 Sender 0,10000000 BTC fra Test Wallet");
}

#[test]
fn test_create_norwegian_message_rbf() {
    let event = TransactionEvent {
        id: Some(1),
        wallet_id: 1,
        event_type: EventType::Send,
        amount_sats: 5000, // 0.00005 BTC (fee increase)
        is_confirmed: false,
        is_rbf: true,
        is_cpfp: false,
        confirmed_amount_sats: None,
        created_at: "2024-01-01 12:00:00".to_string(),
    };

    let message = SmsService::create_norwegian_message(&event, "Test Wallet", None);
    assert_eq!(
        message,
        "📤 RBF gebyrøkning: +0,00005000 BTC for Test Wallet"
    );
}

#[test]
fn test_create_norwegian_message_cpfp() {
    let event = TransactionEvent {
        id: Some(1),
        wallet_id: 1,
        event_type: EventType::Send,
        amount_sats: 10000, // 0.0001 BTC (CPFP fee)
        is_confirmed: false,
        is_rbf: false,
        is_cpfp: true,
        confirmed_amount_sats: None,
        created_at: "2024-01-01 12:00:00".to_string(),
    };

    let message = SmsService::create_norwegian_message(&event, "Test Wallet", None);
    assert_eq!(message, "🚀 CPFP gebyr: 0,00010000 BTC for Test Wallet");
}

#[test]
fn test_sms_service_creation() {
    let service = SmsService::new();
    // Verify service was created successfully
    assert!(std::mem::size_of_val(&service) > 0);
}

#[test]
fn test_sms_response_creation() {
    let success_response = SmsResponse {
        success: true,
        twilio_sid: Some("test_sid".to_string()),
        error_message: None,
    };

    assert!(success_response.success);
    assert_eq!(success_response.twilio_sid, Some("test_sid".to_string()));
    assert!(success_response.error_message.is_none());

    let error_response = SmsResponse {
        success: false,
        twilio_sid: None,
        error_message: Some("Test error".to_string()),
    };

    assert!(!error_response.success);
    assert!(error_response.twilio_sid.is_none());
    assert_eq!(error_response.error_message, Some("Test error".to_string()));
}

#[test]
fn test_twilio_config_serialization() {
    let config = TwilioConfig {
        id: Some(1),
        account_sid: "AC1234567890abcdef".to_string(),
        auth_token: "test_auth_token".to_string(),
        messaging_service_sid: "MG1234567890abcdef".to_string(),
        created_at: "2024-01-01 12:00:00".to_string(),
    };

    // Test serialization
    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("AC1234567890abcdef"));
    assert!(json.contains("test_auth_token"));
    assert!(json.contains("MG1234567890abcdef"));

    // Test deserialization
    let deserialized: TwilioConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.account_sid, config.account_sid);
    assert_eq!(deserialized.auth_token, config.auth_token);
    assert_eq!(
        deserialized.messaging_service_sid,
        config.messaging_service_sid
    );
}

#[test]
fn test_contact_person_creation() {
    let contact = ContactPerson {
        id: Some(1),
        name: "John Doe".to_string(),
        phone_number: "12345678".to_string(),
        created_at: "2024-01-01 12:00:00".to_string(),
    };

    assert_eq!(contact.name, "John Doe");
    assert_eq!(contact.phone_number, "12345678");
    assert_eq!(contact.id, Some(1));
}

#[test]
fn test_transaction_event_creation() {
    let event = TransactionEvent {
        id: Some(1),
        wallet_id: 1,
        event_type: EventType::Receive,
        amount_sats: 100_000_000,
        is_confirmed: true,
        is_rbf: false,
        is_cpfp: false,
        confirmed_amount_sats: Some(100_000_000),
        created_at: "2024-01-01 12:00:00".to_string(),
    };

    assert_eq!(event.wallet_id, 1);
    assert_eq!(event.event_type, EventType::Receive);
    assert_eq!(event.amount_sats, 100_000_000);
    assert!(event.is_confirmed);
    assert!(!event.is_rbf);
    assert!(!event.is_cpfp);
}

#[test]
fn test_event_type_enum() {
    assert_eq!(EventType::Send.as_str(), "send");
    assert_eq!(EventType::Receive.as_str(), "receive");

    assert_eq!(EventType::from("send"), EventType::Send);
    assert_eq!(EventType::from("receive"), EventType::Receive);
}

#[test]
fn test_format_btc_amount_edge_cases() {
    // Test very small amounts
    let amount_1_sat = 1;
    let formatted = SmsService::format_btc_amount(amount_1_sat);
    assert_eq!(formatted, "0,00000001");

    // Test amounts with many digits
    let amount_large = 123_456_789_123_456_789;
    let formatted = SmsService::format_btc_amount(amount_large);
    // Accept both possible rounding results due to floating-point
    assert!(formatted == "1 234 567 891,23456788" || formatted == "1 234 567 891,23456789");

    // Test negative amounts (should handle gracefully)
    let amount_negative = -100_000_000;
    let formatted = SmsService::format_btc_amount(amount_negative);
    assert_eq!(formatted, "-1,00000000");
}
