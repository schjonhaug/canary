use canary::metadata::EventType;

mod common;
use common::docker_environment::IsolatedTestEnvironment;

/// System tests for two-stage send scenarios
/// 
/// These tests verify that the wallet sync logic correctly creates transaction events
/// in the proper two-stage flow: unconfirmed → confirmed for both partial and full sends.

#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_alice_partial_send_bob_two_stage() {
    let mut env = IsolatedTestEnvironment::new().await.expect("Failed to create test environment");
    
    // Initial sync
    env.sync_and_wait().await.expect("Failed to sync");
    
    let initial_alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    let initial_bob_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get Bob events");
    
    println!("📊 Initial state:");
    println!("   Alice events: {}", initial_alice_events.len());
    println!("   Bob events: {}", initial_bob_events.len());
    
    // Send partial amount (0.1 BTC) - DON'T mine immediately
    println!("⚡ Step 1: Alice sends 0.1 BTC to Bob (partial send)");
    let _txid = env.send_transaction("alice", "bob", "0.1").await.expect("Failed to send transaction");
    
    println!("⚡ Step 2: Sync to detect unconfirmed transaction");
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Small delay to ensure database transactions are committed
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Verify UNCONFIRMED events are created (Stage 1)
    let unconfirmed_alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    let unconfirmed_bob_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get Bob events");
    
    println!("🔍 DEBUG: Alice events after partial send sync:");
    println!("   Initial Alice events: {}", initial_alice_events.len());
    println!("   Total Alice events: {}", unconfirmed_alice_events.len());
    for (i, event) in unconfirmed_alice_events.iter().enumerate() {
        let is_new = i >= initial_alice_events.len();
        println!("   Event {}: type={:?}, amount={}, confirmed={}, NEW={}", i, event.event_type, event.amount_sats, event.is_confirmed, is_new);
    }
    
    // Events are stored in reverse chronological order (newest first)
    // Find new events by excluding events that existed initially
    let initial_alice_ids: Vec<Option<String>> = initial_alice_events.iter().map(|e| e.id.clone()).collect();
    let initial_bob_ids: Vec<Option<String>> = initial_bob_events.iter().map(|e| e.id.clone()).collect();
    
    let alice_unconfirmed_sends: Vec<_> = unconfirmed_alice_events.iter()
        .filter(|e| !initial_alice_ids.contains(&e.id)) // Only new events
        .filter(|e| e.event_type == EventType::Send && !e.is_confirmed)
        .collect();
    let bob_unconfirmed_receives: Vec<_> = unconfirmed_bob_events.iter()
        .filter(|e| !initial_bob_ids.contains(&e.id)) // Only new events
        .filter(|e| e.event_type == EventType::Receive && !e.is_confirmed)
        .collect();
    
    println!("🔍 DEBUG: Filtered results:");
    println!("   Alice unconfirmed sends: {}", alice_unconfirmed_sends.len());
    
    assert_eq!(alice_unconfirmed_sends.len(), 1, "Alice should have exactly one unconfirmed Send event");
    assert_eq!(bob_unconfirmed_receives.len(), 1, "Bob should have exactly one unconfirmed Receive event");
    
    println!("✅ Stage 1: Unconfirmed events created correctly");
    
    // Mine block to confirm transaction
    println!("⚡ Step 3: Mine block to confirm transaction");
    env.mine_blocks(1).await.expect("Failed to mine blocks");
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Verify CONFIRMED events exist (Stage 2)
    let final_alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    let final_bob_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get Bob events");
    
    let alice_confirmed_sends: Vec<_> = final_alice_events.iter()
        .filter(|e| e.event_type == EventType::Send && e.is_confirmed)
        .collect();
    let bob_confirmed_receives: Vec<_> = final_bob_events.iter()
        .filter(|e| e.event_type == EventType::Receive && e.is_confirmed)
        .collect();
    
    assert!(!alice_confirmed_sends.is_empty(), "Alice should have confirmed Send event after mining");
    assert!(!bob_confirmed_receives.is_empty(), "Bob should have confirmed Receive event after mining");
    
    // Verify amount is correct (0.1 BTC = 10M sats)
    let expected_amount = 10_000_000i64;
    let bob_received_correct_amount = bob_confirmed_receives.iter()
        .any(|e| e.amount_sats == expected_amount);
    
    assert!(bob_received_correct_amount, "Bob should receive 0.1 BTC (10M sats)");
    
    println!("✅ Stage 2: Confirmed events created correctly");
    println!("✅ Alice partial send two-stage test passed - events created in proper sequence!");
}

