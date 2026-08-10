use bdk_wallet::rusqlite::Connection;
use bdk_wallet::{KeychainKind, Wallet};
use canary::metadata::EventType;

mod common;
use common::docker_environment::IsolatedTestEnvironment;

/// Test 1: High Index Fund Detection (Index 250)
/// Purpose: Verify that wallets can detect funds at high address indexes
///
/// These tests verify that wallets can detect funds and transactions at high
/// address indexes (250+) which is critical for wallet recovery scenarios.

#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_high_index_fund_detection() {
    let mut env = IsolatedTestEnvironment::new_with_charlie()
        .await
        .expect("Failed to create test environment");

    // Initial sync should detect both Alice (index 0) and Charlie (index 250) funding
    env.sync_and_wait().await.expect("Failed to sync");

    let alice_transactions = env
        .get_wallet_transactions(&env.alice_checksum)
        .await
        .expect("Failed to get Alice transactions");
    let charlie_transactions = env
        .get_wallet_transactions(&env.charlie_checksum)
        .await
        .expect("Failed to get Charlie transactions");

    // Alice should have transactions (funded at normal index 0)
    let alice_receive_transactions: Vec<_> = alice_transactions
        .iter()
        .filter(|t| t.transaction_type == EventType::Receive)
        .collect();
    assert!(
        !alice_receive_transactions.is_empty(),
        "Alice should have receive transactions from funding at index 0"
    );

    // Charlie should have transactions (funded at high index 250)
    let charlie_receive_transactions: Vec<_> = charlie_transactions
        .iter()
        .filter(|t| t.transaction_type == EventType::Receive)
        .collect();
    assert!(
        !charlie_receive_transactions.is_empty(),
        "Charlie should have receive transactions from funding at index 250"
    );

    // Recovery scans to the selected depth, but BDK's last_active_indices should persist only
    // Canary's normal 20-address lookahead beyond the highest used address.
    let charlie_metadata = env
        .wallet_manager
        .metadata_db
        .get_wallet_by_checksum(&env.charlie_checksum)
        .await
        .expect("Failed to read Charlie metadata")
        .expect("Charlie metadata missing");
    let wallet_path = env
        .wallet_manager
        .wallet_dir
        .join(format!("{}.sqlite", env.charlie_checksum));
    let mut connection = Connection::open(wallet_path).expect("Failed to open Charlie wallet");
    let charlie_wallet = Wallet::load()
        .two_path_descriptor(charlie_metadata.descriptor)
        .check_network(env.wallet_manager.get_network())
        .load_wallet(&mut connection)
        .expect("Failed to load Charlie wallet")
        .expect("Charlie wallet missing");
    assert_eq!(
        charlie_wallet.derivation_index(KeychainKind::External),
        Some(270),
        "index 250 activity should retain a 20-address lookahead, not the scan depth"
    );

    println!(
        "📊 Alice transactions (index 0): {}",
        alice_receive_transactions.len()
    );
    println!(
        "📊 Charlie transactions (index 250): {}",
        charlie_receive_transactions.len()
    );

    println!("✅ High-index fund detection test passed!");
    println!("   - Alice funded at index 0: detected ✓");
    println!("   - Charlie funded at index 250: detected ✓");
}
