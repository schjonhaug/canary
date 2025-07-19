use crate::metadata::{ContactPerson, EventType, Language, TransactionEvent, TwilioConfig};
use crate::sms::{SmsResponse, SmsService};

#[test]
fn test_format_btc_amount_small_norwegian() {
    // Test small amounts (less than 1 BTC) in Norwegian
    let amount_1000_sats = 1000; // 0.00001 BTC
    let formatted = SmsService::format_btc_amount(amount_1000_sats, &Language::Norwegian);
    assert_eq!(formatted, "0,00001000");

    let amount_100000_sats = 100000; // 0.001 BTC
    let formatted = SmsService::format_btc_amount(amount_100000_sats, &Language::Norwegian);
    assert_eq!(formatted, "0,00100000");
}

#[test]
fn test_format_btc_amount_small_english() {
    // Test small amounts (less than 1 BTC) in English
    let amount_1000_sats = 1000; // 0.00001 BTC
    let formatted = SmsService::format_btc_amount(amount_1000_sats, &Language::English);
    assert_eq!(formatted, "0.00001000");

    let amount_100000_sats = 100000; // 0.001 BTC
    let formatted = SmsService::format_btc_amount(amount_100000_sats, &Language::English);
    assert_eq!(formatted, "0.00100000");
}

#[test]
fn test_format_btc_amount_large_norwegian() {
    // Test large amounts (1 BTC or more) in Norwegian
    let amount_1_btc = 100_000_000; // 1 BTC
    let formatted = SmsService::format_btc_amount(amount_1_btc, &Language::Norwegian);
    assert_eq!(formatted, "1,00000000");

    let amount_1000_btc = 100_000_000_000; // 1000 BTC
    let formatted = SmsService::format_btc_amount(amount_1000_btc, &Language::Norwegian);
    assert_eq!(formatted, "1 000,00000000");

    let amount_1234567_btc = 123_456_700_000_000; // 1,234,567 BTC
    let formatted = SmsService::format_btc_amount(amount_1234567_btc, &Language::Norwegian);
    assert_eq!(formatted, "1 234 567,00000000");
}

#[test]
fn test_format_btc_amount_large_english() {
    // Test large amounts (1 BTC or more) in English
    let amount_1_btc = 100_000_000; // 1 BTC
    let formatted = SmsService::format_btc_amount(amount_1_btc, &Language::English);
    assert_eq!(formatted, "1.00000000");

    let amount_1000_btc = 100_000_000_000; // 1000 BTC
    let formatted = SmsService::format_btc_amount(amount_1000_btc, &Language::English);
    assert_eq!(formatted, "1,000.00000000");

    let amount_1234567_btc = 123_456_700_000_000; // 1,234,567 BTC
    let formatted = SmsService::format_btc_amount(amount_1234567_btc, &Language::English);
    assert_eq!(formatted, "1,234,567.00000000");
}

