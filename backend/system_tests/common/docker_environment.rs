use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;
use canary::config::{AppConfig, NetworkConfig};
use canary::metadata::{MetadataDb, EventType, TransactionEventWithWallet, TransactionEvent};
use canary::wallet::WalletManager;
use canary::subscription::SubscriptionTier;
use tokio::sync::broadcast;
use tempfile::tempdir;
use uuid::Uuid;

// Test configuration constants
const BITCOIN_IMAGE: &str = "bitcoin/bitcoin:27.1";
pub const SYNC_WAIT_MS: u64 = 2000; // Time to wait for sync to complete

/// Helper struct to manage isolated test environment with Docker containers
pub struct IsolatedTestEnvironment {
    pub metadata_db: MetadataDb,
    pub wallet_manager: WalletManager,
    _temp_dir: tempfile::TempDir,
    pub alice_checksum: String,
    pub bob_checksum: String,
    pub charlie_checksum: String,
    // Docker container management
    container_name: String,
    test_id: String,
}

impl IsolatedTestEnvironment {
    /// Create a new isolated test environment with fresh Docker containers
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Generate unique test ID for container isolation
        let test_id = Uuid::new_v4().to_string()[..8].to_string();
        let container_name = format!("test-bitcoin-{}", test_id);
        
        // Find available port for Bitcoin RPC (start from 28332)
        let bitcoin_rpc_port = Self::find_available_port(28332)?;
        
        println!("🚀 Creating isolated test environment: {}", test_id);
        println!("   Container: {}", container_name);
        println!("   RPC Port: {}", bitcoin_rpc_port);
        
        // Create temporary directory for test data
        let temp_dir = tempdir()?;
        let temp_path = temp_dir.path().to_string_lossy().to_string();
        
        // Create test database
        let db_path = temp_dir.path().join("test.db");
        let test_config = AppConfig {
            network: NetworkConfig::Regtest,
            electrum_url: None, // We'll connect directly to Bitcoin RPC for testing
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
        
        // Create wallet manager (will connect to our test Bitcoin container)
        let wallet_dir = temp_dir.path().join("wallets");
        std::fs::create_dir_all(&wallet_dir)?;
        
        let (event_sender, _event_receiver) = broadcast::channel::<TransactionEvent>(100);
        
        // Start isolated Bitcoin container
        Self::start_bitcoin_container(&container_name, bitcoin_rpc_port)?;
        
        // Wait for Bitcoin to be ready
        Self::wait_for_bitcoin_ready(&container_name).await?;
        
        // Setup deterministic wallets (same as docker-utils.sh)
        Self::setup_test_wallets(&container_name).await?;
        
        // Fund Alice and Charlie (matching docker-utils.sh setup)
        Self::fund_test_wallets(&container_name).await?;
        
        let wallet_manager = WalletManager::new(
            event_sender,
            wallet_dir,
            &db_path.to_string_lossy(),
            bdk_wallet::bitcoin::Network::Regtest,
            &format!("tcp://127.0.0.1:{}", bitcoin_rpc_port),
            &test_config,
        ).await;
        
        // Add wallets to metadata database (same descriptors as docker-utils.sh)
        let alice_descriptor = "wpkh(tprv8ZgxMBicQKsPe6D3wxZPHxy7JEMBwjxiimYXSvM8aRaqZmDvU7Jec5c8aSB8rCzFmAP6aEjPhUmiXm2KB7XUzkApX2prmDHcQsUQY5DsxJw/84h/1h/0h/<0;1>/*)#5asejmkj";
        let bob_descriptor = "wpkh(tprv8ZgxMBicQKsPd3nsWkJrKQgXk22UnGFHFPDWNCeTjvzeY9LjRYdi3RNX1NfkCwj4mD1YTsNPCCtGPTTQkxn14oLKNg6vuVkChn55qa5rC7K/84h/1h/0h/<0;1>/*)#y872gtkp";
        let charlie_descriptor = "wpkh(tprv8ZgxMBicQKsPd7Uf69XL1XwhmjHopUGep8GuEiJDZmbQz6o58LninorQAfcKZWARbtRtfnLcJ5MQ2AtHcQJCCRUcMRvmDUjyEmNUWwx8UbK/84h/1h/0h/<0;1>/*)#pe5sgqha";
        
        let alice_checksum = metadata_db.insert_wallet("Alice", alice_descriptor, &test_user_id).await?;
        let bob_checksum = metadata_db.insert_wallet("Bob", bob_descriptor, &test_user_id).await?;
        let charlie_checksum = metadata_db.insert_wallet("Charlie", charlie_descriptor, &test_user_id).await?;
        
        println!("✅ Test environment ready:");
        println!("   Alice checksum: {}", alice_checksum);
        println!("   Bob checksum: {}", bob_checksum);
        println!("   Charlie checksum: {}", charlie_checksum);
        
        Ok(IsolatedTestEnvironment {
            metadata_db,
            wallet_manager,
            _temp_dir: temp_dir,
            alice_checksum,
            bob_checksum,
            charlie_checksum,
            container_name,
            test_id,
        })
    }
    
