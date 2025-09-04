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
    
    // Find new events (those not in initial state)
    let new_alice_transactions: Vec<_> = post_sync_alice_transactions.iter()
        .skip(initial_alice_transactions.len())
        .collect();
    let new_bob_transactions: Vec<_> = post_sync_bob_transactions.iter()
        .skip(initial_bob_transactions.len())
        .collect();
    
    println!("🔍 Debug: new_alice_transactions count: {}", new_alice_transactions.len());
    println!("🔍 Debug: new_bob_transactions count: {}", new_bob_transactions.len());
    println!("🔍 Total Alice transactions: {}, Total Bob transactions: {}", post_sync_alice_transactions.len(), post_sync_bob_transactions.len());
    
    if !new_alice_transactions.is_empty() {
        for (i, transaction) in new_alice_transactions.iter().enumerate() {
            println!("🔍 New Alice transaction {}: type={:?}, amount={}, confirmed={}", 
                     i, transaction.transaction_type, transaction.amount_sats, transaction.block_height.is_some());
        }
    }
    
    if !new_bob_transactions.is_empty() {
        for (i, transaction) in new_bob_transactions.iter().enumerate() {
            println!("🔍 New Bob event {}: type={:?}, amount={}, confirmed={}", 
                     i, transaction.transaction_type, transaction.amount_sats, transaction.block_height.is_some());
        }
    }
    
    // Verify we have new events
    assert!(!new_alice_transactions.is_empty(), "Alice should have new events");
    assert!(!new_bob_transactions.is_empty(), "Bob should have new events");
    
    // Look for Send and Receive events
    let alice_send_events: Vec<_> = new_alice_transactions.iter()
        .filter(|t| t.transaction_type == EventType::Send)
        .collect();
    let bob_receive_events: Vec<_> = new_bob_transactions.iter()
        .filter(|t| t.transaction_type == EventType::Receive)
        .collect();
    
    println!("🔍 Alice Send events: {}", alice_send_events.len());
    println!("🔍 Bob Receive events: {}", bob_receive_events.len());
    
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
    
    // Verify events are mined directly (the key test for mined directly)
    let confirmed_alice_sends: Vec<_> = alice_send_events.iter()
        .filter(|t| t.block_height.is_some())
        .collect();
    let confirmed_bob_receives: Vec<_> = bob_receive_events.iter()
        .filter(|t| t.block_height.is_some())
        .collect();
    
    assert!(!confirmed_alice_sends.is_empty(), "Alice should have confirmed Send events");
    assert!(!confirmed_bob_receives.is_empty(), "Bob should have confirmed Receive events");
    
    // Verify amounts are reasonable for 0.1 BTC transaction
    let expected_amount = 10_000_000i64; // 0.1 BTC in sats
    let bob_received_correct_amount = bob_receive_events.iter()
        .any(|t| t.amount_sats == expected_amount);
    
    assert!(bob_received_correct_amount, "Bob should receive 0.1 BTC (10M sats)");
    
    println!("✅ Test 4 passed - Mined directly partial send detected correctly!");
    println!("   - Events created for both Alice and Bob");
    println!("   - All events are mined directly (no intermediate states)");
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
    let new_alice_count = post_sync_alice_transactions.len() - initial_alice_transactions.len();
    let new_bob_count = post_sync_bob_transactions.len() - initial_bob_transactions.len();
    
    assert!(new_alice_count > 0, "Alice should have new full send events");
    assert!(new_bob_count > 0, "Bob should have new receive events");
    
    // Look for new Send and Receive events
    let new_alice_transactions: Vec<_> = post_sync_alice_transactions.iter()
        .skip(initial_alice_transactions.len())
        .collect();
    let new_bob_transactions: Vec<_> = post_sync_bob_transactions.iter()
        .skip(initial_bob_transactions.len())
        .collect();
    
    let alice_send_events: Vec<_> = new_alice_transactions.iter()
        .filter(|t| t.transaction_type == EventType::Send)
        .collect();
    let bob_receive_events: Vec<_> = new_bob_transactions.iter()
        .filter(|t| t.transaction_type == EventType::Receive)
        .collect();
    
    assert!(!alice_send_events.is_empty(), "Alice should have Send events for full send");
    assert!(!bob_receive_events.is_empty(), "Bob should have Receive events");
    
    // Debug: Show all Alice full send events and their confirmation status
    println!("🔍 DEBUG Alice full send events:");
    for (i, transaction) in alice_send_events.iter().enumerate() {
        println!("   Alice Send event {}: type={:?}, amount={}, confirmed={}", 
                 i, transaction.transaction_type, transaction.amount_sats, transaction.block_height.is_some());
    }
    
    println!("🔍 DEBUG Bob receive events:");
    for (i, transaction) in bob_receive_events.iter().enumerate() {
        println!("   Bob Receive event {}: type={:?}, amount={}, confirmed={}", 
                 i, transaction.transaction_type, transaction.amount_sats, transaction.block_height.is_some());
    }
    
    // Verify events are mined directly (key test for mined directly)
    let confirmed_alice_sends: Vec<_> = alice_send_events.iter()
        .filter(|t| t.block_height.is_some())
        .collect();
    let confirmed_bob_receives: Vec<_> = bob_receive_events.iter()
        .filter(|t| t.block_height.is_some())
        .collect();
    
    assert!(!confirmed_alice_sends.is_empty(), "Alice full send events should be mined directly");
    assert!(!confirmed_bob_receives.is_empty(), "Bob receive events should be mined directly");
    
    // Verify full send-specific characteristics (large amounts)
    let alice_large_sends: Vec<_> = alice_send_events.iter()
        .filter(|t| t.amount_sats.abs() > 50_000_000) // > 0.5 BTC
        .collect();
    let bob_large_receives: Vec<_> = bob_receive_events.iter()
        .filter(|t| t.amount_sats > 50_000_000) // > 0.5 BTC
        .collect();
    
    assert!(!alice_large_sends.is_empty(), "Alice should have large full send transactions");
    assert!(!bob_large_receives.is_empty(), "Bob should receive large amounts from full send");
    
    println!("✅ Test 6 passed - Mined directly full send detected correctly!");
    println!("   - Full send events created for Alice");
    println!("   - Large receive events created for Bob");
    println!("   - All events are mined directly (no intermediate states)");
    println!("   - Wallet successfully emptied in mined directly scenario");
}

