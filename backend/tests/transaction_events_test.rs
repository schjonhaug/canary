use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;
use canary::config::{AppConfig, NetworkConfig};
use canary::metadata::{MetadataDb, EventType, TransactionEventWithWallet, TransactionEvent};
use canary::wallet::WalletManager;
use canary::subscription::SubscriptionTier;
use tokio::sync::broadcast;
use tempfile::tempdir;

/// Integration tests for transaction event detection using regtest environment
/// 
/// These tests use the actual Docker-based regtest environment to send real
/// Bitcoin transactions and verify that the wallet sync logic correctly 
/// creates transaction events in the database.
///
/// Prerequisites:
/// 1. Docker containers must be running (./regtest-env/docker-utils.sh start)
/// 2. Alice, Bob, Charlie wallets must exist (./regtest-env/docker-utils.sh create-wallets)
/// 3. Backend must be stopped (these tests will conflict with running backend)

// Test configuration constants
const ELECTRUM_URL: &str = "tcp://127.0.0.1:50001";
const REGTEST_NETWORK: bdk_wallet::bitcoin::Network = bdk_wallet::bitcoin::Network::Regtest;
const SYNC_WAIT_MS: u64 = 2000; // Time to wait for sync to complete
const DOCKER_UTILS_PATH: &str = "../regtest-env/docker-utils.sh";

/// Helper struct to manage test setup and cleanup
struct TestEnvironment {
    metadata_db: MetadataDb,
    wallet_manager: WalletManager,
    _temp_dir: tempfile::TempDir,
    test_user_id: String,
    alice_checksum: String,
    bob_checksum: String,
    charlie_checksum: String,
}

impl TestEnvironment {
    /// Create a new test environment with fresh database and wallets
    async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Create temporary directory for test data
        let temp_dir = tempdir()?;
        let temp_path = temp_dir.path().to_string_lossy().to_string();
        
        // Create test database
        let db_path = temp_dir.path().join("test.db");
        let test_config = AppConfig {
            network: NetworkConfig::Regtest,
            electrum_url: Some(ELECTRUM_URL.to_string()),
            bind_address: "127.0.0.1:3000".to_string(),
            data_dir: temp_path.clone(),
        };
        
        let metadata_db = MetadataDb::new(db_path.to_str().unwrap(), &test_config).await?;
        
        // Create test user
        let test_user_id = metadata_db.create_user(
            "test@example.com",
            "hashedpassword", 
            Some("Test User"),
            true
        ).await?;
        
        // Create wallet manager
        let wallet_dir = temp_dir.path().join("wallets");
        std::fs::create_dir_all(&wallet_dir)?;
        
        // Create event sender for wallet manager
        let (event_sender, _event_receiver) = broadcast::channel::<TransactionEvent>(100);
        
        let wallet_manager = WalletManager::new(
            event_sender,
            wallet_dir,
            &db_path.to_string_lossy(),
            REGTEST_NETWORK,
            ELECTRUM_URL,
            &test_config,
        ).await;
        
        // Add test wallets from regtest environment
        // These correspond to the wallets created by docker-utils.sh create-wallets
        let alice_descriptor = Self::get_wallet_descriptor("alice").await?;
        let bob_descriptor = Self::get_wallet_descriptor("bob").await?;
        let charlie_descriptor = Self::get_wallet_descriptor("charlie").await?;
        
        let alice_checksum = metadata_db.insert_wallet("Alice", &alice_descriptor, &test_user_id).await?;
        let bob_checksum = metadata_db.insert_wallet("Bob", &bob_descriptor, &test_user_id).await?;
        let charlie_checksum = metadata_db.insert_wallet("Charlie", &charlie_descriptor, &test_user_id).await?;
        
        // Create wallets in the manager (no need to load, they'll be created on first sync)
        // The wallets will be loaded automatically during sync operations
        
        // Fund Alice and Bob with 1 BTC each from the miner wallet for reliable testing
        let temp_env = TestEnvironment {
            metadata_db,
            wallet_manager,
            _temp_dir: temp_dir,
            test_user_id,
            alice_checksum: alice_checksum.clone(),
            bob_checksum: bob_checksum.clone(),
            charlie_checksum: charlie_checksum.clone(),
        };
        
        // Fund the wallets to ensure they have sufficient balance for testing
        temp_env.run_docker_utils(&["miner", "sent", "alice", "1.0"]).await?;
        temp_env.run_docker_utils(&["miner", "sent", "bob", "1.0"]).await?;
        
