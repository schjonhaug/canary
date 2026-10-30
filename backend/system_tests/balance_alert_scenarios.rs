use canary::metadata::{BalanceAlert, BalanceAlertType};
use canary::utils::amount::{sats_to_btc, sats_to_i64};

mod common;
use common::docker_environment::IsolatedTestEnvironment;

// System tests for balance alert scenarios.
//
// These tests verify the complete balance alert system using real Bitcoin transactions
// in Docker-based regtest environment. They test actual balance detection, alert triggering,
// and notification delivery.

/// Helper function to get wallet balance from the wallet manager
async fn get_wallet_balance(
    env: &IsolatedTestEnvironment,
    checksum: &str,
) -> Result<i64, Box<dyn std::error::Error>> {
    let wallets_lock = env.wallet_manager.wallets.lock().await;
    if let Some(wallet_info) = wallets_lock.get(checksum) {
        let wallet_guard = wallet_info.lock().await;
        let balance = wallet_guard.0.balance();
        Ok(sats_to_i64(balance.total().to_sat()))
    } else {
        Err(format!("Wallet {checksum} not found").into())
    }
}

/// Helper function to create a balance alert
async fn setup_balance_alert(
    env: &IsolatedTestEnvironment,
    checksum: &str,
    threshold: i64,
    alert_type: BalanceAlertType,
) -> Result<BalanceAlert, Box<dyn std::error::Error>> {
    let alert = env
        .metadata_db
        .create_balance_alert(checksum, threshold, alert_type, None, None, None)
        .await?;
    Ok(alert)
}

/// Test 1: Balance Alert Below Threshold
/// Purpose: Test balance alert triggers when wallet balance drops below threshold
#[tokio::test]
#[ignore = "requires the Docker regtest environment; run via run-system-tests.sh"]
async fn test_balance_alert_below_threshold() {
    let mut env = IsolatedTestEnvironment::new()
        .await
        .expect("Failed to create test environment");

    // Initial sync to establish baseline
    env.sync_and_wait().await.expect("Failed to sync");

    // Create balance alert: trigger when balance < 0.5 BTC (50,000,000 sats)
    let threshold_sats = 50_000_000; // 0.5 BTC
    let alert = setup_balance_alert(
        &env,
        &env.alice_checksum,
        threshold_sats,
        BalanceAlertType::Below,
    )
    .await
    .expect("Failed to create balance alert");

    // Alice sends 0.6 BTC to Bob (should trigger below 0.5 BTC alert)
    eprintln!("Alice sends 0.6 BTC to Bob");
    let _txid = env
        .send_transaction("alice", "bob", "0.6")
        .expect("Failed to send transaction");

    // Mine block to confirm transaction
    env.mine_blocks(1).await.expect("Failed to mine blocks");

    // Sync wallets to detect balance change
    env.sync_and_wait_with_retries(3)
        .await
        .expect("Failed to sync with retries");

    // Get Alice's new balance
    let new_balance = get_wallet_balance(&env, &env.alice_checksum)
        .await
        .expect("Failed to get Alice balance");

    // Check if alert was triggered during sync (it will be deactivated after triggering)
    let updated_alert = env
        .metadata_db
        .get_all_balance_alerts_for_wallet(&env.alice_checksum)
        .await
        .expect("Failed to get all alerts")
        .into_iter()
        .find(|a| a.id == alert.id)
        .expect("Alert not found in database");

    // Verify alert was triggered (it should be deactivated and have a last_triggered_at timestamp)
    assert!(
        !updated_alert.is_active,
        "Alert should be deactivated after triggering"
    );
    assert!(
        updated_alert.last_triggered_at.is_some(),
        "Alert should have a last_triggered_at timestamp"
    );

    // Verify balance is actually below threshold
    assert!(
        new_balance < threshold_sats,
        "Balance {new_balance} should be below threshold {threshold_sats}"
    );
}

/// Test 2: Balance Alert Above Threshold
/// Purpose: Test balance alert triggers when wallet balance rises above threshold
#[tokio::test]
#[ignore = "requires the Docker regtest environment; run via run-system-tests.sh"]
async fn test_balance_alert_above_threshold() {
    let mut env = IsolatedTestEnvironment::new()
        .await
        .expect("Failed to create test environment");

    // Initial sync to establish baseline
    env.sync_and_wait().await.expect("Failed to sync");

    // Create balance alert: trigger when balance > 0.2 BTC (20,000,000 sats)
    let threshold_sats = 20_000_000; // 0.2 BTC
    let alert = setup_balance_alert(
        &env,
        &env.bob_checksum,
        threshold_sats,
        BalanceAlertType::Above,
    )
    .await
    .expect("Failed to create balance alert");

    // Alice sends 0.3 BTC to Bob (should trigger above 0.2 BTC alert)
    eprintln!("Alice sends 0.3 BTC to Bob");
    let _txid = env
        .send_transaction("alice", "bob", "0.3")
        .expect("Failed to send transaction");

    // Mine block to confirm transaction
    env.mine_blocks(1).await.expect("Failed to mine blocks");

    // Sync wallets to detect balance change
    env.sync_and_wait_with_retries(3)
        .await
        .expect("Failed to sync with retries");

    // Get Bob's new balance
    let new_balance = get_wallet_balance(&env, &env.bob_checksum)
        .await
        .expect("Failed to get Bob balance");

    // Check if alert was triggered during sync
    let updated_alert = env
        .metadata_db
        .get_all_balance_alerts_for_wallet(&env.bob_checksum)
        .await
        .expect("Failed to get all alerts")
        .into_iter()
        .find(|a| a.id == alert.id)
        .expect("Alert not found in database");

    // Verify alert was triggered
    assert!(
        !updated_alert.is_active,
        "Alert should be deactivated after triggering"
    );
    assert!(
        updated_alert.last_triggered_at.is_some(),
        "Alert should have a last_triggered_at timestamp"
    );

    // Verify balance is actually above threshold
    assert!(
        new_balance > threshold_sats,
        "Balance {new_balance} should be above threshold {threshold_sats}"
    );
}

