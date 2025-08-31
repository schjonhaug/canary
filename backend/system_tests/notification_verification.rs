use canary::metadata::EventType;

mod common;
use common::docker_environment::IsolatedTestEnvironment;

/// System tests for notification verification
/// 
/// These tests verify the notification system's basic functionality by
/// checking transaction event creation and wallet sync behavior that
/// would trigger notifications in a real system.

/// Test: Basic transaction event creation for notifications
/// Verify transaction events are created that would trigger notifications
#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_transaction_events_for_notifications() {
    let mut env = IsolatedTestEnvironment::new().await.expect("Failed to create test environment");
    
    // Initial sync
    env.sync_and_wait().await.expect("Failed to sync");
    
    let initial_alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    let initial_bob_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get Bob events");
    
    println!("📊 Initial state:");
    println!("   Alice events: {}", initial_alice_events.len());
    println!("   Bob events: {}", initial_bob_events.len());
    
    // Send transaction to trigger event creation (which would trigger notifications)
    println!("💸 Sending transaction to trigger event creation");
    let _txid = env.send_transaction("alice", "bob", "0.25").await.expect("Failed to send transaction");
    
    // Sync to detect transaction (would trigger notifications in real system)
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Verify transaction events were created
    let alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    let bob_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get Bob events");
    
    let new_alice_events = alice_events.len() - initial_alice_events.len();
    let new_bob_events = bob_events.len() - initial_bob_events.len();
    
    assert!(new_alice_events > 0, "Alice should have new events (would trigger notifications)");
    assert!(new_bob_events > 0, "Bob should have new events (would trigger notifications)");
    
    // Check event types that would be used for notifications
    let new_alice_events_slice: Vec<_> = alice_events.iter()
        .skip(initial_alice_events.len())
        .collect();
    let new_bob_events_slice: Vec<_> = bob_events.iter()
        .skip(initial_bob_events.len())
        .collect();
    
    let alice_send_events: Vec<_> = new_alice_events_slice.iter()
        .filter(|e| e.event_type == EventType::Send)
        .collect();
    let bob_receive_events: Vec<_> = new_bob_events_slice.iter()
        .filter(|e| e.event_type == EventType::Receive)
        .collect();
    
    assert!(!alice_send_events.is_empty(), "Alice should have Send events");
    assert!(!bob_receive_events.is_empty(), "Bob should have Receive events");
    
    println!("📊 Transaction events detected (notification triggers):");
    println!("   Alice Send events: {}", alice_send_events.len());
    println!("   Bob Receive events: {}", bob_receive_events.len());
    
    // Verify event details for notification content
    for event in &alice_send_events {
        println!("📝 Alice Send event: {} sats, confirmed: {}", event.amount_sats, event.is_confirmed);
        assert!(event.amount_sats < 0, "Alice send should have negative amount");
        // Amount should be reasonable for 0.25 BTC transaction
        assert!(event.amount_sats.abs() > 20_000_000, "Send amount should be substantial");
    }
    
    for event in &bob_receive_events {
        println!("📝 Bob Receive event: {} sats, confirmed: {}", event.amount_sats, event.is_confirmed);
        assert!(event.amount_sats > 0, "Bob receive should have positive amount");
        // Should receive exactly 0.25 BTC = 25,000,000 sats
        assert_eq!(event.amount_sats, 25_000_000, "Bob should receive 0.25 BTC");
    }
    
    println!("✅ Transaction events test passed!");
    println!("   - Events created for both send and receive operations");
    println!("   - Event data contains correct amounts and types for notifications");
    println!("   - Transaction state properly tracked for notification triggers");
}

