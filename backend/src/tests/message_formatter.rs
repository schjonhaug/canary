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
    assert_eq!(formatted, "0,00001");

    let amount_100000_sats = 100000; // 0.001 BTC
    let formatted = MessageFormatter::format_btc_amount(amount_100000_sats, &Language::Norwegian);
    assert_eq!(formatted, "0,001");
}

#[test]
fn test_format_btc_amount_small_english() {
    // Test small amounts (less than 1 BTC) in English
    let amount_1000_sats = 1000; // 0.00001 BTC
    let formatted = MessageFormatter::format_btc_amount(amount_1000_sats, &Language::English);
    assert_eq!(formatted, "0.00001");

    let amount_100000_sats = 100000; // 0.001 BTC
    let formatted = MessageFormatter::format_btc_amount(amount_100000_sats, &Language::English);
    assert_eq!(formatted, "0.001");
}

#[test]
fn test_format_btc_amount_large_norwegian() {
    // Test large amounts (1 BTC or more) in Norwegian
    let amount_1_btc = 100_000_000; // 1 BTC
    let formatted = MessageFormatter::format_btc_amount(amount_1_btc, &Language::Norwegian);
    assert_eq!(formatted, "1");

    let amount_1000_btc = 100_000_000_000; // 1000 BTC
    let formatted = MessageFormatter::format_btc_amount(amount_1000_btc, &Language::Norwegian);
    assert_eq!(formatted, "1 000");

    let amount_1234567_btc = 123_456_700_000_000; // 1,234,567 BTC
    let formatted = MessageFormatter::format_btc_amount(amount_1234567_btc, &Language::Norwegian);
    assert_eq!(formatted, "1 234 567");
}

#[test]
fn test_format_btc_amount_large_english() {
    // Test large amounts (1 BTC or more) in English
    let amount_1_btc = 100_000_000; // 1 BTC
    let formatted = MessageFormatter::format_btc_amount(amount_1_btc, &Language::English);
    assert_eq!(formatted, "1");

    let amount_1000_btc = 100_000_000_000; // 1000 BTC
    let formatted = MessageFormatter::format_btc_amount(amount_1000_btc, &Language::English);
    assert_eq!(formatted, "1,000");

    let amount_1234567_btc = 123_456_700_000_000; // 1,234,567 BTC
    let formatted = MessageFormatter::format_btc_amount(amount_1234567_btc, &Language::English);
    assert_eq!(formatted, "1,234,567");
}

#[test]
fn test_format_btc_amount_zero() {
    let amount_0_sats = 0;
    let formatted_no = MessageFormatter::format_btc_amount(amount_0_sats, &Language::Norwegian);
    assert_eq!(formatted_no, "0");

    let formatted_en = MessageFormatter::format_btc_amount(amount_0_sats, &Language::English);
    assert_eq!(formatted_en, "0");
}

#[test]
fn test_format_btc_amount_one_satoshi() {
    // Minimum Bitcoin unit - all 8 decimal places should be preserved
    let amount = 1; // 1 sat = 0.00000001 BTC
    let formatted = MessageFormatter::format_btc_amount(amount, &Language::English);
    assert_eq!(formatted, "0.00000001");

    let formatted_no = MessageFormatter::format_btc_amount(amount, &Language::Norwegian);
    assert_eq!(formatted_no, "0,00000001");
}

#[test]
fn test_format_btc_amount_all_decimals_significant() {
    // All 8 decimal places matter - none should be trimmed
    let amount = 12_345_678; // 0.12345678 BTC
    let formatted = MessageFormatter::format_btc_amount(amount, &Language::English);
    assert_eq!(formatted, "0.12345678");
}

#[test]
fn test_format_btc_amount_trailing_zeros_trimmed() {
    // Common round number - trailing zeros should be trimmed
    let amount = 10_000_000; // 0.1 BTC exactly (0.10000000 -> 0.1)
    let formatted = MessageFormatter::format_btc_amount(amount, &Language::English);
    assert_eq!(formatted, "0.1");

    let formatted_no = MessageFormatter::format_btc_amount(amount, &Language::Norwegian);
    assert_eq!(formatted_no, "0,1");
}

#[test]
fn test_format_btc_amount_mixed_trailing_zeros() {
    // Some trailing zeros, but not all - only trailing zeros removed
    let amount = 10_500_000; // 0.10500000 BTC -> 0.105
    let formatted = MessageFormatter::format_btc_amount(amount, &Language::English);
    assert_eq!(formatted, "0.105");

    // Test with more complex case
    let amount2 = 123_456_000; // 1.23456000 BTC -> 1.23456
    let formatted2 = MessageFormatter::format_btc_amount(amount2, &Language::English);
    assert_eq!(formatted2, "1.23456");
}

