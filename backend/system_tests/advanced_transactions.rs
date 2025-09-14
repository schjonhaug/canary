use canary::metadata::EventType;

mod common;
use common::docker_environment::IsolatedTestEnvironment;

/// System tests for advanced transaction scenarios
///
/// These tests verify RBF (Replace-By-Fee) and CPFP (Child-Pays-For-Parent)
/// transaction handling, ensuring proper event management and fee acceleration.

/// Test: Single RBF from sender and receiver perspective
/// Purpose: Test single RBF transaction replacement from both Alice (sender) and Bob (receiver) perspectives
#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_single_rbf_sender_and_receiver_perspective() {
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

    println!("📊 Initial state:");
    println!(
        "   Alice transactions: {}",
        initial_alice_transactions.len()
    );
    println!("   Bob transactions: {}", initial_bob_transactions.len());

    // Step 1: Alice sends Bitcoin to Bob with low fee (RBF enabled)
    println!("🔄 Step 1: Alice sends 0.1 BTC to Bob with low fee (RBF enabled)");
    let original_txid = env
        .send_rbf_transaction("alice", "bob", "0.1")
        .await
        .expect("Failed to send RBF transaction");

    // Sync to detect the original transaction
    env.sync_and_wait()
        .await
        .expect("Failed to sync after original transaction");

    let after_original_alice_txs = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");
    let after_original_bob_txs = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    // Verify original transaction is detected by both wallets
    let new_alice_count = after_original_alice_txs.len() - initial_alice_transactions.len();
    let new_bob_count = after_original_bob_txs.len() - initial_bob_transactions.len();

    assert!(
        new_alice_count > 0,
        "Alice should detect the original send transaction"
    );
    assert!(
        new_bob_count > 0,
        "Bob should detect the original receive transaction"
    );

    // Find the original transaction in both wallets
    let alice_original_tx = after_original_alice_txs
        .iter()
        .find(|tx| tx.txid == original_txid)
        .expect("Alice should have the original transaction");
    let bob_original_tx = after_original_bob_txs
        .iter()
        .find(|tx| tx.txid == original_txid)
        .expect("Bob should have the original transaction");

    assert_eq!(
        alice_original_tx.transaction_status, "pending",
        "Alice's original transaction should be pending"
    );
    assert_eq!(
        bob_original_tx.transaction_status, "pending",
        "Bob's original transaction should be pending"
    );
    assert_eq!(
        alice_original_tx.transaction_type,
        EventType::Send,
        "Alice should see it as a send"
    );
    assert_eq!(
        bob_original_tx.transaction_type,
        EventType::Receive,
        "Bob should see it as a receive"
    );

    println!("✅ Original RBF transaction detected by both wallets:");
    println!("   Original txid: {}", original_txid);
    println!(
        "   Alice sees: {} transaction with status '{}'",
        alice_original_tx.transaction_type.as_str(),
        alice_original_tx.transaction_status
    );
    println!(
        "   Bob sees: {} transaction with status '{}'",
        bob_original_tx.transaction_type.as_str(),
        bob_original_tx.transaction_status
    );

    // Step 2: Alice replaces transaction with higher fee
    println!("⬆️ Step 2: Alice replaces transaction with higher fee");
    let replacement_txid = env
        .replace_transaction("alice", &original_txid, 10.0)
        .await
        .expect("Failed to replace transaction");

    // Sync to detect the replacement
    env.sync_and_wait()
        .await
        .expect("Failed to sync after replacement");

    let after_replacement_alice_txs = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");
    let after_replacement_bob_txs = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    // Verify RBF detection: original should be marked as replaced, replacement should exist
    let alice_original_replaced = after_replacement_alice_txs
        .iter()
        .find(|tx| tx.txid == original_txid)
        .expect("Alice should still have the original (replaced) transaction");
    let bob_original_replaced = after_replacement_bob_txs
        .iter()
        .find(|tx| tx.txid == original_txid)
        .expect("Bob should still have the original (replaced) transaction");

    let alice_replacement_tx = after_replacement_alice_txs
        .iter()
        .find(|tx| tx.txid == replacement_txid)
        .expect("Alice should have the replacement transaction");
    let bob_replacement_tx = after_replacement_bob_txs
        .iter()
        .find(|tx| tx.txid == replacement_txid)
        .expect("Bob should have the replacement transaction");

    // Verify RBF status tracking
    assert_eq!(
        alice_original_replaced.transaction_status, "replaced",
        "Alice's original transaction should be marked as replaced"
    );
    assert_eq!(
        bob_original_replaced.transaction_status, "replaced",
        "Bob's original transaction should be marked as replaced"
    );
    assert_eq!(
        alice_original_replaced.replaced_by_txid,
        Some(replacement_txid.clone()),
        "Alice's original should reference replacement txid"
    );
    assert_eq!(
        bob_original_replaced.replaced_by_txid,
        Some(replacement_txid.clone()),
        "Bob's original should reference replacement txid"
    );

    assert_eq!(
        alice_replacement_tx.transaction_status, "pending",
        "Alice's replacement transaction should be pending"
    );
    assert_eq!(
        bob_replacement_tx.transaction_status, "pending",
        "Bob's replacement transaction should be pending"
    );

    println!("✅ RBF replacement detected by both wallets:");
    println!("   Replacement txid: {}", replacement_txid);
    println!(
        "   Alice original status: '{}', replaced_by: {:?}",
        alice_original_replaced.transaction_status, alice_original_replaced.replaced_by_txid
    );
    println!(
        "   Bob original status: '{}', replaced_by: {:?}",
        bob_original_replaced.transaction_status, bob_original_replaced.replaced_by_txid
    );
    println!(
        "   Alice replacement status: '{}'",
        alice_replacement_tx.transaction_status
    );
    println!(
        "   Bob replacement status: '{}'",
        bob_replacement_tx.transaction_status
    );

    // Step 3: Mine block to confirm replacement
    println!("⛏️ Step 3: Mining block to confirm replacement transaction");
    env.mine_blocks(1).await.expect("Failed to mine blocks");
    env.sync_and_wait_with_retries(3)
        .await
        .expect("Failed to sync after mining with retries");

    // Verify final confirmed state
    let final_alice_txs = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");
    let final_bob_txs = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    // Check that replacement transaction is now confirmed
    let alice_final_replacement = final_alice_txs
        .iter()
        .find(|tx| tx.txid == replacement_txid)
        .expect("Alice should have the replacement transaction");
    let bob_final_replacement = final_bob_txs
        .iter()
        .find(|tx| tx.txid == replacement_txid)
        .expect("Bob should have the replacement transaction");

    assert_eq!(
        alice_final_replacement.transaction_status, "confirmed",
        "Alice's replacement should be confirmed"
    );
    assert_eq!(
        bob_final_replacement.transaction_status, "confirmed",
        "Bob's replacement should be confirmed"
    );
    assert!(
        alice_final_replacement.block_height.is_some(),
        "Alice's replacement should have block height"
    );
    assert!(
        bob_final_replacement.block_height.is_some(),
        "Bob's replacement should have block height"
    );

    // Original transaction should still be marked as replaced
    let alice_final_original = final_alice_txs
        .iter()
        .find(|tx| tx.txid == original_txid)
        .expect("Alice should still have the original transaction");
    let bob_final_original = final_bob_txs
        .iter()
        .find(|tx| tx.txid == original_txid)
        .expect("Bob should still have the original transaction");

    assert_eq!(
        alice_final_original.transaction_status, "replaced",
        "Alice's original should remain as replaced"
    );
    assert_eq!(
        bob_final_original.transaction_status, "replaced",
        "Bob's original should remain as replaced"
    );

    println!("✅ Single RBF test passed!");
    println!("   - Both wallets detected original transaction as pending");
    println!("   - Both wallets detected RBF replacement correctly");
    println!("   - Original transaction marked as 'replaced' with correct replaced_by_txid");
    println!("   - Replacement transaction confirmed after mining");
    println!("   - Final state: Original='replaced', Replacement='confirmed'");
}

