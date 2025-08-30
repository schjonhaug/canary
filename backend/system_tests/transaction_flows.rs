use canary::metadata::EventType;

mod common;
use common::docker_environment::IsolatedTestEnvironment;

/// System tests for normal transaction flows (send/receive scenarios)
/// 
/// These tests verify the basic transaction detection pipeline works correctly
/// for typical Bitcoin transactions between wallets.

#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_normal_send_receive_flow() {
    let mut env = IsolatedTestEnvironment::new().await.expect("Failed to create test environment");
    
    // Initial sync
    env.sync_and_wait().await.expect("Failed to sync");
    
    let initial_alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get events");
    let initial_bob_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get events");
    
    // Send 0.1 BTC from Alice to Bob
    let _txid = env.send_transaction("alice", "bob", "0.1").await.expect("Failed to send transaction");
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Verify Alice has "Sending" event and Bob has "Receiving" event
    let alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get events");
    let bob_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get events");
    
    assert!(alice_events.len() > initial_alice_events.len(), "Alice should have new send event");
    assert!(bob_events.len() > initial_bob_events.len(), "Bob should have new receive event");
    
    let new_send_events: Vec<_> = alice_events.iter()
        .filter(|e| e.event_type == EventType::Send && !e.is_confirmed)
        .collect();
    let new_receive_events: Vec<_> = bob_events.iter()
        .filter(|e| e.event_type == EventType::Receive && !e.is_confirmed)
        .collect();
        
    assert_eq!(new_send_events.len(), 1, "Should have one unconfirmed send event");
    assert_eq!(new_receive_events.len(), 1, "Should have one unconfirmed receive event");
    
    // Mine block to confirm
    env.mine_blocks(1).await.expect("Failed to mine blocks");
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Verify confirmation events
    let final_alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get events");
    let final_bob_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get events");
    
    let confirmed_send_events: Vec<_> = final_alice_events.iter()
        .filter(|e| e.event_type == EventType::Send && e.is_confirmed)
        .collect();
    let confirmed_receive_events: Vec<_> = final_bob_events.iter()
        .filter(|e| e.event_type == EventType::Receive && e.is_confirmed)
        .collect();
        
    assert!(!confirmed_send_events.is_empty(), "Should have confirmed send event");
    assert!(!confirmed_receive_events.is_empty(), "Should have confirmed receive event");
    
    println!("✅ Normal send/receive transaction flow test passed!");
}

#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_multiple_transactions() {
    let mut env = IsolatedTestEnvironment::new().await.expect("Failed to create test environment");
    
    // Initial sync
    env.sync_and_wait().await.expect("Failed to sync");
    
    let initial_alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get events");
    let initial_bob_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get events");
    
    // Send multiple transactions
    let _txid1 = env.send_transaction("alice", "bob", "0.1").await.expect("Failed to send transaction 1");
    let _txid2 = env.send_transaction("alice", "bob", "0.2").await.expect("Failed to send transaction 2");
    let _txid3 = env.send_transaction("alice", "bob", "0.05").await.expect("Failed to send transaction 3");
    
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Verify multiple events were created
    let alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get events");
    let bob_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get events");
    
    let new_alice_events = alice_events.len() - initial_alice_events.len();
    let new_bob_events = bob_events.len() - initial_bob_events.len();
    
    assert_eq!(new_alice_events, 3, "Alice should have 3 new send events");
    assert_eq!(new_bob_events, 3, "Bob should have 3 new receive events");
    
    // Verify amounts are correct
    let alice_send_events: Vec<_> = alice_events.iter()
        .filter(|e| e.event_type == EventType::Send && !e.is_confirmed)
        .collect();
    let bob_receive_events: Vec<_> = bob_events.iter()
        .filter(|e| e.event_type == EventType::Receive && !e.is_confirmed)
        .collect();
    
    // Check that we have the expected amounts (in satoshis)
    let expected_amounts = [10_000_000i64, 20_000_000i64, 5_000_000i64]; // 0.1, 0.2, 0.05 BTC
    let mut alice_amounts: Vec<i64> = alice_send_events.iter().map(|e| e.amount_sats.abs()).collect();
    let mut bob_amounts: Vec<i64> = bob_receive_events.iter().map(|e| e.amount_sats).collect();
    
    alice_amounts.sort();
    bob_amounts.sort();
    let mut expected_sorted = expected_amounts.to_vec();
    expected_sorted.sort();
    
    assert_eq!(alice_amounts, expected_sorted, "Alice should have correct send amounts");
    assert_eq!(bob_amounts, expected_sorted, "Bob should have correct receive amounts");
    
    println!("✅ Multiple transactions test passed!");
}

#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_no_duplicate_events_on_multiple_syncs() {
    let mut env = IsolatedTestEnvironment::new().await.expect("Failed to create test environment");
    
    // Initial sync
    env.sync_and_wait().await.expect("Failed to sync");
    
    let initial_alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get events");
    let initial_bob_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get events");
    
    // Send transaction but don't confirm immediately  
    let _txid = env.send_transaction("alice", "bob", "0.1").await.expect("Failed to send transaction");
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Sync multiple times to ensure no duplicate events are created
    env.sync_and_wait().await.expect("Failed to sync");
    env.sync_and_wait().await.expect("Failed to sync");
    
    let alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    let bob_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get Bob events");
    
    // Should have exactly one new event each
    assert_eq!(
        alice_events.len() - initial_alice_events.len(), 
        1, 
        "Alice should have exactly one new event"
    );
    assert_eq!(
        bob_events.len() - initial_bob_events.len(),
        1,
        "Bob should have exactly one new event"
    );
    
    // Now confirm the transaction
    env.mine_blocks(1).await.expect("Failed to mine blocks");
    env.sync_and_wait().await.expect("Failed to sync");
    env.sync_and_wait().await.expect("Failed to sync"); // Sync again to check for duplicates
    
    let final_alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    let final_bob_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get Bob events");
    
    // Should have exactly 2 events each (unconfirmed + confirmed, OR just confirmed if fast)
    assert!(
        final_alice_events.len() - initial_alice_events.len() <= 2,
        "Alice should have at most 2 new events (sending + sent)"
    );
    assert!(
        final_bob_events.len() - initial_bob_events.len() <= 2, 
        "Bob should have at most 2 new events (receiving + received)"
    );
    
    println!("✅ No duplicate events test passed!");
}