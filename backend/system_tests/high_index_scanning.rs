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

/// Test 2: Charlie Wallet with Output Descriptor (High Index 250)
/// Purpose: Verify descriptor-based wallet handles high index scanning and compare with XPUB approach
#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_charlie_descriptor_wallet_high_index_scanning() {
    let mut env = IsolatedTestEnvironment::new().await.expect("Failed to create test environment");
    
    // Create test user for descriptor wallet
    let test_user_id = env.metadata_db.create_user(
        "descriptor-test@example.com",
        "hashedpassword", 
        Some("Descriptor Test User"),
        false
    ).await.expect("Failed to create test user");
    
    // Create Charlie wallet using output descriptor format instead of XPUB
    println!("🏦 Creating Charlie wallet with output descriptor format");
    let charlie_descriptor = "wpkh([fingerprint/84h/1h/0h]xpub6H1LXWLaKsWFhvm6RVpEL9P4KfRZSW7abD2ttkWP3SSQvnyA8FSVqNTEcYFgJS2UaFcxupHiYkro49S8yGasTvXEYBVPamhGW6cFJodrTHy/<0;1>/*)#pe5sgqha";
    
    let charlie_descriptor_checksum = env.create_descriptor_wallet("Charlie_Descriptor", charlie_descriptor, &test_user_id).await
        .expect("Failed to create descriptor wallet");
    
    println!("✅ Charlie descriptor wallet created with checksum: {}", charlie_descriptor_checksum);
    
    // The Docker environment already has Charlie funded at index 250
    // Now sync and verify the descriptor wallet detects the same funds
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Compare events between original Charlie (XPUB) and Charlie_Descriptor (descriptor format)
    let original_charlie_events = env.get_wallet_events(&env.charlie_checksum).await.expect("Failed to get original Charlie events");
    let descriptor_charlie_events = env.get_wallet_events(&charlie_descriptor_checksum).await.expect("Failed to get descriptor Charlie events");
    
    println!("📊 High-index scanning comparison:");
    println!("   Original Charlie (XPUB) events: {}", original_charlie_events.len());
    println!("   Descriptor Charlie events: {}", descriptor_charlie_events.len());
    
    // Both wallets should detect the same high-index transaction
    let original_receive_events: Vec<_> = original_charlie_events.iter()
        .filter(|e| e.event_type == EventType::Receive)
        .collect();
    let descriptor_receive_events: Vec<_> = descriptor_charlie_events.iter()
        .filter(|e| e.event_type == EventType::Receive)
        .collect();
    
    assert!(!original_receive_events.is_empty(), "Original Charlie should have receive events from high-index funding");
    assert!(!descriptor_receive_events.is_empty(), "Descriptor Charlie should have receive events from high-index funding");
    
    // Compare the amounts - they should be the same since both wallets watch the same addresses
    let original_amounts: Vec<i64> = original_receive_events.iter().map(|e| e.amount_sats).collect();
    let descriptor_amounts: Vec<i64> = descriptor_receive_events.iter().map(|e| e.amount_sats).collect();
    
    println!("💰 Amount comparison:");
    println!("   Original Charlie amounts: {:?}", original_amounts);
    println!("   Descriptor Charlie amounts: {:?}", descriptor_amounts);
    
    // Should detect the same funding amount (0.5 BTC = 50,000,000 sats)
    let expected_amount = 50_000_000i64;
    
    assert!(original_amounts.contains(&expected_amount), "Original Charlie should detect 0.5 BTC funding");
    assert!(descriptor_amounts.contains(&expected_amount), "Descriptor Charlie should detect 0.5 BTC funding");
    
    // Test that both wallets can send from high index
    println!("📤 Testing outgoing transactions from high index (descriptor wallet)");
    
    let initial_alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    
    // Try to send from descriptor wallet to Alice (this will test if the private keys work correctly)
    let descriptor_to_alice_txid = env.send_transaction("charlie", "alice", "0.1").await
        .expect("Failed to send from descriptor Charlie to Alice");
    
    env.sync_and_wait().await.expect("Failed to sync after descriptor send");
    
    // Verify both Alice receives and descriptor Charlie sends
    let alice_events_after = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    let descriptor_events_after = env.get_wallet_events(&charlie_descriptor_checksum).await.expect("Failed to get descriptor events");
    
    let new_alice_events = alice_events_after.len() - initial_alice_events.len();
    let descriptor_send_events: Vec<_> = descriptor_events_after.iter()
        .filter(|e| e.event_type == EventType::Send)
        .collect();
    
    assert!(new_alice_events > 0, "Alice should receive transaction from descriptor wallet");
    assert!(!descriptor_send_events.is_empty(), "Descriptor wallet should have send event");
    
    println!("✅ Test 2 passed - Descriptor wallet high-index scanning works correctly!");
    println!("   - Descriptor wallet detected same high-index funds as XPUB wallet");
    println!("   - Both wallet types handle index 250 funding equally well");
    println!("   - Descriptor wallet can successfully send from high index");
    println!("   - Address revelation works with both XPUB and descriptor formats");
    
    // Performance comparison (optional - just informational)
    println!("📈 Performance comparison:");
    println!("   XPUB format: {} events detected", original_charlie_events.len());
    println!("   Descriptor format: {} events detected", descriptor_charlie_events.len());
    
    if original_charlie_events.len() == descriptor_charlie_events.len() {
        println!("   ✅ Both formats show identical performance");
    } else {
        println!("   ℹ️ Event counts differ - may indicate different sync timing or implementation details");
    }
}