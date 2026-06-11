use canary::metadata::EventType;

mod common;
use common::docker_environment::IsolatedTestEnvironment;

// System tests for batch (multi-recipient) transaction scenarios.
//
// These tests verify that when a wallet sends to multiple recipients
// in a single transaction (using sendmany), each recipient correctly
// detects their Receive event and the sender records a single Send event.

/// Test: Alice sends to Bob and Charlie in a single batch transaction
/// Purpose: Verify multi-recipient transaction handling
#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_batch_send_to_bob_and_charlie() {
    let mut env = IsolatedTestEnvironment::new_with_charlie()
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
    let initial_charlie_txs = env
        .get_wallet_transactions(&env.charlie_checksum)
        .await
        .expect("Failed to get Charlie transactions");

    println!("📊 Initial state:");
    println!("   Alice transactions: {}", initial_alice_txs.len());
    println!("   Bob transactions: {}", initial_bob_txs.len());
    println!("   Charlie transactions: {}", initial_charlie_txs.len());

    // Alice sends batch: 0.1 BTC to Bob + 0.05 BTC to Charlie
    println!("⚡ Step 1: Alice sends batch to Bob (0.1 BTC) and Charlie (0.05 BTC)");
    let batch_txid = env
        .send_batch_transaction("alice", &[("bob", "0.1"), ("charlie", "0.05")])
        .await
        .expect("Failed to send batch transaction");
    println!("   Batch txid: {}", batch_txid);

    // Mine and sync
    println!("⚡ Step 2: Mine and sync");
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
    let final_charlie_txs = env
        .get_wallet_transactions(&env.charlie_checksum)
        .await
        .expect("Failed to get Charlie transactions");

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
    for (i, tx) in final_charlie_txs.iter().enumerate() {
        println!(
            "   Charlie tx {}: type={:?}, amount={} sats, txid={}",
            i,
            tx.transaction_type,
            tx.amount_sats,
            &tx.txid[..8]
        );
    }

    // Alice should have 1 new Send transaction for the batch
    let alice_batch_sends: Vec<_> = final_alice_txs
        .iter()
        .filter(|t| t.txid == batch_txid && t.transaction_type == EventType::Send)
        .collect();
    assert_eq!(
        alice_batch_sends.len(),
        1,
        "Alice should have exactly 1 Send transaction for the batch"
    );

    // Alice's send amount should be approximately 0.15 BTC (0.1 + 0.05) plus fees
    let alice_send_amount = alice_batch_sends[0].amount_sats;
    println!(
        "💰 Alice's batch send amount: {} sats ({:.8} BTC)",
        alice_send_amount,
        alice_send_amount as f64 / 100_000_000.0
    );
    assert!(
        (15_000_000..=15_050_000).contains(&alice_send_amount),
        "Alice's send should be approximately 0.15 BTC + small fee (got {} sats)",
        alice_send_amount
    );

    // Bob should have 1 new Receive transaction for 0.1 BTC
    let bob_batch_receives: Vec<_> = final_bob_txs
        .iter()
        .filter(|t| t.txid == batch_txid && t.transaction_type == EventType::Receive)
        .collect();
    assert_eq!(
        bob_batch_receives.len(),
        1,
        "Bob should have exactly 1 Receive transaction from the batch"
    );
    assert_eq!(
        bob_batch_receives[0].amount_sats, 10_000_000,
        "Bob should receive exactly 0.1 BTC (10,000,000 sats), got {}",
        bob_batch_receives[0].amount_sats
    );

    // Charlie should have 1 new Receive transaction for 0.05 BTC
    let charlie_batch_receives: Vec<_> = final_charlie_txs
        .iter()
        .filter(|t| t.txid == batch_txid && t.transaction_type == EventType::Receive)
        .collect();
    assert_eq!(
        charlie_batch_receives.len(),
        1,
        "Charlie should have exactly 1 Receive transaction from the batch"
    );
    assert_eq!(
        charlie_batch_receives[0].amount_sats, 5_000_000,
        "Charlie should receive exactly 0.05 BTC (5,000,000 sats), got {}",
        charlie_batch_receives[0].amount_sats
    );

    // All three should reference the same txid
    assert_eq!(
        alice_batch_sends[0].txid, bob_batch_receives[0].txid,
        "Alice's send and Bob's receive should share the same txid"
    );
    assert_eq!(
        alice_batch_sends[0].txid, charlie_batch_receives[0].txid,
        "Alice's send and Charlie's receive should share the same txid"
    );

    // All should be confirmed
    assert!(
        alice_batch_sends[0].block_height.is_some(),
        "Batch transaction should be confirmed"
    );

    println!("✅ Batch transaction test passed!");
}
