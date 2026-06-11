use canary::metadata::EventType;

mod common;
use common::docker_environment::IsolatedTestEnvironment;

// System tests for UTXO consolidation scenarios.
//
// These tests verify that when a wallet has multiple UTXOs and sends them
// all in a single transaction, the system correctly records a single Send
// event with the consolidated amount.

/// Test: Bob consolidates multiple UTXOs into a single send back to Alice
/// Purpose: Verify UTXO management and consolidation transaction handling
#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_utxo_consolidation_send_all_to_bob() {
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

    println!("📊 Initial state:");
    println!("   Alice transactions: {}", initial_alice_txs.len());
    println!("   Bob transactions: {}", initial_bob_txs.len());

    // Step 1: Send 3 separate transactions from Alice to Bob to create multiple UTXOs
    println!("⚡ Step 1: Alice sends 3 separate transactions to Bob");

    let _txid1 = env
        .send_transaction("alice", "bob", "0.1")
        .await
        .expect("Failed to send tx1");
    env.mine_blocks(1).await.expect("Failed to mine");

    let _txid2 = env
        .send_transaction("alice", "bob", "0.15")
        .await
        .expect("Failed to send tx2");
    env.mine_blocks(1).await.expect("Failed to mine");

    let _txid3 = env
        .send_transaction("alice", "bob", "0.2")
        .await
        .expect("Failed to send tx3");
    env.mine_blocks(1).await.expect("Failed to mine");

    // Sync to pick up all 3 transactions
    env.sync_and_wait_with_retries(3)
        .await
        .expect("Failed to sync");

    let mid_bob_txs = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    let bob_receives: Vec<_> = mid_bob_txs
        .iter()
        .filter(|t| t.transaction_type == EventType::Receive)
        .collect();
    println!(
        "📊 After 3 sends: Bob has {} Receive transactions",
        bob_receives.len()
    );
    assert!(
        bob_receives.len() >= 3,
        "Bob should have at least 3 Receive transactions, got {}",
        bob_receives.len()
    );

    // Step 2: Bob consolidates all UTXOs by sending everything to Alice
    println!("⚡ Step 2: Bob sends all funds back to Alice (consolidation)");

    let consolidation_txid = env
        .send_transaction("bob", "alice", "max")
        .await
        .expect("Failed to send consolidation transaction");
    println!("   Consolidation txid: {}", consolidation_txid);

    env.mine_blocks(1).await.expect("Failed to mine");
    env.sync_and_wait_with_retries(3)
        .await
        .expect("Failed to sync");

    // Verify results
    let final_alice_txs = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");
    let final_bob_txs = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    println!("📊 Final state:");
    for (i, tx) in final_alice_txs.iter().enumerate() {
        println!(
            "   Alice tx {}: type={:?}, amount={} sats, txid={}",
            i,
            tx.transaction_type,
            tx.amount_sats,
            &tx.txid[..8]
        );
    }
    for (i, tx) in final_bob_txs.iter().enumerate() {
        println!(
            "   Bob tx {}: type={:?}, amount={} sats, txid={}",
            i,
            tx.transaction_type,
            tx.amount_sats,
            &tx.txid[..8]
        );
    }

    // Bob should have exactly 1 Send transaction (the consolidation)
    let bob_sends: Vec<_> = final_bob_txs
        .iter()
        .filter(|t| t.transaction_type == EventType::Send)
        .collect();
    assert_eq!(
        bob_sends.len(),
        1,
        "Bob should have exactly 1 Send transaction (consolidation), got {}",
        bob_sends.len()
    );

    // The consolidation send should be for approximately 0.45 BTC (0.1 + 0.15 + 0.2 minus fees)
    let consolidation_amount = bob_sends[0].amount_sats;
    println!(
        "💰 Consolidation send amount: {} sats ({:.8} BTC)",
        consolidation_amount,
        consolidation_amount as f64 / 100_000_000.0
    );

    // Should be close to 45_000_000 sats (0.45 BTC) minus small fee
    assert!(
        (44_900_000..=45_010_000).contains(&consolidation_amount),
        "Consolidation amount should be approximately 0.45 BTC (got {} sats)",
        consolidation_amount
    );

    // Alice should have received 1 new Receive transaction for the consolidation
    let alice_receives_after: Vec<_> = final_alice_txs
        .iter()
        .filter(|t| t.transaction_type == EventType::Receive && t.txid == consolidation_txid)
        .collect();
    assert_eq!(
        alice_receives_after.len(),
        1,
        "Alice should have 1 Receive for the consolidation transaction"
    );

    // The consolidation should be a single transaction (one txid)
    assert_eq!(
        bob_sends[0].txid, consolidation_txid,
        "Bob's send should match the consolidation txid"
    );

    // All transactions should be confirmed
    assert!(
        bob_sends[0].block_height.is_some(),
        "Consolidation transaction should be confirmed"
    );

    println!("✅ UTXO consolidation test passed!");
}