#[test]
fn test_format_btc_amount_max_supply() {
    // Maximum Bitcoin supply - 21 million BTC
    let amount = 2_100_000_000_000_000; // 21 million BTC
    let formatted = MessageFormatter::format_btc_amount(amount, &Language::English);
    assert_eq!(formatted, "21,000,000");

    let formatted_no = MessageFormatter::format_btc_amount(amount, &Language::Norwegian);
    assert_eq!(formatted_no, "21 000 000");
}

#[test]
fn test_create_norwegian_message_receive_confirmed() {
    let event = create_test_transaction(EventType::Receive, 100_000_000, true);
    let notification = TransactionNotification::Confirmed(event);

    let message = MessageFormatter::create_localized_message(
        &notification,
        "Test Wallet",
        &Language::Norwegian,
        false,
        None,
    );
    assert_eq!(message, "✅ Mottatt: 1 BTC til Test Wallet");
}

#[test]
fn test_create_norwegian_message_receive_unconfirmed() {
    let event = create_test_transaction(EventType::Receive, 50_000_000, false);
    let notification = TransactionNotification::Pending(event);

    let message = MessageFormatter::create_localized_message(
        &notification,
        "Test Wallet",
        &Language::Norwegian,
        false,
        None,
    );
    assert_eq!(message, "💸 Mottar: 0,5 BTC til Test Wallet (ubekreftet)");
}

#[test]
fn test_create_norwegian_message_send_confirmed() {
    let event = create_test_transaction(EventType::Send, 25_000_000, true);
    let notification = TransactionNotification::Confirmed(event);

    let message = MessageFormatter::create_localized_message(
        &notification,
        "Test Wallet",
        &Language::Norwegian,
        false,
        None,
    );
    assert_eq!(message, "✅ Sendt: 0,25 BTC fra Test Wallet");
}

#[test]
fn test_create_norwegian_message_send_unconfirmed() {
    let event = create_test_transaction(EventType::Send, 75_000_000, false);
    let notification = TransactionNotification::Pending(event);

    let message = MessageFormatter::create_localized_message(
        &notification,
        "Test Wallet",
        &Language::Norwegian,
        false,
        None,
    );
    assert_eq!(message, "📤 Sender: 0,75 BTC fra Test Wallet");
}

#[test]
fn test_create_english_message_receive_confirmed() {
    let event = create_test_transaction(EventType::Receive, 100_000_000, true);
    let notification = TransactionNotification::Confirmed(event);

    let message = MessageFormatter::create_localized_message(
        &notification,
        "Test Wallet",
        &Language::English,
        false,
        None,
    );
    assert_eq!(message, "✅ Received: 1 BTC to Test Wallet");
}

#[test]
fn test_create_english_message_receive_unconfirmed() {
    let event = create_test_transaction(EventType::Receive, 50_000_000, false);
    let notification = TransactionNotification::Pending(event);

    let message = MessageFormatter::create_localized_message(
        &notification,
        "Test Wallet",
        &Language::English,
        false,
        None,
    );
    assert_eq!(
        message,
        "💸 Receiving: 0.5 BTC to Test Wallet (unconfirmed)"
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
        false,
        None,
    );
    assert_eq!(message, "✅ Sent: 0.25 BTC from Test Wallet");
}

#[test]
fn test_create_english_message_send_unconfirmed() {
    let event = create_test_transaction(EventType::Send, 75_000_000, false);
    let notification = TransactionNotification::Pending(event);

    let message = MessageFormatter::create_localized_message(
        &notification,
        "Test Wallet",
        &Language::English,
        false,
        None,
    );
    assert_eq!(message, "📤 Sending: 0.75 BTC from Test Wallet");
}

#[test]
fn test_create_english_message_send_cpfp() {
    let mut event = create_test_transaction(EventType::Send, 100_000, false);
    event.parent_txid = Some("parent-txid".to_string());
    let notification = TransactionNotification::Pending(event);

    let message = MessageFormatter::create_localized_message(
        &notification,
        "Test Wallet",
        &Language::English,
        false,
        None,
    );
    assert_eq!(
        message,
        "⚡ CPFP fee bump: 0.001 BTC from Test Wallet (child pays for parent)"
    );
}

#[test]
fn test_create_english_subject_send_cpfp() {
    let mut event = create_test_transaction(EventType::Send, 100_000, false);
    event.parent_txid = Some("parent-txid".to_string());
    let notification = TransactionNotification::Pending(event);

    let subject = MessageFormatter::create_localized_email_subject(
        &notification,
        "Test Wallet",
        &Language::English,
    );
    assert_eq!(subject, "⚡ CPFP Fee Bump - Test Wallet");
}
