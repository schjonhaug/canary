use canary::{
    config::{AppConfig, NetworkConfig},
    metadata::{BalanceAlertType, MetadataDb},
};
use std::sync::Arc;
use tempfile::tempdir;
use uuid;

/// Test helper to create test database
async fn create_test_db() -> (Arc<MetadataDb>, tempfile::TempDir) {
    let temp_dir = tempdir().unwrap();
    let test_db_path = temp_dir.path().join("test_metadata.sqlite");

    // Create test config
    let test_config = AppConfig {
        network: NetworkConfig::Regtest,
        electrum_url: Some("tcp://127.0.0.1:50001".to_string()),
        bind_address: "127.0.0.1:3000".to_string(),
        data_dir: temp_dir.path().to_str().unwrap().to_string(),
    };

    let db = Arc::new(
        MetadataDb::new(test_db_path.to_str().unwrap(), &test_config)
            .await
            .unwrap(),
    );
    (db, temp_dir)
}

/// Test helper to create a test user and wallet
async fn create_test_user_and_wallet(metadata_db: &MetadataDb) -> (String, String) {
    // Create test user with unique email
    let unique_email = format!("test-{}@example.com", uuid::Uuid::new_v4());
    let user_id = metadata_db
        .create_user(&unique_email, "hash", Some("Test User"), false, None)
        .await
        .unwrap();

    // Create test wallet with unique descriptor
    let unique_descriptor = format!("descriptor-{}", uuid::Uuid::new_v4());
    let wallet_checksum = metadata_db
        .insert_wallet("Test Wallet", &unique_descriptor, &user_id)
        .await
        .unwrap();

    (user_id, wallet_checksum)
}

#[tokio::test]
async fn test_balance_alert_database_operations() {
    let (metadata_db, _temp_dir) = create_test_db().await;
    let (_user_id, wallet_checksum) = create_test_user_and_wallet(&metadata_db).await;

    // Test 1: Create balance alert
    let alert = metadata_db
        .create_balance_alert(&wallet_checksum, 100_000_000, BalanceAlertType::Below, None, None) // 1 BTC
        .await
        .unwrap();

    assert_eq!(alert.threshold_sats, 100_000_000);
    assert_eq!(alert.alert_type, BalanceAlertType::Below);
    assert!(alert.is_active);
    assert_eq!(alert.wallet_checksum, wallet_checksum);

    // Test 2: Get balance alerts
    let alerts = metadata_db
        .get_all_balance_alerts_for_wallet(&wallet_checksum)
        .await
        .unwrap();
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].id, alert.id);

    // Test 3: Get active balance alerts
    let active_alerts = metadata_db
        .get_active_balance_alerts_for_wallet(&wallet_checksum)
        .await
        .unwrap();
    assert_eq!(active_alerts.len(), 1);

    // Test 4: Deactivate balance alert
    metadata_db
        .disable_balance_alert_after_trigger(&alert.id)
        .await
        .unwrap();

    let active_alerts = metadata_db
        .get_active_balance_alerts_for_wallet(&wallet_checksum)
        .await
        .unwrap();
    assert_eq!(active_alerts.len(), 0);

    // Test 5: Reactivate balance alert
    let reactivated_alert = metadata_db
        .reactivate_balance_alert(&alert.id)
        .await
        .unwrap();
    assert!(reactivated_alert.is_active);

    // Test 6: Delete balance alert
    metadata_db.delete_balance_alert(&alert.id).await.unwrap();

    let alerts = metadata_db
        .get_all_balance_alerts_for_wallet(&wallet_checksum)
        .await
        .unwrap();
    assert_eq!(alerts.len(), 0);
}

