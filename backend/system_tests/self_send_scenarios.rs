use canary::metadata::EventType;

mod common;
use common::docker_environment::IsolatedTestEnvironment;

/// System tests for self-send (internal transfer) scenarios
///
/// These tests verify how the system handles transactions where a wallet
/// sends Bitcoin to itself. The sync logic classifies these as Send events
/// with amount = (sent - received), which equals approximately the fee.

/// Test: Partial self-send within the same wallet
/// Purpose: Verify self-send is recorded correctly with net amount = fee
#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_self_send_partial() {
    let mut env = IsolatedTestEnvironment::new()
        .await
        .expect("Failed to create test environment");

    // Initial sync to establish baseline
    env.sync_and_wait().await.expect("Failed to sync");

    let initial_alice_txs = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");

    println!("📊 Initial Alice transactions: {}", initial_alice_txs.len());

    // Alice sends 0.3 BTC to herself
    println!("⚡ Step 1: Alice sends 0.3 BTC to herself");
    let self_send_txid = env
        .send_self_transaction("alice", "0.3")
        .await
        .expect("Failed to send self-transaction");
    println!("   Self-send txid: {}", self_send_txid);

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

    println!("📊 Final state:");
    for (i, tx) in final_alice_txs.iter().enumerate() {
        println!(
            "   Alice tx {}: type={:?}, amount={} sats, txid={}, status={}",
            i,
            tx.transaction_type,
            tx.amount_sats,
            &tx.txid[..8],
            tx.transaction_status
        );
    }

    // Find the self-send transaction
    let self_send_events: Vec<_> = final_alice_txs
        .iter()
        .filter(|t| t.txid == self_send_txid)
        .collect();

    assert_eq!(
        self_send_events.len(),
        1,
        "Self-send should produce exactly 1 event (not separate Send + Receive), got {}",
        self_send_events.len()
    );

    // The system classifies self-sends as Send with amount = (sent - received) = fee
    // Since both send and receive are tracked for the same wallet, the net is just the fee
    let self_send = &self_send_events[0];
    assert_eq!(
        self_send.transaction_type,
        EventType::Send,
        "Self-send should be classified as Send (sent > 0 && received > 0)"
    );

    // The amount should be very small (just the mining fee, typically < 1000 sats)
    println!(
        "💰 Self-send net amount: {} sats (should be approximately the fee)",
        self_send.amount_sats
    );
    assert!(
        self_send.amount_sats < 100_000, // Less than 0.001 BTC (fee should be much less)
        "Self-send amount should be just the fee, got {} sats",
        self_send.amount_sats
    );

    assert!(
        self_send.block_height.is_some(),
        "Self-send should be confirmed"
    );

    println!("✅ Partial self-send test passed!");
}

/// Test: Full amount self-send (wallet drain to own address)
/// Purpose: Verify sending max to yourself records correctly
#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_self_send_full_amount() {
    let mut env = IsolatedTestEnvironment::new()
        .await
        .expect("Failed to create test environment");

    // Initial sync to establish baseline
    env.sync_and_wait().await.expect("Failed to sync");

    let initial_alice_txs = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");

    println!("📊 Initial Alice transactions: {}", initial_alice_txs.len());

    // Alice sends everything to herself
    println!("⚡ Step 1: Alice sends max to herself");
    let self_send_txid = env
        .send_self_transaction("alice", "max")
        .await
        .expect("Failed to send self-transaction");
    println!("   Self-send txid: {}", self_send_txid);

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

    println!("📊 Final state:");
    for (i, tx) in final_alice_txs.iter().enumerate() {
        println!(
            "   Alice tx {}: type={:?}, amount={} sats, txid={}, status={}",
            i,
            tx.transaction_type,
            tx.amount_sats,
            &tx.txid[..8],
            tx.transaction_status
        );
    }

    // Find the self-send transaction
    let self_send_events: Vec<_> = final_alice_txs
        .iter()
        .filter(|t| t.txid == self_send_txid)
        .collect();

    assert_eq!(
        self_send_events.len(),
        1,
        "Full self-send should produce exactly 1 event, got {}",
        self_send_events.len()
    );

    let self_send = &self_send_events[0];
    assert_eq!(
        self_send.transaction_type,
        EventType::Send,
        "Full self-send should be classified as Send"
    );

    // With subtractfeefromamount=true, the entire balance minus fee goes to self.
    // Net amount = sent - received = fee (since received = sent - fee)
    println!(
        "💰 Full self-send net amount: {} sats",
        self_send.amount_sats
    );

    // Amount should be the fee (very small relative to 1 BTC)
    assert!(
        self_send.amount_sats < 100_000,
        "Full self-send amount should be just the fee, got {} sats",
        self_send.amount_sats
    );

    assert!(
        self_send.block_height.is_some(),
        "Self-send should be confirmed"
    );

    // Verify Alice still has a balance (she sent to herself, so balance should be ~1.0 BTC minus fee)
    let bitcoin_container_name = format!("test-bitcoin-{}", env.test_id);
    match IsolatedTestEnvironment::bitcoin_cli(
        &bitcoin_container_name,
        &["-rpcwallet=alice", "getbalance"],
    ) {
        Ok(balance) => {
            let balance_btc: f64 = balance.trim().parse().unwrap_or(0.0);
            println!(
                "💰 Alice's balance after full self-send: {} BTC",
                balance_btc
            );
            assert!(
                balance_btc > 0.99,
                "Alice should still have most of her BTC after self-send, got {} BTC",
                balance_btc
            );
        }
        Err(e) => panic!("Failed to get Alice's balance: {}", e),
    }

    println!("✅ Full self-send test passed!");
}
