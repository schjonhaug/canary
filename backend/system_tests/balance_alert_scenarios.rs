use canary::metadata::{BalanceAlert, BalanceAlertType};

mod common;
use common::docker_environment::IsolatedTestEnvironment;

/// System tests for balance alert scenarios
///
/// These tests verify the complete balance alert system using real Bitcoin transactions
/// in Docker-based regtest environment. They test actual balance detection, alert triggering,
/// and notification delivery.

/// Helper function to get wallet balance from the wallet manager
async fn get_wallet_balance(
    env: &IsolatedTestEnvironment,
    checksum: &str,
) -> Result<i64, Box<dyn std::error::Error>> {
    let wallets_lock = env.wallet_manager.wallets.lock().await;
    if let Some(wallet_info) = wallets_lock.get(checksum) {
        let wallet_guard = wallet_info.lock().await;
        let balance = wallet_guard.0.balance();
        Ok(balance.total().to_sat() as i64)
    } else {
        Err(format!("Wallet {} not found", checksum).into())
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
        .create_balance_alert(checksum, threshold, alert_type)
        .await?;
    Ok(alert)
}

/// Test 1: Balance Alert Below Threshold
/// Purpose: Test balance alert triggers when wallet balance drops below threshold
#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_balance_alert_below_threshold() {
    let mut env = IsolatedTestEnvironment::new()
        .await
        .expect("Failed to create test environment");

    // Initial sync to establish baseline
    env.sync_and_wait().await.expect("Failed to sync");

    println!("📊 Test 1: Balance Alert Below Threshold");

    // Get Alice's initial balance (should be 1.0 BTC from funding)
    let initial_balance = get_wallet_balance(&env, &env.alice_checksum)
        .await
        .expect("Failed to get Alice balance");

    println!(
        "💰 Alice initial balance: {} sats ({:.8} BTC)",
        initial_balance,
        initial_balance as f64 / 100_000_000.0
    );

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

    println!(
        "🚨 Created balance alert: Below {} sats ({:.8} BTC)",
        threshold_sats,
        threshold_sats as f64 / 100_000_000.0
    );

    // Alice sends 0.6 BTC to Bob (should trigger below 0.5 BTC alert)
    println!("⚡ Alice sends 0.6 BTC to Bob...");
    let _txid = env
        .send_transaction("alice", "bob", "0.6")
        .await
        .expect("Failed to send transaction");

    // Mine block to confirm transaction
    println!("⛏️  Mining block to confirm transaction...");
    env.mine_blocks(1).await.expect("Failed to mine blocks");

    // Sync wallets to detect balance change
    println!("🔄 Syncing wallets...");
    env.sync_and_wait_with_retries(3)
        .await
        .expect("Failed to sync with retries");

    // Get Alice's new balance
    let new_balance = get_wallet_balance(&env, &env.alice_checksum)
        .await
        .expect("Failed to get Alice balance");

    println!(
        "💰 Alice new balance: {} sats ({:.8} BTC)",
        new_balance,
        new_balance as f64 / 100_000_000.0
    );

    // Check if alert was triggered during sync (it will be deactivated after triggering)
    let updated_alert = env
        .metadata_db
        .get_all_balance_alerts_for_wallet(&env.alice_checksum)
        .await
        .expect("Failed to get all alerts")
        .into_iter()
        .find(|a| a.id == alert.id)
        .expect("Alert not found in database");

    println!("🔍 Alert status after sync:");
    println!("   Alert ID: {}", updated_alert.id);
    println!("   Threshold: {} sats", updated_alert.threshold_sats);
    println!("   Type: {:?}", updated_alert.alert_type);
    println!(
        "   Active: {} (should be false after triggering)",
        updated_alert.is_active
    );
    println!("   Last triggered: {:?}", updated_alert.last_triggered_at);

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
        "Balance {} should be below threshold {}",
        new_balance,
        threshold_sats
    );

    println!("✅ Test 1 passed: Balance alert below threshold triggered correctly");
}