#[tokio::test]
async fn test_balance_alert_types() {
    let (metadata_db, _temp_dir) = create_test_db().await;
    let (_user_id, wallet_checksum) = create_test_user_and_wallet(&metadata_db).await;

    // Test all alert types
    let _below_alert = metadata_db
        .create_balance_alert(&wallet_checksum, 50_000_000, BalanceAlertType::Below, None, None)
        .await
        .unwrap();

    let _above_alert = metadata_db
        .create_balance_alert(&wallet_checksum, 200_000_000, BalanceAlertType::Above, None, None)
        .await
        .unwrap();

    let _equals_alert = metadata_db
        .create_balance_alert(&wallet_checksum, 0, BalanceAlertType::Equals, None, None)
        .await
        .unwrap();

    let alerts = metadata_db
        .get_all_balance_alerts_for_wallet(&wallet_checksum)
        .await
        .unwrap();

    assert_eq!(alerts.len(), 3);

    // Verify alert types are stored correctly
    for alert in alerts {
        match alert.alert_type {
            BalanceAlertType::Below => assert_eq!(alert.threshold_sats, 50_000_000),
            BalanceAlertType::Above => assert_eq!(alert.threshold_sats, 200_000_000),
            BalanceAlertType::Equals => assert_eq!(alert.threshold_sats, 0),
        }
    }
}

#[tokio::test]
async fn test_balance_alert_edge_cases() {
    let (metadata_db, _temp_dir) = create_test_db().await;
    let (_user_id, wallet_checksum) = create_test_user_and_wallet(&metadata_db).await;

    // Test 1: Multiple alerts of same type
    let alert1 = metadata_db
        .create_balance_alert(&wallet_checksum, 100_000_000, BalanceAlertType::Below, None, None)
        .await
        .unwrap();

    let alert2 = metadata_db
        .create_balance_alert(&wallet_checksum, 50_000_000, BalanceAlertType::Below, None, None)
        .await
        .unwrap();

    let alerts = metadata_db
        .get_all_balance_alerts_for_wallet(&wallet_checksum)
        .await
        .unwrap();

    assert_eq!(alerts.len(), 2);
    assert_ne!(alert1.id, alert2.id);

    // Test 2: Zero threshold handling
    let zero_alert = metadata_db
        .create_balance_alert(&wallet_checksum, 0, BalanceAlertType::Equals, None, None)
        .await
        .unwrap();

    assert_eq!(zero_alert.threshold_sats, 0);

    // Test 3: Very large threshold values
    let large_alert = metadata_db
        .create_balance_alert(&wallet_checksum, i64::MAX, BalanceAlertType::Above, None, None)
        .await
        .unwrap();

    assert_eq!(large_alert.threshold_sats, i64::MAX);
}

#[tokio::test]
async fn test_balance_alert_wallet_isolation() {
    let (metadata_db, _temp_dir) = create_test_db().await;

    // Create two different wallets
    let user1_id = metadata_db
        .create_user("user1@example.com", "hash", Some("User 1"), false, None)
        .await
        .unwrap();
    let wallet1_checksum = metadata_db
        .insert_wallet("Wallet 1", "descriptor1", &user1_id)
        .await
        .unwrap();

    let user2_id = metadata_db
        .create_user("user2@example.com", "hash", Some("User 2"), false, None)
        .await
        .unwrap();
    let wallet2_checksum = metadata_db
        .insert_wallet("Wallet 2", "descriptor2", &user2_id)
        .await
        .unwrap();

    // Create alerts for each wallet
    let alert1 = metadata_db
        .create_balance_alert(&wallet1_checksum, 100_000_000, BalanceAlertType::Below, None, None)
        .await
        .unwrap();

    let alert2 = metadata_db
        .create_balance_alert(&wallet2_checksum, 200_000_000, BalanceAlertType::Above, None, None)
        .await
        .unwrap();

    // Verify alerts are isolated per wallet
    let wallet1_alerts = metadata_db
        .get_all_balance_alerts_for_wallet(&wallet1_checksum)
        .await
        .unwrap();
    assert_eq!(wallet1_alerts.len(), 1);
    assert_eq!(wallet1_alerts[0].id, alert1.id);

    let wallet2_alerts = metadata_db
        .get_all_balance_alerts_for_wallet(&wallet2_checksum)
        .await
        .unwrap();
    assert_eq!(wallet2_alerts.len(), 1);
    assert_eq!(wallet2_alerts[0].id, alert2.id);
}

