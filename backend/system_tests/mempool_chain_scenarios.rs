use canary::metadata::{EventType, TransactionStatus};

mod common;
use common::docker_environment::IsolatedTestEnvironment;

// System tests for chains of unconfirmed transactions.
//
// These tests verify that the system correctly handles multiple unconfirmed
// transactions in sequence, where each subsequent transaction may spend
// from change outputs of prior unconfirmed transactions.

/// Test: Chain of unconfirmed transactions all confirmed together
/// Purpose: Verify multiple unconfirmed transactions are tracked and confirmed correctly
#[tokio::test]
#[ignore = "requires the Docker regtest environment; run via run-system-tests.sh"]
async fn test_chain_of_unconfirmed_transactions() {
    let mut env = IsolatedTestEnvironment::new()
        .await
        .expect("Failed to create test environment");

    // Initial sync to establish baseline
    env.sync_and_wait().await.expect("Failed to sync");

    let initial_alice_txs = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");
    let initial_bob_txs = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    // Step 1: Send first transaction (unconfirmed)
    eprintln!("Step 1: Alice sends 0.1 BTC to Bob (unconfirmed)");
    env.send_transaction("alice", "bob", "0.1")
        .expect("Failed to send tx1");

    // Sync to detect in mempool
    env.sync_and_wait().await.expect("Failed to sync");

    let bob_txs_after_1 = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    let bob_pending_1: Vec<_> = bob_txs_after_1
        .iter()
        .filter(|t| t.block_height.is_none() && t.transaction_type == EventType::Receive)
        .collect();
    assert!(
        !bob_pending_1.is_empty(),
        "Bob should have at least 1 pending Receive after tx1 (mempool detection)"
    );

    // Step 2: Send second transaction (unconfirmed, may spend from Alice's change of tx1)
    eprintln!("Step 2: Alice sends 0.1 BTC to Bob again (unconfirmed)");
    env.send_transaction("alice", "bob", "0.1")
        .expect("Failed to send tx2");

    // Sync to detect second transaction
    env.sync_and_wait().await.expect("Failed to sync");

    // Step 3: Mine all unconfirmed transactions
    eprintln!("Step 3: Mine 1 block to confirm all transactions");
    env.mine_blocks(1).await.expect("Failed to mine");

    env.sync_and_wait_with_retries(3)
        .await
        .expect("Failed to sync");

    // Verify final state
    let final_alice_txs = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");
    let final_bob_txs = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    // Bob should have gained exactly 2 new Receive transactions (delta-based)
    let bob_confirmed_receives: Vec<_> = final_bob_txs
        .iter()
        .filter(|t| {
            t.transaction_type == EventType::Receive
                && t.transaction_status == TransactionStatus::Confirmed
        })
        .collect();
    let initial_bob_receives = initial_bob_txs
        .iter()
        .filter(|t| t.transaction_type == EventType::Receive)
        .count();
    let new_bob_receives = bob_confirmed_receives.len() - initial_bob_receives;
    assert_eq!(
        new_bob_receives, 2,
        "Bob should have gained exactly 2 new confirmed Receive transactions, got {new_bob_receives}"
    );

    // Alice should have gained exactly 2 new Send transactions (delta-based)
    let alice_confirmed_sends: Vec<_> = final_alice_txs
        .iter()
        .filter(|t| {
            t.transaction_type == EventType::Send
                && t.transaction_status == TransactionStatus::Confirmed
        })
        .collect();
    let initial_alice_sends = initial_alice_txs
        .iter()
        .filter(|t| t.transaction_type == EventType::Send)
        .count();
    let new_alice_sends = alice_confirmed_sends.len() - initial_alice_sends;
    assert_eq!(
        new_alice_sends, 2,
        "Alice should have gained exactly 2 new confirmed Send transactions, got {new_alice_sends}"
    );

    // All transactions should now be confirmed (no pending ones left)
    let bob_pending: Vec<_> = final_bob_txs
        .iter()
        .filter(|t| t.transaction_status == TransactionStatus::Pending)
        .collect();
    assert_eq!(
        bob_pending.len(),
        0,
        "No transactions should remain pending after mining, got {} pending",
        bob_pending.len()
    );
}

/// Test: Transactions with separate confirmation rounds
/// Purpose: Verify no events are lost or duplicated across confirmation boundaries
#[tokio::test]
#[ignore = "requires the Docker regtest environment; run via run-system-tests.sh"]
async fn test_unconfirmed_chain_with_separate_confirmations() {
    let mut env = IsolatedTestEnvironment::new()
        .await
        .expect("Failed to create test environment");

    // Initial sync to establish baseline
    env.sync_and_wait().await.expect("Failed to sync");

    let initial_bob_txs = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    // Step 1: Send and confirm first transaction
    eprintln!("Step 1: Alice sends 0.1 BTC to Bob, mine immediately");
    let _txid1 = env
        .send_transaction("alice", "bob", "0.1")
        .expect("Failed to send tx1");
    env.mine_blocks(1).await.expect("Failed to mine");
    env.sync_and_wait_with_retries(3)
        .await
        .expect("Failed to sync");

    // Step 2: Send two more transactions without mining
    eprintln!("Step 2: Alice sends 2 more transactions (unconfirmed)");
    let _txid2 = env
        .send_transaction("alice", "bob", "0.1")
        .expect("Failed to send tx2");
    let _txid3 = env
        .send_transaction("alice", "bob", "0.1")
        .expect("Failed to send tx3");

    env.sync_and_wait().await.expect("Failed to sync");

    // Step 3: Mine to confirm the remaining transactions
    eprintln!("Step 3: Mine to confirm remaining transactions");
    env.mine_blocks(1).await.expect("Failed to mine");
    env.sync_and_wait_with_retries(3)
        .await
        .expect("Failed to sync");

    let final_bob_txs = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    // Bob should have exactly 3 Receive transactions total (from initial count)
    let bob_receives_final: Vec<_> = final_bob_txs
        .iter()
        .filter(|t| t.transaction_type == EventType::Receive)
        .collect();
    let new_receives = bob_receives_final.len()
        - (initial_bob_txs
            .iter()
            .filter(|t| t.transaction_type == EventType::Receive)
            .count());
    assert_eq!(
        new_receives, 3,
        "Bob should have gained exactly 3 new Receive transactions, got {new_receives}"
    );

    // All should be confirmed
    let all_confirmed = bob_receives_final
        .iter()
        .all(|t| t.transaction_status == TransactionStatus::Confirmed);
    assert!(
        all_confirmed,
        "All Receive transactions should be confirmed"
    );

    // No duplicates - each txid should be unique
    let mut txids: Vec<&str> = bob_receives_final.iter().map(|t| t.txid.as_str()).collect();
    txids.sort_unstable();
    txids.dedup();
    assert_eq!(
        txids.len(),
        bob_receives_final.len(),
        "No duplicate transaction events should exist"
    );
}