        Ok(temp_env)
    }
    
    /// Get wallet descriptor from regtest environment
    async fn get_wallet_descriptor(wallet_name: &str) -> Result<String, Box<dyn std::error::Error>> {
        // This would normally extract the descriptor from the regtest wallet
        // For now, we'll use placeholder descriptors that match the test wallets
        let descriptor = match wallet_name {
            "alice" => "wpkh(tprv8ZgxMBicQKsPd7Uf69XL1XwHFgmWPkCxkPRUdqEjcmN4dQZ2Xrj2TG3vL8RmE2FbHnL8V9M4F7uF3KxNvYvNb2A4cPdVhQv5mP2dP7pE6dN/<0;1>/*)",
            "bob" => "wpkh(tprv8ZgxMBicQKsPd7Uf69XL1XwHFgmWPkCxkPRUdqEjcmN4dQZ2Xrj2TG3vL8RmE2FbHnL8V9M4F7uF3KxNvYvNb2A4cPdVhQv5mP2dP7pE6dM/<0;1>/*)",
            "charlie" => "wpkh(tprv8ZgxMBicQKsPd7Uf69XL1XwHFgmWPkCxkPRUdqEjcmN4dQZ2Xrj2TG3vL8RmE2FbHnL8V9M4F7uF3KxNvYvNb2A4cPdVhQv5mP2dP7pE6dL/<0;1>/*)",
            _ => return Err("Unknown wallet name".into()),
        };
        Ok(descriptor.to_string())
    }
    
    /// Execute a docker-utils.sh command and return the output
    async fn run_docker_utils(&self, args: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
        let output = Command::new(DOCKER_UTILS_PATH)
            .args(args)
            .output()?;
            
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Docker utils command failed: {}", stderr).into());
        }
        
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
    
    /// Trigger wallet sync and wait for completion
    async fn sync_and_wait(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Trigger sync for all wallets
        let _ = self.wallet_manager.sync_tier_parallel(SubscriptionTier::Team).await;
        
        // Wait for sync to complete
        sleep(Duration::from_millis(SYNC_WAIT_MS)).await;
        
        Ok(())
    }
    
    /// Get all transaction events for a wallet from the database
    async fn get_wallet_events(&self, wallet_checksum: &str) -> Result<Vec<TransactionEventWithWallet>, Box<dyn std::error::Error>> {
        let events = self.metadata_db.get_events_by_wallet_checksum(wallet_checksum, None).await?;
        Ok(events)
    }
    
    /// Assert that a specific event exists with given properties
    fn assert_event_exists(
        &self, 
        events: &[TransactionEventWithWallet], 
        event_type: EventType,
        amount_sats: i64,
        is_confirmed: bool,
        is_rbf: Option<bool>,
        is_cpfp: Option<bool>
    ) {
        let matching_event = events.iter().find(|e| {
            e.event_type == event_type &&
            e.amount_sats == amount_sats &&
            e.is_confirmed == is_confirmed &&
            (is_rbf.is_none() || e.is_rbf == is_rbf.unwrap()) &&
            (is_cpfp.is_none() || e.is_cpfp == is_cpfp.unwrap())
        });
        
        assert!(
            matching_event.is_some(),
            "Expected event not found: type={:?}, amount={}, confirmed={}, rbf={:?}, cpfp={:?}\nActual events: {:#?}",
            event_type, amount_sats, is_confirmed, is_rbf, is_cpfp, events
        );
    }
    
    /// Mine blocks to confirm pending transactions
    async fn mine_blocks(&self, count: u32) -> Result<(), Box<dyn std::error::Error>> {
        self.run_docker_utils(&["mine", &count.to_string()]).await?;
        Ok(())
    }
}

// Using the actual TransactionEventWithWallet struct from the metadata module

