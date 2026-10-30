use canary::metadata::{EventType, TransactionStatus};

mod common;
use common::docker_environment::IsolatedTestEnvironment;

// System tests for wallet recovery scenarios.
//
// These tests verify that after a service restart (simulated by recreating
// the WalletManager), transaction history persists correctly and no
// duplicate events are created during re-sync.

/// Test: No duplicate events after service restart
/// Purpose: Verify sync process doesn't create duplicate events
#[tokio::test]
#[ignore = "requires the Docker regtest environment; run via run-system-tests.sh"]
async fn test_no_duplicate_events_after_restart() {
    let mut env = IsolatedTestEnvironment::new()
        .await
        .expect("Failed to create test environment");

    // Initial sync to establish baseline
    env.sync_and_wait().await.expect("Failed to sync");

    // Step 1: Alice sends 0.1 BTC to Bob, mine, and sync
    eprintln!("Step 1: Alice sends 0.1 BTC to Bob");
    env.send_transaction("alice", "bob", "0.1")
        .expect("Failed to send transaction");

    env.mine_blocks(1).await.expect("Failed to mine");
    env.sync_and_wait_with_retries(3)
        .await
        .expect("Failed to sync");

    // Record transaction counts before restart
    let pre_restart_alice_txs = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");
    let pre_restart_bob_txs = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    // Step 2: Simulate service restart
    eprintln!("Step 2: Recreating WalletManager (simulating restart)");
    env.recreate_wallet_manager()
        .await
        .expect("Failed to recreate wallet manager");

    // Step 3: Sync again after restart
    eprintln!("Step 3: Syncing after restart");
    env.sync_and_wait_with_retries(3)
        .await
        .expect("Failed to sync after restart");

    // Verify transaction counts are identical
    let post_restart_alice_txs = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");
    let post_restart_bob_txs = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    // Transaction counts should be identical
    assert_eq!(
        pre_restart_alice_txs.len(),
        post_restart_alice_txs.len(),
        "Alice transaction count should be identical after restart ({} before, {} after)",
        pre_restart_alice_txs.len(),
        post_restart_alice_txs.len()
    );
    assert_eq!(
        pre_restart_bob_txs.len(),
        post_restart_bob_txs.len(),
        "Bob transaction count should be identical after restart ({} before, {} after)",
        pre_restart_bob_txs.len(),
        post_restart_bob_txs.len()
    );

    // Verify transaction details match exactly (order-independent comparison by txid)
    let pre_by_txid: std::collections::HashMap<&str, &canary::metadata::TransactionWithWallet> =
        pre_restart_alice_txs
            .iter()
            .map(|t| (t.txid.as_str(), t))
            .collect();
    for post in &post_restart_alice_txs {
        let pre = pre_by_txid.get(post.txid.as_str()).unwrap_or_else(|| {
            panic!(
                "Post-restart Alice txid {} not found in pre-restart set",
                post.txid
            )
        });
        assert_eq!(
            pre.transaction_type, post.transaction_type,
            "Transaction types should match for txid {}",
            pre.txid
        );
        assert_eq!(
            pre.amount_sats, post.amount_sats,
            "Transaction amounts should match for txid {}",
            pre.txid
        );
        assert_eq!(
            pre.transaction_status, post.transaction_status,
            "Transaction statuses should match for txid {}",
            pre.txid
        );
    }

    // Also verify Bob's transactions match (order-independent)
    let pre_bob_by_txid: std::collections::HashMap<&str, &canary::metadata::TransactionWithWallet> =
        pre_restart_bob_txs
            .iter()
            .map(|t| (t.txid.as_str(), t))
            .collect();
    for post in &post_restart_bob_txs {
        let pre = pre_bob_by_txid.get(post.txid.as_str()).unwrap_or_else(|| {
            panic!(
                "Post-restart Bob txid {} not found in pre-restart set",
                post.txid
            )
        });
        assert_eq!(
            pre.transaction_type, post.transaction_type,
            "Transaction types should match for Bob txid {}",
            pre.txid
        );
        assert_eq!(
            pre.amount_sats, post.amount_sats,
            "Transaction amounts should match for Bob txid {}",
            pre.txid
        );
    }
}

/// Test: Recovery detects transactions that occurred during downtime
/// Purpose: Verify transactions mined while service was down are detected on restart
#[tokio::test]
#[ignore = "requires the Docker regtest environment; run via run-system-tests.sh"]
async fn test_recovery_detects_transactions_during_downtime() {
    let mut env = IsolatedTestEnvironment::new()
        .await
        .expect("Failed to create test environment");

    // Initial sync to establish baseline
    env.sync_and_wait().await.expect("Failed to sync");

    // Step 1: Send transaction and mine, but do NOT sync
    eprintln!("Step 1: Alice sends 0.1 BTC to Bob and mine (NO sync)");
    let txid = env
        .send_transaction("alice", "bob", "0.1")
        .expect("Failed to send transaction");

    env.mine_blocks(1).await.expect("Failed to mine");

    // Verify the transaction is NOT yet in our database
    let bob_txs_before_restart = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    let has_tx_before = bob_txs_before_restart.iter().any(|t| t.txid == txid);
    assert!(
        !has_tx_before,
        "Transaction should NOT be in database before sync (it was mined but not synced)"
    );

    // Step 2: Simulate service restart
    eprintln!("Step 2: Recreating WalletManager (simulating restart during downtime)");
    env.recreate_wallet_manager()
        .await
        .expect("Failed to recreate wallet manager");

    // Step 3: Sync after restart — should detect the transaction mined during "downtime"
    eprintln!("Step 3: Syncing after restart (should detect mined transaction)");
    env.sync_and_wait_with_retries(3)
        .await
        .expect("Failed to sync after restart");

    // Verify the transaction is now detected
    let bob_txs_after_restart = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    let recovered_tx: Vec<_> = bob_txs_after_restart
        .iter()
        .filter(|t| t.txid == txid)
        .collect();

    assert_eq!(
        recovered_tx.len(),
        1,
        "Transaction should be detected after restart, found {} matches",
        recovered_tx.len()
    );

    let tx = &recovered_tx[0];
    assert_eq!(
        tx.transaction_type,
        EventType::Receive,
        "Transaction should be a Receive event"
    );

    // Since the transaction was mined before we synced, it should appear as directly confirmed
    assert!(
        tx.block_height.is_some(),
        "Transaction should be confirmed (mined during downtime)"
    );
    assert_eq!(
        tx.transaction_status,
        TransactionStatus::Confirmed,
        "Transaction status should be confirmed"
    );

    // Amount should be 0.1 BTC = 10,000,000 sats
    assert_eq!(
        tx.amount_sats, 10_000_000,
        "Transaction amount should be 0.1 BTC (10,000,000 sats), got {}",
        tx.amount_sats
    );
}
