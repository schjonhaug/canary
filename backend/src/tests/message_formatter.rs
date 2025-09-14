use crate::message_formatter::MessageFormatter;
use crate::metadata::{EventType, Language, Transaction, TransactionNotification};

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
        first_seen_at: 1672574400, // 2023-01-01 12:00:00 UTC
        confirmed_at: if confirmed { Some(1672574400) } else { None },
        transaction_status: "pending".to_string(),
        replaced_by_txid: None,
        replaced_at: None,
        parent_txid: None,
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
    let event = create_test_transaction(EventType::Receive, 100_000_000, true);
    let notification = TransactionNotification::Confirmed(event);

    let message = MessageFormatter::create_localized_message(
        &notification,
        "Test Wallet",
        &Language::Norwegian,
    );
    assert_eq!(message, "✅ Mottatt: 1,00000000 BTC til Test Wallet");
}

#[test]
fn test_create_norwegian_message_receive_unconfirmed() {
    let event = create_test_transaction(EventType::Receive, 50_000_000, false);
    let notification = TransactionNotification::Pending(event);

    let message = MessageFormatter::create_localized_message(
        &notification,
        "Test Wallet",
        &Language::Norwegian,
    );
    assert_eq!(
        message,
        "💸 Mottar: 0,50000000 BTC til Test Wallet (ubekreftet)"
    );
}

#[test]
fn test_create_norwegian_message_send_confirmed() {
    let event = create_test_transaction(EventType::Send, 25_000_000, true);
    let notification = TransactionNotification::Confirmed(event);

    let message = MessageFormatter::create_localized_message(
        &notification,
        "Test Wallet",
        &Language::Norwegian,
    );
    assert_eq!(message, "✅ Sendt: 0,25000000 BTC fra Test Wallet");
}

#[test]
fn test_create_norwegian_message_send_unconfirmed() {
    let event = create_test_transaction(EventType::Send, 75_000_000, false);
    let notification = TransactionNotification::Pending(event);

    let message = MessageFormatter::create_localized_message(
        &notification,
        "Test Wallet",
        &Language::Norwegian,
    );
    assert_eq!(message, "📤 Sender: 0,75000000 BTC fra Test Wallet");
}

#[test]
fn test_create_english_message_receive_confirmed() {
    let event = create_test_transaction(EventType::Receive, 100_000_000, true);
    let notification = TransactionNotification::Confirmed(event);

    let message = MessageFormatter::create_localized_message(
        &notification,
        "Test Wallet",
        &Language::English,
    );
    assert_eq!(message, "✅ Received: 1.00000000 BTC to Test Wallet");
}

#[test]
fn test_create_english_message_receive_unconfirmed() {
    let event = create_test_transaction(EventType::Receive, 50_000_000, false);
    let notification = TransactionNotification::Pending(event);

    let message = MessageFormatter::create_localized_message(
        &notification,
        "Test Wallet",
        &Language::English,
    );
    assert_eq!(
        message,
        "💸 Receiving: 0.50000000 BTC to Test Wallet (unconfirmed)"
    );
}

#[test]
fn test_create_english_message_send_confirmed() {
    let event = create_test_transaction(EventType::Send, 25_000_000, true);
    let notification = TransactionNotification::Confirmed(event);

    let message = MessageFormatter::create_localized_message(
        &notification,
        "Test Wallet",
        &Language::English,
    );
    assert_eq!(message, "✅ Sent: 0.25000000 BTC from Test Wallet");
}

#[test]
fn test_create_english_message_send_unconfirmed() {
    let event = create_test_transaction(EventType::Send, 75_000_000, false);
    let notification = TransactionNotification::Pending(event);

    let message = MessageFormatter::create_localized_message(
        &notification,
        "Test Wallet",
        &Language::English,
    );
    assert_eq!(message, "📤 Sending: 0.75000000 BTC from Test Wallet");
}
