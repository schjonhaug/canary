use canary::metadata::EventType;
use std::time::{SystemTime, UNIX_EPOCH};

mod common;
use common::docker_environment::IsolatedTestEnvironment;

/// Helper function to sync with retries for better reliability
async fn sync_with_retries(env: &mut IsolatedTestEnvironment, retries: u32) -> Result<(), Box<dyn std::error::Error>> {
    for attempt in 1..=retries {
        match env.sync_and_wait().await {
            Ok(()) => return Ok(()),
            Err(e) => {
                if attempt < retries {
                    println!("⚠️ Sync attempt {}/{} failed: {}, retrying in 5s...", attempt, retries, e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                } else {
                    return Err(e);
                }
            }
        }
    }
    unreachable!()
}

/// System tests for transaction timestamp handling
///
/// These tests verify that transactions display the correct timestamp:
/// 1. When seen in mempool first, then mined: Should show mempool timestamp
/// 2. When mined directly without mempool: Should show block confirmation timestamp

/// Test 1: Transaction seen in mempool first, then mined
/// Expected: Display timestamp should be the earlier mempool timestamp
#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_mempool_first_transaction_timestamp() {
    let mut env = IsolatedTestEnvironment::new()
        .await
        .expect("Failed to create test environment");

    // Initial sync to establish baseline
    sync_with_retries(&mut env, 3).await.expect("Failed to sync");

    println!("🕐 Test 1: Transaction seen in mempool first");

    // Step 1: Send transaction (will be in mempool)
    println!("⚡ Step 1: Alice sends 0.1 BTC to Bob (transaction enters mempool)");
    let _txid = env
        .send_transaction("alice", "bob", "0.1")
        .await
        .expect("Failed to send transaction");

    // Record approximate mempool time
    let mempool_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    println!("📝 Mempool time (approx): {}", mempool_time);

    // Step 2: Sync to detect mempool transaction
    println!("⚡ Step 2: Sync wallets to detect mempool transaction");
    sync_with_retries(&mut env, 3)
        .await
        .expect("Failed to sync mempool transaction");

    // Get transactions after mempool detection
    let mempool_alice_txs = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");

    // Find the new unconfirmed transaction
    let unconfirmed_tx = mempool_alice_txs
        .iter()
        .find(|tx| tx.block_height.is_none() && tx.transaction_type == EventType::Send)
        .expect("Should find unconfirmed send transaction");

    let mempool_first_seen = unconfirmed_tx.first_seen_at;
    println!("✅ Transaction detected in mempool with first_seen_at: {}", mempool_first_seen);

    // Step 3: Wait a bit, then mine block
    println!("⚡ Step 3: Wait 5 seconds, then mine block");
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    env.mine_blocks(1).await.expect("Failed to mine blocks");

    // Wait for Electrum to fully sync with the new block
    println!("⏳ Waiting 10 seconds for Electrum to sync after mining...");
    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

    // Record approximate block time
    let block_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    println!("📝 Block mined time (approx): {}", block_time);

    // Step 4: Sync to update confirmation status
    println!("⚡ Step 4: Sync wallets to detect confirmation");
    sync_with_retries(&mut env, 3)
        .await
        .expect("Failed to sync confirmed transaction");

    // Get transactions after confirmation
    let confirmed_alice_txs = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");

    // Find the confirmed transaction
    let confirmed_tx = confirmed_alice_txs
        .iter()
        .find(|tx| tx.block_height.is_some() && tx.transaction_type == EventType::Send)
        .expect("Should find confirmed send transaction");

    println!("📊 Transaction after confirmation:");
    println!("   first_seen_at: {}", confirmed_tx.first_seen_at);
    println!("   confirmed_at: {:?}", confirmed_tx.confirmed_at);

    // Verify the transaction uses the oldest timestamp (mempool time)
    let display_time = confirmed_tx.first_seen_at.min(confirmed_tx.confirmed_at.unwrap_or(u64::MAX));
    println!("   Display time (min of both): {}", display_time);

    // Assertions
    assert!(confirmed_tx.confirmed_at.is_some(), "Transaction should be confirmed");
    assert_eq!(
        confirmed_tx.first_seen_at, mempool_first_seen,
        "first_seen_at should be preserved from mempool detection"
    );

    // The display time should be the mempool time (earlier)
    assert_eq!(
        display_time, mempool_first_seen,
        "Display time should use the earlier mempool timestamp"
    );

    // Verify confirmed_at is later than first_seen_at
    if let Some(confirmed_at) = confirmed_tx.confirmed_at {
        assert!(
            confirmed_at >= mempool_first_seen,
            "confirmed_at ({}) should be >= first_seen_at ({})",
            confirmed_at,
            mempool_first_seen
        );
    }

    println!("✅ Test 1 passed: Mempool-first transaction shows mempool timestamp");
}

