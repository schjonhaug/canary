use crate::message_formatter::MessageFormatter;
use crate::metadata::{EventType, Language, TransactionEvent};

fn create_test_event(event_type: EventType, amount_sats: i64, is_confirmed: bool) -> TransactionEvent {
    TransactionEvent {
        id: Some(1),
        wallet_checksum: "test_wallet".to_string(),
        event_type,
        amount_sats,
        is_confirmed,
        is_rbf: false,
        is_cpfp: false,
        balance_total: Some(150_000_000),
        transaction_time: 1672574400, // 2023-01-01 12:00:00 UTC
        notification_status: vec![],
    }
}

#[test]
fn test_format_btc_amount_small_norwegian() {
    // Test small amounts (less than 1 BTC) in Norwegian
    let amount_1000_sats = 1000; // 0.00001 BTC
    let formatted = MessageFormatter::format_btc_amount(amount_1000_sats, &Language::Norwegian);
    assert_eq!(formatted, "0,00001000");

    let amount_100000_sats = 100000; // 0.001 BTC
    let formatted = MessageFormatter::format_btc_amount(amount_100000_sats, &Language::Norwegian);
    assert_eq!(formatted, "0,00100000");
}

#[test]
fn test_format_btc_amount_small_english() {
    
    // Test small amounts (less than 1 BTC) in English
    let amount_1000_sats = 1000; // 0.00001 BTC
    let formatted = MessageFormatter::format_btc_amount(amount_1000_sats, &Language::English);
    assert_eq!(formatted, "0.00001000");

    let amount_100000_sats = 100000; // 0.001 BTC
    let formatted = MessageFormatter::format_btc_amount(amount_100000_sats, &Language::English);
    assert_eq!(formatted, "0.00100000");
}

#[test]
fn test_format_btc_amount_large_norwegian() {
    
    // Test large amounts (1 BTC or more) in Norwegian
    let amount_1_btc = 100_000_000; // 1 BTC
    let formatted = MessageFormatter::format_btc_amount(amount_1_btc, &Language::Norwegian);
    assert_eq!(formatted, "1,00000000");

    let amount_1000_btc = 100_000_000_000; // 1000 BTC
    let formatted = MessageFormatter::format_btc_amount(amount_1000_btc, &Language::Norwegian);
    assert_eq!(formatted, "1 000,00000000");

    let amount_1234567_btc = 123_456_700_000_000; // 1,234,567 BTC
    let formatted = MessageFormatter::format_btc_amount(amount_1234567_btc, &Language::Norwegian);
    assert_eq!(formatted, "1 234 567,00000000");
}

#[test]
fn test_format_btc_amount_large_english() {
    
    // Test large amounts (1 BTC or more) in English
    let amount_1_btc = 100_000_000; // 1 BTC
    let formatted = MessageFormatter::format_btc_amount(amount_1_btc, &Language::English);
    assert_eq!(formatted, "1.00000000");

    let amount_1000_btc = 100_000_000_000; // 1000 BTC
    let formatted = MessageFormatter::format_btc_amount(amount_1000_btc, &Language::English);
    assert_eq!(formatted, "1,000.00000000");

    let amount_1234567_btc = 123_456_700_000_000; // 1,234,567 BTC
    let formatted = MessageFormatter::format_btc_amount(amount_1234567_btc, &Language::English);
    assert_eq!(formatted, "1,234,567.00000000");
}

#[test]
fn test_format_btc_amount_zero() {
    
    let amount_0_sats = 0;
    let formatted_no = MessageFormatter::format_btc_amount(amount_0_sats, &Language::Norwegian);
    assert_eq!(formatted_no, "0,00000000");
    
    let formatted_en = MessageFormatter::format_btc_amount(amount_0_sats, &Language::English);
    assert_eq!(formatted_en, "0.00000000");
}

#[test]
fn test_create_norwegian_message_receive_confirmed() {
    let event = create_test_event(EventType::Receive, 100_000_000, true);

    let message = MessageFormatter::create_localized_message(&event, "Test Wallet", &Language::Norwegian);
    assert_eq!(
        message,
        "✅ Mottak bekreftet: 1,00000000 BTC til Test Wallet. Total balanse: 1,50000000 BTC"
    );
}

#[test]
fn test_create_norwegian_message_receive_unconfirmed() {
    let event = create_test_event(EventType::Receive, 50_000_000, false);

    let message = MessageFormatter::create_localized_message(&event, "Test Wallet", &Language::Norwegian);
    assert_eq!(
        message,
        "💸 Nye bitcoins oppdaget: 0,50000000 BTC til Test Wallet (ubekreftet). Total balanse: 1,50000000 BTC"
    );
}

#[test]
fn test_create_norwegian_message_send_confirmed() {
    let event = create_test_event(EventType::Send, 25_000_000, true);

    let message = MessageFormatter::create_localized_message(&event, "Test Wallet", &Language::Norwegian);
    assert_eq!(
        message,
        "✅ Sending bekreftet: 0,25000000 BTC fra Test Wallet. Total balanse: 1,50000000 BTC"
    );
}

#[test]
fn test_create_norwegian_message_send_unconfirmed() {
    let event = create_test_event(EventType::Send, 75_000_000, false);

    let message = MessageFormatter::create_localized_message(&event, "Test Wallet", &Language::Norwegian);
    assert_eq!(
        message,
        "📤 Sending kringkastet: 0,75000000 BTC fra Test Wallet. Total balanse: 1,50000000 BTC"
    );
}

#[test]
fn test_create_english_message_receive_confirmed() {
    let event = create_test_event(EventType::Receive, 100_000_000, true);

    let message = MessageFormatter::create_localized_message(&event, "Test Wallet", &Language::English);
    assert_eq!(
        message,
        "✅ Receive confirmed: 1.00000000 BTC to Test Wallet. Total balance: 1.50000000 BTC"
    );
}

#[test]
fn test_create_english_message_receive_unconfirmed() {
    let event = create_test_event(EventType::Receive, 50_000_000, false);

    let message = MessageFormatter::create_localized_message(&event, "Test Wallet", &Language::English);
    assert_eq!(
        message,
        "💸 New bitcoins detected: 0.50000000 BTC to Test Wallet (unconfirmed). Total balance: 1.50000000 BTC"
    );
}

#[test]
fn test_create_english_message_send_confirmed() {
    let event = create_test_event(EventType::Send, 25_000_000, true);

    let message = MessageFormatter::create_localized_message(&event, "Test Wallet", &Language::English);
    assert_eq!(
        message,
        "✅ Send confirmed: 0.25000000 BTC from Test Wallet. Total balance: 1.50000000 BTC"
    );
}

#[test]
fn test_create_english_message_send_unconfirmed() {
    let event = create_test_event(EventType::Send, 75_000_000, false);

    let message = MessageFormatter::create_localized_message(&event, "Test Wallet", &Language::English);
    assert_eq!(
        message,
        "📤 Send broadcast: 0.75000000 BTC from Test Wallet. Total balance: 1.50000000 BTC"
    );
}