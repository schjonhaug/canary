use crate::config::{AppConfig, NetworkConfig, OperatingMode};
use crate::metadata::{
    BalanceAlertType, ContactNotificationSettings, CreateBalanceAlertInput, EventType, Language,
    MetadataDb, ProviderType, SubscriptionUpdateParams, TransactionInsert,
};
use tempfile::tempdir;

async fn create_test_db() -> (MetadataDb, tempfile::TempDir) {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create test config
    let test_config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        temp_dir.path().to_string_lossy().to_string(),
        OperatingMode::SelfHosted,
        None,
        None, // No JWT secret needed for self-hosted mode
    );

    let db = MetadataDb::new(db_path.to_str().unwrap(), &test_config)
        .await
        .unwrap();
    (db, temp_dir) // Return both to keep temp_dir alive
}

#[tokio::test]
async fn test_delete_wallet_contact_authorization() {
    let (db, _temp_dir) = create_test_db().await;

    // Create test user
    let user_id = db
        .create_user(
            "test@example.com",
            "hashedpassword",
            Some("Test User"),
            true,
            None, // preferred_currency
            None, // preferred_language
        )
        .await
        .unwrap();
    let other_user_id = db
        .create_user(
            "other@example.com",
            "hashedpassword",
            Some("Other User"),
            true,
            None, // preferred_currency
            None, // preferred_language
        )
        .await
        .unwrap();

    // Create two wallets for different users
    let wallet1_checksum = db
        .insert_wallet("Wallet 1", "descriptor1", &user_id)
        .await
        .unwrap();
    let wallet2_checksum = db
        .insert_wallet("Wallet 2", "descriptor2", &other_user_id)
        .await
        .unwrap();

    // Add contacts to each wallet using the notification methods API
    let contact1_id = db
        .insert_contact_with_notification_methods(&wallet1_checksum, "Contact 1", vec![])
        .await
        .unwrap();
    let contact2_id = db
        .insert_contact_with_notification_methods(&wallet2_checksum, "Contact 2", vec![])
        .await
        .unwrap();

    // Test 1: User can delete their own contact
    let result = db
        .delete_wallet_contact(&wallet1_checksum, &contact1_id)
        .await
        .unwrap();
    assert!(result, "Should be able to delete own contact");

    // Test 2: User cannot delete contact from another user's wallet (IDOR protection)
    let result = db
        .delete_wallet_contact(&wallet1_checksum, &contact2_id)
        .await
        .unwrap();
    assert!(
        !result,
        "Should NOT be able to delete contact from different wallet"
    );

    // Test 3: Contact should still exist in its original wallet
    let contacts = db
        .get_contacts_with_notification_methods(&wallet2_checksum)
        .await
        .unwrap();
    assert_eq!(
        contacts.len(),
        1,
        "Contact should still exist in original wallet"
    );
    assert_eq!(contacts[0].id.clone().unwrap(), contact2_id);
}

#[tokio::test]
async fn test_update_contact_preserves_matching_notification_method_ids() {
    let (db, _temp_dir) = create_test_db().await;
    let user_id = db
        .create_user(
            "owner@example.com",
            "hashedpassword",
            Some("Owner"),
            true,
            None,
            None,
        )
        .await
        .unwrap();
    let wallet_checksum = db
        .insert_wallet("Wallet", "descriptor", &user_id)
        .await
        .unwrap();
    let contact_id = db
        .insert_contact_with_notification_settings(
            &wallet_checksum,
            "Alice",
            vec![
                (ProviderType::Email, "alice@example.com".to_string(), true),
                (ProviderType::Ntfy, "canary-alice".to_string(), true),
            ],
            ContactNotificationSettings::defaults_for_new_contact(),
        )
        .await
        .unwrap();

    let original_contact = db
        .get_single_contact_with_methods(&contact_id, &wallet_checksum)
        .await
        .unwrap()
        .unwrap();
    let original_email_method = original_contact
        .notification_methods
        .iter()
        .find(|method| method.notification_target == "alice@example.com")
        .unwrap();
    let original_email_method_id = original_email_method.id.clone().unwrap();

    db.update_contact_with_methods(
        &contact_id,
        &wallet_checksum,
        "Alice Updated",
        vec![
            (ProviderType::Email, "alice@example.com".to_string(), false),
            (ProviderType::Ntfy, "canary-alice-new".to_string(), true),
        ],
        ContactNotificationSettings {
            notify_cpfp: false,
            notify_rbf: false,
            ..ContactNotificationSettings::defaults_for_new_contact()
        },
    )
    .await
    .unwrap();

    let updated_contact = db
        .get_single_contact_with_methods(&contact_id, &wallet_checksum)
        .await
        .unwrap()
        .unwrap();
    let updated_email_method = updated_contact
        .notification_methods
        .iter()
        .find(|method| method.notification_target == "alice@example.com")
        .unwrap();

    assert_eq!(
        updated_email_method.id.as_ref().unwrap(),
        &original_email_method_id
    );
    assert!(!updated_email_method.is_enabled);
    assert!(!updated_contact
        .notification_methods
        .iter()
        .any(|method| method.notification_target == "canary-alice"));
}

#[tokio::test]
async fn test_legacy_wallet_level_balance_alerts_remain_active_and_manageable() {
    let (db, _temp_dir) = create_test_db().await;
    let user_id = db
        .create_user(
            "owner@example.com",
            "hashedpassword",
            Some("Owner"),
            true,
            None,
            None,
        )
        .await
        .unwrap();
    let wallet_checksum = db
        .insert_wallet("Wallet", "descriptor", &user_id)
        .await
        .unwrap();

    let legacy_alert = db
        .create_balance_alert(
            &wallet_checksum,
            100_000_000,
            BalanceAlertType::Above,
            None,
            None,
            Some(50_000_000),
        )
        .await
        .unwrap();

    let active_alerts = db
        .get_active_balance_alerts_for_wallet(&wallet_checksum)
        .await
        .unwrap();
    assert_eq!(active_alerts.len(), 1);
    assert_eq!(active_alerts[0].id, legacy_alert.id);

    let all_alerts = db
        .get_all_balance_alerts_for_wallet(&wallet_checksum)
        .await
        .unwrap();
    assert_eq!(all_alerts.len(), 1);
    assert_eq!(all_alerts[0].id, legacy_alert.id);
    assert!(all_alerts[0].is_active);

    let contact_id = db
        .insert_contact_with_notification_methods(
            &wallet_checksum,
            "Alice",
            vec![(ProviderType::Ntfy, "canary-alice".to_string())],
        )
        .await
        .unwrap();
    let contact_alert = db
        .create_balance_alert_with_contact(CreateBalanceAlertInput {
            wallet_checksum: &wallet_checksum,
            contact_id: Some(&contact_id),
            threshold_sats: 200_000_000,
            alert_type: BalanceAlertType::Below,
            threshold_currency: None,
            threshold_fiat_amount: None,
            current_balance_sats: Some(250_000_000),
        })
        .await
        .unwrap();

    let active_alerts = db
        .get_active_balance_alerts_for_wallet(&wallet_checksum)
        .await
        .unwrap();
    assert_eq!(active_alerts.len(), 2);
    assert!(active_alerts
        .iter()
        .any(|alert| alert.id == legacy_alert.id));
    assert!(active_alerts
        .iter()
        .any(|alert| alert.id == contact_alert.id));
}

#[tokio::test]
async fn test_wallet_status_accepts_failed_and_rejects_invalid_statuses() {
    let (db, _temp_dir) = create_test_db().await;

    let user_id = db
        .create_user(
            "wallet-status@example.com",
            "hashedpassword",
            Some("Wallet Status"),
            true,
            None,
            None,
        )
        .await
        .unwrap();
    let wallet_checksum = db
        .insert_wallet("Failed Wallet", "descriptor_failed_status", &user_id)
        .await
        .unwrap();

    db.update_wallet_status(&wallet_checksum, "failed")
        .await
        .expect("failed should be an accepted wallet status");

    let wallet = db
        .get_wallet_by_checksum(&wallet_checksum)
        .await
        .unwrap()
        .expect("wallet should exist");
    assert_eq!(wallet.status, "failed");

    let invalid_result = db.update_wallet_status(&wallet_checksum, "retrying").await;
    assert!(
        invalid_result.is_err(),
        "invalid wallet statuses should still be rejected"
    );
}

