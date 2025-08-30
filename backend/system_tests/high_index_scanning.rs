use canary::metadata::EventType;

mod common;
use common::docker_environment::IsolatedTestEnvironment;

/// System tests for high-index address scanning and deep wallet discovery
/// 
/// These tests verify that wallets can detect funds and transactions at high
/// address indexes (250+) which is critical for wallet recovery scenarios.

#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_high_index_fund_detection() {
    let mut env = IsolatedTestEnvironment::new().await.expect("Failed to create test environment");
    
    // Initial sync should detect both Alice (index 0) and Charlie (index 250) funding
    env.sync_and_wait().await.expect("Failed to sync");
    
    let alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    let charlie_events = env.get_wallet_events(&env.charlie_checksum).await.expect("Failed to get Charlie events");
    
    // Alice should have events (funded at normal index 0)
    let alice_receive_events: Vec<_> = alice_events.iter()
        .filter(|e| e.event_type == EventType::Receive)
        .collect();
    assert!(!alice_receive_events.is_empty(), "Alice should have receive events from funding at index 0");
    
    // Charlie should have events (funded at high index 250)
    let charlie_receive_events: Vec<_> = charlie_events.iter()
        .filter(|e| e.event_type == EventType::Receive)
        .collect();
    assert!(!charlie_receive_events.is_empty(), "Charlie should have receive events from funding at index 250");
    
    println!("📊 Alice events (index 0): {}", alice_receive_events.len());
    println!("📊 Charlie events (index 250): {}", charlie_receive_events.len());
    
    println!("✅ High-index fund detection test passed!");
    println!("   - Alice funded at index 0: detected ✓");
    println!("   - Charlie funded at index 250: detected ✓"); 
}

#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_high_index_outgoing_transactions() {
    let mut env = IsolatedTestEnvironment::new().await.expect("Failed to create test environment");
    
    // Initial sync
    env.sync_and_wait().await.expect("Failed to sync");
    
    let initial_alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    
    // Test that Charlie can send from high index to Alice
    let initial_alice_event_count = initial_alice_events.len();
    
    // Send from Charlie (high index) to Alice (normal index)
    let _txid = env.send_transaction("charlie", "alice", "0.1").await.expect("Failed to send from Charlie");
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Verify the transaction was detected on both sides
    let updated_alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    let updated_charlie_events = env.get_wallet_events(&env.charlie_checksum).await.expect("Failed to get Charlie events");
    
    assert!(updated_alice_events.len() > initial_alice_event_count, "Alice should have new receive event from Charlie");
    
    let charlie_send_events: Vec<_> = updated_charlie_events.iter()
        .filter(|e| e.event_type == EventType::Send && !e.is_confirmed)
        .collect();
    assert!(!charlie_send_events.is_empty(), "Charlie should have new send event");
    
    println!("✅ High-index outgoing transaction test passed!");
    println!("   - Charlie can send from index 250: detected ✓");
    println!("   - Alice receives from high-index sender: detected ✓");
}

#[tokio::test]
#[ignore] // System test - requires Docker  
async fn test_address_revelation_up_to_high_indexes() {
    let mut env = IsolatedTestEnvironment::new().await.expect("Failed to create test environment");
    
    // This test verifies that our wallet manager can handle address revelation
    // up to at least index 250, which is where Charlie's funds are located
    
    env.sync_and_wait().await.expect("Failed to sync");
    
    let charlie_events = env.get_wallet_events(&env.charlie_checksum).await.expect("Failed to get Charlie events");
    
    // If Charlie has events, it means the wallet manager successfully:
    // 1. Revealed addresses from 0 to 250
    // 2. Detected the transaction at index 250
    // 3. Created appropriate database events
    
    let charlie_receive_events: Vec<_> = charlie_events.iter()
        .filter(|e| e.event_type == EventType::Receive)
        .collect();
        
    assert!(!charlie_receive_events.is_empty(), 
        "Charlie should have receive events, proving address revelation worked up to index 250");
    
    println!("✅ Address revelation test passed!");
    println!("   - Wallet manager successfully revealed addresses 0-250+");
    println!("   - Transaction detected at high index 250");
    println!("   - Events created for high-index transaction");
}