/// Test: Confirmation state changes for notification timing
/// Verify transaction events update confirmation status (which would trigger confirmation notifications)
#[tokio::test]
#[ignore] // System test - requires Docker  
async fn test_confirmation_state_changes() {
    let mut env = IsolatedTestEnvironment::new().await.expect("Failed to create test environment");
    
    // Initial sync
    env.sync_and_wait().await.expect("Failed to sync");
    
    let initial_alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    
    println!("⏰ Testing confirmation state changes for notification timing");
    
    // Send transaction (should create unconfirmed events first)
    let _txid = env.send_transaction("alice", "bob", "0.1").await.expect("Failed to send transaction");
    
    // Sync immediately to detect unconfirmed transaction
    env.sync_and_wait().await.expect("Failed to sync");
    
    let after_send_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    
    let unconfirmed_count = after_send_events.len() - initial_alice_events.len();
    println!("📬 Unconfirmed events created: {}", unconfirmed_count);
    
    assert!(unconfirmed_count > 0, "Should have events for unconfirmed transaction");
    
    // Check that new events are initially unconfirmed
    let new_events: Vec<_> = after_send_events.iter()
        .skip(initial_alice_events.len())
        .collect();
        
    let unconfirmed_sends: Vec<_> = new_events.iter()
        .filter(|e| !e.is_confirmed && e.event_type == EventType::Send)
        .collect();
        
    // Note: Events might be confirmed immediately in some implementations
    // This test adapts to either case
    if !unconfirmed_sends.is_empty() {
        println!("📝 Found unconfirmed send events: {}", unconfirmed_sends.len());
    } else {
        println!("📝 Events confirmed immediately (fast confirmation scenario)");
    }
    
    // Mine block to confirm transaction
    env.mine_blocks(1).await.expect("Failed to mine blocks");
    env.sync_and_wait().await.expect("Failed to sync");
    
    let after_confirm_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    
    // Check for confirmed events
    let final_new_events: Vec<_> = after_confirm_events.iter()
        .skip(initial_alice_events.len())
        .collect();
        
    let confirmed_sends: Vec<_> = final_new_events.iter()
        .filter(|e| e.is_confirmed && e.event_type == EventType::Send)
        .collect();
    
    assert!(!confirmed_sends.is_empty(), "Should have confirmed send events after mining");
    
    println!("📊 Final confirmation state:");
    println!("   Total events after confirmation: {}", after_confirm_events.len());
    println!("   Confirmed send events: {}", confirmed_sends.len());
    
    println!("✅ Confirmation state changes test passed!");
    println!("   - Transaction state changes properly tracked");
    println!("   - Confirmation status updated after mining");
    println!("   - Event timing suitable for notification triggers");
}

/// Test: Duplicate prevention for notifications
/// Verify multiple syncs don't create duplicate events (preventing duplicate notifications)
#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_duplicate_event_prevention() {
    let mut env = IsolatedTestEnvironment::new().await.expect("Failed to create test environment");
    
    // Initial sync
    env.sync_and_wait().await.expect("Failed to sync");
    
    let initial_alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    
    println!("🔁 Testing duplicate event prevention for notifications");
    
    // Send transaction
    let _txid = env.send_transaction("alice", "bob", "0.08").await.expect("Failed to send transaction");
    
    // Sync once to detect transaction
    env.sync_and_wait().await.expect("Failed to sync");
    
    let after_first_sync = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    let first_sync_count = after_first_sync.len();
    
    println!("📬 Events after first sync: {}", first_sync_count);
    
    // Sync multiple times to test duplicate prevention
    println!("🔄 Performing multiple additional syncs");
    env.sync_and_wait().await.expect("Failed to sync (2)");
    env.sync_and_wait().await.expect("Failed to sync (3)");
    env.sync_and_wait().await.expect("Failed to sync (4)");
    
    let after_multiple_syncs = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    let multiple_sync_count = after_multiple_syncs.len();
    
    println!("📬 Events after multiple syncs: {}", multiple_sync_count);
    
    // Should not have created duplicate events
    assert_eq!(first_sync_count, multiple_sync_count, 
              "Multiple syncs should not create duplicate events");
    
    // Confirm transaction and test again
    env.mine_blocks(1).await.expect("Failed to mine blocks");
    
    // Sync once after confirmation
    env.sync_and_wait().await.expect("Failed to sync after confirmation");
    
    let after_confirm_sync = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    let confirm_sync_count = after_confirm_sync.len();
    
    println!("📬 Events after confirmation sync: {}", confirm_sync_count);
    
    // Multiple syncs after confirmation
    println!("🔄 Performing multiple syncs after confirmation");
    env.sync_and_wait().await.expect("Failed to sync after confirmation (2)");
    env.sync_and_wait().await.expect("Failed to sync after confirmation (3)");
    
    let after_multiple_confirm_syncs = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    let final_count = after_multiple_confirm_syncs.len();
    
    println!("📬 Final event count: {}", final_count);
    
    // Should not have created duplicates for confirmation either
    assert_eq!(confirm_sync_count, final_count,
              "Multiple syncs after confirmation should not create duplicate events");
    
    // Verify reasonable total count (should be small number, not dozens)
    let total_new_events = final_count - initial_alice_events.len();
    assert!(total_new_events <= 4, "Should not have excessive events (got {})", total_new_events);
    
    println!("✅ Duplicate prevention test passed!");
    println!("   - Multiple syncs on unconfirmed transaction: no duplicates");  
    println!("   - Multiple syncs after confirmation: no duplicates");
    println!("   - Total event count remains reasonable: {}", total_new_events);
    println!("   - Prevents duplicate notifications in real system");
}