#[tokio::test]
async fn test_failed_wallets_do_not_count_toward_wallet_limit() {
    let (db, _temp_dir) = create_test_db().await;

    let user_id = db
        .create_user(
            "wallet-count@example.com",
            "hashedpassword",
            Some("Wallet Count"),
            true,
            None,
            None,
        )
        .await
        .unwrap();

    let ready_wallet_checksum = db
        .insert_wallet("Ready Wallet", "descriptor_ready_count", &user_id)
        .await
        .unwrap();
    let failed_wallet_checksum = db
        .insert_wallet("Failed Wallet", "descriptor_failed_count", &user_id)
        .await
        .unwrap();
    let deleted_wallet_checksum = db
        .insert_wallet("Deleted Wallet", "descriptor_deleted_count", &user_id)
        .await
        .unwrap();

    db.update_wallet_status(&ready_wallet_checksum, "ready")
        .await
        .unwrap();
    db.update_wallet_status(&failed_wallet_checksum, "failed")
        .await
        .unwrap();
    db.mark_wallet_as_deleted(&deleted_wallet_checksum)
        .await
        .unwrap();

    let wallet_count = db.count_wallets_for_user(&user_id).await.unwrap();
    assert_eq!(
        wallet_count, 1,
        "only active ready or pending wallets should count toward wallet limits"
    );
}

#[tokio::test]
async fn test_failed_wallets_do_not_consume_active_limit_slots() {
    let (db, _temp_dir) = create_test_db().await;

    let user_id = db
        .create_user(
            "wallet-active-count@example.com",
            "hashedpassword",
            Some("Wallet Active Count"),
            true,
            None,
            None,
        )
        .await
        .unwrap();

    let failed_wallet_checksum = db
        .insert_wallet("Failed Wallet", "descriptor_failed_active_count", &user_id)
        .await
        .unwrap();
    db.update_wallet_status(&failed_wallet_checksum, "failed")
        .await
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let ready_wallet_checksum = db
        .insert_wallet("Ready Wallet", "descriptor_ready_active_count", &user_id)
        .await
        .unwrap();
    db.update_wallet_status(&ready_wallet_checksum, "ready")
        .await
        .unwrap();

    let wallets = db
        .get_wallets_for_user_oldest_first(&user_id)
        .await
        .unwrap();

    let wallet_limit = 1;
    let mut active_wallet_count = 0;
    let mut non_failed_wallet_count = 0;
    for wallet in &wallets {
        let (should_be_active, _) = crate::subscription::wallet_active_limit_decision(
            &wallet.status,
            wallet_limit,
            &mut active_wallet_count,
            &mut non_failed_wallet_count,
        );
        db.update_wallet_active_status(&wallet.checksum, should_be_active)
            .await
            .unwrap();
    }

    let failed_wallet = db
        .get_wallet_by_checksum(&failed_wallet_checksum)
        .await
        .unwrap()
        .expect("failed wallet should exist");
    let ready_wallet = db
        .get_wallet_by_checksum(&ready_wallet_checksum)
        .await
        .unwrap()
        .expect("ready wallet should exist");

    assert!(
        !failed_wallet.is_active,
        "failed wallets should not be active"
    );
    assert!(
        ready_wallet.is_active,
        "ready wallet should stay active even when an older wallet failed"
    );
}

#[tokio::test]
async fn test_wallet_status_if_not_deleted_does_not_overwrite_deleted_wallets() {
    let (db, _temp_dir) = create_test_db().await;

    let user_id = db
        .create_user(
            "wallet-status-deleted@example.com",
            "hashedpassword",
            Some("Deleted Wallet Status"),
            true,
            None,
            None,
        )
        .await
        .unwrap();
    let wallet_checksum = db
        .insert_wallet("Deleted Wallet", "descriptor_deleted_status", &user_id)
        .await
        .unwrap();

    db.mark_wallet_as_deleted(&wallet_checksum).await.unwrap();

    let marked_failed = db
        .update_wallet_status_if_not_deleted(&wallet_checksum, "failed")
        .await
        .unwrap();
    let marked_ready = db
        .update_wallet_status_if_not_deleted(&wallet_checksum, "ready")
        .await
        .unwrap();

    assert!(
        !marked_failed,
        "deleted wallets should not be marked failed"
    );
    assert!(!marked_ready, "deleted wallets should not be marked ready");

    let wallet = db
        .get_wallet_by_checksum(&wallet_checksum)
        .await
        .unwrap()
        .expect("wallet should exist");
    assert_eq!(wallet.status, "deleted");
}

#[tokio::test]
async fn test_delete_wallet_contact_nonexistent() {
    let (db, _temp_dir) = create_test_db().await;

    // Create test user and wallet
    let user_id = db
        .create_user(
            "test@example.com",
            "hashedpassword",
            Some("Test User"),
            true,
            None, // preferred_currency
            None, // preferred_language
        )
        .await
        .unwrap();
    let wallet_checksum = db
        .insert_wallet("Test Wallet", "descriptor", &user_id)
        .await
        .unwrap();

    // Try to delete nonexistent contact
    let result = db
        .delete_wallet_contact(&wallet_checksum, "550e8400-e29b-41d4-a716-446655440999")
        .await
        .unwrap();
    assert!(!result, "Should return false for nonexistent contact");
}

#[tokio::test]
async fn test_delete_wallet_contact_wrong_wallet() {
    let (db, _temp_dir) = create_test_db().await;

    // Create test users and wallets
    let user1_id = db
        .create_user(
            "user1@example.com",
            "hashedpassword",
            Some("User 1"),
            true,
            None, // preferred_currency
            None, // preferred_language
        )
        .await
        .unwrap();
    let user2_id = db
        .create_user(
            "user2@example.com",
            "hashedpassword",
            Some("User 2"),
            true,
            None, // preferred_currency
            None, // preferred_language
        )
        .await
        .unwrap();

    let wallet1_checksum = db
        .insert_wallet("Wallet 1", "descriptor1", &user1_id)
        .await
        .unwrap();
    let wallet2_checksum = db
        .insert_wallet("Wallet 2", "descriptor2", &user2_id)
        .await
        .unwrap();

    // Add contact to wallet1
    let contact_id = db
        .insert_contact_with_notification_methods(&wallet1_checksum, "Test Contact", vec![])
        .await
        .unwrap();

    // Try to delete contact using wrong wallet checksum
    let result = db
        .delete_wallet_contact(&wallet2_checksum, &contact_id)
        .await
        .unwrap();
    assert!(
        !result,
        "Should NOT be able to delete contact using wrong wallet checksum"
    );

    // Verify contact still exists in correct wallet
    let contacts = db
        .get_contacts_with_notification_methods(&wallet1_checksum)
        .await
        .unwrap();
    assert_eq!(
        contacts.len(),
        1,
        "Contact should still exist in original wallet"
    );
}

#[tokio::test]
async fn test_get_user_preferred_language_default() {
    let (db, _temp_dir) = create_test_db().await;

    // Create user without preferred language
    let user_id = db
        .create_user(
            "test@example.com",
            "hashedpassword",
            Some("Test User"),
            false,
            None, // preferred_currency
            None, // preferred_language - not set
        )
        .await
        .unwrap();

    // Should return English as default
    let language = db.get_user_preferred_language(&user_id).await.unwrap();
    assert_eq!(
        language,
        Language::English,
        "Should default to English when no preference set"
    );
}