#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_alice_full_send_bob_two_stage() {
    let mut env = IsolatedTestEnvironment::new().await.expect("Failed to create test environment");
    
    // Initial sync
    env.sync_and_wait().await.expect("Failed to sync");
    
    let initial_alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    let initial_bob_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get Bob events");
    
    println!("📊 Initial state:");
    println!("   Alice events: {}", initial_alice_events.len());
    println!("   Bob events: {}", initial_bob_events.len());
    
    // Send entire wallet balance - DON'T mine immediately
    println!("🔥 Step 1: Alice sends maximum balance to Bob (full send / wallet drain)");
    let _txid = env.send_transaction("alice", "bob", "max").await.expect("Failed to send max transaction");
    
    println!("⚡ Step 2: Sync to detect unconfirmed full send transaction");
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Small delay to ensure database transactions are committed
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Verify UNCONFIRMED events are created (Stage 1)
    let unconfirmed_alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    let unconfirmed_bob_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get Bob events");
    
    // Events are stored in reverse chronological order (newest first)
    // Find new events by excluding events that existed initially
    let initial_alice_ids: Vec<Option<String>> = initial_alice_events.iter().map(|e| e.id.clone()).collect();
    let initial_bob_ids: Vec<Option<String>> = initial_bob_events.iter().map(|e| e.id.clone()).collect();
    
    let alice_unconfirmed_sends: Vec<_> = unconfirmed_alice_events.iter()
        .filter(|e| !initial_alice_ids.contains(&e.id)) // Only new events
        .filter(|e| e.event_type == EventType::Send && !e.is_confirmed)
        .collect();
    let bob_unconfirmed_receives: Vec<_> = unconfirmed_bob_events.iter()
        .filter(|e| !initial_bob_ids.contains(&e.id)) // Only new events
        .filter(|e| e.event_type == EventType::Receive && !e.is_confirmed)
        .collect();
    
    assert_eq!(alice_unconfirmed_sends.len(), 1, "Alice should have exactly one unconfirmed Send event for full send");
    assert_eq!(bob_unconfirmed_receives.len(), 1, "Bob should have exactly one unconfirmed Receive event");
    
    // Verify it's a large amount (full send should be > 0.5 BTC)
    let alice_full_send_amount = alice_unconfirmed_sends[0].amount_sats.abs();
    assert!(alice_full_send_amount > 50_000_000, "Alice should be sending large amount (> 0.5 BTC), got: {} sats", alice_full_send_amount);
    
    println!("✅ Stage 1: Unconfirmed full send events created correctly");
    
    // Mine block to confirm transaction
    println!("⚡ Step 3: Mine block to confirm full send transaction");
    env.mine_blocks(1).await.expect("Failed to mine blocks");
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Verify CONFIRMED events exist (Stage 2)
    let final_alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    let final_bob_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get Bob events");
    
    let alice_confirmed_sends: Vec<_> = final_alice_events.iter()
        .filter(|e| e.event_type == EventType::Send && e.is_confirmed)
        .collect();
    let bob_confirmed_receives: Vec<_> = final_bob_events.iter()
        .filter(|e| e.event_type == EventType::Receive && e.is_confirmed)
        .collect();
    
    assert!(!alice_confirmed_sends.is_empty(), "Alice should have confirmed Send event after mining");
    assert!(!bob_confirmed_receives.is_empty(), "Bob should have confirmed Receive event after mining");
    
    // Verify large amounts were confirmed correctly
    let alice_confirmed_large_sends: Vec<_> = alice_confirmed_sends.iter()
        .filter(|e| e.amount_sats.abs() > 50_000_000) // > 0.5 BTC
        .collect();
    let bob_confirmed_large_receives: Vec<_> = bob_confirmed_receives.iter()
        .filter(|e| e.amount_sats > 50_000_000) // > 0.5 BTC
        .collect();
    
    assert!(!alice_confirmed_large_sends.is_empty(), "Alice should have large confirmed Send transactions");
    assert!(!bob_confirmed_large_receives.is_empty(), "Bob should receive large amounts from full send");
    
    println!("✅ Stage 2: Confirmed full send events created correctly");
    println!("✅ Alice full send two-stage test passed - wallet drain events created in proper sequence!");
}