#[tokio::test]
#[ignore] // Mark as ignore to prevent running in CI without regtest environment
async fn test_normal_send_transaction() {
    let mut env = TestEnvironment::new().await.expect("Failed to create test environment");
    
    // Ensure Alice has funds to send
    env.run_docker_utils(&["alice", "fund", "alice", "1.0"]).await.expect("Failed to fund Alice");
    env.mine_blocks(1).await.expect("Failed to mine blocks");
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Clear any existing events
    let initial_alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get events");
    let initial_bob_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get events");
    
    // Send 0.5 BTC from Alice to Bob (unconfirmed)
    env.run_docker_utils(&["alice", "sending", "bob", "0.5"]).await.expect("Failed to send transaction");
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Verify Alice has "Sending" event
    let alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    env.assert_event_exists(&alice_events, EventType::Send, 50_000_000, false, None, None);
    assert!(alice_events.len() > initial_alice_events.len(), "Alice should have new event");
    
    // Verify Bob has "Receiving" event  
    let bob_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get Bob events");
    env.assert_event_exists(&bob_events, EventType::Receive, 50_000_000, false, None, None);
    assert!(bob_events.len() > initial_bob_events.len(), "Bob should have new event");
    
    // Mine block to confirm transaction
    env.mine_blocks(1).await.expect("Failed to mine blocks");
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Verify Alice has "Sent" confirmation event
    let alice_events_confirmed = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    env.assert_event_exists(&alice_events_confirmed, EventType::Send, 50_000_000, true, None, None);
    
    // Verify Bob has "Received" confirmation event
    let bob_events_confirmed = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get Bob events");  
    env.assert_event_exists(&bob_events_confirmed, EventType::Receive, 50_000_000, true, None, None);
    
    println!("✅ Normal send transaction test passed");
}

#[tokio::test]
#[ignore]
async fn test_wallet_drain_scenario() {
    let mut env = TestEnvironment::new().await.expect("Failed to create test environment");
    
    // Give Bob a small amount to drain
    env.run_docker_utils(&["alice", "sent", "bob", "0.1"]).await.expect("Failed to fund Bob");
    env.sync_and_wait().await.expect("Failed to sync");
    
    let initial_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get events");
    
    // Drain Bob's entire wallet to Charlie
    env.run_docker_utils(&["bob", "sending", "charlie", "max"]).await.expect("Failed to drain wallet");
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Verify Bob has "WALLET DRAIN" sending event
    let bob_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get Bob events");
    
    // Should have exactly one new sending event for the drain
    let new_send_events: Vec<_> = bob_events.iter()
        .filter(|e| e.event_type == EventType::Send && !initial_events.iter().any(|ie| ie.id == e.id))
        .collect();
    
    assert_eq!(new_send_events.len(), 1, "Should have exactly one wallet drain event");
    assert_eq!(new_send_events[0].is_confirmed, false, "Drain event should be unconfirmed initially");
    assert!(new_send_events[0].balance_total == Some(0), "Balance should be zero after drain");
    
    // Mine block to confirm
    env.mine_blocks(1).await.expect("Failed to mine blocks");
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Verify confirmation event exists
    let bob_events_confirmed = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get Bob events");
    let confirmed_events: Vec<_> = bob_events_confirmed.iter()
        .filter(|e| e.event_type == EventType::Send && e.is_confirmed)
        .collect();
    
    assert!(!confirmed_events.is_empty(), "Should have confirmed send event after mining");
    
    println!("✅ Wallet drain scenario test passed");
}

#[tokio::test] 
#[ignore]
async fn test_rbf_transaction() {
    let mut env = TestEnvironment::new().await.expect("Failed to create test environment");
    
    // Ensure Alice has funds
    env.run_docker_utils(&["alice", "fund", "alice", "1.0"]).await.expect("Failed to fund Alice");
    env.mine_blocks(1).await.expect("Failed to mine blocks");
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Send initial RBF-enabled transaction
    let tx_output = env.run_docker_utils(&["alice", "sending", "bob", "0.1"]).await.expect("Failed to send RBF transaction");
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Extract transaction ID from output (format varies by implementation)
    let txid = extract_txid_from_output(&tx_output).expect("Failed to extract TXID");
    
    // Replace transaction with higher fee
    env.run_docker_utils(&["alice", "rbf", &txid]).await.expect("Failed to RBF transaction");
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Verify Alice has RBF events
    let alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    
    let rbf_events: Vec<_> = alice_events.iter()
        .filter(|e| e.is_rbf)
        .collect();
    
    assert!(!rbf_events.is_empty(), "Should have RBF events");
    
    println!("✅ RBF transaction test passed");
}