#[tokio::test]
async fn test_get_user_preferred_language_norwegian() {
    let (db, _temp_dir) = create_test_db().await;

    // Create user with Norwegian preference
    let user_id = db
        .create_user(
            "nordic@example.com",
            "hashedpassword",
            Some("Nordic User"),
            false,
            None,       // preferred_currency
            Some("nb"), // preferred_language - Norwegian (Bokmål)
        )
        .await
        .unwrap();

    // Should return Norwegian
    let language = db.get_user_preferred_language(&user_id).await.unwrap();
    assert_eq!(
        language,
        Language::Norwegian,
        "Should return Norwegian when set"
    );
}

#[tokio::test]
async fn test_get_user_preferred_language_japanese() {
    let (db, _temp_dir) = create_test_db().await;

    // Create user with Japanese preference
    let user_id = db
        .create_user(
            "japan@example.com",
            "hashedpassword",
            Some("Japanese User"),
            false,
            None,       // preferred_currency
            Some("ja"), // preferred_language - Japanese
        )
        .await
        .unwrap();

    // Should return Japanese
    let language = db.get_user_preferred_language(&user_id).await.unwrap();
    assert_eq!(
        language,
        Language::Japanese,
        "Should return Japanese when set"
    );
}

#[tokio::test]
async fn test_transaction_ordering_expression_index_is_applied() {
    let (db, _temp_dir) = create_test_db().await;

    let user_id = db
        .create_user(
            "perf@example.com",
            "hashedpassword",
            Some("Perf User"),
            true,
            None,
            None,
        )
        .await
        .unwrap();
    let wallet_checksum = db
        .insert_wallet("Perf Wallet", "wpkh(testkey)#perf01", &user_id)
        .await
        .unwrap();

    for (txid, first_seen_at, confirmed_at) in [
        ("tx-001", 1_700_000_001_u64, None),
        ("tx-002", 1_700_000_002_u64, Some(1_700_000_020_u64)),
        ("tx-003", 1_700_000_003_u64, Some(1_700_000_010_u64)),
    ] {
        db.insert_transaction(&TransactionInsert {
            txid: txid.to_string(),
            wallet_checksum: wallet_checksum.clone(),
            transaction_type: EventType::Receive,
            amount_sats: 50_000,
            fee_sats: None,
            block_height: confirmed_at.map(|_| 100),
            first_seen_at,
            confirmed_at,
            parent_txid: None,
            transaction_status: if confirmed_at.is_some() {
                "confirmed".to_string()
            } else {
                "pending".to_string()
            },
            replaced_by_txid: None,
            replaced_at: None,
        })
        .await
        .unwrap();
    }

    let conn = db.pool.get().unwrap();

    let index_names: Vec<String> = conn
        .prepare("PRAGMA index_list('transactions')")
        .unwrap()
        .query_map([], |row| row.get(1))
        .unwrap()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap();

    assert!(
        index_names
            .iter()
            .any(|name| name == "idx_transactions_wallet_ordering"),
        "Expected idx_transactions_wallet_ordering to exist, found {index_names:?}"
    );

    let ordered_txids: Vec<String> = conn
        .prepare(
            "SELECT t.txid
             FROM transactions t
             WHERE t.wallet_checksum = ?1
             ORDER BY COALESCE(t.confirmed_at, t.first_seen_at) DESC, t.txid DESC",
        )
        .unwrap()
        .query_map([wallet_checksum.as_str()], |row| row.get(0))
        .unwrap()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(ordered_txids, vec!["tx-002", "tx-003", "tx-001"]);

    // Column 3 is SQLite's human-readable plan detail.
    let ordering_plan_details: Vec<String> = conn
        .prepare(
            "EXPLAIN QUERY PLAN
             SELECT t.txid
             FROM transactions t
             WHERE t.wallet_checksum = ?1
             ORDER BY COALESCE(t.confirmed_at, t.first_seen_at) DESC, t.txid DESC
             LIMIT ?2",
        )
        .unwrap()
        .query_map([wallet_checksum.as_str(), "50"], |row| row.get(3))
        .unwrap()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap();

    assert!(
        ordering_plan_details
            .iter()
            .all(|detail| !detail.contains("USE TEMP B-TREE FOR ORDER BY")),
        "Expected ordering query plan to avoid temp sorting, found {ordering_plan_details:?}"
    );

    let last_activity: Option<i64> = conn
        .query_row(
            "SELECT MAX(COALESCE(t.confirmed_at, t.first_seen_at))
             FROM transactions t
             WHERE t.wallet_checksum = ?1",
            [wallet_checksum.as_str()],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(last_activity, Some(1_700_000_020));
}

#[tokio::test]
async fn test_get_user_preferred_language_nonexistent_user() {
    let (db, _temp_dir) = create_test_db().await;

    // Query for non-existent user should return English default
    let language = db
        .get_user_preferred_language("nonexistent-user-id")
        .await
        .unwrap();
    assert_eq!(
        language,
        Language::English,
        "Should default to English for non-existent user"
    );
}

#[tokio::test]
async fn test_update_user_preferred_tx_explorer_id() {
    let (db, _temp_dir) = create_test_db().await;

    let user_id = db
        .create_user(
            "explorer@example.com",
            "hashedpassword",
            Some("Explorer User"),
            false,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(
        db.get_user_preferred_tx_explorer_id(&user_id)
            .await
            .unwrap(),
        None
    );

    db.update_user_preferred_tx_explorer_id(&user_id, Some("bitfeed"))
        .await
        .unwrap();
    assert_eq!(
        db.get_user_preferred_tx_explorer_id(&user_id)
            .await
            .unwrap(),
        Some("bitfeed".to_string())
    );

    db.update_user_preferred_tx_explorer_id(&user_id, None)
        .await
        .unwrap();
    assert_eq!(
        db.get_user_preferred_tx_explorer_id(&user_id)
            .await
            .unwrap(),
        None
    );
}

// Unit tests for Language::twilio_locale()

#[test]
fn test_twilio_locale_english() {
    assert_eq!(Language::English.twilio_locale(), "en");
}

#[test]
fn test_twilio_locale_norwegian() {
    assert_eq!(Language::Norwegian.twilio_locale(), "nb");
}

#[test]
fn test_twilio_locale_spanish() {
    assert_eq!(Language::Spanish.twilio_locale(), "es");
}

#[test]
fn test_twilio_locale_portuguese() {
    // Twilio uses lowercase "pt-br" unlike BCP 47 "pt-BR"
    assert_eq!(Language::Portuguese.twilio_locale(), "pt-br");
}

#[test]
fn test_twilio_locale_german() {
    assert_eq!(Language::German.twilio_locale(), "de");
}

#[test]
fn test_twilio_locale_french() {
    assert_eq!(Language::French.twilio_locale(), "fr");
}

#[test]
fn test_twilio_locale_japanese() {
    assert_eq!(Language::Japanese.twilio_locale(), "ja");
}

#[test]
fn test_twilio_locale_danish() {
    assert_eq!(Language::Danish.twilio_locale(), "da");
}

#[test]
fn test_twilio_locale_swedish() {
    assert_eq!(Language::Swedish.twilio_locale(), "sv");
}

#[test]
fn test_twilio_locale_all_languages_are_valid() {
    // Verify all locales are non-empty and valid Twilio locale format
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

    for lang in languages {
        let locale = lang.twilio_locale();
        assert!(
            !locale.is_empty(),
            "Locale should not be empty for {:?}",
            lang
        );
        assert!(
            locale.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
            "Locale should be lowercase ASCII for {:?}: {}",
            lang,
            locale
        );
    }
}

// ============================
// Cross-wallet verification tests
// ============================

use crate::metadata::TransactionPageRequest;

#[tokio::test]
async fn test_cross_wallet_verification_same_user_sms() {
    let (db, _temp_dir) = create_test_db().await;

    // Create user with two wallets
    let user_id = db
        .create_user(
            "test@example.com",
            "hashedpassword",
            Some("Test User"),
            true,
            None,
            None,
        )
        .await
        .unwrap();

    let wallet1_checksum = db
        .insert_wallet("Wallet 1", "descriptor1", &user_id)
        .await
        .unwrap();
    let wallet2_checksum = db
        .insert_wallet("Wallet 2", "descriptor2", &user_id)
        .await
        .unwrap();

    let phone = "+4712345678";

    // Initially, phone should not be verified for user
    let result = db
        .is_notification_target_verified_for_user(&user_id, "sms", phone)
        .await
        .unwrap();
    assert!(!result, "Phone should not be verified initially");

    // Add contact with phone to wallet1
    db.insert_contact_with_notification_methods(
        &wallet1_checksum,
        "Contact 1",
        vec![(ProviderType::Sms, phone.to_string())],
    )
    .await
    .unwrap();

    // Now phone should be verified for user (can be used on wallet2 without OTP)
    let result = db
        .is_notification_target_verified_for_user(&user_id, "sms", phone)
        .await
        .unwrap();
    assert!(
        result,
        "Phone should be verified after adding to another wallet"
    );

    // Verify it works for any wallet owned by this user
    let _ = wallet2_checksum; // Wallet2 exists but has no contacts yet
}

#[tokio::test]
async fn test_cross_wallet_verification_same_user_email() {
    let (db, _temp_dir) = create_test_db().await;

    // Create user with two wallets
    let user_id = db
        .create_user(
            "test@example.com",
            "hashedpassword",
            Some("Test User"),
            true,
            None,
            None,
        )
        .await
        .unwrap();

    let wallet1_checksum = db
        .insert_wallet("Wallet 1", "descriptor1", &user_id)
        .await
        .unwrap();

    let email = "contact@example.com";

    // Initially, email should not be verified for user
    let result = db
        .is_notification_target_verified_for_user(&user_id, "email", email)
        .await
        .unwrap();
    assert!(!result, "Email should not be verified initially");

    // Add contact with email to wallet1
    db.insert_contact_with_notification_methods(
        &wallet1_checksum,
        "Contact 1",
        vec![(ProviderType::Email, email.to_string())],
    )
    .await
    .unwrap();

    // Now email should be verified for user
    let result = db
        .is_notification_target_verified_for_user(&user_id, "email", email)
        .await
        .unwrap();
    assert!(
        result,
        "Email should be verified after adding to another wallet"
    );
}

#[tokio::test]
async fn test_cross_wallet_verification_different_users() {
    let (db, _temp_dir) = create_test_db().await;

    // Create two different users
    let user1_id = db
        .create_user(
            "user1@example.com",
            "hashedpassword",
            Some("User 1"),
            true,
            None,
            None,
        )
        .await
        .unwrap();
    let user2_id = db
        .create_user(
            "user2@example.com",
            "hashedpassword",
            Some("User 2"),
            true,
            None,
            None,
        )
        .await
        .unwrap();

    let wallet1_checksum = db
        .insert_wallet("Wallet 1", "descriptor1", &user1_id)
        .await
        .unwrap();

    let phone = "+4712345678";

    // Add contact with phone to user1's wallet
    db.insert_contact_with_notification_methods(
        &wallet1_checksum,
        "Contact 1",
        vec![(ProviderType::Sms, phone.to_string())],
    )
    .await
    .unwrap();

    // Phone should be verified for user1
    let result = db
        .is_notification_target_verified_for_user(&user1_id, "sms", phone)
        .await
        .unwrap();
    assert!(result, "Phone should be verified for user1");

    // Phone should NOT be verified for user2 (different user)
    let result = db
        .is_notification_target_verified_for_user(&user2_id, "sms", phone)
        .await
        .unwrap();
    assert!(
        !result,
        "Phone should NOT be verified for different user - security boundary"
    );
}

#[tokio::test]
async fn test_cross_wallet_verification_email_case_insensitive() {
    let (db, _temp_dir) = create_test_db().await;

    let user_id = db
        .create_user(
            "test@example.com",
            "hashedpassword",
            Some("Test User"),
            true,
            None,
            None,
        )
        .await
        .unwrap();

    let wallet_checksum = db
        .insert_wallet("Wallet 1", "descriptor1", &user_id)
        .await
        .unwrap();

    // Add contact with lowercase email
    db.insert_contact_with_notification_methods(
        &wallet_checksum,
        "Contact 1",
        vec![(ProviderType::Email, "contact@example.com".to_string())],
    )
    .await
    .unwrap();

    // Should match with different case
    let result = db
        .is_notification_target_verified_for_user(&user_id, "email", "CONTACT@EXAMPLE.COM")
        .await
        .unwrap();
    assert!(result, "Email comparison should be case-insensitive");

    let result = db
        .is_notification_target_verified_for_user(&user_id, "email", "Contact@Example.Com")
        .await
        .unwrap();
    assert!(result, "Email comparison should be case-insensitive");
}

#[tokio::test]
async fn test_cross_wallet_verification_sms_exact_match() {
    let (db, _temp_dir) = create_test_db().await;

    let user_id = db
        .create_user(
            "test@example.com",
            "hashedpassword",
            Some("Test User"),
            true,
            None,
            None,
        )
        .await
        .unwrap();

    let wallet_checksum = db
        .insert_wallet("Wallet 1", "descriptor1", &user_id)
        .await
        .unwrap();

    // Add contact with phone in E.164 format
    db.insert_contact_with_notification_methods(
        &wallet_checksum,
        "Contact 1",
        vec![(ProviderType::Sms, "+4712345678".to_string())],
    )
    .await
    .unwrap();

    // Should match exact E.164 format
    let result = db
        .is_notification_target_verified_for_user(&user_id, "sms", "+4712345678")
        .await
        .unwrap();
    assert!(result, "Phone should match with exact E.164 format");

    // Should NOT match with different format (not normalized)
    let result = db
        .is_notification_target_verified_for_user(&user_id, "sms", "4712345678")
        .await
        .unwrap();
    assert!(
        !result,
        "Phone comparison should be exact match (E.164 format)"
    );
}

// ============================
// Subscription limit tests
// ============================

#[tokio::test]
async fn test_deleted_wallets_excluded_from_subscription_limits() {
    let (db, _temp_dir) = create_test_db().await;

    // Create a user with 3 wallets (simulating Team tier with 5-wallet limit)
    let user_id = db
        .create_user(
            "test@example.com",
            "hashedpassword",
            Some("Test User"),
            false,
            None,
            None,
        )
        .await
        .unwrap();

    let wallet1 = db
        .insert_wallet("Wallet 1", "descriptor1", &user_id)
        .await
        .unwrap();

    // Small delay to ensure different created_at timestamps
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let wallet2 = db
        .insert_wallet("Wallet 2", "descriptor2", &user_id)
        .await
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let wallet3 = db
        .insert_wallet("Wallet 3", "descriptor3", &user_id)
        .await
        .unwrap();

    // Soft-delete wallets 1 and 2
    db.mark_wallet_as_deleted(&wallet1).await.unwrap();
    db.mark_wallet_as_deleted(&wallet2).await.unwrap();

    // get_wallets_for_user_oldest_first (used by apply_subscription_limits)
    // should exclude deleted wallets
    let wallets = db
        .get_wallets_for_user_oldest_first(&user_id)
        .await
        .unwrap();

    assert_eq!(wallets.len(), 1, "Should only return non-deleted wallets");
    assert_eq!(
        wallets[0].checksum, wallet3,
        "The only remaining wallet should be wallet3"
    );
}

#[tokio::test]
async fn test_deleted_wallets_do_not_consume_limit_slots() {
    let (db, _temp_dir) = create_test_db().await;

    // Simulate the reported bug scenario:
    // User on Personal tier (1 wallet limit) with deleted wallets
    let user_id = db
        .create_user(
            "andreas@example.com",
            "hashedpassword",
            Some("Andreas"),
            false,
            None,
            None,
        )
        .await
        .unwrap();

    // Create 3 wallets in order
    let wallet1 = db
        .insert_wallet("Old Wallet 1", "desc1", &user_id)
        .await
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let wallet2 = db
        .insert_wallet("Old Wallet 2", "desc2", &user_id)
        .await
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let wallet3 = db
        .insert_wallet("Current Wallet", "desc3", &user_id)
        .await
        .unwrap();

    // Delete the first two (they were created earlier, so they'd occupy slots 0 and 1)
    db.mark_wallet_as_deleted(&wallet1).await.unwrap();
    db.mark_wallet_as_deleted(&wallet2).await.unwrap();

    // Now apply Personal tier limits (max 1 wallet)
    // The remaining wallet3 should be at index 0 and active
    let wallets = db
        .get_wallets_for_user_oldest_first(&user_id)
        .await
        .unwrap();

    assert_eq!(wallets.len(), 1);

    // Simulate what apply_subscription_limits does: activate first N wallets
    let wallet_limit: usize = 1; // Personal tier
    for (index, wallet) in wallets.iter().enumerate() {
        let should_be_active = index < wallet_limit;
        db.update_wallet_active_status(&wallet.checksum, should_be_active)
            .await
            .unwrap();
    }

    // Verify wallet3 is active (it's the only non-deleted wallet, within limit)
    let wallets = db
        .get_wallets_for_user_oldest_first(&user_id)
        .await
        .unwrap();
    assert!(
        wallets[0].is_active,
        "Wallet3 should be active since deleted wallets no longer consume limit slots"
    );
    assert_eq!(wallets[0].checksum, wallet3);
}

#[tokio::test]
async fn test_inactive_contacts_do_not_consume_limit_slots() {
    let (db, _temp_dir) = create_test_db().await;

    let user_id = db
        .create_user(
            "contacts@example.com",
            "hashedpassword",
            Some("Contacts User"),
            false,
            None,
            None,
        )
        .await
        .unwrap();

    let wallet_checksum = db
        .insert_wallet("Contacts Wallet", "descriptor", &user_id)
        .await
        .unwrap();

    let active_contact_id = db
        .insert_contact_with_notification_methods(&wallet_checksum, "Active Contact", vec![])
        .await
        .unwrap();

    let inactive_contact_id = db
        .insert_contact_with_notification_methods(&wallet_checksum, "Inactive Contact", vec![])
        .await
        .unwrap();

    db.update_contact_active_status(&inactive_contact_id, false)
        .await
        .unwrap();

    let active_count = db
        .count_active_contacts_for_wallet(&wallet_checksum)
        .await
        .unwrap();
    assert_eq!(
        active_count, 1,
        "Only active contacts should count toward limits"
    );

    let total_count = db
        .count_contacts_for_wallet(&wallet_checksum)
        .await
        .unwrap();
    assert_eq!(
        total_count, 2,
        "Total contact counts should include inactive rows"
    );

    let contacts = db
        .get_contacts_with_notification_methods_filtered(&wallet_checksum, true)
        .await
        .unwrap();

    assert_eq!(
        contacts.len(),
        2,
        "Inactive contacts should remain persisted"
    );
    assert!(contacts
        .iter()
        .any(|contact| contact.id.as_deref() == Some(&active_contact_id) && contact.is_active));
    assert!(contacts
        .iter()
        .any(|contact| contact.id.as_deref() == Some(&inactive_contact_id) && !contact.is_active));
}

// ============================
// Transaction ordering tests
// ============================

#[tokio::test]
async fn test_transaction_ordering_prefers_confirmed_at() {
    let (db, _temp_dir) = create_test_db().await;

    let user_id = db
        .create_user(
            "test@example.com",
            "hashedpassword",
            Some("Test User"),
            true,
            None,
            None,
        )
        .await
        .unwrap();

    let wallet_checksum = db
        .insert_wallet("Test Wallet", "descriptor1", &user_id)
        .await
        .unwrap();

    let now = 1740000000u64; // Recent timestamp

    // Transaction 1: Old confirmed transaction imported recently
    // (e.g. Genesis block tx synced today — confirmed_at is old, first_seen_at is now)
    db.insert_transaction(&TransactionInsert {
        txid: "tx_old_confirmed".to_string(),
        wallet_checksum: wallet_checksum.clone(),
        transaction_type: EventType::Receive,
        amount_sats: 5_000_000_000,
        fee_sats: None,
        block_height: Some(1),
        first_seen_at: now,
        confirmed_at: Some(1231006505), // Jan 3, 2009 (Genesis block)
        parent_txid: None,
        transaction_status: "confirmed".to_string(),
        replaced_by_txid: None,
        replaced_at: None,
    })
    .await
    .unwrap();

    // Transaction 2: Recent confirmed transaction
    db.insert_transaction(&TransactionInsert {
        txid: "tx_recent_confirmed".to_string(),
        wallet_checksum: wallet_checksum.clone(),
        transaction_type: EventType::Receive,
        amount_sats: 1000,
        fee_sats: None,
        block_height: Some(800000),
        first_seen_at: now - 100,
        confirmed_at: Some(now - 50),
        parent_txid: None,
        transaction_status: "confirmed".to_string(),
        replaced_by_txid: None,
        replaced_at: None,
    })
    .await
    .unwrap();

    // Transaction 3: Pending mempool transaction (no confirmed_at)
    db.insert_transaction(&TransactionInsert {
        txid: "tx_pending".to_string(),
        wallet_checksum: wallet_checksum.clone(),
        transaction_type: EventType::Receive,
        amount_sats: 500,
        fee_sats: None,
        block_height: None,
        first_seen_at: now + 10,
        confirmed_at: None,
        parent_txid: None,
        transaction_status: "pending".to_string(),
        replaced_by_txid: None,
        replaced_at: None,
    })
    .await
    .unwrap();

    let transactions = db
        .get_transactions_by_wallet_checksum(&wallet_checksum, None, false)
        .await
        .unwrap();

    assert_eq!(transactions.len(), 3);

    // Expected order (newest first):
    // 1. Pending tx (first_seen_at = now + 10, most recent)
    // 2. Recent confirmed tx (confirmed_at = now - 50)
    // 3. Old confirmed tx (confirmed_at = Genesis 2009, despite first_seen_at = now)
    assert_eq!(
        transactions[0].txid, "tx_pending",
        "Pending transaction should appear first (newest)"
    );
    assert_eq!(
        transactions[1].txid, "tx_recent_confirmed",
        "Recent confirmed transaction should appear second"
    );
    assert_eq!(
        transactions[2].txid, "tx_old_confirmed",
        "Old confirmed transaction should appear last despite recent first_seen_at"
    );
}

#[tokio::test]
async fn test_wallet_last_activity_uses_confirmed_at_for_confirmed_transactions() {
    let (db, _temp_dir) = create_test_db().await;

    let user_id = db
        .create_user(
            "wallet-last-activity@example.com",
            "hashedpassword",
            Some("Test User"),
            true,
            None,
            None,
        )
        .await
        .unwrap();

    let wallet_checksum = db
        .insert_wallet("Test Wallet", "descriptor1", &user_id)
        .await
        .unwrap();

    let empty_wallets = db.get_wallets_for_user(Some(&user_id)).await.unwrap();
    assert_eq!(empty_wallets.len(), 1);
    assert_eq!(
        empty_wallets[0].last_activity, None,
        "Wallet without transactions should have no last_activity"
    );

    let now = 1740000000u64;

    db.insert_transaction(&TransactionInsert {
        txid: "tx_old_confirmed".to_string(),
        wallet_checksum: wallet_checksum.clone(),
        transaction_type: EventType::Receive,
        amount_sats: 100_000,
        fee_sats: None,
        block_height: Some(1),
        first_seen_at: now,
        confirmed_at: Some(1231006505),
        parent_txid: None,
        transaction_status: "confirmed".to_string(),
        replaced_by_txid: None,
        replaced_at: None,
    })
    .await
    .unwrap();

    db.insert_transaction(&TransactionInsert {
        txid: "tx_recent_confirmed".to_string(),
        wallet_checksum: wallet_checksum.clone(),
        transaction_type: EventType::Receive,
        amount_sats: 1000,
        fee_sats: None,
        block_height: Some(800000),
        first_seen_at: now - 100,
        confirmed_at: Some(now - 50),
        parent_txid: None,
        transaction_status: "confirmed".to_string(),
        replaced_by_txid: None,
        replaced_at: None,
    })
    .await
    .unwrap();

    db.insert_transaction(&TransactionInsert {
        txid: "tx_pending_older".to_string(),
        wallet_checksum: wallet_checksum.clone(),
        transaction_type: EventType::Receive,
        amount_sats: 500,
        fee_sats: None,
        block_height: None,
        first_seen_at: now - 200,
        confirmed_at: None,
        parent_txid: None,
        transaction_status: "pending".to_string(),
        replaced_by_txid: None,
        replaced_at: None,
    })
    .await
    .unwrap();

    let wallets = db.get_wallets_for_user(Some(&user_id)).await.unwrap();
    let expected_last_activity = (now - 50).to_string();

    assert_eq!(wallets.len(), 1);
    assert_eq!(
        wallets[0].last_activity.as_deref(),
        Some(expected_last_activity.as_str()),
        "Wallet last_activity should use the latest confirmed_at value, not first_seen_at"
    );
}

#[tokio::test]
async fn test_wallet_last_activity_falls_back_to_first_seen_at_for_pending_transactions() {
    let (db, _temp_dir) = create_test_db().await;

    let user_id = db
        .create_user(
            "wallet-pending-last-activity@example.com",
            "hashedpassword",
            Some("Test User"),
            true,
            None,
            None,
        )
        .await
        .unwrap();

    let wallet_checksum = db
        .insert_wallet("Pending Wallet", "descriptor2", &user_id)
        .await
        .unwrap();

    let pending_first_seen_at = 1740000200u64;

    db.insert_transaction(&TransactionInsert {
        txid: "tx_pending_only".to_string(),
        wallet_checksum: wallet_checksum.clone(),
        transaction_type: EventType::Receive,
        amount_sats: 750,
        fee_sats: None,
        block_height: None,
        first_seen_at: pending_first_seen_at,
        confirmed_at: None,
        parent_txid: None,
        transaction_status: "pending".to_string(),
        replaced_by_txid: None,
        replaced_at: None,
    })
    .await
    .unwrap();

    let wallets = db.get_wallets_for_user(Some(&user_id)).await.unwrap();
    let expected_last_activity = pending_first_seen_at.to_string();

    assert_eq!(wallets.len(), 1);
    assert_eq!(
        wallets[0].last_activity.as_deref(),
        Some(expected_last_activity.as_str()),
        "Pending-only wallets should use first_seen_at as the last_activity fallback"
    );
}

#[tokio::test]
async fn test_transaction_pagination_uses_cursor() {
    let (db, _temp_dir) = create_test_db().await;

    let user_id = db
        .create_user(
            "cursor-test@example.com",
            "hashedpassword",
            Some("Test User"),
            true,
            None,
            None,
        )
        .await
        .unwrap();

    let wallet_checksum = db
        .insert_wallet("Cursor Wallet", "descriptor_cursor", &user_id)
        .await
        .unwrap();

    let base_timestamp = 1740000000u64;
    for (offset, txid) in ["tx_c", "tx_b", "tx_a"].iter().enumerate() {
        db.insert_transaction(&TransactionInsert {
            txid: txid.to_string(),
            wallet_checksum: wallet_checksum.clone(),
            transaction_type: EventType::Receive,
            amount_sats: 1_000 + offset as i64,
            fee_sats: None,
            block_height: Some(100 + offset as u32),
            first_seen_at: base_timestamp + offset as u64,
            confirmed_at: Some(base_timestamp + offset as u64),
            parent_txid: None,
            transaction_status: "confirmed".to_string(),
            replaced_by_txid: None,
            replaced_at: None,
        })
        .await
        .unwrap();
    }

    let first_page = db
        .get_transactions_page_by_wallet_checksum(
            &wallet_checksum,
            TransactionPageRequest {
                limit: 2,
                cursor: None,
                since_timestamp: None,
                include_notifications: false,
            },
        )
        .await
        .unwrap();

    assert_eq!(
        first_page
            .transactions
            .iter()
            .map(|transaction| transaction.txid.as_str())
            .collect::<Vec<_>>(),
        vec!["tx_a", "tx_b"]
    );
    assert!(first_page.has_more);

    let second_page = db
        .get_transactions_page_by_wallet_checksum(
            &wallet_checksum,
            TransactionPageRequest {
                limit: 2,
                cursor: first_page.next_cursor.clone(),
                since_timestamp: None,
                include_notifications: false,
            },
        )
        .await
        .unwrap();

    assert_eq!(
        second_page
            .transactions
            .iter()
            .map(|transaction| transaction.txid.as_str())
            .collect::<Vec<_>>(),
        vec!["tx_c"]
    );
    assert!(!second_page.has_more);
}

#[tokio::test]
async fn test_transaction_pagination_since_timestamp_returns_changed_rows() {
    let (db, _temp_dir) = create_test_db().await;

    let user_id = db
        .create_user(
            "since-test@example.com",
            "hashedpassword",
            Some("Test User"),
            true,
            None,
            None,
        )
        .await
        .unwrap();

    let wallet_checksum = db
        .insert_wallet("Since Wallet", "descriptor_since", &user_id)
        .await
        .unwrap();

    db.insert_transaction(&TransactionInsert {
        txid: "tx_old".to_string(),
        wallet_checksum: wallet_checksum.clone(),
        transaction_type: EventType::Receive,
        amount_sats: 1_000,
        fee_sats: None,
        block_height: Some(100),
        first_seen_at: 1_000,
        confirmed_at: Some(1_000),
        parent_txid: None,
        transaction_status: "confirmed".to_string(),
        replaced_by_txid: None,
        replaced_at: None,
    })
    .await
    .unwrap();

    db.insert_transaction(&TransactionInsert {
        txid: "tx_replaced".to_string(),
        wallet_checksum: wallet_checksum.clone(),
        transaction_type: EventType::Send,
        amount_sats: -2_000,
        fee_sats: Some(200),
        block_height: None,
        first_seen_at: 1_100,
        confirmed_at: None,
        parent_txid: None,
        transaction_status: "replaced".to_string(),
        replaced_by_txid: Some("tx_replacement".to_string()),
        replaced_at: Some(1_500),
    })
    .await
    .unwrap();

    let page = db
        .get_transactions_page_by_wallet_checksum(
            &wallet_checksum,
            TransactionPageRequest {
                limit: 10,
                cursor: None,
                since_timestamp: Some(1_200),
                include_notifications: false,
            },
        )
        .await
        .unwrap();

    assert_eq!(
        page.transactions
            .iter()
            .map(|transaction| transaction.txid.as_str())
            .collect::<Vec<_>>(),
        vec!["tx_replaced"]
    );
    assert_eq!(page.applied_since_timestamp, Some(1_200));
}

#[tokio::test]
async fn test_get_wallets_for_tier_sync_uses_transaction_last_activity() {
    let (db, _temp_dir) = create_test_db().await;

    let empty_wallet_checksum = db
        .insert_wallet("Empty Sync Wallet", "descriptor_sync_empty", "foss-user")
        .await
        .unwrap();
    db.update_wallet_status(&empty_wallet_checksum, "ready")
        .await
        .unwrap();

    let wallet_checksum = db
        .insert_wallet("Sync Wallet", "descriptor_sync", "foss-user")
        .await
        .unwrap();
    db.update_wallet_status(&wallet_checksum, "ready")
        .await
        .unwrap();

    db.insert_transaction(&TransactionInsert {
        txid: "tx_sync_last_activity".to_string(),
        wallet_checksum: wallet_checksum.clone(),
        transaction_type: EventType::Receive,
        amount_sats: 1000,
        fee_sats: None,
        block_height: None,
        first_seen_at: 1_740_000_123,
        confirmed_at: None,
        parent_txid: None,
        transaction_status: "pending".to_string(),
        replaced_by_txid: None,
        replaced_at: None,
    })
    .await
    .unwrap();

    let wallets = db
        .get_wallets_for_tier_sync(
            &crate::subscription::SubscriptionTier::Team,
            &NetworkConfig::Regtest,
        )
        .await
        .unwrap();

    let empty_wallet = wallets
        .iter()
        .find(|wallet| wallet.checksum == empty_wallet_checksum)
        .expect("empty wallet should be returned for sync");

    assert_eq!(
        empty_wallet.last_activity, None,
        "wallets without transactions should have no derived last_activity"
    );

    let wallet = wallets
        .into_iter()
        .find(|wallet| wallet.checksum == wallet_checksum)
        .expect("wallet should be returned for sync");

    assert_eq!(
        wallet.last_activity.as_deref(),
        Some("1740000123"),
        "sync query should derive last_activity from transactions instead of the stale wallets column"
    );
}

#[tokio::test]
async fn test_get_wallets_for_tier_sync_excludes_pending_descriptor_wallets() {
    let (db, _temp_dir) = create_test_db().await;

    let pending_descriptor_checksum = db
        .insert_wallet(
            "Pending Descriptor",
            "descriptor_pending_tier_sync",
            "foss-user",
        )
        .await
        .unwrap();
    let ready_descriptor_checksum = db
        .insert_wallet(
            "Ready Descriptor",
            "descriptor_ready_tier_sync",
            "foss-user",
        )
        .await
        .unwrap();
    let pending_address_checksum = db
        .insert_wallet_with_type(
            "Pending Address",
            "addr(bcrt1qpendingaddresssync)",
            "foss-user",
            "address",
        )
        .await
        .unwrap();

    db.update_wallet_status(&ready_descriptor_checksum, "ready")
        .await
        .unwrap();

    let wallets = db
        .get_wallets_for_tier_sync(
            &crate::subscription::SubscriptionTier::Team,
            &NetworkConfig::Regtest,
        )
        .await
        .unwrap();
    let checksums = wallets
        .iter()
        .map(|wallet| wallet.checksum.as_str())
        .collect::<Vec<_>>();

    assert!(
        !checksums.contains(&pending_descriptor_checksum.as_str()),
        "pending descriptor wallets should wait for the creation task"
    );
    assert!(
        checksums.contains(&ready_descriptor_checksum.as_str()),
        "ready descriptor wallets should sync normally"
    );
    assert!(
        checksums.contains(&pending_address_checksum.as_str()),
        "pending address watches should remain eligible for initial sync"
    );
}

// ============================
// Notification batching tests
// ============================

#[tokio::test]
async fn test_include_notifications_batches_correctly() {
    let (db, _temp_dir) = create_test_db().await;

    let user_id = db
        .create_user(
            "notif-test@example.com",
            "hashedpassword",
            Some("Test User"),
            true,
            None,
            None,
        )
        .await
        .unwrap();

    let wallet_checksum = db
        .insert_wallet("Notif Wallet", "descriptor_notif", &user_id)
        .await
        .unwrap();

    let now = 1740000000u64;

    // Insert two transactions
    for (i, txid) in ["tx_a", "tx_b"].iter().enumerate() {
        db.insert_transaction(&TransactionInsert {
            txid: txid.to_string(),
            wallet_checksum: wallet_checksum.clone(),
            transaction_type: EventType::Receive,
            amount_sats: 1000 * (i as i64 + 1),
            fee_sats: None,
            block_height: Some(100 + i as u32),
            first_seen_at: now + i as u64,
            confirmed_at: Some(now + i as u64),
            parent_txid: None,
            transaction_status: "confirmed".to_string(),
            replaced_by_txid: None,
            replaced_at: None,
        })
        .await
        .unwrap();
    }

    // Insert notification logs directly via SQL (avoids needing contacts/methods)
    // Explicit created_at timestamps ensure deterministic ORDER BY created_at ASC
    {
        let conn = db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO notification_logs (id, transaction_txid, transaction_wallet_checksum, provider_name, status, message_content, notification_type, contact_name_snapshot, notification_target_snapshot, provider_type_snapshot, created_at)
             VALUES ('log1', 'tx_a', ?1, 'ntfy', 'sent', 'msg1', 'confirmed', 'Alice', 'topic1', 'ntfy', '2025-01-01 00:00:01')",
            [&wallet_checksum],
        ).unwrap();
        conn.execute(
            "INSERT INTO notification_logs (id, transaction_txid, transaction_wallet_checksum, provider_name, status, message_content, notification_type, contact_name_snapshot, notification_target_snapshot, provider_type_snapshot, created_at)
             VALUES ('log2', 'tx_a', ?1, 'email', 'sent', 'msg2', 'confirmed', 'Bob', 'bob@example.com', 'email', '2025-01-01 00:00:02')",
            [&wallet_checksum],
        ).unwrap();
        conn.execute(
            "INSERT INTO notification_logs (id, transaction_txid, transaction_wallet_checksum, provider_name, status, message_content, notification_type, contact_name_snapshot, notification_target_snapshot, provider_type_snapshot, created_at)
             VALUES ('log3', 'tx_b', ?1, 'sms', 'failed', 'msg3', 'pending', 'Charlie', '+1234567890', 'sms', '2025-01-01 00:00:03')",
            [&wallet_checksum],
        ).unwrap();
    }

    // Without notifications
    let txs = db
        .get_transactions_by_wallet_checksum(&wallet_checksum, None, false)
        .await
        .unwrap();
    assert_eq!(txs.len(), 2);
    for tx in &txs {
        assert!(
            tx.notification_status.is_empty(),
            "Should have no notifications when include_notifications=false"
        );
    }

    // With notifications
    let txs = db
        .get_transactions_by_wallet_checksum(&wallet_checksum, None, true)
        .await
        .unwrap();
    assert_eq!(txs.len(), 2);

    let tx_a = txs.iter().find(|t| t.txid == "tx_a").unwrap();
    let tx_b = txs.iter().find(|t| t.txid == "tx_b").unwrap();

    assert_eq!(
        tx_a.notification_status.len(),
        2,
        "tx_a should have 2 notifications"
    );
    assert_eq!(tx_a.notification_status[0].contact_name, "Alice");
    assert_eq!(tx_a.notification_status[0].provider_name, "ntfy");
    assert_eq!(tx_a.notification_status[1].contact_name, "Bob");
    assert_eq!(tx_a.notification_status[1].provider_name, "email");

    assert_eq!(
        tx_b.notification_status.len(),
        1,
        "tx_b should have 1 notification"
    );
    assert_eq!(tx_b.notification_status[0].contact_name, "Charlie");
    assert_eq!(tx_b.notification_status[0].status, "failed");
}

#[tokio::test]
async fn test_auth_rate_limit_blocks_after_threshold_within_window() {
    let (db, _temp_dir) = create_test_db().await;

    assert!(db
        .check_auth_rate_limit("register", "person@example.com", 3, 60)
        .await
        .unwrap());
    assert!(db
        .check_auth_rate_limit("register", "person@example.com", 3, 60)
        .await
        .unwrap());
    assert!(db
        .check_auth_rate_limit("register", "person@example.com", 3, 60)
        .await
        .unwrap());
    assert!(!db
        .check_auth_rate_limit("register", "person@example.com", 3, 60)
        .await
        .unwrap());
}

#[tokio::test]
async fn test_auth_rate_limit_is_scoped_by_endpoint_and_identifier() {
    let (db, _temp_dir) = create_test_db().await;

    for _ in 0..4 {
        let _ = db
            .check_auth_rate_limit("forgot_password", "person@example.com", 3, 60)
            .await
            .unwrap();
    }

    assert!(db
        .check_auth_rate_limit("register", "person@example.com", 3, 60)
        .await
        .unwrap());
    assert!(db
        .check_auth_rate_limit("forgot_password", "other@example.com", 3, 60)
        .await
        .unwrap());
}

#[tokio::test]
async fn test_auth_rate_limit_normalizes_identifier() {
    let (db, _temp_dir) = create_test_db().await;

    assert!(db
        .check_auth_rate_limit("register", " Person@Example.com ", 3, 60)
        .await
        .unwrap());
    assert!(db
        .check_auth_rate_limit("register", "person@example.com", 3, 60)
        .await
        .unwrap());
    assert!(db
        .check_auth_rate_limit("register", "PERSON@EXAMPLE.COM", 3, 60)
        .await
        .unwrap());
    assert!(!db
        .check_auth_rate_limit("register", "person@example.com", 3, 60)
        .await
        .unwrap());
}

#[tokio::test]
async fn test_auth_rate_limit_resets_after_block_expires() {
    let (db, _temp_dir) = create_test_db().await;

    let now = chrono::Utc::now();
    let stale_first_attempt = (now - chrono::Duration::minutes(5))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let expired_block = (now - chrono::Duration::minutes(1))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    {
        let conn = db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO auth_rate_limits (scope, identifier, attempt_count, first_attempt_at, blocked_until)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                "forgot_password",
                "person@example.com",
                3,
                &stale_first_attempt,
                &expired_block,
            ),
        )
        .unwrap();
    }

    assert!(db
        .check_auth_rate_limit("forgot_password", "person@example.com", 3, 60)
        .await
        .unwrap());
}