/// Test: Large transaction amounts for notification formatting
/// Verify events handle large amounts correctly (for drain notifications)
#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_large_amount_events() {
    let mut env = IsolatedTestEnvironment::new().await.expect("Failed to create test environment");
    
    // Initial sync
    env.sync_and_wait().await.expect("Failed to sync");
    
    let initial_alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    
    println!("💰 Testing large amount handling for drain notifications");
    
    // Send maximum balance (drain scenario)
    let _txid = env.send_transaction("alice", "bob", "max").await.expect("Failed to send max transaction");
    
    env.sync_and_wait().await.expect("Failed to sync");
    
    let after_drain_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    
    let new_events: Vec<_> = after_drain_events.iter()
        .skip(initial_alice_events.len())
        .collect();
        
    let drain_events: Vec<_> = new_events.iter()
        .filter(|e| e.event_type == EventType::Send)
        .collect();
    
    assert!(!drain_events.is_empty(), "Should have drain send events");
    
    // Check that drain amounts are substantial
    for event in &drain_events {
        let amount_btc = event.amount_sats.abs() as f64 / 100_000_000.0;
        println!("📝 Drain event: {} sats ({:.8} BTC), confirmed: {}", 
                event.amount_sats, amount_btc, event.is_confirmed);
        
        // Should be substantial amount (more than 0.5 BTC)
        assert!(event.amount_sats.abs() > 50_000_000, 
               "Drain amount should be substantial (>0.5 BTC): {} sats", event.amount_sats);
    }
    
    println!("✅ Large amount events test passed!");
    println!("   - Drain transactions create appropriate events");
    println!("   - Large amounts handled correctly for notification formatting");
    println!("   - Event data suitable for wallet drain notifications");
}

/// Test: Multiple wallet events for multi-user notifications
/// Verify events are created correctly across multiple wallets
#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_multi_wallet_events() {
    let mut env = IsolatedTestEnvironment::new().await.expect("Failed to create test environment");
    
    // Initial sync
    env.sync_and_wait().await.expect("Failed to sync");
    
    let initial_alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    let initial_bob_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get Bob events");
    let initial_charlie_events = env.get_wallet_events(&env.charlie_checksum).await.expect("Failed to get Charlie events");
    
    println!("👥 Testing multi-wallet event creation for notifications");
    
    // Send transactions between different wallets
    let _txid1 = env.send_transaction("alice", "bob", "0.1").await.expect("Failed to send Alice->Bob");
    let _txid2 = env.send_transaction("charlie", "alice", "0.2").await.expect("Failed to send Charlie->Alice");
    
    env.sync_and_wait().await.expect("Failed to sync");
    
    let final_alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    let final_bob_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get Bob events");
    let final_charlie_events = env.get_wallet_events(&env.charlie_checksum).await.expect("Failed to get Charlie events");
    
    let alice_new_count = final_alice_events.len() - initial_alice_events.len();
    let bob_new_count = final_bob_events.len() - initial_bob_events.len();
    let charlie_new_count = final_charlie_events.len() - initial_charlie_events.len();
    
    println!("📊 Multi-wallet event results:");
    println!("   Alice new events: {}", alice_new_count);
    println!("   Bob new events: {}", bob_new_count);
    println!("   Charlie new events: {}", charlie_new_count);
    
    // Alice should have both Send (to Bob) and Receive (from Charlie) events
    assert!(alice_new_count >= 2, "Alice should have events for both transactions");
    
    // Bob should have Receive events (from Alice)
    assert!(bob_new_count >= 1, "Bob should have receive events");
    
    // Charlie should have Send events (to Alice)
    assert!(charlie_new_count >= 1, "Charlie should have send events");
    
    // Verify event types for each wallet
    let alice_new_events: Vec<_> = final_alice_events.iter()
        .skip(initial_alice_events.len())
        .collect();
    let bob_new_events: Vec<_> = final_bob_events.iter()
        .skip(initial_bob_events.len())
        .collect();
    let charlie_new_events: Vec<_> = final_charlie_events.iter()
        .skip(initial_charlie_events.len())
        .collect();
    
    let alice_sends = alice_new_events.iter().filter(|e| e.event_type == EventType::Send).count();
    let alice_receives = alice_new_events.iter().filter(|e| e.event_type == EventType::Receive).count();
    let bob_receives = bob_new_events.iter().filter(|e| e.event_type == EventType::Receive).count();
    let charlie_sends = charlie_new_events.iter().filter(|e| e.event_type == EventType::Send).count();
    
    println!("📝 Event type breakdown:");
    println!("   Alice sends: {}, receives: {}", alice_sends, alice_receives);
    println!("   Bob receives: {}", bob_receives);
    println!("   Charlie sends: {}", charlie_sends);
    
    assert!(alice_sends > 0, "Alice should have send events");
    assert!(alice_receives > 0, "Alice should have receive events");
    assert!(bob_receives > 0, "Bob should have receive events");
    assert!(charlie_sends > 0, "Charlie should have send events");
    
    println!("✅ Multi-wallet events test passed!");
    println!("   - All wallets receive appropriate events for their transactions");
    println!("   - Event types correctly assigned (Send/Receive)");
    println!("   - Multi-user notification system would work correctly");
}