/// Test: Multiple partial sends with two-stage flow
/// Verifies that multiple partial send transactions create separate unconfirmed events,
/// then get combined into a single confirmed event when mined together
#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_multiple_partial_sends_bob_two_stage() {
    let mut env = IsolatedTestEnvironment::new().await.expect("Failed to create test environment");
    
    // Initial sync
    env.sync_and_wait().await.expect("Failed to sync");
    
    let initial_alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    let initial_bob_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get Bob events");
    
    println!("📊 Initial state:");
    println!("   Alice events: {}", initial_alice_events.len());
    println!("   Bob events: {}", initial_bob_events.len());
    
    // Send multiple partial send transactions - DON'T mine immediately
    println!("⚡ Step 1: Alice sends multiple partial transactions to Bob (0.1, 0.2, 0.05 BTC)");
    
    let _txid1 = env.send_transaction("alice", "bob", "0.1").await.expect("Failed to send transaction 1");
    let _txid2 = env.send_transaction("alice", "bob", "0.2").await.expect("Failed to send transaction 2"); 
    let _txid3 = env.send_transaction("alice", "bob", "0.05").await.expect("Failed to send transaction 3");
    
    println!("⚡ Step 2: Sync to detect unconfirmed transactions (mempool)");
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Small delay to ensure database transactions are committed
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Verify UNCONFIRMED events are created (Stage 1) - should have 3 separate events
    let unconfirmed_alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    let unconfirmed_bob_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get Bob events");
    
    // Events are stored in reverse chronological order (newest first)
    // Find new events by excluding events that existed initially
    let initial_alice_ids: Vec<Option<String>> = initial_alice_events.iter().map(|e| e.id.clone()).collect();
    let initial_bob_ids: Vec<Option<String>> = initial_bob_events.iter().map(|e| e.id.clone()).collect();
    
    let alice_unconfirmed_sends: Vec<_> = unconfirmed_alice_events.iter()
        .filter(|e| !initial_alice_ids.contains(&e.id)) // Only new events
        .filter(|e| e.event_type == EventType::Send && !e.is_confirmed)
        .collect();
    let bob_unconfirmed_receives: Vec<_> = unconfirmed_bob_events.iter()
        .filter(|e| !initial_bob_ids.contains(&e.id)) // Only new events
        .filter(|e| e.event_type == EventType::Receive && !e.is_confirmed)
        .collect();
    
    println!("📊 Stage 1 - Mempool events:");
    println!("   Alice unconfirmed sends: {}", alice_unconfirmed_sends.len());
    println!("   Bob unconfirmed receives: {}", bob_unconfirmed_receives.len());
    
    // Debug: Show all amounts
    for (i, event) in alice_unconfirmed_sends.iter().enumerate() {
        println!("   Alice Send {}: {} sats", i, event.amount_sats.abs());
    }
    for (i, event) in bob_unconfirmed_receives.iter().enumerate() {
        println!("   Bob Receive {}: {} sats", i, event.amount_sats);
    }
    
    assert_eq!(alice_unconfirmed_sends.len(), 3, "Alice should have exactly 3 unconfirmed Send events");
    assert_eq!(bob_unconfirmed_receives.len(), 3, "Bob should have exactly 3 unconfirmed Receive events");
    
    // Verify amounts are correct (0.1, 0.2, 0.05 BTC)
    let expected_amounts = [10_000_000i64, 20_000_000i64, 5_000_000i64]; // 0.1, 0.2, 0.05 BTC in sats
    let mut alice_amounts: Vec<i64> = alice_unconfirmed_sends.iter().map(|e| e.amount_sats.abs()).collect();
    let mut bob_amounts: Vec<i64> = bob_unconfirmed_receives.iter().map(|e| e.amount_sats).collect();
    
    alice_amounts.sort();
    bob_amounts.sort();
    let mut expected_sorted = expected_amounts.to_vec();
    expected_sorted.sort();
    
    assert_eq!(alice_amounts, expected_sorted, "Alice should have correct unconfirmed send amounts");
    assert_eq!(bob_amounts, expected_sorted, "Bob should have correct unconfirmed receive amounts");
    
    println!("✅ Stage 1: 3 separate unconfirmed events created correctly");
    
    // Mine block to confirm all transactions together
    println!("⚡ Step 3: Mine block to confirm all transactions together");
    env.mine_blocks(1).await.expect("Failed to mine blocks");
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Verify CONFIRMED events exist (Stage 2) - should have 1 net event each
    let final_alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    let final_bob_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get Bob events");
    
    let alice_confirmed_sends: Vec<_> = final_alice_events.iter()
        .filter(|e| e.event_type == EventType::Send && e.is_confirmed)
        .collect();
    let bob_confirmed_receives: Vec<_> = final_bob_events.iter()
        .filter(|e| e.event_type == EventType::Receive && e.is_confirmed)
        .collect();
    
    println!("📊 Stage 2 - Confirmed events:");
    println!("   Alice confirmed sends: {}", alice_confirmed_sends.len()); 
    println!("   Bob confirmed receives: {}", bob_confirmed_receives.len());
    
    // Debug: Show confirmed amounts
    for (i, event) in alice_confirmed_sends.iter().enumerate() {
        println!("   Alice Confirmed Send {}: {} sats", i, event.amount_sats.abs());
    }
    for (i, event) in bob_confirmed_receives.iter().enumerate() {
        println!("   Bob Confirmed Receive {}: {} sats", i, event.amount_sats);
    }
    
    assert!(!alice_confirmed_sends.is_empty(), "Alice should have confirmed Send events after mining");
    assert!(!bob_confirmed_receives.is_empty(), "Bob should have confirmed Receive events after mining");
    
    // Since all transactions are mined in the same block, they should be combined into 1 net event each
    // Expected net amount: 0.1 + 0.2 + 0.05 = 0.35 BTC = 35M sats
    let expected_net_amount = 35_000_000i64;
    
    let alice_has_net_amount = alice_confirmed_sends.iter()
        .any(|e| e.amount_sats.abs() == expected_net_amount);
    let bob_has_net_amount = bob_confirmed_receives.iter()
        .any(|e| e.amount_sats == expected_net_amount);
    
    assert!(alice_has_net_amount, "Alice should have net confirmed send of 35M sats");
    assert!(bob_has_net_amount, "Bob should have net confirmed receive of 35M sats");
    
    println!("✅ Stage 2: Net confirmed events created correctly (35M sats each)");
    println!("✅ Multiple partial sends two-stage test passed!");
    println!("   - Stage 1: 3 separate unconfirmed events in mempool");
    println!("   - Stage 2: 1 net confirmed event when mined together");
}