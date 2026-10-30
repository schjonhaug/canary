use canary::metadata::EventType;

mod common;
use common::docker_environment::IsolatedTestEnvironment;

// System tests for mined directly scenarios.
//
// These tests verify transaction detection when transactions are confirmed
// immediately (before sync), resulting in direct "Sent"/"Received" events
// without intermediate "Sending"/"Receiving" states.

/// Test 4: Alice Partial Send Bob (Mined Directly)
/// Purpose: Test mined directly scenarios where partial send transactions are mined immediately
#[tokio::test]
#[ignore = "requires the Docker regtest environment; run via run-system-tests.sh"]
async fn test_alice_partial_send_bob_mined_directly() {
    let mut env = IsolatedTestEnvironment::new()
        .await
        .expect("Failed to create test environment");

    // Initial sync to establish baseline
    env.sync_and_wait().await.expect("Failed to sync");

    let initial_alice_transactions = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");
    let initial_bob_transactions = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    // Send transaction and immediately mine block (before sync)
    eprintln!("Step 1: Alice sends 0.1 BTC to Bob");
    let _txid = env
        .send_transaction("alice", "bob", "0.1")
        .expect("Failed to send transaction");

    eprintln!("Step 2: Immediately mine 1 block (before sync)");
    env.mine_blocks(1).await.expect("Failed to mine blocks");

    eprintln!("Step 3: Now sync wallets");
    env.sync_and_wait_with_retries(3)
        .await
        .expect("Failed to sync with retries");

    // Verify transactions show mined directly state (no intermediate states)
    let post_sync_alice_transactions = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");
    let post_sync_bob_transactions = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    // Expected: Alice should have 1 Send transaction, Bob should have 1 Receive transaction

    // Look for Send and Receive transactions in all transactions (not just "new" ones)
    let alice_send_transactions: Vec<_> = post_sync_alice_transactions
        .iter()
        .filter(|t| t.transaction_type == EventType::Send)
        .collect();
    let bob_receive_transactions: Vec<_> = post_sync_bob_transactions
        .iter()
        .filter(|t| t.transaction_type == EventType::Receive)
        .collect();

    // New Transaction System Expectations:
    // We test for NEW transactions created during the test (not historical funding)
    // Alice should gain 1 Send transaction (to Bob)
    // Bob should gain 1 Receive transaction (from Alice)

    let expected_alice_count = initial_alice_transactions.len() + 1;
    let expected_bob_count = initial_bob_transactions.len() + 1;

    assert_eq!(
        post_sync_alice_transactions.len(),
        expected_alice_count,
        "Alice should have {} transaction(s): {} initial + 1 send to Bob",
        expected_alice_count,
        initial_alice_transactions.len()
    );
    assert_eq!(
        post_sync_bob_transactions.len(),
        expected_bob_count,
        "Bob should have {} transaction(s): {} initial + 1 receive from Alice",
        expected_bob_count,
        initial_bob_transactions.len()
    );

    // Alice should have exactly 1 Send transaction
    assert_eq!(
        alice_send_transactions.len(),
        1,
        "Alice should have exactly 1 Send transaction"
    );

    // Bob should have exactly 1 Receive transaction
    assert_eq!(
        bob_receive_transactions.len(),
        1,
        "Bob should have exactly 1 Receive transaction"
    );

    // Verify transactions are confirmed (mined directly)
    let alice_send = alice_send_transactions[0];
    let bob_receive = bob_receive_transactions[0];

    assert!(
        alice_send.block_height.is_some(),
        "Alice's Send transaction should be confirmed (mined directly)"
    );
    assert!(
        bob_receive.block_height.is_some(),
        "Bob's Receive transaction should be confirmed (mined directly)"
    );

    // Verify amounts are reasonable for 0.1 BTC transaction
    let expected_bob_amount = 10_000_000i64; // 0.1 BTC in sats
    assert_eq!(
        bob_receive.amount_sats, expected_bob_amount,
        "Bob should receive exactly 0.1 BTC"
    );

    // Alice's send amount should be 0.1 BTC + fees (slightly more than 0.1 BTC)
    assert!(
        alice_send.amount_sats >= expected_bob_amount,
        "Alice's send amount should be at least 0.1 BTC (including fees)"
    );
}