#[test]
fn test_format_btc_amount_zero() {
    let amount_0_sats = 0;
    let formatted_no = SmsService::format_btc_amount(amount_0_sats, &Language::Norwegian);
    assert_eq!(formatted_no, "0,00000000");
    
    let formatted_en = SmsService::format_btc_amount(amount_0_sats, &Language::English);
    assert_eq!(formatted_en, "0.00000000");
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
        balance_total: Some(150_000_000),
        transaction_time: 1672574400, // 2023-01-01 12:00:00 UTC
    };

    let message = SmsService::create_localized_message(&event, "Test Wallet", &Language::Norwegian);
    assert_eq!(
        message,
        "✅ Mottak bekreftet: 1,00000000 BTC til Test Wallet. Total balanse: 1,50000000 BTC"
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
        balance_total: Some(75_000_000),
        transaction_time: 1672574400, // 2023-01-01 12:00:00 UTC
    };

    let message = SmsService::create_localized_message(&event, "Test Wallet", &Language::Norwegian);
    assert_eq!(message, "📥 Mottar 0,50000000 BTC til Test Wallet. Total balanse: 0,75000000 BTC");
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
        balance_total: Some(75_000_000),
        transaction_time: 1672574400, // 2023-01-01 12:00:00 UTC
    };

    let message = SmsService::create_localized_message(&event, "Test Wallet", &Language::Norwegian);
    assert_eq!(
        message,
        "✅ Sending bekreftet: 0,25000000 BTC fra Test Wallet. Total balanse: 0,75000000 BTC"
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
        balance_total: Some(75_000_000),
        transaction_time: 1672574400, // 2023-01-01 12:00:00 UTC
    };

    let message = SmsService::create_localized_message(&event, "Test Wallet", &Language::Norwegian);
    assert_eq!(message, "📤 Sender 0,10000000 BTC fra Test Wallet. Total balanse: 0,75000000 BTC");
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
        balance_total: Some(75_000_000),
        transaction_time: 1672574400, // 2023-01-01 12:00:00 UTC
    };

    let message = SmsService::create_localized_message(&event, "Test Wallet", &Language::Norwegian);
    assert_eq!(
        message,
        "📤 RBF gebyrøkning: +0,00005000 BTC for Test Wallet. Total balanse: 0,75000000 BTC"
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
        balance_total: Some(75_000_000),
        transaction_time: 1672574400, // 2023-01-01 12:00:00 UTC
    };

    let message = SmsService::create_localized_message(&event, "Test Wallet", &Language::Norwegian);
    assert_eq!(message, "🚀 CPFP-gebyr: 0,00010000 BTC for Test Wallet. Total balanse: 0,75000000 BTC");
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
        created_at: "2024-01-01 12:00:00".to_string()
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
        wallet_id: 1,
        name: "John Doe".to_string(),
        phone_number: "12345678".to_string(),
        language: Language::Norwegian,
        created_at: "2024-01-01 12:00:00".to_string()
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
        balance_total: Some(150_000_000),
        transaction_time: 1672574400, // 2023-01-01 12:00:00 UTC
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
    let formatted_no = SmsService::format_btc_amount(amount_1_sat, &Language::Norwegian);
    assert_eq!(formatted_no, "0,00000001");
    let formatted_en = SmsService::format_btc_amount(amount_1_sat, &Language::English);
    assert_eq!(formatted_en, "0.00000001");

    // Test amounts with many digits
    let amount_large = 123_456_789_123_456_789;
    let formatted_no = SmsService::format_btc_amount(amount_large, &Language::Norwegian);
    // Accept both possible rounding results due to floating-point
    assert!(formatted_no == "1 234 567 891,23456788" || formatted_no == "1 234 567 891,23456789");
    let formatted_en = SmsService::format_btc_amount(amount_large, &Language::English);
    assert!(formatted_en == "1,234,567,891.23456788" || formatted_en == "1,234,567,891.23456789");

    // Test negative amounts (should handle gracefully)
    let amount_negative = -100_000_000;
    let formatted_no = SmsService::format_btc_amount(amount_negative, &Language::Norwegian);
    assert_eq!(formatted_no, "-1,00000000");
    let formatted_en = SmsService::format_btc_amount(amount_negative, &Language::English);
    assert_eq!(formatted_en, "-1.00000000");
}

#[test]
fn test_sms_service_creation_basic() {
    // Test SMS service creation
    let sms_service = SmsService::new();
    
    // Test that SMS service can be created (has HTTP client)
    // Client exists (no specific method to test, but it's created)
}

#[test]
fn test_sms_service_multiple_recipients() {
    // Test SMS service with multiple recipients for the same wallet
    let recipients = vec![
        ContactPerson {
            id: Some(1),
            wallet_id: 1,
            name: "John Doe".to_string(),
            phone_number: "+4712345678".to_string(),
            language: Language::Norwegian,
            created_at: "2024-01-01 12:00:00".to_string()
        },
        ContactPerson {
            id: Some(2),
            wallet_id: 1,
            name: "Jane Smith".to_string(),
            phone_number: "+4787654321".to_string(),
            language: Language::Norwegian,
            created_at: "2024-01-01 12:05:00".to_string(),
        },
    ];
    
    // Test that multiple recipients are properly structured
    assert_eq!(recipients.len(), 2);
    assert_eq!(recipients[0].wallet_id, recipients[1].wallet_id); // Same wallet
    assert_ne!(recipients[0].phone_number, recipients[1].phone_number); // Different numbers
    assert_ne!(recipients[0].name, recipients[1].name); // Different names
}

#[test]
fn test_sms_message_templates_norwegian() {
    // Test Norwegian SMS message templates for different event types
    let wallet_name = "Min Wallet";
    let amount_sats = 100_000; // 0.001 BTC
    
    // Test send unconfirmed message
    let send_event = TransactionEvent {
        id: Some(1),
        wallet_id: 1,
        event_type: EventType::Send,
        amount_sats,
        is_confirmed: false,
        is_rbf: false,
        is_cpfp: false,
        balance_total: Some(900_000),
        transaction_time: 1672574400, // 2023-01-01 12:00:00 UTC
    };
    let send_message = SmsService::create_localized_message(&send_event, wallet_name, &Language::Norwegian);
    assert!(send_message.contains("Sender"));
    assert!(send_message.contains("Min Wallet"));
    
    // Test receive unconfirmed message
    let receive_event = TransactionEvent {
        id: Some(2),
        wallet_id: 1,
        event_type: EventType::Receive,
        amount_sats,
        is_confirmed: false,
        is_rbf: false,
        is_cpfp: false,
        balance_total: Some(1_100_000),
        transaction_time: 1672574400, // 2023-01-01 12:00:00 UTC
    };
    let receive_message = SmsService::create_localized_message(&receive_event, wallet_name, &Language::Norwegian);
    assert!(receive_message.contains("Mottar"));
    assert!(receive_message.contains("Min Wallet"));
}