/// Test 3: Balance Drain Alert (Equals Zero)
/// Purpose: Test balance alert triggers when wallet is completely drained
#[tokio::test]
#[ignore = "requires the Docker regtest environment; run via run-system-tests.sh"]
async fn test_balance_drain_alert_equals_zero() {
    let mut env = IsolatedTestEnvironment::new()
        .await
        .expect("Failed to create test environment");

    // Initial sync to establish baseline
    env.sync_and_wait().await.expect("Failed to sync");

    // Alice sends a small amount to Bob first to give Bob some funds
    eprintln!("Setting up: Alice sends 0.1 BTC to Bob");
    let _setup_txid = env
        .send_transaction("alice", "bob", "0.1")
        .expect("Failed to send setup transaction");

    env.mine_blocks(1)
        .await
        .expect("Failed to mine setup block");
    env.sync_and_wait()
        .await
        .expect("Failed to sync after setup");

    // Get Bob's balance after receiving funds
    let bob_balance = get_wallet_balance(&env, &env.bob_checksum)
        .await
        .expect("Failed to get Bob balance");

    // Create balance alert: trigger when Bob's balance equals 0
    let threshold_sats = 0;
    let alert = setup_balance_alert(
        &env,
        &env.bob_checksum,
        threshold_sats,
        BalanceAlertType::Equals,
    )
    .await
    .expect("Failed to create balance alert");

    // Bob sends almost all his funds back to Alice (leaving a bit for fees)
    // We'll send slightly less than the balance to account for transaction fees
    eprintln!("Bob drains wallet by sending funds back to Alice");
    let send_amount = bob_balance - 1000; // Leave 1,000 sats for fees (should result in small balance)
    let send_amount_btc = format!("{:.8}", sats_to_btc(send_amount));
    let _drain_txid = env
        .send_transaction("bob", "alice", &send_amount_btc)
        .expect("Failed to send drain transaction");

    // Mine block to confirm transaction
    env.mine_blocks(1).await.expect("Failed to mine blocks");

    // Sync wallets to detect balance change
    env.sync_and_wait_with_retries(3)
        .await
        .expect("Failed to sync with retries");

    // Get Bob's new balance (should be 0 or very close to 0 after fees)
    let final_balance = get_wallet_balance(&env, &env.bob_checksum)
        .await
        .expect("Failed to get Bob balance");

    // Check if alert was triggered during sync
    let updated_alert = env
        .metadata_db
        .get_all_balance_alerts_for_wallet(&env.bob_checksum)
        .await
        .expect("Failed to get all alerts")
        .into_iter()
        .find(|a| a.id == alert.id)
        .expect("Alert not found in database");

    // Verify alert behavior based on final balance
    // Note: Due to transaction fees, balance is typically a few hundred sats, not exactly 0
    if final_balance == 0 {
        // If balance is exactly 0 (rare), alert should trigger
        assert!(
            !updated_alert.is_active,
            "Alert should be deactivated after triggering"
        );
        assert!(
            updated_alert.last_triggered_at.is_some(),
            "Alert should have a last_triggered_at timestamp"
        );
    } else if final_balance > 0 && final_balance < 10000 {
        // More common case: small balance remains due to fees
        assert!(
            updated_alert.is_active,
            "Alert should remain active when balance is not exactly 0"
        );
        assert!(
            updated_alert.last_triggered_at.is_none(),
            "Alert should not have triggered"
        );
    } else {
        panic!(
            "Unexpected final balance: {final_balance} sats (should be near 0, got {final_balance} which is > 10,000)"
        );
    }
}

