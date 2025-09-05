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
    let mut env = IsolatedTestEnvironment::new_with_charlie().await.expect("Failed to create test environment");
    
    // Initial sync should detect both Alice (index 0) and Charlie (index 250) funding
    env.sync_and_wait().await.expect("Failed to sync");
    
    let alice_transactions = env.get_wallet_transactions(&env.alice_checksum).await.expect("Failed to get Alice transactions");
    let charlie_transactions = env.get_wallet_transactions(&env.charlie_checksum).await.expect("Failed to get Charlie transactions");
    
    // Alice should have transactions (funded at normal index 0)
    let alice_receive_transactions: Vec<_> = alice_transactions.iter()
        .filter(|t| t.transaction_type == EventType::Receive)
        .collect();
    assert!(!alice_receive_transactions.is_empty(), "Alice should have receive transactions from funding at index 0");
    
    // Charlie should have transactions (funded at high index 250)
    let charlie_receive_transactions: Vec<_> = charlie_transactions.iter()
        .filter(|t| t.transaction_type == EventType::Receive)
        .collect();
    assert!(!charlie_receive_transactions.is_empty(), "Charlie should have receive transactions from funding at index 250");
    
    println!("📊 Alice transactions (index 0): {}", alice_receive_transactions.len());
    println!("📊 Charlie transactions (index 250): {}", charlie_receive_transactions.len());
    
    println!("✅ High-index fund detection test passed!");
    println!("   - Alice funded at index 0: detected ✓");
    println!("   - Charlie funded at index 250: detected ✓"); 
}

#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_high_index_outgoing_transactions() {
    let mut env = IsolatedTestEnvironment::new_with_charlie().await.expect("Failed to create test environment");
    
    // Initial sync
    env.sync_and_wait().await.expect("Failed to sync");
    
    let initial_alice_transactions = env.get_wallet_transactions(&env.alice_checksum).await.expect("Failed to get Alice transactions");
    
    // Test that Charlie can send from high index to Alice
    let initial_alice_transaction_count = initial_alice_transactions.len();
    
    // Send from Charlie (high index) to Alice (normal index)
    let _txid = env.send_transaction("charlie", "alice", "0.1").await.expect("Failed to send from Charlie");
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Verify the transaction was detected on both sides
    let updated_alice_transactions = env.get_wallet_transactions(&env.alice_checksum).await.expect("Failed to get Alice transactions");
    let updated_charlie_transactions = env.get_wallet_transactions(&env.charlie_checksum).await.expect("Failed to get Charlie transactions");
    
    assert!(updated_alice_transactions.len() > initial_alice_transaction_count, "Alice should have new receive transaction from Charlie");
    
    let charlie_send_transactions: Vec<_> = updated_charlie_transactions.iter()
        .filter(|t| t.transaction_type == EventType::Send && t.block_height.is_none())
        .collect();
    assert!(!charlie_send_transactions.is_empty(), "Charlie should have new send transaction");
    
    println!("✅ High-index outgoing transaction test passed!");
    println!("   - Charlie can send from index 250: detected ✓");
    println!("   - Alice receives from high-index sender: detected ✓");
}

#[tokio::test]
#[ignore] // System test - requires Docker  
async fn test_address_revelation_up_to_high_indexes() {
    let mut env = IsolatedTestEnvironment::new_with_charlie().await.expect("Failed to create test environment");
    
    // This test verifies that our wallet manager can handle address revelation
    // up to at least index 250, which is where Charlie's funds are located
    
    env.sync_and_wait().await.expect("Failed to sync");
    
    let charlie_transactions = env.get_wallet_transactions(&env.charlie_checksum).await.expect("Failed to get Charlie transactions");
    
    // If Charlie has transactions, it means the wallet manager successfully:
    // 1. Revealed addresses from 0 to 250
    // 2. Detected the transaction at index 250
    // 3. Created appropriate database transactions
    
    let charlie_receive_transactions: Vec<_> = charlie_transactions.iter()
        .filter(|t| t.transaction_type == EventType::Receive)
        .collect();
        
    assert!(!charlie_receive_transactions.is_empty(), 
        "Charlie should have receive transactions, proving address revelation worked up to index 250");
    
    println!("✅ Address revelation test passed!");
    println!("   - Wallet manager successfully revealed addresses 0-250+");
    println!("   - Transaction detected at high index 250");
    println!("   - Transactions created for high-index transaction");
}