#[tokio::test]
async fn test_auth_rate_limit_stays_blocked_until_expiry() {
    let (db, _temp_dir) = create_test_db().await;

    let now = chrono::Utc::now();
    let first_attempt = now.format("%Y-%m-%d %H:%M:%S").to_string();
    let future_block = (now + chrono::Duration::minutes(10))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    {
        let conn = db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO auth_rate_limits (scope, identifier, attempt_count, first_attempt_at, blocked_until)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                "forgot_password",
                "person@example.com",
                3,
                &first_attempt,
                &future_block,
            ),
        )
        .unwrap();
    }

    assert!(!db
        .check_auth_rate_limit("forgot_password", "person@example.com", 3, 60)
        .await
        .unwrap());
}

#[tokio::test]
async fn test_rate_limit_reports_remaining_retry_after_seconds() {
    let (db, _temp_dir) = create_test_db().await;

    let now = chrono::Utc::now();
    let first_attempt = now.format("%Y-%m-%d %H:%M:%S").to_string();
    let future_block = (now + chrono::Duration::minutes(2))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    {
        let conn = db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO auth_rate_limits (scope, identifier, attempt_count, first_attempt_at, blocked_until)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                "database_health",
                "foss-user",
                6,
                &first_attempt,
                &future_block,
            ),
        )
        .unwrap();
    }

    let decision = db
        .check_endpoint_rate_limit("database_health", "foss-user", 6, 5)
        .await
        .unwrap();

    assert!(!decision.allowed);
    let retry_after = decision.retry_after_seconds.unwrap();
    assert!(
        (1..=120).contains(&retry_after),
        "retry_after should be remaining seconds, got {retry_after}"
    );
}