/// Test 6: Alice Full Send Bob (Mined Directly)
/// Purpose: Test mined directly full send where wallet is completely emptied when mined directly
#[tokio::test]
#[ignore = "requires the Docker regtest environment; run via run-system-tests.sh"]
async fn test_alice_full_send_bob_mined_directly() {
    let mut env = IsolatedTestEnvironment::new()
        .await
        .expect("Failed to create test environment");

    // Initial sync to establish baseline
    env.sync_and_wait().await.expect("Failed to sync");

    let initial_alice_transactions = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");
    let initial_bob_transactions = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    // Send maximum balance and immediately mine (full send scenario)
    eprintln!("Step 1: Alice sends maximum balance to Bob (full send)");
    let _txid = env
        .send_transaction("alice", "bob", "max")
        .expect("Failed to send max transaction");

    eprintln!("Step 2: Immediately mine 1 block (before sync)");
    env.mine_blocks(1).await.expect("Failed to mine blocks");

    eprintln!("Step 3: Now sync wallets");
    env.sync_and_wait_with_retries(3)
        .await
        .expect("Failed to sync with retries");

    // Verify full send transactions show mined directly state
    let post_sync_alice_transactions = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");
    let post_sync_bob_transactions = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    // Find new events (full send events)
    // Look for Send and Receive transactions in all transactions (not just "new" ones)
    let alice_send_transactions: Vec<_> = post_sync_alice_transactions
        .iter()
        .filter(|t| t.transaction_type == EventType::Send)
        .collect();
    let bob_receive_transactions: Vec<_> = post_sync_bob_transactions
        .iter()
        .filter(|t| t.transaction_type == EventType::Receive)
        .collect();

    // New Transaction System Expectations for Full Send:
    // We test for NEW transactions created during the test (not historical funding)
    // Alice should gain 1 Send transaction (full send to Bob)
    // Bob should gain 1 Receive transaction (from Alice)

    let expected_alice_count = initial_alice_transactions.len() + 1;
    let expected_bob_count = initial_bob_transactions.len() + 1;

    assert_eq!(
        post_sync_alice_transactions.len(),
        expected_alice_count,
        "Alice should have {} transaction(s): {} initial + 1 full send to Bob",
        expected_alice_count,
        initial_alice_transactions.len()
    );
    assert_eq!(
        post_sync_bob_transactions.len(),
        expected_bob_count,
        "Bob should have {} transaction(s): {} initial + 1 receive from Alice",
        expected_bob_count,
        initial_bob_transactions.len()
    );

    // Alice should have exactly 1 Send transaction
    assert_eq!(
        alice_send_transactions.len(),
        1,
        "Alice should have exactly 1 Send transaction for full send"
    );

    // Bob should have exactly 1 Receive transaction
    assert_eq!(
        bob_receive_transactions.len(),
        1,
        "Bob should have exactly 1 Receive transaction"
    );

    // Verify transactions are confirmed (mined directly)
    let alice_send = alice_send_transactions[0];
    let bob_receive = bob_receive_transactions[0];

    assert!(
        alice_send.block_height.is_some(),
        "Alice's Send transaction should be confirmed (mined directly)"
    );
    assert!(
        bob_receive.block_height.is_some(),
        "Bob's Receive transaction should be confirmed (mined directly)"
    );

    // Verify full send amounts (should be close to 1 BTC minus fees)
    let expected_min_amount = 50_000_000i64; // At least 0.5 BTC (should be much more)

    assert!(
        alice_send.amount_sats >= expected_min_amount,
        "Alice's full send should be a large amount (> 0.5 BTC). Got: {} sats",
        alice_send.amount_sats
    );
    assert!(
        bob_receive.amount_sats >= expected_min_amount,
        "Bob should receive a large amount (> 0.5 BTC). Got: {} sats",
        bob_receive.amount_sats
    );

    // Bob's receive should be slightly less than Alice's send (due to fees)
    assert!(
        bob_receive.amount_sats <= alice_send.amount_sats,
        "Bob's receive amount should be less than or equal to Alice's send amount (due to fees)"
    );
}