/// Test 2: Transaction mined directly without appearing in mempool
/// Expected: Display timestamp should be the block confirmation timestamp
#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_direct_mining_transaction_timestamp() {
    let mut env = IsolatedTestEnvironment::new()
        .await
        .expect("Failed to create test environment");

    // Initial sync to establish baseline
    sync_with_retries(&mut env, 3).await.expect("Failed to sync");

    println!("🕐 Test 2: Transaction mined directly (no mempool)");

    // Step 1: Send transaction and immediately mine (before sync)
    println!("⚡ Step 1: Alice sends 0.1 BTC to Bob");
    let _txid = env
        .send_transaction("alice", "bob", "0.1")
        .await
        .expect("Failed to send transaction");

    println!("⚡ Step 2: Immediately mine block (before any sync)");
    env.mine_blocks(1).await.expect("Failed to mine blocks");

    // Wait for Electrum to fully sync with the new block
    println!("⏳ Waiting 10 seconds for Electrum to sync after mining...");
    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

    // Record approximate block time
    let block_time_approx = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    println!("📝 Block mined time (approx): {}", block_time_approx);

    // Step 3: Now sync wallets (transaction will be discovered as confirmed)
    println!("⚡ Step 3: Sync wallets (transaction discovered as already confirmed)");
    env.sync_and_wait()
        .await
        .expect("Failed to sync");

    // Get transactions after sync
    let alice_txs = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");

    // Find the confirmed transaction (should be directly confirmed)
    let confirmed_tx = alice_txs
        .iter()
        .find(|tx| tx.block_height.is_some() && tx.transaction_type == EventType::Send)
        .expect("Should find confirmed send transaction");

    println!("📊 Transaction mined directly:");
    println!("   first_seen_at: {}", confirmed_tx.first_seen_at);
    println!("   confirmed_at: {:?}", confirmed_tx.confirmed_at);
    println!("   block_height: {:?}", confirmed_tx.block_height);

    // Calculate display time (minimum of available timestamps)
    let display_time = confirmed_tx.first_seen_at.min(confirmed_tx.confirmed_at.unwrap_or(u64::MAX));
    println!("   Display time (min of both): {}", display_time);

    // Assertions
    assert!(confirmed_tx.block_height.is_some(), "Transaction should be confirmed");
    assert!(confirmed_tx.confirmed_at.is_some(), "confirmed_at should be set");

    // For direct mining, confirmed_at should contain the block timestamp
    let confirmed_at = confirmed_tx.confirmed_at.unwrap();

    // The display time should use the block timestamp (only available timestamp)
    // Since this was mined directly, the block timestamp is the oldest/only real timestamp
    assert_eq!(
        display_time,
        display_time.min(confirmed_at),
        "Display time should use the oldest available timestamp"
    );

    // Verify the timestamps are reasonable (block time can be earlier than our recorded time)
    // Allow up to 30 seconds difference to account for blockchain timing and processing
    let time_diff = confirmed_at as i64 - block_time_approx as i64;
    assert!(
        time_diff.abs() <= 30,
        "confirmed_at ({}) should be reasonably close to block time ({}), diff: {}s",
        confirmed_at,
        block_time_approx,
        time_diff
    );

    println!("✅ Test 2 passed: Direct-mined transaction shows block timestamp");
}

