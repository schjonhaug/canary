use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;
use canary::config::{AppConfig, NetworkConfig};
use canary::metadata::{MetadataDb, TransactionWithWallet, TransactionNotification};
use canary::wallet::WalletManager;
use canary::subscription::SubscriptionTier;
use canary::api::AppServices;
use tokio::sync::broadcast;
use tempfile::tempdir;
use uuid::Uuid;
use serde_json::{self, Value};
use std::fs;
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub const SYNC_WAIT_MS: u64 = 10000; // Time to wait for mempool propagation and sync to complete

/// Helper struct to manage isolated test environment with Docker Compose
#[allow(dead_code)] // charlie_checksum and bitcoin_rpc_port used in other test files
pub struct IsolatedTestEnvironment {
    pub metadata_db: MetadataDb,
    pub wallet_manager: WalletManager,
    _temp_dir: tempfile::TempDir,
    pub alice_checksum: String,
    pub bob_checksum: String,
    pub charlie_checksum: String,
    // Docker compose management
    compose_dir: std::path::PathBuf,
    pub test_id: String,
    bitcoin_rpc_port: u16,
    fulcrum_port: u16,
}

impl IsolatedTestEnvironment {
    /// Wait for all wallets to be marked as ready in the database
    async fn wait_for_wallets_ready(metadata_db: &MetadataDb, checksums: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
        println!("⏳ Waiting for all wallets to be marked as ready...");
        let start_time = std::time::Instant::now();
        let timeout = Duration::from_secs(30);
        
        loop {
            let ready_wallets = metadata_db.get_ready_wallets().await?;
            let ready_checksums: std::collections::HashSet<_> = ready_wallets.iter()
                .map(|w| w.checksum.as_str())
                .collect();
            
            let all_ready = checksums.iter().all(|checksum| ready_checksums.contains(checksum));
            
            if all_ready {
                println!("✅ All wallets are ready after {:?}", start_time.elapsed());
                return Ok(());
            }
            
            if start_time.elapsed() > timeout {
                println!("❌ Timeout waiting for wallets to be ready. Current ready wallets: {:?}", ready_checksums);
                return Err("Timeout waiting for wallets to be marked as ready".into());
            }
            
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    /// Create a new isolated test environment using Docker Compose
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Generate unique test ID for isolation
        let test_id = Uuid::new_v4().to_string()[..8].to_string();
        
        // Clean up any orphaned test containers from previous runs
        Self::cleanup_orphaned_test_containers();
        
        // Use test_id to generate consistent port offset to avoid conflicts
        let test_id_bytes = test_id.as_bytes();
        let test_offset = (test_id_bytes[0] as u16) * 10;
        
        // Find available ports for Bitcoin RPC and Fulcrum with same offset
        let bitcoin_rpc_port = Self::find_available_port_with_offset(28332, test_offset)?;
        let fulcrum_port = Self::find_available_port_with_offset(50001, test_offset)?;
        
        println!("🚀 Creating isolated test environment: {}", test_id);
        println!("   Bitcoin RPC Port: {}", bitcoin_rpc_port);
        println!("   Fulcrum Electrum Port: {}", fulcrum_port);
        
        // Create temporary directory for test data
        let temp_dir = tempdir()?;
        let temp_path = temp_dir.path().to_string_lossy().to_string();
        
        // Create compose directory within temp dir
        let compose_dir = temp_dir.path().join("compose");
        fs::create_dir_all(&compose_dir)?;
        
        // Create test-specific docker-compose.yml
        Self::create_test_docker_compose(&compose_dir, &test_id, bitcoin_rpc_port, fulcrum_port)?;
        
        // Create test database
        let db_path = temp_dir.path().join("test.db");
        // Set FOSS mode for simpler testing without Stripe dependencies
        std::env::set_var("CANARY_MODE", "foss");
        
        let test_config = AppConfig {
            network: NetworkConfig::Regtest,
            electrum_url: None,
            bind_address: "127.0.0.1:3000".to_string(),
            data_dir: temp_path.clone(),
        };
        
        let metadata_db = MetadataDb::new(db_path.to_str().unwrap(), &test_config).await?;
        
        // In FOSS mode, the hardcoded foss-user is created automatically as admin
        let test_user_id = "foss-user".to_string();
        
        // Start Docker Compose environment
        Self::docker_compose_up(&compose_dir)?;
        
        // Wait for services to be ready
        Self::wait_for_bitcoin_ready(&compose_dir, &test_id).await?;
        Self::wait_for_fulcrum_ready(&compose_dir, fulcrum_port).await?;
        
        // Setup deterministic wallets (Alice and Bob only for mined directly tests)
        Self::setup_alice_bob_wallets(&compose_dir, &test_id).await?;
        
        // Fund Alice (for mined directly tests, Charlie not needed)
        Self::fund_alice_only(&compose_dir, &test_id, fulcrum_port).await?;
        
        // Create wallet manager (connects to Fulcrum)
        let wallet_dir = temp_dir.path().join("wallets");
        std::fs::create_dir_all(&wallet_dir)?;
        
        let (notification_sender, _notification_receiver) = broadcast::channel::<TransactionNotification>(100);
        
        let mut wallet_manager = WalletManager::new(
            notification_sender,
            wallet_dir,
            &db_path.to_string_lossy(),
            bdk_wallet::bitcoin::Network::Regtest,
            &format!("tcp://127.0.0.1:{}", fulcrum_port),
            &test_config,
        ).await;
        
        // Create AppServices to access wallet creation service
        let wallet_creation_service = canary::wallet::WalletCreationService::new(
            wallet_manager.wallet_dir.clone(),
            metadata_db.clone(),
            wallet_manager.electrum_client.clone(),
            wallet_manager.get_network(),
        );
        let app_services = AppServices {
            metadata_db: metadata_db.clone(),
            wallet_creation_service,
        };
        
        // Create wallets using the correct XPUB descriptors from docker-utils.sh
        let alice_descriptor = "wpkh([805c684b/84h/1h/0h]tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5/<0;1>/*)#8nt3y08q";
        let bob_descriptor = "wpkh([aeea3541/84h/1h/0h]tpubDDCjkgMuodinFyfhacZPTzffAKtCbuZejpkSMJB673c9ZSsVrq5FnL5rhjFjyCDva5Pka7sn9UDe7xmzpRCNnKNqXbteTnPzLRVNcsvCcpk/<0;1>/*)#ff9zpyxa";
        
        let alice_metadata = app_services.wallet_creation_service.create_wallet_non_blocking(
            "Alice", alice_descriptor, &test_user_id, true, Some("auto"), Some("20")
        ).await?;
        let bob_metadata = app_services.wallet_creation_service.create_wallet_non_blocking(
            "Bob", bob_descriptor, &test_user_id, true, Some("auto"), Some("20")
        ).await?;
        
        let alice_checksum = alice_metadata.checksum;
        let bob_checksum = bob_metadata.checksum;
        
        println!("✅ Test environment ready:");
        println!("   Alice checksum: {}", alice_checksum);
        println!("   Bob checksum: {}", bob_checksum);
        
        // Wait for all wallets to be marked as ready before proceeding
        Self::wait_for_wallets_ready(&metadata_db, &[&alice_checksum, &bob_checksum]).await?;
        
        // CRITICAL: Do an initial sync to ensure historical transactions are properly processed
        // This prevents the first test sync from detecting historical transactions as new
        println!("🔄 Running initial sync to establish historical transaction baseline...");
        let _ = wallet_manager.sync_tier_parallel(SubscriptionTier::Team).await;
        sleep(Duration::from_millis(500)).await;
        println!("✅ Initial historical sync completed");
        
        Ok(IsolatedTestEnvironment {
            metadata_db,
            wallet_manager,
            _temp_dir: temp_dir,
            alice_checksum,
            bob_checksum,
            charlie_checksum: String::new(), // Empty for mined directly tests
            compose_dir,
            test_id,
            bitcoin_rpc_port,
            fulcrum_port,
        })
    }
    
    /// Find an available port starting from the given port with offset per test
    /// Uses a more robust port allocation strategy to avoid conflicts
    fn find_available_port_with_offset(start_port: u16, test_offset: u16) -> Result<u16, Box<dyn std::error::Error>> {
        let base_port = start_port + test_offset;
        
        // First, try the base port with offset
        for port in base_port..base_port + 50 {
            if Self::is_port_available(port) {
                return Ok(port);
            }
        }
        
        // If that fails, try a wider random range to avoid collisions
        use rand::Rng;
        let mut rng = rand::thread_rng();
        for _ in 0..100 {
            let random_port = rng.gen_range(30000..50000);
            if Self::is_port_available(random_port) {
                return Ok(random_port);
            }
        }
        
        Err("No available ports found".into())
    }
    
    /// Check if a port is available by attempting to bind and also checking for Docker containers
    fn is_port_available(port: u16) -> bool {
        // First check if we can bind to the port
        if std::net::TcpListener::bind(format!("127.0.0.1:{}", port)).is_err() {
            return false;
        }
        
        // Also check if Docker has any containers using this port
        let output = std::process::Command::new("docker")
            .args(&["ps", "--filter", &format!("publish={}", port), "--quiet"])
            .output();
            
        match output {
            Ok(output) => {
                let containers = String::from_utf8_lossy(&output.stdout);
                containers.trim().is_empty() // Port is available if no containers are using it
            }
            Err(_) => true // If docker command fails, assume port is available
        }
    }
    
    /// Clean up any orphaned test containers from previous runs
    fn cleanup_orphaned_test_containers() {
        println!("🧹 Cleaning up orphaned test containers...");
        
        // Stop and remove any containers with names starting with 'test-'
        let cleanup_commands = vec![
            vec!["docker", "stop", "$(docker", "ps", "-q", "--filter", "name=test-)", "2>/dev/null", "||", "true"],
            vec!["docker", "rm", "$(docker", "ps", "-aq", "--filter", "name=test-)", "2>/dev/null", "||", "true"],
        ];
        
        for cmd in cleanup_commands {
            let result = std::process::Command::new("sh")
                .arg("-c")
                .arg(cmd.join(" "))
                .output();
                
            match result {
                Ok(output) => {
                    if !output.status.success() {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        if !stderr.is_empty() && !stderr.contains("No such container") {
                            println!("⚠️  Cleanup warning: {}", stderr.trim());
                        }
                    }
                }
                Err(e) => {
                    println!("⚠️  Cleanup command failed: {}", e);
                }
            }
        }
        
        println!("✅ Orphaned container cleanup completed");
    }
    
    /// Create test-specific docker-compose.yml and config files
    fn create_test_docker_compose(
        compose_dir: &std::path::Path,
        test_id: &str,
        bitcoin_rpc_port: u16,
        fulcrum_port: u16
    ) -> Result<(), Box<dyn std::error::Error>> {
        let bitcoin_container_name = format!("test-bitcoin-{}", test_id);
        let fulcrum_container_name = format!("test-fulcrum-{}", test_id);
        
        // Create bitcoin.conf (matching dev setup)
        let bitcoin_conf = r#"# Test Regtest configuration
regtest=1
txindex=1
server=1
rpcbind=0.0.0.0:8332
rpcuser=test
rpcpassword=test
disablewallet=0
fallbackfee=0.00001
printtoconsole=1
persistmempool=0
"#;
        fs::write(compose_dir.join("bitcoin.conf"), bitcoin_conf)?;
        
        // Create fulcrum.conf
        let fulcrum_conf = format!(r#"# Test Fulcrum Configuration
bitcoind = {}:8332
rpcuser = test
rpcpassword = test
tcp = 0.0.0.0:50001
datadir = /data
worker_threads = 1
utxo-cache = 1024
debug = 1
"#, bitcoin_container_name);
        fs::write(compose_dir.join("fulcrum.conf"), fulcrum_conf)?;
        
        // Create docker-compose.yml
        let compose_yml = format!(r#"services:
  {}:
    image: ghcr.io/sethforprivacy/bitcoind:latest
    container_name: {}
    ports:
      - "{}:8332"
    volumes:
      - ./bitcoin.conf:/bitcoin/.bitcoin/bitcoin.conf
      - bitcoin_data_{}:/bitcoin/.bitcoin
    environment:
      - RPC_USER=test
      - RPC_PASSWORD=test

  {}:
    image: cculianu/fulcrum:latest
    container_name: {}
    depends_on:
      - {}
    ports:
      - "{}:50001"
    volumes:
      - ./fulcrum.conf:/data/fulcrum.conf:ro
      - fulcrum_data_{}:/data
    command: ["Fulcrum", "/data/fulcrum.conf"]

volumes:
  bitcoin_data_{}:
  fulcrum_data_{}:
"#, 
            bitcoin_container_name, bitcoin_container_name, bitcoin_rpc_port, test_id,
            fulcrum_container_name, fulcrum_container_name, bitcoin_container_name, 
            fulcrum_port, test_id,
            test_id, test_id
        );
        fs::write(compose_dir.join("docker-compose.yml"), compose_yml)?;
        
        Ok(())
    }
    
    /// Start Docker Compose environment
    fn docker_compose_up(compose_dir: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
        println!("🐳 Starting Docker Compose environment...");
        
        let output = Command::new("docker-compose")
            .current_dir(compose_dir)
            .args(&["up", "-d"])
            .output()?;
            
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Failed to start Docker Compose: {}", stderr).into());
        }
        
        println!("✅ Docker Compose environment started");
        Ok(())
    }
    
    /// Wait for Bitcoin RPC to be ready
    async fn wait_for_bitcoin_ready(_compose_dir: &std::path::Path, test_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!("⏳ Waiting for Bitcoin RPC to be ready...");
        let bitcoin_container_name = format!("test-bitcoin-{}", test_id);
        
        for attempt in 1..=30 {
            let output = Command::new("docker")
                .args(&[
                    "exec", &bitcoin_container_name,
                    "bitcoin-cli", "-regtest", "-rpcport=8332", "-rpcuser=test", "-rpcpassword=test",
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
    
    /// Wait for Fulcrum to be ready and able to serve Electrum requests
    async fn wait_for_fulcrum_ready(_compose_dir: &std::path::Path, fulcrum_port: u16) -> Result<(), Box<dyn std::error::Error>> {
        println!("⏳ Waiting for Fulcrum Electrum server to be ready...");
        
        // First wait for port to be open
        for attempt in 1..=120 { // Increased timeout for Fulcrum startup
            let connection_test = std::net::TcpStream::connect(format!("127.0.0.1:{}", fulcrum_port));
            
            if connection_test.is_ok() {
                println!("   📡 Fulcrum port {} is open after {} seconds", fulcrum_port, attempt);
                
                // Give Fulcrum some more time to fully initialize before we proceed
                println!("   ⏳ Waiting additional 10 seconds for Fulcrum to fully initialize...");
                sleep(Duration::from_secs(10)).await;
                
                println!("✅ Fulcrum Electrum server ready");
                return Ok(());
            }
            
            if attempt % 15 == 0 {
                println!("   ⏳ Still waiting for Fulcrum port... ({}/120 seconds)", attempt);
            }
            
            sleep(Duration::from_secs(2)).await;
        }
        
        Err("Fulcrum port failed to open within 4 minutes".into())
    }
    
    /// Wait for Fulcrum to sync with Bitcoin Core after mining blocks
    async fn wait_for_fulcrum_sync_after_mining(bitcoin_container: &str, _fulcrum_port: u16) -> Result<(), Box<dyn std::error::Error>> {
        // Get Bitcoin Core's current block height for logging
        let btc_height_str = Self::bitcoin_cli(bitcoin_container, &["getblockcount"])?;
        let btc_height: u64 = btc_height_str.trim().parse()
            .map_err(|_| "Failed to parse Bitcoin block height")?;
        
        println!("   🧱 Bitcoin Core height: {}, giving Fulcrum time to sync...", btc_height);
        
        // For now, use a fixed delay to let Fulcrum sync
        // This is simpler than trying to query Fulcrum's height which is causing connection issues
        sleep(Duration::from_secs(5)).await;
        
        println!("   ✅ Fulcrum should have synced");
        Ok(())
    }
    
    /// Setup deterministic test wallets (same as docker-utils.sh)
    #[allow(dead_code)] // Used in other test files
    async fn setup_test_wallets(_compose_dir: &std::path::Path, test_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!("🏦 Setting up test wallets...");
        let bitcoin_container_name = format!("test-bitcoin-{}", test_id);
        
        // Create Alice wallet with deterministic descriptor
        Self::bitcoin_cli(&bitcoin_container_name, &[
            "-named", "createwallet", "wallet_name=alice", "disable_private_keys=false", 
            "blank=true", "descriptors=true"
        ])?;
        
        // Import Alice's deterministic descriptors (same as docker-utils.sh)
        Self::bitcoin_cli(&bitcoin_container_name, &[
            "-rpcwallet=alice", "importdescriptors",
            r#"[{"desc": "wpkh(tprv8ZgxMBicQKsPe6D3wxZPHxy7JEMBwjxiimYXSvM8aRaqZmDvU7Jec5c8aSB8rCzFmAP6aEjPhUmiXm2KB7XUzkApX2prmDHcQsUQY5DsxJw/84h/1h/0h/0/*)#5asejmkj", "timestamp": "now", "active": true, "internal": false, "range": [0, 999]}, {"desc": "wpkh(tprv8ZgxMBicQKsPe6D3wxZPHxy7JEMBwjxiimYXSvM8aRaqZmDvU7Jec5c8aSB8rCzFmAP6aEjPhUmiXm2KB7XUzkApX2prmDHcQsUQY5DsxJw/84h/1h/0h/1/*)#9f4c0wx2", "timestamp": "now", "active": true, "internal": true, "range": [0, 999]}]"#
        ])?;
        
        // Create Bob wallet with deterministic descriptor
        Self::bitcoin_cli(&bitcoin_container_name, &[
            "-named", "createwallet", "wallet_name=bob", "disable_private_keys=false", 
            "blank=true", "descriptors=true"
        ])?;
        
        Self::bitcoin_cli(&bitcoin_container_name, &[
            "-rpcwallet=bob", "importdescriptors",
            r#"[{"desc": "wpkh(tprv8ZgxMBicQKsPd3nsWkJrKQgXk22UnGFHFPDWNCeTjvzeY9LjRYdi3RNX1NfkCwj4mD1YTsNPCCtGPTTQkxn14oLKNg6vuVkChn55qa5rC7K/84h/1h/0h/0/*)#y872gtkp", "timestamp": "now", "active": true, "internal": false, "range": [0, 999]}, {"desc": "wpkh(tprv8ZgxMBicQKsPd3nsWkJrKQgXk22UnGFHFPDWNCeTjvzeY9LjRYdi3RNX1NfkCwj4mD1YTsNPCCtGPTTQkxn14oLKNg6vuVkChn55qa5rC7K/84h/1h/0h/1/*)#4nmt47xe", "timestamp": "now", "active": true, "internal": true, "range": [0, 999]}]"#
        ])?;
        
        // Create Charlie wallet with deterministic descriptor
        Self::bitcoin_cli(&bitcoin_container_name, &[
            "-named", "createwallet", "wallet_name=charlie", "disable_private_keys=false", 
            "blank=true", "descriptors=true"
        ])?;
        
        Self::bitcoin_cli(&bitcoin_container_name, &[
            "-rpcwallet=charlie", "importdescriptors",
            r#"[{"desc": "wpkh(tprv8ZgxMBicQKsPd7Uf69XL1XwhmjHopUGep8GuEiJDZmbQz6o58LninorQAfcKZWARbtRtfnLcJ5MQ2AtHcQJCCRUcMRvmDUjyEmNUWwx8UbK/84h/1h/0h/0/*)#pe5sgqha", "timestamp": "now", "active": true, "internal": false, "range": [0, 999]}, {"desc": "wpkh(tprv8ZgxMBicQKsPd7Uf69XL1XwhmjHopUGep8GuEiJDZmbQz6o58LninorQAfcKZWARbtRtfnLcJ5MQ2AtHcQJCCRUcMRvmDUjyEmNUWwx8UbK/84h/1h/0h/1/*)#sd334489", "timestamp": "now", "active": true, "internal": true, "range": [0, 999]}]"#
        ])?;
        
        // Create miner wallet for generating blocks
        Self::bitcoin_cli(&bitcoin_container_name, &[
            "-named", "createwallet", "wallet_name=miner"
        ])?;
        
        println!("✅ Test wallets created");
        Ok(())
    }
    
    /// Setup Alice and Bob wallets only (for mined directly tests)
    async fn setup_alice_bob_wallets(_compose_dir: &std::path::Path, test_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!("🏦 Setting up Alice and Bob test wallets...");
        let bitcoin_container_name = format!("test-bitcoin-{}", test_id);
        
        // Create Alice wallet with deterministic descriptor (from docker-utils.sh)
        Self::bitcoin_cli(&bitcoin_container_name, &[
            "-named", "createwallet", "wallet_name=alice", "disable_private_keys=false", 
            "blank=true", "descriptors=true"
        ])?;
        
        // Import Alice's deterministic descriptors (same as docker-utils.sh)
        Self::bitcoin_cli(&bitcoin_container_name, &[
            "-rpcwallet=alice", "importdescriptors",
            r#"[{"desc": "wpkh(tprv8ZgxMBicQKsPe6D3wxZPHxy7JEMBwjxiimYXSvM8aRaqZmDvU7Jec5c8aSB8rCzFmAP6aEjPhUmiXm2KB7XUzkApX2prmDHcQsUQY5DsxJw/84h/1h/0h/0/*)#5asejmkj", "timestamp": "now", "active": true, "internal": false, "range": [0, 999]}, {"desc": "wpkh(tprv8ZgxMBicQKsPe6D3wxZPHxy7JEMBwjxiimYXSvM8aRaqZmDvU7Jec5c8aSB8rCzFmAP6aEjPhUmiXm2KB7XUzkApX2prmDHcQsUQY5DsxJw/84h/1h/0h/1/*)#9f4c0wx2", "timestamp": "now", "active": true, "internal": true, "range": [0, 999]}]"#
        ])?;
        
        // Create Bob wallet with deterministic descriptor
        Self::bitcoin_cli(&bitcoin_container_name, &[
            "-named", "createwallet", "wallet_name=bob", "disable_private_keys=false", 
            "blank=true", "descriptors=true"
        ])?;
        
        Self::bitcoin_cli(&bitcoin_container_name, &[
            "-rpcwallet=bob", "importdescriptors",
            r#"[{"desc": "wpkh(tprv8ZgxMBicQKsPd3nsWkJrKQgXk22UnGFHFPDWNCeTjvzeY9LjRYdi3RNX1NfkCwj4mD1YTsNPCCtGPTTQkxn14oLKNg6vuVkChn55qa5rC7K/84h/1h/0h/0/*)#y872gtkp", "timestamp": "now", "active": true, "internal": false, "range": [0, 999]}, {"desc": "wpkh(tprv8ZgxMBicQKsPd3nsWkJrKQgXk22UnGFHFPDWNCeTjvzeY9LjRYdi3RNX1NfkCwj4mD1YTsNPCCtGPTTQkxn14oLKNg6vuVkChn55qa5rC7K/84h/1h/0h/1/*)#4nmt47xe", "timestamp": "now", "active": true, "internal": true, "range": [0, 999]}]"#
        ])?;
        
        // Create miner wallet for generating blocks
        Self::bitcoin_cli(&bitcoin_container_name, &[
            "-named", "createwallet", "wallet_name=miner"
        ])?;
        
        println!("✅ Alice and Bob test wallets created");
        Ok(())
    }
    
    /// Fund Alice and Charlie wallets for testing (matching docker-utils.sh setup)
    #[allow(dead_code)] // Used in other test files
    async fn fund_test_wallets(_compose_dir: &std::path::Path, test_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!("💰 Funding test wallets...");
        let bitcoin_container_name = format!("test-bitcoin-{}", test_id);
        
        // Generate some blocks to miner first
        let miner_address = Self::bitcoin_cli(&bitcoin_container_name, &["-rpcwallet=miner", "getnewaddress"])?
            .trim().trim_matches('"').to_string();
            
        Self::bitcoin_cli(&bitcoin_container_name, &["generatetoaddress", "101", &miner_address])?;
        
        // Fund Alice with 1.0 BTC (normal case)
        let alice_address = Self::bitcoin_cli(&bitcoin_container_name, &["-rpcwallet=alice", "getnewaddress"])?
            .trim().trim_matches('"').to_string();
            
        Self::bitcoin_cli(&bitcoin_container_name, &["-rpcwallet=miner", "sendtoaddress", &alice_address, "1.0"])?;
        
        // Fund Charlie at high index (250) - same as docker-utils.sh
        println!("💰 Funding Charlie at index 250 for high-index testing...");
        
        // Generate addresses up to index 250 (0-250 = 251 addresses)
        let mut charlie_addr_250 = String::new();
        for i in 0..=250 {
            let addr = Self::bitcoin_cli(&bitcoin_container_name, &["-rpcwallet=charlie", "getnewaddress"])?
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
        Self::bitcoin_cli(&bitcoin_container_name, &["-rpcwallet=miner", "sendtoaddress", &charlie_addr_250, "0.5"])?;
        
        // Mine a block to confirm both transactions
        Self::bitcoin_cli(&bitcoin_container_name, &["generatetoaddress", "1", &miner_address])?;
        
        println!("✅ Alice funded with 1.0 BTC (index 0)");
        println!("✅ Charlie funded with 0.5 BTC (index 250)");
        println!("✅ Bob unfunded (for testing receive scenarios)");
        Ok(())
    }
    
    /// Fund Alice wallet only (for mined directly tests that don't need Charlie)
    async fn fund_alice_only(_compose_dir: &std::path::Path, test_id: &str, fulcrum_port: u16) -> Result<(), Box<dyn std::error::Error>> {
        let bitcoin_container_name = format!("test-bitcoin-{}", test_id);
        
        // Generate some blocks to miner first
        let miner_address = Self::bitcoin_cli(&bitcoin_container_name, &["-rpcwallet=miner", "getnewaddress"])?
            .trim().trim_matches('"').to_string();
            
        Self::bitcoin_cli(&bitcoin_container_name, &["generatetoaddress", "101", &miner_address])?;
        
        // Fund Alice with 1.0 BTC (normal case)
        let alice_address = Self::bitcoin_cli(&bitcoin_container_name, &["-rpcwallet=alice", "getnewaddress"])?
            .trim().trim_matches('"').to_string();
            
        Self::bitcoin_cli(&bitcoin_container_name, &["-rpcwallet=miner", "sendtoaddress", &alice_address, "1.0"])?;
        
        // Mine a block to confirm the transaction
        Self::bitcoin_cli(&bitcoin_container_name, &["generatetoaddress", "1", &miner_address])?;
        
        println!("✅ Alice funded with 1.0 BTC (index 0)");
        println!("✅ Bob unfunded (for testing receive scenarios)");
        
        // CRITICAL: Wait for Fulcrum to sync with Bitcoin Core before proceeding
        Self::wait_for_fulcrum_sync_after_mining(&bitcoin_container_name, fulcrum_port).await?;
        
        Ok(())
    }
    
    /// Simple Electrum client to verify Fulcrum is ready to serve requests
    #[allow(dead_code)] // Used in other test files
    async fn electrum_call(host: &str, port: u16, method: &str, params: &[Value]) -> Result<Value, Box<dyn std::error::Error>> {
        let mut stream = TcpStream::connect(format!("{}:{}", host, port)).await?;
        
        // Create Electrum JSON-RPC request (note: Electrum uses jsonrpc 1.0, not 2.0)
        let request = serde_json::json!({
            "id": 1,
            "method": method,
            "params": params
        });
        
        let request_str = format!("{}\n", serde_json::to_string(&request)?);
        stream.write_all(request_str.as_bytes()).await?;
        
        // Read response with a timeout
        let mut buffer = vec![0; 8192];
        let n = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buffer)).await
            .map_err(|_| "Timeout reading from Fulcrum")??;
        
        if n == 0 {
            return Err("Connection closed by Fulcrum".into());
        }
        
        let response_str = String::from_utf8_lossy(&buffer[..n]);
        println!("   🔍 Fulcrum response: {}", response_str.trim());
        
        // Parse JSON response
        let response: Value = serde_json::from_str(response_str.trim())?;
        
        if let Some(error) = response.get("error") {
            return Err(format!("Electrum error: {}", error).into());
        }
        
        if let Some(result) = response.get("result") {
            Ok(result.clone())
        } else {
            Err("No result in Electrum response".into())
        }
    }
    
    /// Check if Fulcrum can serve basic blockchain info
    #[allow(dead_code)] // Used in other test files
    async fn verify_fulcrum_blockchain_ready(fulcrum_port: u16) -> Result<(), Box<dyn std::error::Error>> {
        // Try to get server version first (simpler call)
        match Self::electrum_call("127.0.0.1", fulcrum_port, "server.version", &[
            Value::String("canary-test".to_string()),
            Value::String("1.4".to_string())
        ]).await {
            Ok(result) => {
                println!("   📡 Fulcrum server version: {:?}", result);
                Ok(())
            }
            Err(e) => {
                // If server.version fails, try a simpler ping-like call
                println!("   ⚠️  server.version failed: {}, trying server.ping...", e);
                let _result = Self::electrum_call("127.0.0.1", fulcrum_port, "server.ping", &[]).await?;
                println!("   📡 Fulcrum ping successful");
                Ok(())
            }
        }
    }
    
    /// Get current block height from Fulcrum
    #[allow(dead_code)] // Used in other test files
    async fn get_fulcrum_block_height(fulcrum_port: u16) -> Result<u64, Box<dyn std::error::Error>> {
        let result = Self::electrum_call("127.0.0.1", fulcrum_port, "blockchain.headers.subscribe", &[]).await?;
        
        if let Some(header) = result.get("height") {
            if let Some(height) = header.as_u64() {
                return Ok(height);
            }
        }
        
        Err("Could not get block height from Fulcrum".into())
    }
    
    /// Execute bitcoin-cli command in the container
    pub fn bitcoin_cli(container_name: &str, args: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
        let mut cmd_args = vec![
            "exec", container_name, "bitcoin-cli", "-regtest", "-rpcport=8332", "-rpcuser=test", "-rpcpassword=test"
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
        self.send_transaction_with_options(from_wallet, to_wallet, amount, false, None).await
    }
    
    /// Send transaction with advanced options (RBF support, custom fee rate)
    pub async fn send_transaction_with_options(
        &self, 
        from_wallet: &str, 
        to_wallet: &str, 
        amount: &str, 
        replaceable: bool,
        fee_rate: Option<f64>
    ) -> Result<String, Box<dyn std::error::Error>> {
        println!("💸 Sending {} BTC from {} to {} (RBF: {}, fee_rate: {:?})", 
                amount, from_wallet, to_wallet, replaceable, fee_rate);
        
        let bitcoin_container_name = format!("test-bitcoin-{}", self.test_id);
        
        let to_address = Self::bitcoin_cli(&bitcoin_container_name, &[
            &format!("-rpcwallet={}", to_wallet), "getnewaddress"
        ])?.trim().trim_matches('"').to_string();
        
        let mut send_args = vec![
            format!("-rpcwallet={}", from_wallet),
            "sendtoaddress".to_string(),
            to_address,
        ];
        
        if amount == "max" {
            // Drain the wallet
            let balance = Self::bitcoin_cli(&bitcoin_container_name, &[
                &format!("-rpcwallet={}", from_wallet), "getbalance"
            ])?.trim().to_string();
            send_args.push(balance);
            send_args.push("".to_string()); // comment
            send_args.push("".to_string()); // comment_to
            send_args.push("true".to_string()); // subtractfeefromamount
        } else {
            send_args.push(amount.to_string());
            send_args.push("".to_string()); // comment
            send_args.push("".to_string()); // comment_to
            send_args.push("false".to_string()); // subtractfeefromamount
        }
        
        send_args.push(replaceable.to_string()); // replaceable
        
        if let Some(rate) = fee_rate {
            send_args.push("null".to_string()); // conf_target (null for explicit fee_rate)
            send_args.push("economical".to_string()); // estimate_mode
            send_args.push(rate.to_string()); // fee_rate
        }
        
        let send_args_str: Vec<&str> = send_args.iter().map(|s| s.as_str()).collect();
        let txid = Self::bitcoin_cli(&bitcoin_container_name, &send_args_str)?
            .trim().trim_matches('"').to_string();
        
        println!("✅ Transaction sent: {}", txid);
        Ok(txid)
    }
    
    /// Mine blocks to confirm transactions
    pub async fn mine_blocks(&self, count: u32) -> Result<(), Box<dyn std::error::Error>> {
        let bitcoin_container_name = format!("test-bitcoin-{}", self.test_id);
        
        let miner_address = Self::bitcoin_cli(&bitcoin_container_name, &["-rpcwallet=miner", "getnewaddress"])?
            .trim().trim_matches('"').to_string();
            
        Self::bitcoin_cli(&bitcoin_container_name, &[
            "generatetoaddress", &count.to_string(), &miner_address
        ])?;
        
        println!("⛏️ Mined {} blocks", count);
        
        // Wait for Fulcrum to sync the new blocks before proceeding
        println!("   ⏳ Waiting for Fulcrum to sync after mining...");
        Self::wait_for_fulcrum_sync_after_mining(&bitcoin_container_name, self.fulcrum_port).await?;
        
        Ok(())
    }
    
    /// Trigger wallet sync and wait for completion
    pub async fn sync_and_wait(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Wait for mempool propagation before syncing
        sleep(Duration::from_millis(SYNC_WAIT_MS)).await;
        let _ = self.wallet_manager.sync_tier_parallel(SubscriptionTier::Team).await;
        Ok(())
    }
    
    
    /// Get all transactions for a wallet from the database
    pub async fn get_wallet_transactions(&self, wallet_checksum: &str) -> Result<Vec<TransactionWithWallet>, Box<dyn std::error::Error>> {
        let transactions = self.metadata_db.get_transactions_by_wallet_checksum(wallet_checksum, None).await?;
        Ok(transactions)
    }
}

impl Drop for IsolatedTestEnvironment {
    /// Cleanup: Stop Docker Compose environment
    fn drop(&mut self) {
        println!("🧹 Cleaning up test environment: {}", self.test_id);
        
        // Cleanup automatically to prevent port conflicts
        let result = Command::new("docker-compose")
            .current_dir(&self.compose_dir)
            .args(&["down", "-v"])
            .output();
            
        match result {
            Ok(output) => {
                if output.status.success() {
                    println!("✅ Test containers cleaned up successfully");
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    println!("⚠️  Cleanup warning: {}", stderr);
                }
            }
            Err(e) => {
                println!("❌ Failed to cleanup containers: {}", e);
                println!("   Manual cleanup: docker-compose -f {}/docker-compose.yml down -v", self.compose_dir.display());
            }
        }
    }
}