/// Test: Multiple RBF from sender and receiver perspective  
/// Purpose: Test multiple RBF transaction replacements from both Alice (sender) and Bob (receiver) perspectives
#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_multiple_rbf_sender_and_receiver_perspective() {
    let mut env = IsolatedTestEnvironment::new()
        .await
        .expect("Failed to create test environment");

    // Initial sync
    env.sync_and_wait().await.expect("Failed to sync");

    let initial_alice_transactions = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");
    let initial_bob_transactions = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    println!("📊 Initial state:");
    println!(
        "   Alice transactions: {}",
        initial_alice_transactions.len()
    );
    println!("   Bob transactions: {}", initial_bob_transactions.len());

    // Step 1: Create original RBF transaction
    println!("🔄 Step 1: Creating original RBF transaction with very low fee");
    let original_txid = env
        .send_rbf_transaction("alice", "bob", "0.05")
        .await
        .expect("Failed to send original RBF transaction");

    env.sync_and_wait().await.expect("Failed to sync");

    let after_original_alice_txs = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");
    let after_original_bob_txs = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    // Verify original transaction detected by both wallets
    let alice_original = after_original_alice_txs
        .iter()
        .find(|tx| tx.txid == original_txid)
        .expect("Alice should have the original transaction");
    let bob_original = after_original_bob_txs
        .iter()
        .find(|tx| tx.txid == original_txid)
        .expect("Bob should have the original transaction");

    assert_eq!(
        alice_original.transaction_status, "pending",
        "Alice's original should be pending"
    );
    assert_eq!(
        bob_original.transaction_status, "pending",
        "Bob's original should be pending"
    );

    println!("✅ Original transaction detected:");
    println!("   Original txid: {}", original_txid);
    println!(
        "   Alice status: '{}', Bob status: '{}'",
        alice_original.transaction_status, bob_original.transaction_status
    );

    // Step 2: First replacement
    println!("⬆️ Step 2: First replacement - bumping fee to 5.0 sat/vB");
    let replacement1_txid = env
        .replace_transaction("alice", &original_txid, 5.0)
        .await
        .expect("Failed to create first replacement");

    env.sync_and_wait().await.expect("Failed to sync");

    let after_repl1_alice_txs = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");
    let after_repl1_bob_txs = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    // Verify first replacement detection
    let alice_original_repl1 = after_repl1_alice_txs
        .iter()
        .find(|tx| tx.txid == original_txid)
        .expect("Alice should still have the original transaction");
    let bob_original_repl1 = after_repl1_bob_txs
        .iter()
        .find(|tx| tx.txid == original_txid)
        .expect("Bob should still have the original transaction");

    let alice_replacement1 = after_repl1_alice_txs
        .iter()
        .find(|tx| tx.txid == replacement1_txid)
        .expect("Alice should have the first replacement");
    let bob_replacement1 = after_repl1_bob_txs
        .iter()
        .find(|tx| tx.txid == replacement1_txid)
        .expect("Bob should have the first replacement");

    // Verify RBF status after first replacement
    assert_eq!(
        alice_original_repl1.transaction_status, "replaced",
        "Alice's original should be replaced"
    );
    assert_eq!(
        bob_original_repl1.transaction_status, "replaced",
        "Bob's original should be replaced"
    );
    assert_eq!(
        alice_original_repl1.replaced_by_txid,
        Some(replacement1_txid.clone()),
        "Alice's original should reference first replacement"
    );
    assert_eq!(
        bob_original_repl1.replaced_by_txid,
        Some(replacement1_txid.clone()),
        "Bob's original should reference first replacement"
    );

    assert_eq!(
        alice_replacement1.transaction_status, "pending",
        "Alice's first replacement should be pending"
    );
    assert_eq!(
        bob_replacement1.transaction_status, "pending",
        "Bob's first replacement should be pending"
    );

    println!("✅ First replacement detected:");
    println!("   First replacement txid: {}", replacement1_txid);
    println!("   Original now marked as 'replaced' by both wallets");
    println!("   First replacement is 'pending' for both wallets");

    // Step 3: Second replacement
    println!("⬆️ Step 3: Second replacement - bumping fee to 15.0 sat/vB");
    let replacement2_txid = env
        .replace_transaction("alice", &replacement1_txid, 15.0)
        .await
        .expect("Failed to create second replacement");

    env.sync_and_wait().await.expect("Failed to sync");

    let after_repl2_alice_txs = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");
    let after_repl2_bob_txs = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    // Verify second replacement detection
    let alice_replacement1_replaced = after_repl2_alice_txs
        .iter()
        .find(|tx| tx.txid == replacement1_txid)
        .expect("Alice should still have the first replacement");
    let bob_replacement1_replaced = after_repl2_bob_txs
        .iter()
        .find(|tx| tx.txid == replacement1_txid)
        .expect("Bob should still have the first replacement");

    let alice_replacement2 = after_repl2_alice_txs
        .iter()
        .find(|tx| tx.txid == replacement2_txid)
        .expect("Alice should have the second replacement");
    let bob_replacement2 = after_repl2_bob_txs
        .iter()
        .find(|tx| tx.txid == replacement2_txid)
        .expect("Bob should have the second replacement");

    // Verify RBF chain: original→replaced, first_replacement→replaced, second_replacement→pending
    assert_eq!(
        alice_replacement1_replaced.transaction_status, "replaced",
        "Alice's first replacement should be replaced"
    );
    assert_eq!(
        bob_replacement1_replaced.transaction_status, "replaced",
        "Bob's first replacement should be replaced"
    );
    assert_eq!(
        alice_replacement1_replaced.replaced_by_txid,
        Some(replacement2_txid.clone()),
        "Alice's first replacement should reference second replacement"
    );
    assert_eq!(
        bob_replacement1_replaced.replaced_by_txid,
        Some(replacement2_txid.clone()),
        "Bob's first replacement should reference second replacement"
    );

    assert_eq!(
        alice_replacement2.transaction_status, "pending",
        "Alice's second replacement should be pending"
    );
    assert_eq!(
        bob_replacement2.transaction_status, "pending",
        "Bob's second replacement should be pending"
    );

    println!("✅ Second replacement detected:");
    println!("   Second replacement txid: {}", replacement2_txid);
    println!("   First replacement now marked as 'replaced' by both wallets");
    println!("   Second replacement is 'pending' for both wallets");

    // Step 4: Mine to confirm final replacement
    println!("⛏️ Step 4: Mining block to confirm final replacement");
    env.mine_blocks(1).await.expect("Failed to mine blocks");
    env.sync_and_wait_with_retries(3)
        .await
        .expect("Failed to sync with retries");

    // Verify final state
    let final_alice_txs = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");
    let final_bob_txs = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    // Check final replacement is confirmed
    let alice_final_replacement = final_alice_txs
        .iter()
        .find(|tx| tx.txid == replacement2_txid)
        .expect("Alice should have the final replacement");
    let bob_final_replacement = final_bob_txs
        .iter()
        .find(|tx| tx.txid == replacement2_txid)
        .expect("Bob should have the final replacement");

    assert_eq!(
        alice_final_replacement.transaction_status, "confirmed",
        "Alice's final replacement should be confirmed"
    );
    assert_eq!(
        bob_final_replacement.transaction_status, "confirmed",
        "Bob's final replacement should be confirmed"
    );
    assert!(
        alice_final_replacement.block_height.is_some(),
        "Alice's final replacement should have block height"
    );
    assert!(
        bob_final_replacement.block_height.is_some(),
        "Bob's final replacement should have block height"
    );

    // Verify replacement chain remains intact
    let alice_final_original = final_alice_txs
        .iter()
        .find(|tx| tx.txid == original_txid)
        .expect("Alice should still have the original");
    let alice_final_first_repl = final_alice_txs
        .iter()
        .find(|tx| tx.txid == replacement1_txid)
        .expect("Alice should still have the first replacement");

    assert_eq!(
        alice_final_original.transaction_status, "replaced",
        "Alice's original should remain replaced"
    );
    assert_eq!(
        alice_final_first_repl.transaction_status, "replaced",
        "Alice's first replacement should remain replaced"
    );
    assert_eq!(
        alice_final_original.replaced_by_txid,
        Some(replacement1_txid.clone()),
        "Original should still reference first replacement"
    );
    assert_eq!(
        alice_final_first_repl.replaced_by_txid,
        Some(replacement2_txid.clone()),
        "First replacement should still reference second replacement"
    );

    // Count total transactions to ensure we have all the right pieces
    let final_alice_count = final_alice_txs.len() - initial_alice_transactions.len();
    let final_bob_count = final_bob_txs.len() - initial_bob_transactions.len();

    println!("✅ Multiple RBF test passed!");
    println!(
        "   - Original transaction: {} (status: 'replaced')",
        original_txid
    );
    println!(
        "   - First replacement: {} (status: 'replaced')",
        replacement1_txid
    );
    println!(
        "   - Final replacement: {} (status: 'confirmed')",
        replacement2_txid
    );
    println!("   - Alice total new transactions: {}", final_alice_count);
    println!("   - Bob total new transactions: {}", final_bob_count);
    println!("   - RBF chain tracked correctly: Original→First→Final");
    println!("   - Both sender and receiver perspective validated");
}