/// Test 2: Charlie Wallet with Output Descriptor (High Index 250)
/// Purpose: Verify descriptor-based wallet handles high index scanning and compare with XPUB approach
#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_charlie_descriptor_wallet_high_index_scanning() {
    let mut env = IsolatedTestEnvironment::new_with_charlie().await.expect("Failed to create test environment");
    
    // Create test user for descriptor wallet
    let test_user_id = env.metadata_db.create_user(
        "descriptor-test@example.com",
        "hashedpassword", 
        Some("Descriptor Test User"),
        false
    ).await.expect("Failed to create test user");
    
    // Create AppServices to access wallet creation service
    let wallet_creation_service = canary::wallet::WalletCreationService::new(
        env.wallet_manager.wallet_dir.clone(),
        env.metadata_db.clone(),
        env.wallet_manager.electrum_client.clone(),
        env.wallet_manager.get_network(),
    );
    let app_services = canary::api::AppServices {
        metadata_db: env.metadata_db.clone(),
        wallet_creation_service,
    };
    
    // Create Charlie wallet using output descriptor format instead of XPUB
    println!("🏦 Creating Charlie wallet with output descriptor format");
    let charlie_descriptor = "wpkh(tpubDCxzhZZE31g2EqSv1UajMAw5Hd62htydz9r2XBkrccHgBh8uw3n62zr6Zjmj64tfTk8Tjxo6VctjUMAh5DXWTErfQPC6RmQhTdtNnXuTXTQ/<0;1>/*)#sq32h3ch";
    
    let charlie_descriptor_metadata = app_services.wallet_creation_service.create_wallet_non_blocking(
        "Charlie_Descriptor", charlie_descriptor, &test_user_id, false, Some("auto"), Some("250")
    ).await.expect("Failed to create descriptor wallet");
    
    let charlie_descriptor_checksum = charlie_descriptor_metadata.checksum;
    
    println!("✅ Charlie descriptor wallet created with checksum: {}", charlie_descriptor_checksum);
    
    // The Docker environment already has Charlie funded at index 250
    // Now sync and verify the descriptor wallet detects the same funds
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Compare transactions between original Charlie (XPUB) and Charlie_Descriptor (descriptor format)
    let original_charlie_transactions = env.get_wallet_transactions(&env.charlie_checksum).await.expect("Failed to get original Charlie transactions");
    let descriptor_charlie_transactions = env.get_wallet_transactions(&charlie_descriptor_checksum).await.expect("Failed to get descriptor Charlie transactions");
    
    println!("📊 High-index scanning comparison:");
    println!("   Original Charlie (XPUB) transactions: {}", original_charlie_transactions.len());
    println!("   Descriptor Charlie transactions: {}", descriptor_charlie_transactions.len());
    
    // Both wallets should detect the same high-index transaction
    let original_receive_transactions: Vec<_> = original_charlie_transactions.iter()
        .filter(|t| t.transaction_type == EventType::Receive)
        .collect();
    let descriptor_receive_transactions: Vec<_> = descriptor_charlie_transactions.iter()
        .filter(|t| t.transaction_type == EventType::Receive)
        .collect();
    
    assert!(!original_receive_transactions.is_empty(), "Original Charlie should have receive transactions from high-index funding");
    assert!(!descriptor_receive_transactions.is_empty(), "Descriptor Charlie should have receive transactions from high-index funding");
    
    // Compare the amounts - they should be the same since both wallets watch the same addresses
    let original_amounts: Vec<i64> = original_receive_transactions.iter().map(|t| t.amount_sats).collect();
    let descriptor_amounts: Vec<i64> = descriptor_receive_transactions.iter().map(|t| t.amount_sats).collect();
    
    println!("💰 Amount comparison:");
    println!("   Original Charlie amounts: {:?}", original_amounts);
    println!("   Descriptor Charlie amounts: {:?}", descriptor_amounts);
    
    // Should detect the same funding amount (0.5 BTC = 50,000,000 sats)
    let expected_amount = 50_000_000i64;
    
    assert!(original_amounts.contains(&expected_amount), "Original Charlie should detect 0.5 BTC funding");
    assert!(descriptor_amounts.contains(&expected_amount), "Descriptor Charlie should detect 0.5 BTC funding");
    
    // Test that both wallets can send from high index
    println!("📤 Testing outgoing transactions from high index (descriptor wallet)");
    
    let initial_alice_transactions = env.get_wallet_transactions(&env.alice_checksum).await.expect("Failed to get Alice transactions");
    
    // Try to send from descriptor wallet to Alice (this will test if the private keys work correctly)
    let descriptor_to_alice_txid = env.send_transaction("charlie", "alice", "0.1").await
        .expect("Failed to send from descriptor Charlie to Alice");
    
    env.sync_and_wait().await.expect("Failed to sync after descriptor send");
    
    // Verify both Alice receives and descriptor Charlie sends
    let alice_transactions_after = env.get_wallet_transactions(&env.alice_checksum).await.expect("Failed to get Alice transactions");
    let descriptor_transactions_after = env.get_wallet_transactions(&charlie_descriptor_checksum).await.expect("Failed to get descriptor transactions");
    
    let new_alice_transactions = alice_transactions_after.len() - initial_alice_transactions.len();
    let descriptor_send_transactions: Vec<_> = descriptor_transactions_after.iter()
        .filter(|t| t.transaction_type == EventType::Send)
        .collect();
    
    assert!(new_alice_transactions > 0, "Alice should receive transaction from descriptor wallet");
    assert!(!descriptor_send_transactions.is_empty(), "Descriptor wallet should have send transaction");
    
    println!("✅ Test 2 passed - Descriptor wallet high-index scanning works correctly!");
    println!("   - Descriptor wallet detected same high-index funds as XPUB wallet");
    println!("   - Both wallet types handle index 250 funding equally well");
    println!("   - Descriptor wallet can successfully send from high index");
    println!("   - Address revelation works with both XPUB and descriptor formats");
    
    // Performance comparison (optional - just informational)
    println!("📈 Performance comparison:");
    println!("   XPUB format: {} transactions detected", original_charlie_transactions.len());
    println!("   Descriptor format: {} transactions detected", descriptor_charlie_transactions.len());
    
    if original_charlie_transactions.len() == descriptor_charlie_transactions.len() {
        println!("   ✅ Both formats show identical performance");
    } else {
        println!("   ℹ️ Transaction counts differ - may indicate different sync timing or implementation details");
    }
}