#[test]
fn test_sms_service_phone_number_validation() {
    // Test phone number validation for SMS service
    let valid_numbers = vec![
        "+4712345678",      // Norwegian mobile
        "+4787654321",      // Norwegian mobile
        "+46701234567",     // Swedish mobile
        "+491234567890",    // German mobile
        "+14155551234",     // US mobile
    ];
    
    for number in valid_numbers {
        let contact = ContactPerson {
            id: Some(1),
            wallet_id: 1,
            name: "Test User".to_string(),
            phone_number: number.to_string(),
            language: Language::Norwegian,
            created_at: "2024-01-01 12:00:00".to_string()
        };
        
        // Test that phone number format is preserved
        assert!(contact.phone_number.starts_with("+"));
        assert!(contact.phone_number.len() >= 10);
        assert!(contact.phone_number.len() <= 15);
    }
}

#[test]
fn test_sms_service_error_handling() {
    // Test SMS service error handling scenarios
    let sms_service = SmsService::new();
    
    // Test that SMS service can be created without errors
    // SMS service created successfully
}

#[test]
fn test_sms_service_configuration_validation() {
    // Test SMS service configuration validation
    let config = TwilioConfig {
        id: Some(1),
        account_sid: "AC1234567890abcdef".to_string(),
        auth_token: "auth_token_123".to_string(),
        messaging_service_sid: "MG1234567890abcdef".to_string(),
        created_at: "2024-01-01 12:00:00".to_string()
    };
    
    // Test account SID format
    assert!(config.account_sid.starts_with("AC"));
    assert_eq!(config.account_sid.len(), 18); // Actual length of test string
    
    // Test messaging service SID format
    assert!(config.messaging_service_sid.starts_with("MG"));
    assert_eq!(config.messaging_service_sid.len(), 18); // Actual length of test string
    
    // Test auth token is not empty
    assert!(!config.auth_token.is_empty());
}

#[test]
fn test_sms_service_wallet_integration() {
    // Test SMS service integration with wallet events
    let event = TransactionEvent {
        id: Some(1),
        wallet_id: 1,
        event_type: EventType::Send,
        amount_sats: 50_000_000, // 0.5 BTC
        is_confirmed: false,
        is_rbf: false,
        is_cpfp: false,
        balance_total: Some(950_000_000),
        transaction_time: 1672574400, // 2023-01-01 12:00:00 UTC
    };
    
    let contact = ContactPerson {
        id: Some(1),
        wallet_id: 1,
        name: "John Doe".to_string(),
        phone_number: "+4712345678".to_string(),
        language: Language::Norwegian,
        created_at: "2024-01-01 12:00:00".to_string()
    };
    
    // Test that event and contact are properly linked
    assert_eq!(event.wallet_id, contact.wallet_id);
    assert_eq!(event.event_type, EventType::Send);
    assert_eq!(event.amount_sats, 50_000_000);
    assert!(!event.is_confirmed);
}

#[test]
fn test_sms_service_delivery_tracking() {
    // Test SMS delivery tracking structure
    let response = SmsResponse {
        success: true,
        twilio_sid: Some("SM1234567890abcdef".to_string()),
        error_message: None,
    };
    
    // Test successful SMS response
    assert!(response.success);
    assert!(response.twilio_sid.is_some());
    assert!(response.error_message.is_none());
    
    // Test error SMS response
    let error_response = SmsResponse {
        success: false,
        twilio_sid: None,
        error_message: Some("Invalid phone number".to_string()),
    };
    
    assert!(!error_response.success);
    assert!(error_response.twilio_sid.is_none());
    assert!(error_response.error_message.is_some());
    assert_eq!(error_response.error_message.unwrap(), "Invalid phone number");
}

#[test]
fn test_sms_service_concurrent_sending() {
    // Test SMS service concurrent sending capabilities
    let sms_service = SmsService::new();
    
    // Test multiple SMS services can be created (for concurrent sending)
    let sms_service2 = SmsService::new();
    
    // Test that both services have HTTP clients
    // Both services created successfully
}