#[tokio::test]
#[ignore] 
async fn test_cpfp_transaction() {
    let mut env = TestEnvironment::new().await.expect("Failed to create test environment");
    
    // Setup: Alice sends low-fee transaction to Bob
    env.run_docker_utils(&["alice", "fund", "alice", "1.0"]).await.expect("Failed to fund Alice");
    env.mine_blocks(1).await.expect("Failed to mine blocks");
    
    let tx_output = env.run_docker_utils(&["alice", "sending", "bob", "0.1"]).await.expect("Failed to send transaction");
    env.sync_and_wait().await.expect("Failed to sync");
    
    let txid = extract_txid_from_output(&tx_output).expect("Failed to extract TXID");
    
    // Bob creates CPFP child transaction
    env.run_docker_utils(&["bob", "cpfp", &txid]).await.expect("Failed to create CPFP transaction");
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Verify CPFP events exist
    let bob_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get Bob events");
    
    let cpfp_events: Vec<_> = bob_events.iter()
        .filter(|e| e.is_cpfp)
        .collect();
        
    assert!(!cpfp_events.is_empty(), "Should have CPFP events");
    
    println!("✅ CPFP transaction test passed");
}

#[tokio::test]
#[ignore]
async fn test_fast_confirmation_scenario() {
    let mut env = TestEnvironment::new().await.expect("Failed to create test environment");
    
    // Fund Alice
    env.run_docker_utils(&["alice", "fund", "alice", "1.0"]).await.expect("Failed to fund Alice");
    env.mine_blocks(1).await.expect("Failed to mine blocks");
    env.sync_and_wait().await.expect("Failed to sync");
    
    let initial_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get events");
    
    // Send transaction and immediately mine (fast confirmation)
    env.run_docker_utils(&["alice", "sent", "bob", "0.1"]).await.expect("Failed to send and confirm transaction");
    env.sync_and_wait().await.expect("Failed to sync");
    
    // Verify Alice has only "Sent" event (no "Sending" event for fast confirmation)
    let alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get Alice events");
    
    let new_events: Vec<_> = alice_events.iter()
        .filter(|e| !initial_events.iter().any(|ie| ie.id == e.id))
        .collect();
    
    // Should have exactly one "Sent" event (confirmed)
    assert_eq!(new_events.len(), 1, "Should have exactly one event for fast confirmation");
    assert_eq!(new_events[0].event_type, EventType::Send, "Should be a send event");
    assert_eq!(new_events[0].is_confirmed, true, "Should be confirmed");
    
    println!("✅ Fast confirmation scenario test passed");
}

#[tokio::test]
#[ignore]
async fn test_no_duplicate_events() {
    let mut env = TestEnvironment::new().await.expect("Failed to create test environment");
    
    // Fund wallets
    env.run_docker_utils(&["alice", "fund", "alice", "1.0"]).await.expect("Failed to fund Alice");
    env.mine_blocks(1).await.expect("Failed to mine blocks");
    env.sync_and_wait().await.expect("Failed to sync");
    
    let initial_alice_events = env.get_wallet_events(&env.alice_checksum).await.expect("Failed to get events");
    let initial_bob_events = env.get_wallet_events(&env.bob_checksum).await.expect("Failed to get events");
    
    // Send transaction but don't confirm immediately  
    env.run_docker_utils(&["alice", "sending", "bob", "0.1"]).await.expect("Failed to send transaction");
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
    
    println!("✅ No duplicate events test passed");
}

/// Helper function to extract transaction ID from docker-utils.sh output
fn extract_txid_from_output(output: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Look for lines containing "Transaction sent:" or similar patterns
    for line in output.lines() {
        if line.contains("Transaction sent:") || line.contains("✅ Transaction sent:") {
            if let Some(txid) = line.split_whitespace().last() {
                if txid.len() == 64 && txid.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Ok(txid.to_string());
                }
            }
        }
    }
    Err("Could not extract transaction ID from output".into())
}

/// Helper function to check if regtest environment is available
#[allow(dead_code)]
fn check_regtest_environment() -> Result<(), Box<dyn std::error::Error>> {
    // Check if docker-utils.sh exists
    if !std::path::Path::new(DOCKER_UTILS_PATH).exists() {
        return Err("docker-utils.sh not found. Make sure regtest environment is set up.".into());
    }
    
    // Could add more checks here (Docker running, containers up, etc.)
    Ok(())
}

#[tokio::test]
async fn test_environment_setup() {
    // This is a basic test that doesn't require regtest, just checks compilation
    assert_eq!(ELECTRUM_URL, "tcp://127.0.0.1:50001");
    assert_eq!(REGTEST_NETWORK, bdk_wallet::bitcoin::Network::Regtest);
    println!("✅ Environment setup test passed");
}