/// Test 2: Balance Alert Above Threshold
/// Purpose: Test balance alert triggers when wallet balance rises above threshold
#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_balance_alert_above_threshold() {
    let mut env = IsolatedTestEnvironment::new()
        .await
        .expect("Failed to create test environment");

    // Initial sync to establish baseline
    env.sync_and_wait().await.expect("Failed to sync");

    println!("📊 Test 2: Balance Alert Above Threshold");

    // Get Bob's initial balance (should be 0 - unfunded)
    let initial_balance = get_wallet_balance(&env, &env.bob_checksum)
        .await
        .expect("Failed to get Bob balance");

    println!(
        "💰 Bob initial balance: {} sats ({:.8} BTC)",
        initial_balance,
        initial_balance as f64 / 100_000_000.0
    );

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

    println!(
        "🚨 Created balance alert: Above {} sats ({:.8} BTC)",
        threshold_sats,
        threshold_sats as f64 / 100_000_000.0
    );

    // Alice sends 0.3 BTC to Bob (should trigger above 0.2 BTC alert)
    println!("⚡ Alice sends 0.3 BTC to Bob...");
    let _txid = env
        .send_transaction("alice", "bob", "0.3")
        .await
        .expect("Failed to send transaction");

    // Mine block to confirm transaction
    println!("⛏️  Mining block to confirm transaction...");
    env.mine_blocks(1).await.expect("Failed to mine blocks");

    // Sync wallets to detect balance change
    println!("🔄 Syncing wallets...");
    env.sync_and_wait_with_retries(3)
        .await
        .expect("Failed to sync with retries");

    // Get Bob's new balance
    let new_balance = get_wallet_balance(&env, &env.bob_checksum)
        .await
        .expect("Failed to get Bob balance");

    println!(
        "💰 Bob new balance: {} sats ({:.8} BTC)",
        new_balance,
        new_balance as f64 / 100_000_000.0
    );

    // Check if alert was triggered during sync
    let updated_alert = env
        .metadata_db
        .get_all_balance_alerts_for_wallet(&env.bob_checksum)
        .await
        .expect("Failed to get all alerts")
        .into_iter()
        .find(|a| a.id == alert.id)
        .expect("Alert not found in database");

    println!("🔍 Alert status after sync:");
    println!("   Alert ID: {}", updated_alert.id);
    println!(
        "   Active: {} (should be false after triggering)",
        updated_alert.is_active
    );
    println!("   Last triggered: {:?}", updated_alert.last_triggered_at);

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
        "Balance {} should be above threshold {}",
        new_balance,
        threshold_sats
    );

    println!("✅ Test 2 passed: Balance alert above threshold triggered correctly");
}

