use crate::message_formatter::MessageFormatter;
use crate::metadata::{
    BalanceAlertNotification, BalanceAlertType, EventType, Language, NotificationContentFields,
    Transaction, TransactionNotification,
};

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

fn privacy_test_notifications() -> Vec<TransactionNotification> {
    let mut sending = create_test_transaction(EventType::Send, 123_456_789, false);
    sending.txid = "feedface".repeat(8);
    let mut receiving = create_test_transaction(EventType::Receive, 123_456_789, false);
    receiving.txid = "feedface".repeat(8);
    let mut sent = sending.clone();
    sent.transaction_status = "confirmed".to_string();
    let mut received = receiving.clone();
    received.transaction_status = "confirmed".to_string();
    let mut rbf = sending.clone();
    rbf.transaction_status = "replaced".to_string();
    rbf.replaced_by_txid = Some("deadbeef".repeat(8));
    let mut cpfp = sending.clone();
    cpfp.parent_txid = Some("cafebabe".repeat(8));

    vec![
        TransactionNotification::Pending(sending),
        TransactionNotification::Confirmed(sent),
        TransactionNotification::Pending(receiving),
        TransactionNotification::Confirmed(received),
        TransactionNotification::Pending(rbf),
        TransactionNotification::Pending(cpfp),
        TransactionNotification::BalanceAlert(BalanceAlertNotification {
            id: "balance-notification-secret".to_string(),
            balance_alert_id: "balance-alert-secret".to_string(),
            wallet_checksum: "wallet-checksum-secret".to_string(),
            contact_id: Some("contact-secret".to_string()),
            threshold_sats: 500_000_000,
            current_balance_sats: 987_654_321,
            alert_type: BalanceAlertType::Above,
            notification_sent_at: 1_700_000_000,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            threshold_currency: Some("USD".to_string()),
            threshold_fiat_amount: Some(50_000.0),
            exchange_rate_snapshot: Some(60_000.0),
        }),
    ]
}

#[test]
fn explicit_content_fields_never_render_unchecked_data_for_any_event_or_locale() {
    let languages = [
        Language::English,
        Language::Norwegian,
        Language::Spanish,
        Language::Portuguese,
        Language::German,
        Language::French,
        Language::Japanese,
        Language::Danish,
        Language::Swedish,
    ];

    for language in languages {
        for notification in privacy_test_notifications() {
            let generic = MessageFormatter::create_filtered_content(
                &notification,
                "Cold Storage Secret",
                Some(987_654_321),
                NotificationContentFields::minimal(),
            );
            let generic_message =
                MessageFormatter::create_localized_filtered_message(&generic, &language);
            let generic_title =
                MessageFormatter::create_localized_filtered_title(&generic, &language);
            assert_eq!(generic_message, generic_title);
            assert!(generic.wallet_name.is_none());
            assert!(generic.event.is_none());
            assert!(generic.transaction_amount_sats.is_none());
            assert!(generic.transaction_balance_sats.is_none());
            assert!(generic.balance_alert_condition.is_none());
            assert!(generic.balance_alert_threshold.is_none());
            assert!(generic.balance_alert_balance_sats.is_none());

            let event_only = NotificationContentFields {
                event_type: true,
                ..NotificationContentFields::minimal()
            };
            let filtered = MessageFormatter::create_filtered_content(
                &notification,
                "Cold Storage Secret",
                Some(987_654_321),
                event_only,
            );
            let message = MessageFormatter::create_localized_filtered_message(&filtered, &language);
            let title = MessageFormatter::create_localized_filtered_title(&filtered, &language);
            for excluded in [
                "Cold Storage Secret",
                "987654321",
                "feedface",
                "deadbeef",
                "wallet-checksum-secret",
                "contact-secret",
                "balance-alert-secret",
            ] {
                assert!(
                    !message.contains(excluded),
                    "message leaked {excluded}: {message}"
                );
                assert!(
                    !title.contains(excluded),
                    "title leaked {excluded}: {title}"
                );
            }
        }
    }
}

#[test]
fn filtered_rbf_content_never_contains_replacement_identifiers() {
    let notification = privacy_test_notifications()
        .into_iter()
        .find(|notification| {
            matches!(notification, TransactionNotification::Pending(tx) if tx.transaction_status == "replaced")
        })
        .expect("RBF notification");
    let content = MessageFormatter::create_filtered_content(
        &notification,
        "Cold Storage",
        Some(987_654_321),
        NotificationContentFields::detailed(true),
    );
    let message = MessageFormatter::create_localized_filtered_message(&content, &Language::English);
    assert!(!message.contains("feedface"));
    assert!(!message.contains("deadbeef"));
}
