use canary::metadata::EventType;

mod common;
use common::docker_environment::IsolatedTestEnvironment;

/// System tests for mined directly scenarios
/// 
/// These tests verify transaction detection when transactions are confirmed 
/// immediately (before sync), resulting in direct "Sent"/"Received" events 
/// without intermediate "Sending"/"Receiving" states.

/// Test 4: Alice Partial Send Bob (Mined Directly)
/// Purpose: Test mined directly scenarios where partial send transactions are mined immediately
#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_alice_partial_send_bob_mined_directly() {
    let mut env = IsolatedTestEnvironment::new().await.expect("Failed to create test environment");
    
    // Initial sync to establish baseline
    env.sync_and_wait().await.expect("Failed to sync");
    
    let initial_alice_transactions = env.get_wallet_transactions(&env.alice_checksum).await.expect("Failed to get Alice transactions");
    let initial_bob_transactions = env.get_wallet_transactions(&env.bob_checksum).await.expect("Failed to get Bob transactions");
    
    println!("📊 Initial state:");
    println!("   Alice transactions: {}", initial_alice_transactions.len());
    println!("   Bob transactions: {}", initial_bob_transactions.len());
    
    // Show initial Alice transactions details
    if !initial_alice_transactions.is_empty() {
        for (i, transaction) in initial_alice_transactions.iter().enumerate() {
            println!("   Initial Alice transaction {}: type={:?}, amount={}, confirmed={}", 
                     i, transaction.transaction_type, transaction.amount_sats, transaction.block_height.is_some());
        }
    }
    
    // Send transaction and immediately mine block (before sync)
    println!("⚡ Step 1: Alice sends 0.1 BTC to Bob");
    let _txid = env.send_transaction("alice", "bob", "0.1").await.expect("Failed to send transaction");
    
    println!("⚡ Step 2: Immediately mine 1 block (before sync)");
    env.mine_blocks(1).await.expect("Failed to mine blocks");
    
    println!("⚡ Step 3: Now sync wallets");
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Verify transactions show mined directly state (no intermediate states)
    let post_sync_alice_transactions = env.get_wallet_transactions(&env.alice_checksum).await.expect("Failed to get Alice transactions");
    let post_sync_bob_transactions = env.get_wallet_transactions(&env.bob_checksum).await.expect("Failed to get Bob transactions");
    
    println!("📊 Post-sync state:");
    println!("   Alice transactions: {}", post_sync_alice_transactions.len());
    println!("   Bob transactions: {}", post_sync_bob_transactions.len());
    
    // Expected: Alice should have 1 Send transaction, Bob should have 1 Receive transaction
    println!("🔍 Alice total transactions: {}", post_sync_alice_transactions.len());
    println!("🔍 Bob total transactions: {}", post_sync_bob_transactions.len());
    
    // Look for Send and Receive transactions in all transactions (not just "new" ones)
    let alice_send_transactions: Vec<_> = post_sync_alice_transactions.iter()
        .filter(|t| t.transaction_type == EventType::Send)
        .collect();
    let bob_receive_transactions: Vec<_> = post_sync_bob_transactions.iter()
        .filter(|t| t.transaction_type == EventType::Receive)
        .collect();
    
    println!("🔍 Alice Send transactions: {}", alice_send_transactions.len());
    println!("🔍 Bob Receive transactions: {}", bob_receive_transactions.len());
    
    // Debug: Show all Alice transactions
    for (i, transaction) in post_sync_alice_transactions.iter().enumerate() {
        println!("🔍 Alice transaction {}: type={:?}, amount={}, confirmed={}", 
                 i, transaction.transaction_type, transaction.amount_sats, transaction.block_height.is_some());
    }
    
    // Debug: Show all Bob transactions  
    for (i, transaction) in post_sync_bob_transactions.iter().enumerate() {
        println!("🔍 Bob transaction {}: type={:?}, amount={}, confirmed={}", 
                 i, transaction.transaction_type, transaction.amount_sats, transaction.block_height.is_some());
    }
    
    // DEBUGGING: Check actual Bitcoin Core balances to verify if Alice got 1 or 2 BTC
    let bitcoin_container_name = format!("test-bitcoin-{}", env.test_id);
    
    match IsolatedTestEnvironment::bitcoin_cli(&bitcoin_container_name, &["-rpcwallet=alice", "getbalance"]) {
        Ok(balance) => {
            println!("💰 Alice's actual Bitcoin Core balance: {} BTC", balance.trim());
            let balance_sats: f64 = balance.trim().parse().unwrap_or(0.0) * 100_000_000.0;
            println!("💰 Alice's balance in sats: {:.0}", balance_sats);
        }
        Err(e) => println!("❌ Failed to get Alice's balance: {}", e),
    }
    
    // Check transaction history to see how many 1 BTC receives Alice actually has
    match IsolatedTestEnvironment::bitcoin_cli(&bitcoin_container_name, &["-rpcwallet=alice", "listtransactions", "\"*\"", "100"]) {
        Ok(txs) => {
            println!("📋 Alice's transaction history from Bitcoin Core:");
            println!("{}", txs);
        }
        Err(e) => println!("❌ Failed to get Alice's transactions: {}", e),
    }
    
    // New Transaction System Expectations:
    // Alice should have: 1 Receive (initial funding) + 1 Send (to Bob) = 2 total transactions
    // Bob should have: 1 Receive (from Alice) = 1 total transaction
    
    assert_eq!(post_sync_alice_transactions.len(), 2, 
        "Alice should have 2 transactions: 1 initial receive + 1 send to Bob");
    assert_eq!(post_sync_bob_transactions.len(), 1, 
        "Bob should have 1 transaction: 1 receive from Alice");
    
    // Alice should have exactly 1 Send transaction
    assert_eq!(alice_send_transactions.len(), 1, 
        "Alice should have exactly 1 Send transaction");
    
    // Bob should have exactly 1 Receive transaction  
    assert_eq!(bob_receive_transactions.len(), 1,
        "Bob should have exactly 1 Receive transaction");
    
    // Verify transactions are confirmed (mined directly)
    let alice_send = alice_send_transactions[0];
    let bob_receive = bob_receive_transactions[0];
    
    assert!(alice_send.block_height.is_some(), 
        "Alice's Send transaction should be confirmed (mined directly)");
    assert!(bob_receive.block_height.is_some(),
        "Bob's Receive transaction should be confirmed (mined directly)");
    
    // Verify amounts are reasonable for 0.1 BTC transaction
    let expected_bob_amount = 10_000_000i64; // 0.1 BTC in sats
    assert_eq!(bob_receive.amount_sats, expected_bob_amount,
        "Bob should receive exactly 0.1 BTC");
    
    // Alice's send amount should be 0.1 BTC + fees (slightly more than 0.1 BTC)
    assert!(alice_send.amount_sats >= expected_bob_amount,
        "Alice's send amount should be at least 0.1 BTC (including fees)");
    
    println!("✅ Test passed - Mined directly partial send detected correctly!");
    println!("   - Alice has 1 Send transaction, Bob has 1 Receive transaction");
    println!("   - All transactions are mined directly (no intermediate states)");
    println!("   - Correct transaction amounts detected");
}