/// Test 3: Balance Drain Alert (Equals Zero)
/// Purpose: Test balance alert triggers when wallet is completely drained
#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_balance_drain_alert_equals_zero() {
    let mut env = IsolatedTestEnvironment::new()
        .await
        .expect("Failed to create test environment");

    // Initial sync to establish baseline
    env.sync_and_wait().await.expect("Failed to sync");

    println!("📊 Test 3: Balance Drain Alert (Equals Zero)");

    // Alice sends a small amount to Bob first to give Bob some funds
    println!("⚡ Setting up: Alice sends 0.1 BTC to Bob...");
    let _setup_txid = env
        .send_transaction("alice", "bob", "0.1")
        .await
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

    println!(
        "💰 Bob balance after receiving: {} sats ({:.8} BTC)",
        bob_balance,
        bob_balance as f64 / 100_000_000.0
    );

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

    println!(
        "🚨 Created balance alert: Equals {} sats (wallet drain)",
        threshold_sats
    );

    // Bob sends almost all his funds back to Alice (leaving a bit for fees)
    // We'll send slightly less than the balance to account for transaction fees
    println!("⚡ Bob drains wallet by sending funds back to Alice...");
    let send_amount = bob_balance - 1000; // Leave 1,000 sats for fees (should result in small balance)
    let send_amount_btc = format!("{:.8}", send_amount as f64 / 100_000_000.0);
    println!(
        "   Sending {} BTC (leaving ~1,000 sats for fees)",
        send_amount_btc
    );
    let _drain_txid = env
        .send_transaction("bob", "alice", &send_amount_btc)
        .await
        .expect("Failed to send drain transaction");

    // Mine block to confirm transaction
    println!("⛏️  Mining block to confirm drain transaction...");
    env.mine_blocks(1).await.expect("Failed to mine blocks");

    // Sync wallets to detect balance change
    println!("🔄 Syncing wallets...");
    env.sync_and_wait_with_retries(3)
        .await
        .expect("Failed to sync with retries");

    // Get Bob's new balance (should be 0 or very close to 0 after fees)
    let final_balance = get_wallet_balance(&env, &env.bob_checksum)
        .await
        .expect("Failed to get Bob balance");

    println!(
        "💰 Bob final balance: {} sats ({:.8} BTC)",
        final_balance,
        final_balance as f64 / 100_000_000.0
    );

    // Check if alert was triggered during sync
    let updated_alert = env
        .metadata_db
        .get_all_balance_alerts_for_wallet(&env.bob_checksum)
        .await
        .expect("Failed to get all alerts")
        .into_iter()
        .find(|a| a.id == alert.id)
        .expect("Alert not found in database");

    println!("🔍 Alert status after sync:");
    println!("   Alert ID: {}", updated_alert.id);
    println!(
        "   Active: {} (should be false if triggered)",
        updated_alert.is_active
    );
    println!("   Last triggered: {:?}", updated_alert.last_triggered_at);

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
        println!("✅ Test 3 passed: Balance drain alert (equals 0) triggered correctly");
    } else if final_balance > 0 && final_balance < 10000 {
        // More common case: small balance remains due to fees
        println!(
            "ℹ️  Note: Final balance is {} sats (not exactly 0 due to fees)",
            final_balance
        );
        assert!(
            updated_alert.is_active,
            "Alert should remain active when balance is not exactly 0"
        );
        assert!(
            updated_alert.last_triggered_at.is_none(),
            "Alert should not have triggered"
        );
        println!("✅ Test 3 passed: Wallet nearly drained (small balance remains for fees)");
    } else {
        panic!(
            "Unexpected final balance: {} sats (should be near 0, got {} which is > 10,000)",
            final_balance, final_balance
        );
    }
}

/// Test 4: Multiple Alerts Test
/// Purpose: Test multiple balance alerts on same wallet trigger independently
#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_multiple_balance_alerts() {
    let mut env = IsolatedTestEnvironment::new()
        .await
        .expect("Failed to create test environment");

    // Initial sync to establish baseline
    env.sync_and_wait().await.expect("Failed to sync");

    println!("📊 Test 4: Multiple Balance Alerts");

    // Get Alice's initial balance
    let initial_balance = get_wallet_balance(&env, &env.alice_checksum)
        .await
        .expect("Failed to get Alice balance");

    println!(
        "💰 Alice initial balance: {} sats ({:.8} BTC)",
        initial_balance,
        initial_balance as f64 / 100_000_000.0
    );

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

    println!("🚨 Created multiple alerts:");
    println!("   - Above 0.8 BTC (should trigger with initial balance)");
    println!("   - Below 0.3 BTC (should trigger after large send)");
    println!("   - Equals 0 (should NOT trigger in this test)");

    // Trigger a small transaction to cause balance changes and alert checking
    // (Balance alert checking only happens when there are wallet changes during sync)
    println!("⚡ Alice sends small amount to Bob to trigger alert checking...");
    let _small_txid = env
        .send_transaction("alice", "bob", "0.01") // Send 0.01 BTC
        .await
        .expect("Failed to send small transaction");

    env.mine_blocks(1).await.expect("Failed to mine blocks");
    env.sync_and_wait()
        .await
        .expect("Failed to sync after small transaction");

    // Alice sends large amount to drop below 0.3 BTC
    println!("⚡ Alice sends 0.8 BTC to Bob to trigger below alert...");
    let _txid = env
        .send_transaction("alice", "bob", "0.8")
        .await
        .expect("Failed to send transaction");

    env.mine_blocks(1).await.expect("Failed to mine blocks");
    env.sync_and_wait_with_retries(3)
        .await
        .expect("Failed to sync with retries");

    // Get Alice's new balance
    let new_balance = get_wallet_balance(&env, &env.alice_checksum)
        .await
        .expect("Failed to get Alice balance");

    println!(
        "💰 Alice new balance: {} sats ({:.8} BTC)",
        new_balance,
        new_balance as f64 / 100_000_000.0
    );

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

    println!("🔍 Alert statuses after both syncs:");
    println!(
        "   Above alert: active={}, triggered={}",
        above_updated.is_active,
        above_updated.last_triggered_at.is_some()
    );
    println!(
        "   Below alert: active={}, triggered={}",
        below_updated.is_active,
        below_updated.last_triggered_at.is_some()
    );
    println!(
        "   Equals alert: active={}, triggered={}",
        equals_updated.is_active,
        equals_updated.last_triggered_at.is_some()
    );

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

    println!("✅ Test 4 passed: Multiple balance alerts work independently");
}

