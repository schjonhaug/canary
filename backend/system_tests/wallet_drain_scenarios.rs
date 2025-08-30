use canary::metadata::EventType;

mod common;
use common::docker_environment::IsolatedTestEnvironment;

/// System tests for wallet drain detection scenarios
/// 
/// These tests verify that the wallet sync logic correctly detects and creates
/// transaction events when users drain their wallets (send entire balance).

#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_alice_wallet_drain() {
    let mut env = IsolatedTestEnvironment::new().await.expect("Failed to create test environment");
    
    // Initial sync to detect Alice's funding
    env.sync_and_wait().await.expect("Failed to sync");
    
    let initial_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get events");
    println!("📊 Initial events for Alice: {} events", initial_events.len());
    
    // Drain Alice's entire wallet to Bob
    let _txid = env.send_transaction("alice", "bob", "max").await.expect("Failed to drain wallet");
    
    // Sync to detect the drain transaction
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Check events after drain (before confirmation)
    let post_drain_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get events");
    println!("📊 Post-drain events for Alice: {} events", post_drain_events.len());
    
    let new_send_events: Vec<_> = post_drain_events.iter()
        .filter(|e| e.event_type == EventType::Send && !initial_events.iter().any(|ie| ie.id == e.id))
        .collect();
    
    // THIS IS THE KEY TEST - wallet drain should create exactly one event
    assert_eq!(new_send_events.len(), 1, "Should have exactly one wallet drain event, got: {}", new_send_events.len());
    assert_eq!(new_send_events[0].is_confirmed, false, "Drain event should be unconfirmed initially");
    
    // Mine a block to confirm the transaction
    env.mine_blocks(1).await.expect("Failed to mine blocks");
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Verify the transaction is now confirmed
    let final_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get events");
    let confirmed_send_events: Vec<_> = final_events.iter()
        .filter(|e| e.event_type == EventType::Send && e.is_confirmed)
        .collect();
    
    assert!(!confirmed_send_events.is_empty(), "Should have confirmed send event after mining");
    
    println!("✅ Alice wallet drain test passed - transaction events created correctly!");
}

#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_charlie_wallet_drain_from_high_index() {
    let mut env = IsolatedTestEnvironment::new().await.expect("Failed to create test environment");
    
    // Initial sync to detect Charlie's high-index funding
    env.sync_and_wait().await.expect("Failed to sync");
    
    let initial_charlie_events = env.get_wallet_events(&env.charlie_checksum).await.expect("Failed to get Charlie events");
    println!("📊 Initial events for Charlie: {} events", initial_charlie_events.len());
    
    // Drain Charlie's entire wallet (from high index 250) to Bob
    let _txid = env.send_transaction("charlie", "bob", "max").await.expect("Failed to drain Charlie's wallet");
    
    // Sync to detect the drain transaction
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Check events after drain
    let post_drain_charlie_events = env.get_wallet_events(&env.charlie_checksum).await.expect("Failed to get Charlie events");
    println!("📊 Post-drain events for Charlie: {} events", post_drain_charlie_events.len());
    
    let new_send_events: Vec<_> = post_drain_charlie_events.iter()
        .filter(|e| e.event_type == EventType::Send && !initial_charlie_events.iter().any(|ie| ie.id == e.id))
        .collect();
    
    // Test wallet drain from high index
    assert_eq!(new_send_events.len(), 1, "Charlie should have exactly one wallet drain event from index 250, got: {}", new_send_events.len());
    assert_eq!(new_send_events[0].is_confirmed, false, "Charlie's drain event should be unconfirmed initially");
    
    // Mine to confirm and verify
    env.mine_blocks(1).await.expect("Failed to mine blocks");
    env.sync_and_wait().await.expect("Failed to sync");
    
    let final_charlie_events = env.get_wallet_events(&env.charlie_checksum).await.expect("Failed to get Charlie events");
    let confirmed_send_events: Vec<_> = final_charlie_events.iter()
        .filter(|e| e.event_type == EventType::Send && e.is_confirmed)
        .collect();
    
    assert!(!confirmed_send_events.is_empty(), "Charlie should have confirmed send event after mining");
    
    println!("✅ Charlie wallet drain test passed - high-index wallet drain detected correctly!");
}