/// Test 6: Alice Full Send Bob (Mined Directly)
/// Purpose: Test mined directly full send where wallet is completely emptied when mined directly
#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_alice_full_send_bob_mined_directly() {
    let mut env = IsolatedTestEnvironment::new().await.expect("Failed to create test environment");
    
    // Initial sync to establish baseline
    env.sync_and_wait().await.expect("Failed to sync");
    
    let initial_alice_transactions = env.get_wallet_transactions(&env.alice_checksum).await.expect("Failed to get Alice transactions");
    let initial_bob_transactions = env.get_wallet_transactions(&env.bob_checksum).await.expect("Failed to get Bob transactions");
    
    println!("📊 Initial state:");
    println!("   Alice transactions: {}", initial_alice_transactions.len());
    println!("   Bob transactions: {}", initial_bob_transactions.len());
    
    // Send maximum balance and immediately mine (full send scenario)
    println!("🔥 Step 1: Alice sends maximum balance to Bob (full send)");
    let _txid = env.send_transaction("alice", "bob", "max").await.expect("Failed to send max transaction");
    
    println!("⚡ Step 2: Immediately mine 1 block (before sync)");
    env.mine_blocks(1).await.expect("Failed to mine blocks");
    
    println!("⚡ Step 3: Now sync wallets");
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Verify full send transactions show mined directly state
    let post_sync_alice_transactions = env.get_wallet_transactions(&env.alice_checksum).await.expect("Failed to get Alice transactions");
    let post_sync_bob_transactions = env.get_wallet_transactions(&env.bob_checksum).await.expect("Failed to get Bob transactions");
    
    println!("📊 Post-sync state:");
    println!("   Alice transactions: {}", post_sync_alice_transactions.len());
    println!("   Bob transactions: {}", post_sync_bob_transactions.len());
    
    // Find new events (full send events)
    // Look for Send and Receive transactions in all transactions (not just "new" ones)
    let alice_send_transactions: Vec<_> = post_sync_alice_transactions.iter()
        .filter(|t| t.transaction_type == EventType::Send)
        .collect();
    let bob_receive_transactions: Vec<_> = post_sync_bob_transactions.iter()
        .filter(|t| t.transaction_type == EventType::Receive)
        .collect();
    
    // New Transaction System Expectations for Full Send:
    // Alice should have: 1 Receive (initial funding) + 1 Send (full send to Bob) = 2 total transactions
    // Bob should have: 1 Receive (from Alice) = 1 total transaction
    
    assert_eq!(post_sync_alice_transactions.len(), 2, 
        "Alice should have 2 transactions: 1 initial receive + 1 full send to Bob");
    assert_eq!(post_sync_bob_transactions.len(), 1, 
        "Bob should have 1 transaction: 1 receive from Alice");
    
    // Alice should have exactly 1 Send transaction
    assert_eq!(alice_send_transactions.len(), 1, 
        "Alice should have exactly 1 Send transaction for full send");
    
    // Bob should have exactly 1 Receive transaction  
    assert_eq!(bob_receive_transactions.len(), 1,
        "Bob should have exactly 1 Receive transaction");
    
    // Verify transactions are confirmed (mined directly)
    let alice_send = alice_send_transactions[0];
    let bob_receive = bob_receive_transactions[0];
    
    assert!(alice_send.block_height.is_some(), 
        "Alice's Send transaction should be confirmed (mined directly)");
    assert!(bob_receive.block_height.is_some(),
        "Bob's Receive transaction should be confirmed (mined directly)");
    
    // Verify full send amounts (should be close to 1 BTC minus fees)
    let expected_min_amount = 50_000_000i64; // At least 0.5 BTC (should be much more)
    
    assert!(alice_send.amount_sats >= expected_min_amount,
        "Alice's full send should be a large amount (> 0.5 BTC). Got: {} sats", alice_send.amount_sats);
    assert!(bob_receive.amount_sats >= expected_min_amount,
        "Bob should receive a large amount (> 0.5 BTC). Got: {} sats", bob_receive.amount_sats);
    
    // Bob's receive should be slightly less than Alice's send (due to fees)
    assert!(bob_receive.amount_sats <= alice_send.amount_sats,
        "Bob's receive amount should be less than or equal to Alice's send amount (due to fees)");
    
    println!("✅ Test passed - Mined directly full send detected correctly!");
    println!("   - Alice has 1 Send transaction, Bob has 1 Receive transaction");
    println!("   - All transactions are mined directly (no intermediate states)");
    println!("   - Large transaction amounts detected correctly (Alice: {} sats, Bob: {} sats)", 
             alice_send.amount_sats, bob_receive.amount_sats);
}