#[tokio::test]
async fn test_auth_rate_limit_resets_after_window_expires() {
    let (db, _temp_dir) = create_test_db().await;

    let expired_first_attempt = (chrono::Utc::now() - chrono::Duration::minutes(61))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    {
        let conn = db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO auth_rate_limits (scope, identifier, attempt_count, first_attempt_at, blocked_until)
             VALUES (?1, ?2, ?3, ?4, NULL)",
            ("register", "person@example.com", 3, &expired_first_attempt),
        )
        .unwrap();
    }

    assert!(db
        .check_auth_rate_limit("register", "person@example.com", 3, 60)
        .await
        .unwrap());
}

#[tokio::test]
async fn expiring_subscription_clears_stripe_id_without_changing_tier() {
    let (db, _temp_dir) = create_test_db().await;
    let user_id = db
        .create_user(
            "subscriber@example.com",
            "hashedpassword",
            Some("Subscriber"),
            true,
            None,
            None,
        )
        .await
        .unwrap();

    db.update_user_subscription(
        &user_id,
        &SubscriptionUpdateParams {
            subscription_tier: "personal",
            subscription_status: "active",
            stripe_subscription_id: Some("sub_current"),
            subscription_started_at: None,
            subscription_ends_at: None,
            trial_ends_at: None,
        },
    )
    .await
    .unwrap();

    db.expire_user_subscription(&user_id).await.unwrap();

    let user = db.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert_eq!(user.subscription_tier.as_str(), "personal");
    assert_eq!(user.subscription_status, "expired");
    assert_eq!(user.stripe_subscription_id, None);
}