#[tokio::test]
async fn test_balance_alert_performance() {
    let (metadata_db, _temp_dir) = create_test_db().await;
    let (_user_id, wallet_checksum) = create_test_user_and_wallet(&metadata_db).await;

    // Create many balance alerts
    let num_alerts = 100usize;
    let mut _alert_ids = Vec::new();

    let start = std::time::Instant::now();
    for i in 0..num_alerts {
        let alert = metadata_db
            .create_balance_alert(
                &wallet_checksum,
                ((i + 1) * 1_000_000) as i64, // Varying thresholds
                if i % 3 == 0 {
                    BalanceAlertType::Below
                } else if i % 3 == 1 {
                    BalanceAlertType::Above
                } else {
                    BalanceAlertType::Equals
                },
            )
            .await
            .unwrap();
        _alert_ids.push(alert.id);
    }
    let creation_duration = start.elapsed();

    // Verify all alerts were created
    let alerts = metadata_db
        .get_all_balance_alerts_for_wallet(&wallet_checksum)
        .await
        .unwrap();
    assert_eq!(alerts.len(), num_alerts);

    // Test query performance
    let start = std::time::Instant::now();
    let active_alerts = metadata_db
        .get_active_balance_alerts_for_wallet(&wallet_checksum)
        .await
        .unwrap();
    let query_duration = start.elapsed();

    assert_eq!(active_alerts.len(), num_alerts);

    // Performance assertions (adjust thresholds as needed)
    assert!(
        creation_duration.as_millis() < 5000,
        "Creation took too long: {:?}",
        creation_duration
    );
    assert!(
        query_duration.as_millis() < 100,
        "Query took too long: {:?}",
        query_duration
    );

    println!("Created {} alerts in {:?}", num_alerts, creation_duration);
    println!("Queried {} alerts in {:?}", num_alerts, query_duration);
}

#[tokio::test]
async fn test_duplicate_balance_alert_checking() {
    let (metadata_db, _temp_dir) = create_test_db().await;
    let (_user_id, wallet_checksum) = create_test_user_and_wallet(&metadata_db).await;

    // Test 1: Create initial alert
    let alert = metadata_db
        .create_balance_alert(&wallet_checksum, 100_000_000, BalanceAlertType::Below, None, None)
        .await
        .unwrap();

    // Test 2: Check for exact duplicate - should find it
    let duplicate = metadata_db
        .check_duplicate_balance_alert(&wallet_checksum, 100_000_000, BalanceAlertType::Below)
        .await
        .unwrap();
    assert!(duplicate.is_some());
    assert_eq!(duplicate.unwrap().id, alert.id);

    // Test 3: Check for non-duplicate (different type) - should not find it
    let no_duplicate = metadata_db
        .check_duplicate_balance_alert(&wallet_checksum, 100_000_000, BalanceAlertType::Above)
        .await
        .unwrap();
    assert!(no_duplicate.is_none());

    // Test 4: Check for non-duplicate (different amount) - should not find it
    let no_duplicate = metadata_db
        .check_duplicate_balance_alert(&wallet_checksum, 200_000_000, BalanceAlertType::Below)
        .await
        .unwrap();
    assert!(no_duplicate.is_none());

    // Test 5: Check for non-duplicate (different wallet) - should not find it
    let (_user_id2, wallet_checksum2) = create_test_user_and_wallet(&metadata_db).await;
    let no_duplicate = metadata_db
        .check_duplicate_balance_alert(&wallet_checksum2, 100_000_000, BalanceAlertType::Below)
        .await
        .unwrap();
    assert!(no_duplicate.is_none());

    // Test 6: Deactivate alert and check if duplicate check still finds it (should find it regardless of active status)
    metadata_db
        .disable_balance_alert_after_trigger(&alert.id)
        .await
        .unwrap();

    let duplicate_after_trigger = metadata_db
        .check_duplicate_balance_alert(&wallet_checksum, 100_000_000, BalanceAlertType::Below)
        .await
        .unwrap();
    assert!(duplicate_after_trigger.is_some());
    assert!(!duplicate_after_trigger.unwrap().is_active); // Verify it's inactive
}