/// Test: Multiple partial sends mined directly
/// Additional test to verify multiple partial send transactions can be handled when mined directly
#[tokio::test]
#[ignore = "requires the Docker regtest environment; run via run-system-tests.sh"]
async fn test_multiple_partial_sends_mined_directly() {
    let mut env = IsolatedTestEnvironment::new()
        .await
        .expect("Failed to create test environment");

    // Initial sync
    env.sync_and_wait().await.expect("Failed to sync");

    // Capture initial transaction counts for comparison
    let initial_alice_transactions = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");
    let initial_bob_transactions = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    // Send multiple partial send transactions and mine them all at once
    eprintln!("Sending multiple partial send transactions that will be mined directly");

    let _txid1 = env
        .send_transaction("alice", "bob", "0.1")
        .expect("Failed to send transaction 1");
    let _txid2 = env
        .send_transaction("alice", "bob", "0.2")
        .expect("Failed to send transaction 2");
    let _txid3 = env
        .send_transaction("alice", "bob", "0.05")
        .expect("Failed to send transaction 3");

    // Mine all transactions at once
    env.mine_blocks(1).await.expect("Failed to mine blocks");

    // Now sync
    env.sync_and_wait_with_retries(3)
        .await
        .expect("Failed to sync with retries");

    // Verify all partial send transactions are detected as mined directly
    let final_alice_transactions = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");
    let final_bob_transactions = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    // New Transaction System Expectations for Multiple Partial Sends:
    // We test for NEW transactions created during the test (not historical funding)
    // Alice should gain 3 Send transactions (to Bob)
    // Bob should gain 3 Receive transactions (from Alice)

    let expected_alice_count = initial_alice_transactions.len() + 3;
    let expected_bob_count = initial_bob_transactions.len() + 3;

    assert_eq!(
        final_alice_transactions.len(),
        expected_alice_count,
        "Alice should have {} transaction(s): {} initial + 3 sends to Bob",
        expected_alice_count,
        initial_alice_transactions.len()
    );
    assert_eq!(
        final_bob_transactions.len(),
        expected_bob_count,
        "Bob should have {} transaction(s): {} initial + 3 receives from Alice",
        expected_bob_count,
        initial_bob_transactions.len()
    );

    // Find Send and Receive transactions
    let alice_send_transactions: Vec<_> = final_alice_transactions
        .iter()
        .filter(|t| t.transaction_type == EventType::Send)
        .collect();
    let bob_receive_transactions: Vec<_> = final_bob_transactions
        .iter()
        .filter(|t| t.transaction_type == EventType::Receive)
        .collect();

    // Alice should have exactly 3 Send transactions
    assert_eq!(
        alice_send_transactions.len(),
        3,
        "Alice should have exactly 3 Send transactions"
    );

    // Bob should have exactly 3 Receive transactions
    assert_eq!(
        bob_receive_transactions.len(),
        3,
        "Bob should have exactly 3 Receive transactions"
    );

    // All transactions should be confirmed (mined directly)
    let confirmed_alice_sends = alice_send_transactions
        .iter()
        .filter(|t| t.block_height.is_some())
        .count();
    let confirmed_bob_receives = bob_receive_transactions
        .iter()
        .filter(|t| t.block_height.is_some())
        .count();

    assert_eq!(
        confirmed_alice_sends, 3,
        "All 3 Alice Send transactions should be confirmed"
    );
    assert_eq!(
        confirmed_bob_receives, 3,
        "All 3 Bob Receive transactions should be confirmed"
    );

    // Verify expected amounts (0.1, 0.2, 0.05 BTC)
    let expected_amounts = [10_000_000i64, 20_000_000i64, 5_000_000i64]; // 0.1, 0.2, 0.05 BTC in sats
    let bob_amounts: Vec<i64> = bob_receive_transactions
        .iter()
        .map(|t| t.amount_sats)
        .collect();

    for expected_amount in expected_amounts {
        assert!(
            bob_amounts.contains(&expected_amount),
            "Bob should have received {expected_amount} sats. Got: {bob_amounts:?}"
        );
    }
}