#[tokio::test]
async fn stripe_webhook_events_are_idempotent_and_ordered() {
    let (db, _temp_dir) = create_test_db().await;
    let user_id = db
        .create_user(
            "subscriber@example.com",
            "hashedpassword",
            None,
            true,
            None,
            None,
        )
        .await
        .unwrap();

    let (first_claim, duplicate_claim) = tokio::join!(
        db.claim_stripe_webhook_event("evt_1", 200, "customer.subscription.updated"),
        db.claim_stripe_webhook_event("evt_1", 200, "customer.subscription.updated"),
    );
    let claim_token = first_claim.unwrap().or(duplicate_claim.unwrap()).unwrap();
    assert!(db
        .complete_stripe_webhook_event("evt_1", &claim_token)
        .await
        .unwrap());
    assert!(db.is_stripe_webhook_event_complete("evt_1").await.unwrap());
    assert!(db
        .claim_stripe_webhook_event("evt_1", 200, "customer.subscription.updated")
        .await
        .unwrap()
        .is_none());

    let failed_claim = db
        .claim_stripe_webhook_event("evt_failed", 200, "customer.subscription.updated")
        .await
        .unwrap()
        .unwrap();
    assert!(db
        .fail_stripe_webhook_event("evt_failed", &failed_claim)
        .await
        .unwrap());
    assert!(db
        .claim_stripe_webhook_event("evt_failed", 200, "customer.subscription.updated")
        .await
        .unwrap()
        .is_some());

    assert!(!db.trial_ending_email_was_sent("evt_failed").await.unwrap());
    assert!(db.mark_trial_ending_email_sent("evt_failed").await.unwrap());
    assert!(db.trial_ending_email_was_sent("evt_failed").await.unwrap());
    assert!(!db.mark_trial_ending_email_sent("evt_failed").await.unwrap());

    let newer = SubscriptionUpdateParams {
        subscription_tier: "personal",
        subscription_status: "active",
        stripe_subscription_id: Some("sub_new"),
        subscription_started_at: None,
        subscription_ends_at: None,
        trial_ends_at: None,
    };
    let older = SubscriptionUpdateParams {
        subscription_tier: "team",
        subscription_status: "expired",
        stripe_subscription_id: None,
        subscription_started_at: None,
        subscription_ends_at: None,
        trial_ends_at: None,
    };
    assert!(db
        .update_user_subscription_for_stripe_event(&user_id, &newer, 200, "evt_newer")
        .await
        .unwrap());
    assert!(!db
        .update_user_subscription_for_stripe_event(&user_id, &older, 199, "evt_older")
        .await
        .unwrap());
    let user = db.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert_eq!(user.subscription_tier.as_str(), "personal");
    assert_eq!(user.subscription_status, "active");
    assert!(db
        .update_user_subscription_for_stripe_event(&user_id, &older, 200, "evt_same_second")
        .await
        .unwrap());
    assert!(db
        .update_user_subscription_for_stripe_event(&user_id, &newer, 200, "evt_newer")
        .await
        .unwrap());

    let user = db.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert_eq!(user.subscription_tier.as_str(), "personal");
    assert_eq!(user.subscription_status, "active");
}