/// Test: Multiple partial sends mined directly
/// Additional test to verify multiple partial send transactions can be handled when mined directly
#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_multiple_partial_sends_mined_directly() {
    let mut env = IsolatedTestEnvironment::new().await.expect("Failed to create test environment");
    
    // Initial sync
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Send multiple partial send transactions and mine them all at once
    println!("⚡ Sending multiple partial send transactions that will be mined directly");
    
    let _txid1 = env.send_transaction("alice", "bob", "0.1").await.expect("Failed to send transaction 1");
    let _txid2 = env.send_transaction("alice", "bob", "0.2").await.expect("Failed to send transaction 2");
    let _txid3 = env.send_transaction("alice", "bob", "0.05").await.expect("Failed to send transaction 3");
    
    // Mine all transactions at once
    env.mine_blocks(1).await.expect("Failed to mine blocks");
    
    // Now sync
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Verify all partial send transactions are detected as mined directly
    let final_alice_transactions = env.get_wallet_transactions(&env.alice_checksum).await.expect("Failed to get Alice transactions");
    let final_bob_transactions = env.get_wallet_transactions(&env.bob_checksum).await.expect("Failed to get Bob transactions");
    
    // New Transaction System Expectations for Multiple Partial Sends:
    // Alice should have: 1 Receive (initial funding) + 3 Sends (to Bob) = 4 total transactions
    // Bob should have: 3 Receives (from Alice) = 3 total transactions
    
    assert_eq!(final_alice_transactions.len(), 4, 
        "Alice should have 4 transactions: 1 initial receive + 3 sends to Bob");
    assert_eq!(final_bob_transactions.len(), 3, 
        "Bob should have 3 transactions: 3 receives from Alice");
    
    // Find Send and Receive transactions
    let alice_send_transactions: Vec<_> = final_alice_transactions.iter()
        .filter(|t| t.transaction_type == EventType::Send)
        .collect();
    let bob_receive_transactions: Vec<_> = final_bob_transactions.iter()
        .filter(|t| t.transaction_type == EventType::Receive)
        .collect();
    
    // Alice should have exactly 3 Send transactions
    assert_eq!(alice_send_transactions.len(), 3, 
        "Alice should have exactly 3 Send transactions");
    
    // Bob should have exactly 3 Receive transactions  
    assert_eq!(bob_receive_transactions.len(), 3,
        "Bob should have exactly 3 Receive transactions");
    
    // All transactions should be confirmed (mined directly)
    let confirmed_alice_sends = alice_send_transactions.iter()
        .filter(|t| t.block_height.is_some())
        .count();
    let confirmed_bob_receives = bob_receive_transactions.iter()
        .filter(|t| t.block_height.is_some())
        .count();
        
    assert_eq!(confirmed_alice_sends, 3, "All 3 Alice Send transactions should be confirmed");
    assert_eq!(confirmed_bob_receives, 3, "All 3 Bob Receive transactions should be confirmed");
    
    // Verify expected amounts (0.1, 0.2, 0.05 BTC)
    let expected_amounts = [10_000_000i64, 20_000_000i64, 5_000_000i64]; // 0.1, 0.2, 0.05 BTC in sats
    let bob_amounts: Vec<i64> = bob_receive_transactions.iter().map(|t| t.amount_sats).collect();
    
    for expected_amount in expected_amounts {
        assert!(bob_amounts.contains(&expected_amount), 
            "Bob should have received {} sats. Got: {:?}", expected_amount, bob_amounts);
    }
    
    println!("✅ Multiple partial sends mined directly test passed!");
    println!("   - Alice has 3 Send transactions, Bob has 3 Receive transactions");
    println!("   - All transactions are mined directly (no intermediate states)");
    println!("   - Individual transactions detected (not aggregated): Alice sends: {}, Bob receives: {}", 
             confirmed_alice_sends, confirmed_bob_receives);
}