/// Test 4: Multiple Alerts Test
/// Purpose: Test multiple balance alerts on same wallet trigger independently
#[tokio::test]
#[ignore = "requires the Docker regtest environment; run via run-system-tests.sh"]
async fn test_multiple_balance_alerts() {
    let mut env = IsolatedTestEnvironment::new()
        .await
        .expect("Failed to create test environment");

    // Initial sync to establish baseline
    env.sync_and_wait().await.expect("Failed to sync");

    // Create multiple alerts for Alice's wallet
    let above_alert = setup_balance_alert(
        &env,
        &env.alice_checksum,
        80_000_000, // Above 0.8 BTC
        BalanceAlertType::Above,
    )
    .await
    .expect("Failed to create above alert");

    let below_alert = setup_balance_alert(
        &env,
        &env.alice_checksum,
        30_000_000, // Below 0.3 BTC
        BalanceAlertType::Below,
    )
    .await
    .expect("Failed to create below alert");

    let equals_alert = setup_balance_alert(
        &env,
        &env.alice_checksum,
        0, // Equals 0 (won't trigger in this test)
        BalanceAlertType::Equals,
    )
    .await
    .expect("Failed to create equals alert");

    // Trigger a small transaction to cause balance changes and alert checking
    // (Balance alert checking only happens when there are wallet changes during sync)
    eprintln!("Alice sends small amount to Bob to trigger alert checking");
    let _small_txid = env
        .send_transaction("alice", "bob", "0.01")
        .expect("Failed to send small transaction");

    env.mine_blocks(1).await.expect("Failed to mine blocks");
    env.sync_and_wait()
        .await
        .expect("Failed to sync after small transaction");

    // Alice sends large amount to drop below 0.3 BTC
    eprintln!("Alice sends 0.8 BTC to Bob to trigger below alert");
    let _txid = env
        .send_transaction("alice", "bob", "0.8")
        .expect("Failed to send transaction");

    env.mine_blocks(1).await.expect("Failed to mine blocks");
    env.sync_and_wait_with_retries(3)
        .await
        .expect("Failed to sync with retries");

    // Check all alert statuses after both syncs
    let all_alerts = env
        .metadata_db
        .get_all_balance_alerts_for_wallet(&env.alice_checksum)
        .await
        .expect("Failed to get all alerts");

    let above_updated = all_alerts
        .iter()
        .find(|a| a.id == above_alert.id)
        .expect("Above alert not found");
    let below_updated = all_alerts
        .iter()
        .find(|a| a.id == below_alert.id)
        .expect("Below alert not found");
    let equals_updated = all_alerts
        .iter()
        .find(|a| a.id == equals_alert.id)
        .expect("Equals alert not found");

    // Verify correct alerts were triggered
    assert!(
        !above_updated.is_active && above_updated.last_triggered_at.is_some(),
        "Above alert should have triggered with initial balance"
    );
    assert!(
        !below_updated.is_active && below_updated.last_triggered_at.is_some(),
        "Below alert should have triggered after send"
    );
    assert!(
        equals_updated.is_active && equals_updated.last_triggered_at.is_none(),
        "Equals alert should NOT have triggered"
    );
}

/// Test 5: Alert Deactivation Test
/// Purpose: Test deactivated alerts don't trigger
#[tokio::test]
#[ignore = "requires the Docker regtest environment; run via run-system-tests.sh"]
async fn test_balance_alert_deactivation() {
    let mut env = IsolatedTestEnvironment::new()
        .await
        .expect("Failed to create test environment");

    // Initial sync to establish baseline
    env.sync_and_wait().await.expect("Failed to sync");

    // Create balance alert
    let alert = setup_balance_alert(
        &env,
        &env.alice_checksum,
        50_000_000, // Below 0.5 BTC
        BalanceAlertType::Below,
    )
    .await
    .expect("Failed to create balance alert");

    // Deactivate the alert
    env.metadata_db
        .deactivate_balance_alert(&alert.id)
        .await
        .expect("Failed to deactivate alert");

    // Alice sends 0.6 BTC to Bob (would normally trigger alert)
    eprintln!("Alice sends 0.6 BTC to Bob (should NOT trigger deactivated alert)");
    let _txid = env
        .send_transaction("alice", "bob", "0.6")
        .expect("Failed to send transaction");

    env.mine_blocks(1).await.expect("Failed to mine blocks");
    env.sync_and_wait_with_retries(3)
        .await
        .expect("Failed to sync with retries");

    // Get Alice's new balance
    let new_balance = get_wallet_balance(&env, &env.alice_checksum)
        .await
        .expect("Failed to get Alice balance");

    // Check alert status (should remain deactivated and not triggered)
    let updated_alert = env
        .metadata_db
        .get_all_balance_alerts_for_wallet(&env.alice_checksum)
        .await
        .expect("Failed to get all alerts")
        .into_iter()
        .find(|a| a.id == alert.id)
        .expect("Alert not found in database");

    // Verify alert remained deactivated and didn't trigger
    assert!(!updated_alert.is_active, "Alert should remain deactivated");
    assert!(
        updated_alert.last_triggered_at.is_none(),
        "Deactivated alert should not have triggered"
    );

    // Verify balance would have triggered if alert was active
    assert!(
        new_balance < 50_000_000,
        "Balance {new_balance} should be below threshold (would trigger if active)"
    );
}