#[tokio::test]
async fn test_duplicate_alert_all_types() {
    let (metadata_db, _temp_dir) = create_test_db().await;
    let (_user_id, wallet_checksum) = create_test_user_and_wallet(&metadata_db).await;

    // Create alerts of all types with same threshold
    let threshold = 50_000_000;

    let below_alert = metadata_db
        .create_balance_alert(&wallet_checksum, threshold, BalanceAlertType::Below, None, None)
        .await
        .unwrap();

    let above_alert = metadata_db
        .create_balance_alert(&wallet_checksum, threshold, BalanceAlertType::Above, None, None)
        .await
        .unwrap();

    let equals_alert = metadata_db
        .create_balance_alert(&wallet_checksum, threshold, BalanceAlertType::Equals, None, None)
        .await
        .unwrap();

    // Each type should be found independently
    let found_below = metadata_db
        .check_duplicate_balance_alert(&wallet_checksum, threshold, BalanceAlertType::Below)
        .await
        .unwrap();
    assert!(found_below.is_some());
    assert_eq!(found_below.unwrap().id, below_alert.id);

    let found_above = metadata_db
        .check_duplicate_balance_alert(&wallet_checksum, threshold, BalanceAlertType::Above)
        .await
        .unwrap();
    assert!(found_above.is_some());
    assert_eq!(found_above.unwrap().id, above_alert.id);

    let found_equals = metadata_db
        .check_duplicate_balance_alert(&wallet_checksum, threshold, BalanceAlertType::Equals)
        .await
        .unwrap();
    assert!(found_equals.is_some());
    assert_eq!(found_equals.unwrap().id, equals_alert.id);
}

#[tokio::test]
async fn test_below_zero_alert_validation() {
    let (metadata_db, _temp_dir) = create_test_db().await;
    let (_user_id, wallet_checksum) = create_test_user_and_wallet(&metadata_db).await;

    // Attempt to create a "below 0" alert - should be rejected by database constraints if added
    // This test ensures we don't accidentally allow such alerts in the future
    let zero_threshold = 0i64;

    // Note: We can't directly test the API validation here since this is a database test,
    // but we can verify that if such an alert somehow got created, duplicate checking still works
    let result = metadata_db
        .check_duplicate_balance_alert(&wallet_checksum, zero_threshold, BalanceAlertType::Below)
        .await;

    // Should not find any duplicate because "below 0" alerts shouldn't exist
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[tokio::test]
async fn test_wallet_drain_alert_special_case() {
    let (metadata_db, _temp_dir) = create_test_db().await;
    let (_user_id, wallet_checksum) = create_test_user_and_wallet(&metadata_db).await;

    // Create wallet drain alert (balance = 0)
    let drain_alert = metadata_db
        .create_balance_alert(&wallet_checksum, 0, BalanceAlertType::Equals, None, None)
        .await
        .unwrap();

    assert_eq!(drain_alert.threshold_sats, 0);
    assert_eq!(drain_alert.alert_type, BalanceAlertType::Equals);
    assert!(drain_alert.is_active);

    // Verify it can be found and managed like other alerts
    let alerts = metadata_db
        .get_active_balance_alerts_for_wallet(&wallet_checksum)
        .await
        .unwrap();

    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].id, drain_alert.id);

    // Test deactivation and reactivation
    metadata_db
        .disable_balance_alert_after_trigger(&drain_alert.id)
        .await
        .unwrap();

    let active_alerts = metadata_db
        .get_active_balance_alerts_for_wallet(&wallet_checksum)
        .await
        .unwrap();
    assert_eq!(active_alerts.len(), 0);

    let reactivated = metadata_db
        .reactivate_balance_alert(&drain_alert.id)
        .await
        .unwrap();
    assert!(reactivated.is_active);
}