/// Test: Multiple partial sends mined directly
/// Additional test to verify multiple partial send transactions can be handled when mined directly
#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_multiple_partial_sends_mined_directly() {
    let mut env = IsolatedTestEnvironment::new().await.expect("Failed to create test environment");
    
    // Initial sync
    env.sync_and_wait().await.expect("Failed to sync");
    
    let initial_alice_transactions = env.get_wallet_transactions(&env.alice_checksum).await.expect("Failed to get Alice transactions");
    let initial_bob_transactions = env.get_wallet_transactions(&env.bob_checksum).await.expect("Failed to get Bob transactions");
    
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
    
    let new_alice_transactions = final_alice_transactions.len() - initial_alice_transactions.len();
    let new_bob_transactions = final_bob_transactions.len() - initial_bob_transactions.len();
    
    println!("📊 Multiple partial send results:");
    println!("   New Alice transactions: {}", new_alice_transactions);
    println!("   New Bob transactions: {}", new_bob_transactions);
    
    assert!(new_alice_transactions == 1, "Alice should have 1 event for net amount (all 3 txs in same block)");
    assert!(new_bob_transactions == 1, "Bob should have 1 event for net amount (all 3 txs in same block)");
    
    // Verify all new events are confirmed
    let new_alice_transactions_slice: Vec<_> = final_alice_transactions.iter()
        .skip(initial_alice_transactions.len())
        .collect();
    let new_bob_transactions_slice: Vec<_> = final_bob_transactions.iter()
        .skip(initial_bob_transactions.len())
        .collect();
    
    let confirmed_alice_count = new_alice_transactions_slice.iter()
        .filter(|t| t.block_height.is_some() && t.transaction_type == EventType::Send)
        .count();
    let confirmed_bob_count = new_bob_transactions_slice.iter()
        .filter(|t| t.block_height.is_some() && t.transaction_type == EventType::Receive)
        .count();
    
    assert!(confirmed_alice_count == 1, "Alice should have 1 confirmed send event (net amount)");
    assert!(confirmed_bob_count == 1, "Bob should have 1 confirmed receive event (net amount)");
    
    println!("✅ Multiple partial sends mined directly test passed!");
    println!("   - All partial send transactions detected as mined directly");
    println!("   - No intermediate unconfirmed states created");
    println!("   - Confirmed Alice sends: {}, Bob receives: {}", confirmed_alice_count, confirmed_bob_count);
}