    /// Find an available port starting from the given port
    fn find_available_port(start_port: u16) -> Result<u16, Box<dyn std::error::Error>> {
        for port in start_port..start_port + 100 {
            if std::net::TcpListener::bind(format!("127.0.0.1:{}", port)).is_ok() {
                return Ok(port);
            }
        }
        Err("No available ports found".into())
    }
    
    /// Start an isolated Bitcoin regtest container
    fn start_bitcoin_container(container_name: &str, rpc_port: u16) -> Result<(), Box<dyn std::error::Error>> {
        println!("🐳 Starting Bitcoin container: {}", container_name);
        
        let output = Command::new("docker")
            .args(&[
                "run", "-d", "--rm",
                "--name", container_name,
                "-p", &format!("{}:8332", rpc_port),
                BITCOIN_IMAGE,
                "bitcoind",
                "-regtest",
                "-rpcuser=test",
                "-rpcpassword=test",
                "-rpcbind=0.0.0.0",
                "-rpcallowip=0.0.0.0/0",
                "-fallbackfee=0.0001",
                "-txindex",
            ])
            .output()?;
            
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Failed to start Bitcoin container: {}", stderr).into());
        }
        
        println!("✅ Bitcoin container started");
        Ok(())
    }
    
    /// Wait for Bitcoin RPC to be ready
    async fn wait_for_bitcoin_ready(container_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!("⏳ Waiting for Bitcoin RPC to be ready...");
        
        for attempt in 1..=30 {
            let output = Command::new("docker")
                .args(&[
                    "exec", container_name,
                    "bitcoin-cli", "-regtest", "-rpcuser=test", "-rpcpassword=test",
                    "getblockchaininfo"
                ])
                .output();
                
            if output.is_ok() && output.unwrap().status.success() {
                println!("✅ Bitcoin RPC ready after {} seconds", attempt);
                return Ok(());
            }
            
            sleep(Duration::from_secs(1)).await;
        }
        
        Err("Bitcoin RPC failed to start within 30 seconds".into())
    }
    
    /// Setup deterministic test wallets (same as docker-utils.sh)
    async fn setup_test_wallets(container_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!("🏦 Setting up test wallets...");
        
        // Create Alice wallet with deterministic descriptor
        Self::bitcoin_cli(container_name, &[
            "-named", "createwallet", "wallet_name=alice", "disable_private_keys=false", 
            "blank=true", "descriptors=true"
        ])?;
        
        // Import Alice's deterministic descriptors (same as docker-utils.sh)
        Self::bitcoin_cli(container_name, &[
            "-rpcwallet=alice", "importdescriptors",
            r#"[{"desc": "wpkh(tprv8ZgxMBicQKsPe6D3wxZPHxy7JEMBwjxiimYXSvM8aRaqZmDvU7Jec5c8aSB8rCzFmAP6aEjPhUmiXm2KB7XUzkApX2prmDHcQsUQY5DsxJw/84h/1h/0h/0/*)#5asejmkj", "timestamp": "now", "active": true, "internal": false, "range": [0, 999]}, {"desc": "wpkh(tprv8ZgxMBicQKsPe6D3wxZPHxy7JEMBwjxiimYXSvM8aRaqZmDvU7Jec5c8aSB8rCzFmAP6aEjPhUmiXm2KB7XUzkApX2prmDHcQsUQY5DsxJw/84h/1h/0h/1/*)#9f4c0wx2", "timestamp": "now", "active": true, "internal": true, "range": [0, 999]}]"#
        ])?;
        
        // Create Bob wallet with deterministic descriptor
        Self::bitcoin_cli(container_name, &[
            "-named", "createwallet", "wallet_name=bob", "disable_private_keys=false", 
            "blank=true", "descriptors=true"
        ])?;
        
        Self::bitcoin_cli(container_name, &[
            "-rpcwallet=bob", "importdescriptors",
            r#"[{"desc": "wpkh(tprv8ZgxMBicQKsPd3nsWkJrKQgXk22UnGFHFPDWNCeTjvzeY9LjRYdi3RNX1NfkCwj4mD1YTsNPCCtGPTTQkxn14oLKNg6vuVkChn55qa5rC7K/84h/1h/0h/0/*)#y872gtkp", "timestamp": "now", "active": true, "internal": false, "range": [0, 999]}, {"desc": "wpkh(tprv8ZgxMBicQKsPd3nsWkJrKQgXk22UnGFHFPDWNCeTjvzeY9LjRYdi3RNX1NfkCwj4mD1YTsNPCCtGPTTQkxn14oLKNg6vuVkChn55qa5rC7K/84h/1h/0h/1/*)#4nmt47xe", "timestamp": "now", "active": true, "internal": true, "range": [0, 999]}]"#
        ])?;
        
        // Create Charlie wallet with deterministic descriptor
        Self::bitcoin_cli(container_name, &[
            "-named", "createwallet", "wallet_name=charlie", "disable_private_keys=false", 
            "blank=true", "descriptors=true"
        ])?;
        
        Self::bitcoin_cli(container_name, &[
            "-rpcwallet=charlie", "importdescriptors",
            r#"[{"desc": "wpkh(tprv8ZgxMBicQKsPd7Uf69XL1XwhmjHopUGep8GuEiJDZmbQz6o58LninorQAfcKZWARbtRtfnLcJ5MQ2AtHcQJCCRUcMRvmDUjyEmNUWwx8UbK/84h/1h/0h/0/*)#pe5sgqha", "timestamp": "now", "active": true, "internal": false, "range": [0, 999]}, {"desc": "wpkh(tprv8ZgxMBicQKsPd7Uf69XL1XwhmjHopUGep8GuEiJDZmbQz6o58LninorQAfcKZWARbtRtfnLcJ5MQ2AtHcQJCCRUcMRvmDUjyEmNUWwx8UbK/84h/1h/0h/1/*)#sd334489", "timestamp": "now", "active": true, "internal": true, "range": [0, 999]}]"#
        ])?;
        
        // Create miner wallet for generating blocks
        Self::bitcoin_cli(container_name, &[
            "-named", "createwallet", "wallet_name=miner"
        ])?;
        
        println!("✅ Test wallets created");
        Ok(())
    }
    
    /// Fund Alice and Charlie wallets for testing (matching docker-utils.sh setup)
    async fn fund_test_wallets(container_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!("💰 Funding test wallets...");
        
        // Generate some blocks to miner first
        let miner_address = Self::bitcoin_cli(container_name, &["-rpcwallet=miner", "getnewaddress"])?
            .trim().trim_matches('"').to_string();
            
        Self::bitcoin_cli(container_name, &["generatetoaddress", "101", &miner_address])?;
        
        // Fund Alice with 1.0 BTC (normal case)
        let alice_address = Self::bitcoin_cli(container_name, &["-rpcwallet=alice", "getnewaddress"])?
            .trim().trim_matches('"').to_string();
            
        Self::bitcoin_cli(container_name, &["-rpcwallet=miner", "sendtoaddress", &alice_address, "1.0"])?;
        
        // Fund Charlie at high index (250) - same as docker-utils.sh
        println!("💰 Funding Charlie at index 250 for high-index testing...");
        
        // Generate addresses up to index 250 (0-250 = 251 addresses)
        let mut charlie_addr_250 = String::new();
        for i in 0..=250 {
            let addr = Self::bitcoin_cli(container_name, &["-rpcwallet=charlie", "getnewaddress"])?
                .trim().trim_matches('"').to_string();
            
            if i == 250 {
                charlie_addr_250 = addr;
                println!("   🎯 Charlie address at index 250: {}", charlie_addr_250);
            }
            
            // Show progress every 50 addresses
            if i > 0 && i % 50 == 0 {
                println!("   📍 Generated Charlie addresses 0-{}...", i);
            }
        }
        
        // Send 0.5 BTC to Charlie's address at index 250
        Self::bitcoin_cli(container_name, &["-rpcwallet=miner", "sendtoaddress", &charlie_addr_250, "0.5"])?;
        
        // Mine a block to confirm both transactions
        Self::bitcoin_cli(container_name, &["generatetoaddress", "1", &miner_address])?;
        
        println!("✅ Alice funded with 1.0 BTC (index 0)");
        println!("✅ Charlie funded with 0.5 BTC (index 250)");
        println!("✅ Bob unfunded (for testing receive scenarios)");
        Ok(())
    }
    
    /// Execute bitcoin-cli command in the container
    fn bitcoin_cli(container_name: &str, args: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
        let mut cmd_args = vec![
            "exec", container_name, "bitcoin-cli", "-regtest", "-rpcuser=test", "-rpcpassword=test"
        ];
        cmd_args.extend_from_slice(args);
        
        let output = Command::new("docker")
            .args(&cmd_args)
            .output()?;
            
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Bitcoin CLI command failed: {}", stderr).into());
        }
        
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
    
    /// Send transaction between wallets
    pub async fn send_transaction(&self, from_wallet: &str, to_wallet: &str, amount: &str) -> Result<String, Box<dyn std::error::Error>> {
        println!("💸 Sending {} BTC from {} to {}", amount, from_wallet, to_wallet);
        
        let to_address = Self::bitcoin_cli(&self.container_name, &[
            &format!("-rpcwallet={}", to_wallet), "getnewaddress"
        ])?.trim().trim_matches('"').to_string();
        
        let txid = if amount == "max" {
            // Drain the wallet
            let balance = Self::bitcoin_cli(&self.container_name, &[
                &format!("-rpcwallet={}", from_wallet), "getbalance"
            ])?.trim().to_string();
            
            Self::bitcoin_cli(&self.container_name, &[
                &format!("-rpcwallet={}", from_wallet), 
                "sendtoaddress", &to_address, &balance, "", "", "true"  // subtractfeefromamount=true
            ])?
        } else {
            Self::bitcoin_cli(&self.container_name, &[
                &format!("-rpcwallet={}", from_wallet), 
                "sendtoaddress", &to_address, amount
            ])?
        }.trim().trim_matches('"').to_string();
        
        println!("✅ Transaction sent: {}", txid);
        Ok(txid)
    }
    
    /// Mine blocks to confirm transactions
    pub async fn mine_blocks(&self, count: u32) -> Result<(), Box<dyn std::error::Error>> {
        let miner_address = Self::bitcoin_cli(&self.container_name, &["-rpcwallet=miner", "getnewaddress"])?
            .trim().trim_matches('"').to_string();
            
        Self::bitcoin_cli(&self.container_name, &[
            "generatetoaddress", &count.to_string(), &miner_address
        ])?;
        
        println!("⛏️ Mined {} blocks", count);
        Ok(())
    }
    
    /// Trigger wallet sync and wait for completion
    pub async fn sync_and_wait(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let _ = self.wallet_manager.sync_tier_parallel(SubscriptionTier::Team).await;
        sleep(Duration::from_millis(SYNC_WAIT_MS)).await;
        Ok(())
    }
    
    /// Get all transaction events for a wallet from the database
    pub async fn get_wallet_events(&self, wallet_checksum: &str) -> Result<Vec<TransactionEventWithWallet>, Box<dyn std::error::Error>> {
        let events = self.metadata_db.get_events_by_wallet_checksum(wallet_checksum, None).await?;
        Ok(events)
    }
}

impl Drop for IsolatedTestEnvironment {
    /// Cleanup: Stop and remove the Docker container
    fn drop(&mut self) {
        println!("🧹 Cleaning up test environment: {}", self.test_id);
        
        let _ = Command::new("docker")
            .args(&["stop", &self.container_name])
            .output();
            
        println!("✅ Test cleanup completed");
    }
}