/// Test: CPFP (Child-Pays-For-Parent) detection and tracking
/// Purpose: Test CPFP transaction relationship detection when a child transaction spends from an unconfirmed parent
#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_cpfp_detection_and_tracking() {
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

    println!("📊 Initial state:");
    println!(
        "   Alice transactions: {}",
        initial_alice_transactions.len()
    );
    println!("   Bob transactions: {}", initial_bob_transactions.len());

    // Step 1: Alice sends Bitcoin to Bob with low fee (will be parent transaction)
    println!("🔄 Step 1: Alice sends 0.002222 BTC to Bob with low fee (parent transaction)");
    let parent_txid = env
        .send_transaction("alice", "bob", "0.002222")
        .await
        .expect("Failed to send parent transaction");

    // Wait specifically for the parent transaction to appear in both wallets
    let alice_checksum = env.alice_checksum.clone();
    let bob_checksum = env.bob_checksum.clone();
    env.wait_for_transaction_in_wallet(&alice_checksum, &parent_txid, 30)
        .await
        .expect("Alice should detect the parent transaction within 30 seconds");
    env.wait_for_transaction_in_wallet(&bob_checksum, &parent_txid, 30)
        .await
        .expect("Bob should detect the parent transaction within 30 seconds");

    let after_parent_alice_txs = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");
    let after_parent_bob_txs = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    // Verify parent transaction is detected by both wallets
    let alice_parent_tx = after_parent_alice_txs
        .iter()
        .find(|tx| tx.txid == parent_txid)
        .expect("Alice should have the parent transaction");
    let bob_parent_tx = after_parent_bob_txs
        .iter()
        .find(|tx| tx.txid == parent_txid)
        .expect("Bob should have the parent transaction");

    assert_eq!(
        alice_parent_tx.transaction_status, "pending",
        "Alice's parent transaction should be pending"
    );
    assert_eq!(
        bob_parent_tx.transaction_status, "pending",
        "Bob's parent transaction should be pending"
    );
    assert_eq!(
        alice_parent_tx.transaction_type,
        EventType::Send,
        "Alice should see parent as a send"
    );
    assert_eq!(
        bob_parent_tx.transaction_type,
        EventType::Receive,
        "Bob should see parent as a receive"
    );
    assert!(
        alice_parent_tx.parent_txid.is_none(),
        "Parent transaction should have no parent_txid"
    );
    assert!(
        bob_parent_tx.parent_txid.is_none(),
        "Parent transaction should have no parent_txid"
    );

    println!("✅ Parent transaction detected by both wallets:");
    println!("   Parent txid: {}", parent_txid);
    println!(
        "   Alice sees: {} transaction with status '{}', parent_txid: {:?}",
        alice_parent_tx.transaction_type.as_str(),
        alice_parent_tx.transaction_status,
        alice_parent_tx.parent_txid
    );
    println!(
        "   Bob sees: {} transaction with status '{}', parent_txid: {:?}",
        bob_parent_tx.transaction_type.as_str(),
        bob_parent_tx.transaction_status,
        bob_parent_tx.parent_txid
    );

    // Step 2: Bob creates a CPFP child transaction spending from the unconfirmed parent
    println!("👶 Step 2: Bob creates CPFP child transaction with high fee to accelerate parent");
    let child_txid = env
        .create_cpfp_transaction("bob", &parent_txid)
        .await
        .expect("Failed to create CPFP child transaction");

    // Wait specifically for the child transaction to appear in Bob's wallet
    let bob_checksum_for_child = env.bob_checksum.clone();
    env.wait_for_transaction_in_wallet(&bob_checksum_for_child, &child_txid, 30)
        .await
        .expect("Bob should detect the child transaction within 30 seconds");

    let after_child_alice_txs = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");
    let after_child_bob_txs = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    // Verify CPFP child transaction is detected
    let bob_child_tx = after_child_bob_txs
        .iter()
        .find(|tx| tx.txid == child_txid)
        .expect("Bob should have the child transaction");

    // Alice might not see the child transaction if it doesn't affect her wallet
    // But Bob should definitely see it since he created it
    assert_eq!(
        bob_child_tx.transaction_status, "pending",
        "Bob's child transaction should be pending"
    );
    assert_eq!(
        bob_child_tx.parent_txid,
        Some(parent_txid.clone()),
        "Bob's child should have parent_txid set"
    );

    // Verify parent transaction still exists and is unchanged
    let alice_parent_after_child = after_child_alice_txs
        .iter()
        .find(|tx| tx.txid == parent_txid)
        .expect("Alice should still have the parent transaction");
    let bob_parent_after_child = after_child_bob_txs
        .iter()
        .find(|tx| tx.txid == parent_txid)
        .expect("Bob should still have the parent transaction");

    assert_eq!(
        alice_parent_after_child.transaction_status, "pending",
        "Alice's parent should still be pending"
    );
    assert_eq!(
        bob_parent_after_child.transaction_status, "pending",
        "Bob's parent should still be pending"
    );
    assert!(
        alice_parent_after_child.parent_txid.is_none(),
        "Parent should still have no parent_txid"
    );
    assert!(
        bob_parent_after_child.parent_txid.is_none(),
        "Parent should still have no parent_txid"
    );

    println!("✅ CPFP child transaction detected:");
    println!("   Child txid: {}", child_txid);
    println!(
        "   Bob's child transaction status: '{}', parent_txid: {:?}",
        bob_child_tx.transaction_status, bob_child_tx.parent_txid
    );
    println!("   Parent transaction remains pending in both wallets");

    // Step 3: Mine block to confirm both transactions together (CPFP effect)
    println!("⛏️ Step 3: Mining block to confirm both parent and child together (CPFP effect)");
    env.mine_blocks(1).await.expect("Failed to mine blocks");
    env.sync_and_wait_with_retries(3)
        .await
        .expect("Failed to sync after mining with retries");

    // Verify both transactions are confirmed in the same block
    let final_alice_txs = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");
    let final_bob_txs = env
        .get_wallet_transactions(&env.bob_checksum)
        .await
        .expect("Failed to get Bob transactions");

    let alice_final_parent = final_alice_txs
        .iter()
        .find(|tx| tx.txid == parent_txid)
        .expect("Alice should have the parent transaction");
    let bob_final_parent = final_bob_txs
        .iter()
        .find(|tx| tx.txid == parent_txid)
        .expect("Bob should have the parent transaction");
    let bob_final_child = final_bob_txs
        .iter()
        .find(|tx| tx.txid == child_txid)
        .expect("Bob should have the child transaction");

    // Verify both transactions are confirmed
    assert_eq!(
        alice_final_parent.transaction_status, "confirmed",
        "Alice's parent should be confirmed"
    );
    assert_eq!(
        bob_final_parent.transaction_status, "confirmed",
        "Bob's parent should be confirmed"
    );
    assert_eq!(
        bob_final_child.transaction_status, "confirmed",
        "Bob's child should be confirmed"
    );

    // Verify they were confirmed in the same block
    assert!(
        alice_final_parent.block_height.is_some(),
        "Alice's parent should have block height"
    );
    assert!(
        bob_final_parent.block_height.is_some(),
        "Bob's parent should have block height"
    );
    assert!(
        bob_final_child.block_height.is_some(),
        "Bob's child should have block height"
    );
    assert_eq!(
        alice_final_parent.block_height, bob_final_parent.block_height,
        "Parent should have same block height in both wallets"
    );
    assert_eq!(
        bob_final_parent.block_height, bob_final_child.block_height,
        "Parent and child should be in same block"
    );

    // Verify CPFP relationship is preserved after confirmation
    assert!(
        alice_final_parent.parent_txid.is_none(),
        "Parent should still have no parent_txid after confirmation"
    );
    assert!(
        bob_final_parent.parent_txid.is_none(),
        "Parent should still have no parent_txid after confirmation"
    );
    assert_eq!(
        bob_final_child.parent_txid,
        Some(parent_txid.clone()),
        "Child should still have parent_txid after confirmation"
    );

    println!("✅ CPFP test passed!");
    println!(
        "   - Parent transaction: {} (status: 'confirmed', block: {:?})",
        parent_txid, alice_final_parent.block_height
    );
    println!(
        "   - Child transaction: {} (status: 'confirmed', block: {:?})",
        child_txid, bob_final_child.block_height
    );
    println!("   - Both transactions confirmed in same block (CPFP effect working)");
    println!(
        "   - Parent-child relationship preserved: child.parent_txid = {}",
        parent_txid
    );
    println!("   - CPFP detection and tracking successful from mempool to confirmation");
}