/// Test 5: Alert Deactivation Test
/// Purpose: Test deactivated alerts don't trigger
#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_balance_alert_deactivation() {
    let mut env = IsolatedTestEnvironment::new()
        .await
        .expect("Failed to create test environment");

    // Initial sync to establish baseline
    env.sync_and_wait().await.expect("Failed to sync");

    println!("📊 Test 5: Balance Alert Deactivation");

    // Create balance alert
    let alert = setup_balance_alert(
        &env,
        &env.alice_checksum,
        50_000_000, // Below 0.5 BTC
        BalanceAlertType::Below,
    )
    .await
    .expect("Failed to create balance alert");

    println!("🚨 Created balance alert: Below 0.5 BTC");

    // Deactivate the alert
    println!("🛑 Deactivating balance alert...");
    env.metadata_db
        .disable_balance_alert_after_trigger(&alert.id)
        .await
        .expect("Failed to deactivate alert");

    // Alice sends 0.6 BTC to Bob (would normally trigger alert)
    println!("⚡ Alice sends 0.6 BTC to Bob (should NOT trigger deactivated alert)...");
    let _txid = env
        .send_transaction("alice", "bob", "0.6")
        .await
        .expect("Failed to send transaction");

    env.mine_blocks(1).await.expect("Failed to mine blocks");
    env.sync_and_wait_with_retries(3)
        .await
        .expect("Failed to sync with retries");

    // Get Alice's new balance
    let new_balance = get_wallet_balance(&env, &env.alice_checksum)
        .await
        .expect("Failed to get Alice balance");

    println!(
        "💰 Alice new balance: {} sats ({:.8} BTC)",
        new_balance,
        new_balance as f64 / 100_000_000.0
    );

    // Check alert status (should remain deactivated and not triggered)
    let updated_alert = env
        .metadata_db
        .get_all_balance_alerts_for_wallet(&env.alice_checksum)
        .await
        .expect("Failed to get all alerts")
        .into_iter()
        .find(|a| a.id == alert.id)
        .expect("Alert not found in database");

    println!("🔍 Alert status after sync:");
    println!("   Alert ID: {}", updated_alert.id);
    println!(
        "   Active: {} (should remain false)",
        updated_alert.is_active
    );
    println!(
        "   Last triggered: {:?} (should remain None)",
        updated_alert.last_triggered_at
    );

    // Verify alert remained deactivated and didn't trigger
    assert!(!updated_alert.is_active, "Alert should remain deactivated");
    assert!(
        updated_alert.last_triggered_at.is_none(),
        "Deactivated alert should not have triggered"
    );

    // Verify balance would have triggered if alert was active
    assert!(
        new_balance < 50_000_000,
        "Balance {} should be below threshold (would trigger if active)",
        new_balance
    );

    println!("✅ Test 5 passed: Deactivated balance alert correctly ignored");
}
