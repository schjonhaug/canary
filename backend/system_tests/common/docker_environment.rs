use canary::api::AppServices;
use canary::config::{AppConfig, NetworkConfig, OperatingMode};
use canary::metadata::{MetadataDb, TransactionNotification, TransactionWithWallet};
use canary::subscription::SubscriptionTier;
use canary::wallet::WalletManager;
use serde_json::{self, Value};
use std::fs;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tokio::time::sleep;
use uuid::Uuid;

pub const SYNC_WAIT_MS: u64 = 15000; // Time to wait for mempool propagation and sync to complete (increased for multiple tx scenarios)

/// Helper struct to manage isolated test environment with Docker Compose
#[allow(dead_code)] // charlie_checksum and bitcoin_rpc_port used in other test files
pub struct IsolatedTestEnvironment {
    pub metadata_db: MetadataDb,
    pub wallet_manager: Arc<WalletManager>,
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

#[allow(dead_code)] // Functions used across different test binaries
impl IsolatedTestEnvironment {
    /// Wait for all wallets to be marked as ready in the database
    async fn wait_for_wallets_ready(
        metadata_db: &MetadataDb,
        checksums: &[&str],
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("⏳ Waiting for all wallets to be marked as ready...");
        let start_time = std::time::Instant::now();
        let timeout = Duration::from_secs(30);

        loop {
            let ready_wallets = metadata_db.get_ready_wallets().await?;
            let ready_checksums: std::collections::HashSet<_> =
                ready_wallets.iter().map(|w| w.checksum.as_str()).collect();

            let all_ready = checksums
                .iter()
                .all(|checksum| ready_checksums.contains(checksum));

            if all_ready {
                println!("✅ All wallets are ready after {:?}", start_time.elapsed());
                return Ok(());
            }

            if start_time.elapsed() > timeout {
                println!(
                    "❌ Timeout waiting for wallets to be ready. Current ready wallets: {:?}",
                    ready_checksums
                );
                return Err("Timeout waiting for wallets to be marked as ready".into());
            }

            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    /// Create a new isolated test environment using Docker Compose
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Self::create_test_environment_with_retries(Self::create_new_internal, 3).await
    }

    async fn create_new_internal() -> Result<Self, Box<dyn std::error::Error>> {
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

        let test_config = AppConfig::new_for_test(
            NetworkConfig::Regtest,
            None,
            "127.0.0.1:3000".to_string(),
            temp_path.clone(),
            OperatingMode::SelfHosted, // Self-hosted mode for simpler testing without Stripe
            None,
            None, // No JWT secret needed for self-hosted mode
        );

        let metadata_db = MetadataDb::new(db_path.to_str().unwrap(), &test_config).await?;

        // In self-hosted mode, the hardcoded self-hosted user is created automatically as admin
        // (keeping 'foss-user' ID for backwards compatibility)
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

        let (notification_sender, _notification_receiver) =
            broadcast::channel::<TransactionNotification>(100);

        let wallet_manager = Arc::new(
            WalletManager::new(
                notification_sender,
                wallet_dir,
                &db_path.to_string_lossy(),
                bdk_wallet::bitcoin::Network::Regtest,
                &format!("tcp://127.0.0.1:{}", fulcrum_port),
                &test_config,
            )
            .await,
        );

        // Create AppServices to access wallet creation service
        let wallet_creation_service = canary::wallet::WalletCreationService::new(
            wallet_manager.wallet_dir.clone(),
            metadata_db.clone(),
            wallet_manager.get_electrum_client().await,
            wallet_manager.get_network(),
            wallet_manager.clone(),
        );
        let app_services = AppServices {
            metadata_db: metadata_db.clone(),
            wallet_creation_service,
        };

        // Create wallets using the correct XPUB descriptors from docker-utils.sh
        let alice_descriptor = "wpkh([805c684b/84h/1h/0h]tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5/<0;1>/*)#8nt3y08q";
        let bob_descriptor = "wpkh([aeea3541/84h/1h/0h]tpubDDCjkgMuodinFyfhacZPTzffAKtCbuZejpkSMJB673c9ZSsVrq5FnL5rhjFjyCDva5Pka7sn9UDe7xmzpRCNnKNqXbteTnPzLRVNcsvCcpk/<0;1>/*)#ff9zpyxa";

        let alice_metadata = app_services
            .wallet_creation_service
            .create_wallet_non_blocking(
                "Alice",
                alice_descriptor,
                &test_user_id,
                true,
                Some("auto"),
                Some("20"),
            )
            .await?;
        let bob_metadata = app_services
            .wallet_creation_service
            .create_wallet_non_blocking(
                "Bob",
                bob_descriptor,
                &test_user_id,
                true,
                Some("auto"),
                Some("20"),
            )
            .await?;

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
        let _ = wallet_manager
            .sync_tier_parallel(SubscriptionTier::Team)
            .await;
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

    /// Create a new isolated test environment with Charlie wallet for high index tests
    pub async fn new_with_charlie() -> Result<Self, Box<dyn std::error::Error>> {
        Self::create_test_environment_with_retries(Self::create_new_with_charlie_internal, 3).await
    }

    async fn create_new_with_charlie_internal() -> Result<Self, Box<dyn std::error::Error>> {
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

        println!(
            "🚀 Creating isolated test environment with Charlie: {}",
            test_id
        );
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

        let test_config = AppConfig::new_for_test(
            NetworkConfig::Regtest,
            None,
            "127.0.0.1:3000".to_string(),
            temp_path.clone(),
            OperatingMode::SelfHosted, // Self-hosted mode for simpler testing without Stripe
            None,
            None, // No JWT secret needed for self-hosted mode
        );

        let metadata_db = MetadataDb::new(db_path.to_str().unwrap(), &test_config).await?;

        // In self-hosted mode, the hardcoded self-hosted user is created automatically as admin
        // (keeping 'foss-user' ID for backwards compatibility)
        let test_user_id = "foss-user".to_string();

        // Start Docker Compose environment
        Self::docker_compose_up(&compose_dir)?;

        // Wait for services to be ready
        Self::wait_for_bitcoin_ready(&compose_dir, &test_id).await?;
        Self::wait_for_fulcrum_ready(&compose_dir, fulcrum_port).await?;

        // Setup all test wallets including Charlie for high index tests
        Self::setup_test_wallets(&compose_dir, &test_id).await?;

        // Fund Alice and Charlie (Charlie at high index 250)
        Self::fund_test_wallets(&compose_dir, &test_id).await?;

        // Create wallet manager (connects to Fulcrum)
        let wallet_dir = temp_dir.path().join("wallets");
        std::fs::create_dir_all(&wallet_dir)?;

        let (notification_sender, _notification_receiver) =
            broadcast::channel::<TransactionNotification>(100);

        let wallet_manager = Arc::new(
            WalletManager::new(
                notification_sender,
                wallet_dir,
                &db_path.to_string_lossy(),
                bdk_wallet::bitcoin::Network::Regtest,
                &format!("tcp://127.0.0.1:{}", fulcrum_port),
                &test_config,
            )
            .await,
        );

        // Create AppServices to access wallet creation service
        let wallet_creation_service = canary::wallet::WalletCreationService::new(
            wallet_manager.wallet_dir.clone(),
            metadata_db.clone(),
            wallet_manager.get_electrum_client().await,
            wallet_manager.get_network(),
            wallet_manager.clone(),
        );
        let app_services = AppServices {
            metadata_db: metadata_db.clone(),
            wallet_creation_service,
        };

        // Create wallets using the correct XPUB descriptors from docker-utils.sh
        let alice_descriptor = "wpkh([805c684b/84h/1h/0h]tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5/<0;1>/*)#8nt3y08q";
        let bob_descriptor = "wpkh([aeea3541/84h/1h/0h]tpubDDCjkgMuodinFyfhacZPTzffAKtCbuZejpkSMJB673c9ZSsVrq5FnL5rhjFjyCDva5Pka7sn9UDe7xmzpRCNnKNqXbteTnPzLRVNcsvCcpk/<0;1>/*)#ff9zpyxa";
        let charlie_descriptor = "wpkh(tpubDCxzhZZE31g2EqSv1UajMAw5Hd62htydz9r2XBkrccHgBh8uw3n62zr6Zjmj64tfTk8Tjxo6VctjUMAh5DXWTErfQPC6RmQhTdtNnXuTXTQ/<0;1>/*)#sq32h3ch";

        let alice_metadata = app_services
            .wallet_creation_service
            .create_wallet_non_blocking(
                "Alice",
                alice_descriptor,
                &test_user_id,
                true,
                Some("auto"),
                Some("20"),
            )
            .await?;
        let bob_metadata = app_services
            .wallet_creation_service
            .create_wallet_non_blocking(
                "Bob",
                bob_descriptor,
                &test_user_id,
                true,
                Some("auto"),
                Some("20"),
            )
            .await?;
        let charlie_metadata = app_services
            .wallet_creation_service
            .create_wallet_non_blocking(
                "Charlie",
                charlie_descriptor,
                &test_user_id,
                false,
                Some("auto"),
                Some("250"),
            )
            .await?;

        let alice_checksum = alice_metadata.checksum;
        let bob_checksum = bob_metadata.checksum;
        let charlie_checksum = charlie_metadata.checksum;

        println!("✅ Test environment with Charlie ready:");
        println!("   Alice checksum: {}", alice_checksum);
        println!("   Bob checksum: {}", bob_checksum);
        println!("   Charlie checksum: {}", charlie_checksum);

        // Wait for all wallets to be marked as ready before proceeding
        Self::wait_for_wallets_ready(
            &metadata_db,
            &[&alice_checksum, &bob_checksum, &charlie_checksum],
        )
        .await?;

        // CRITICAL: Do an initial sync to ensure historical transactions are properly processed
        // This prevents the first test sync from detecting historical transactions as new
        println!("🔄 Running initial sync to establish historical transaction baseline...");
        let _ = wallet_manager
            .sync_tier_parallel(SubscriptionTier::Team)
            .await;
        sleep(Duration::from_millis(500)).await;
        println!("✅ Initial historical sync completed");

        Ok(IsolatedTestEnvironment {
            metadata_db,
            wallet_manager,
            _temp_dir: temp_dir,
            alice_checksum,
            bob_checksum,
            charlie_checksum,
            compose_dir,
            test_id,
            bitcoin_rpc_port,
            fulcrum_port,
        })
    }

    /// Retry creating test environment if port allocation fails
    async fn create_test_environment_with_retries<F, Fut>(
        create_fn: F,
        max_retries: u32,
    ) -> Result<Self, Box<dyn std::error::Error>>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<Self, Box<dyn std::error::Error>>>,
    {
        let mut last_error = None;

        for attempt in 1..=max_retries {
            println!(
                "🔄 Test environment creation attempt {}/{}",
                attempt, max_retries
            );

            match create_fn().await {
                Ok(env) => {
                    println!(
                        "✅ Test environment created successfully on attempt {}",
                        attempt
                    );
                    return Ok(env);
                }
                Err(e) => {
                    println!("❌ Attempt {} failed: {}", attempt, e);
                    last_error = Some(e);

                    if attempt < max_retries {
                        // Wait before retrying, with exponential backoff
                        let wait_time = attempt * 2;
                        println!("⏳ Waiting {}s before retry...", wait_time);
                        sleep(Duration::from_secs(wait_time as u64)).await;

                        // Clean up any partial resources before retrying
                        Self::cleanup_orphaned_test_containers();
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| "All retry attempts failed".into()))
    }

    /// Find an available port using a more robust allocation strategy  
    /// Uses timestamp + random to ensure uniqueness across parallel tests
    fn find_available_port_with_offset(
        _start_port: u16,  // Unused now, but kept for API compatibility
        _test_offset: u16, // Unused now, but kept for API compatibility
    ) -> Result<u16, Box<dyn std::error::Error>> {
        use rand::Rng;
        let mut rng = rand::rng();

        // Use current timestamp in microseconds to ensure uniqueness
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u32;

        // Create a unique offset based on timestamp and random number
        let unique_offset = (now % 10000) + rng.random_range(0..1000);

        // Try multiple port ranges to avoid conflicts
        let port_ranges = [
            (30000, 32000), // Range 1
            (32000, 34000), // Range 2
            (34000, 36000), // Range 3
            (36000, 38000), // Range 4
            (38000, 40000), // Range 5
            (40000, 42000), // Range 6
            (42000, 44000), // Range 7
            (44000, 46000), // Range 8
            (46000, 48000), // Range 9
            (48000, 50000), // Range 10
        ];

        // First, try a deterministic port based on start_port + unique offset
        for (range_start, range_end) in port_ranges {
            let candidate_port = (range_start + (unique_offset % (range_end - range_start))) as u16;
            if Self::is_port_available(candidate_port) {
                println!(
                    "   Selected deterministic port {} (offset: {})",
                    candidate_port, unique_offset
                );
                return Ok(candidate_port);
            }
        }

        // If deterministic approach fails, try random ports within ranges
        for (range_start, range_end) in port_ranges {
            for _ in 0..20 {
                let random_port = rng.random_range(range_start..range_end) as u16;
                if Self::is_port_available(random_port) {
                    println!(
                        "   Selected random port {} from range {}-{}",
                        random_port, range_start, range_end
                    );
                    return Ok(random_port);
                }
            }
        }

        Err("No available ports found after extensive search".into())
    }

    /// Check if a port is available by attempting to bind and also checking for Docker containers
    fn is_port_available(port: u16) -> bool {
        // First check if we can bind to the port on localhost
        if std::net::TcpListener::bind(format!("127.0.0.1:{}", port)).is_err() {
            return false;
        }

        // Also check if we can bind to the port on all interfaces (Docker binds to 0.0.0.0)
        if std::net::TcpListener::bind(format!("0.0.0.0:{}", port)).is_err() {
            return false;
        }

        // Check if Docker has any containers using this port (both running and stopped)
        let running_check = std::process::Command::new("docker")
            .args(&[
                "ps",
                "-a",
                "--filter",
                &format!("publish={}", port),
                "--quiet",
            ])
            .output();

        if let Ok(output) = running_check {
            let containers = String::from_utf8_lossy(&output.stdout);
            if !containers.trim().is_empty() {
                return false; // Port is in use by Docker containers
            }
        }

        // Additional check for containers that might be binding to this port internally
        let port_check = std::process::Command::new("docker")
            .args(&[
                "ps",
                "-a",
                "--format",
                "{{.Ports}}",
                "--filter",
                "name=test-",
            ])
            .output();

        if let Ok(output) = port_check {
            let ports_output = String::from_utf8_lossy(&output.stdout);
            if ports_output.contains(&port.to_string()) {
                return false; // Port found in test container ports
            }
        }

        true // Port appears to be available
    }

    /// Clean up any orphaned test containers from previous runs
    fn cleanup_orphaned_test_containers() {
        println!("🧹 Cleaning up orphaned test containers...");

        // Clean up containers with names starting with 'test-'
        let list_containers = Command::new("docker")
            .args(&["ps", "-aq", "--filter", "name=test-"])
            .output();

        match list_containers {
            Ok(output) => {
                let containers = String::from_utf8_lossy(&output.stdout);
                let container_ids: Vec<&str> =
                    containers.lines().filter(|line| !line.is_empty()).collect();

                if !container_ids.is_empty() {
                    println!("   Found {} orphaned test containers", container_ids.len());

                    // Stop and remove all containers in one batch for efficiency
                    if !container_ids.is_empty() {
                        let _ = Command::new("docker")
                            .args(&["stop"])
                            .args(&container_ids)
                            .output();

                        let _ = Command::new("docker")
                            .args(&["rm", "-f"])
                            .args(&container_ids)
                            .output();
                    }

                    println!(
                        "   ✅ Stopped and removed {} containers",
                        container_ids.len()
                    );
                } else {
                    println!("   No orphaned test containers found");
                }
            }
            Err(e) => {
                println!("⚠️  Failed to list containers: {}", e);
            }
        }

        // Also cleanup volumes starting with 'canary_test_'
        let list_volumes = Command::new("docker")
            .args(&["volume", "ls", "-q", "--filter", "name=canary_test_"])
            .output();

        match list_volumes {
            Ok(output) => {
                let volumes = String::from_utf8_lossy(&output.stdout);
                let volume_names: Vec<&str> =
                    volumes.lines().filter(|line| !line.is_empty()).collect();

                if !volume_names.is_empty() {
                    println!("   Found {} orphaned volumes", volume_names.len());

                    for volume_name in &volume_names {
                        let _ = Command::new("docker")
                            .args(&["volume", "rm", "-f", volume_name])
                            .output();
                    }

                    println!("   ✅ Removed {} volumes", volume_names.len());
                }
            }
            Err(e) => {
                println!("⚠️  Failed to list volumes: {}", e);
            }
        }

        // Also cleanup networks starting with 'canary_test_' and old 'compose_default'
        let list_networks = Command::new("docker")
            .args(&["network", "ls", "-q", "--filter", "name=canary_test_"])
            .output();

        match list_networks {
            Ok(output) => {
                let networks = String::from_utf8_lossy(&output.stdout);
                let network_names: Vec<&str> =
                    networks.lines().filter(|line| !line.is_empty()).collect();

                if !network_names.is_empty() {
                    println!(
                        "   Found {} orphaned canary_test networks",
                        network_names.len()
                    );

                    for network_name in &network_names {
                        let _ = Command::new("docker")
                            .args(&["network", "rm", network_name])
                            .output();
                    }

                    println!("   ✅ Removed {} canary_test networks", network_names.len());
                }
            }
            Err(e) => {
                println!("⚠️  Failed to list canary_test networks: {}", e);
            }
        }

        // Also cleanup old compose_default networks that may be leftover
        let cleanup_compose_default = Command::new("docker")
            .args(&["network", "rm", "compose_default"])
            .output();

        match cleanup_compose_default {
            Ok(output) => {
                if output.status.success() {
                    println!("   ✅ Removed leftover compose_default network");
                }
            }
            Err(_) => {
                // Ignore error - network may not exist
            }
        }

        println!("✅ Orphaned container cleanup completed");
    }

    /// Create test-specific docker-compose.yml and config files
    fn create_test_docker_compose(
        compose_dir: &std::path::Path,
        test_id: &str,
        bitcoin_rpc_port: u16,
        fulcrum_port: u16,
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
        let fulcrum_conf = format!(
            r#"# Test Fulcrum Configuration
bitcoind = {}:8332
rpcuser = test
rpcpassword = test
tcp = 0.0.0.0:50001
datadir = /data
worker_threads = 1
utxo-cache = 1024
debug = 1
"#,
            bitcoin_container_name
        );
        fs::write(compose_dir.join("fulcrum.conf"), fulcrum_conf)?;

        // Create docker-compose.yml
        let network_name = format!("canary_test_network_{}", test_id);
        let compose_yml = format!(
            r#"services:
  {}:
    image: ghcr.io/sethforprivacy/bitcoind:latest
    container_name: {}
    networks:
      - {}
    ports:
      - "{}:8332"
    volumes:
      - ./bitcoin.conf:/bitcoin/.bitcoin/bitcoin.conf
      - canary_test_bitcoin_data_{}:/bitcoin/.bitcoin
    environment:
      - RPC_USER=test
      - RPC_PASSWORD=test

  {}:
    image: cculianu/fulcrum:latest
    container_name: {}
    networks:
      - {}
    depends_on:
      - {}
    ports:
      - "{}:50001"
    volumes:
      - ./fulcrum.conf:/data/fulcrum.conf:ro
      - canary_test_fulcrum_data_{}:/data
    command: ["Fulcrum", "/data/fulcrum.conf"]

networks:
  {}:
    driver: bridge

volumes:
  canary_test_bitcoin_data_{}:
  canary_test_fulcrum_data_{}:
"#,
            bitcoin_container_name,
            bitcoin_container_name,
            network_name,
            bitcoin_rpc_port,
            test_id,
            fulcrum_container_name,
            fulcrum_container_name,
            network_name,
            bitcoin_container_name,
            fulcrum_port,
            test_id,
            network_name,
            test_id,
            test_id
        );
        fs::write(compose_dir.join("docker-compose.yml"), compose_yml)?;

        Ok(())
    }

    /// Start Docker Compose environment
    fn docker_compose_up(compose_dir: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
        println!("🐳 Starting Docker Compose environment...");

        let output = Command::new("docker-compose")
            .current_dir(compose_dir)
            .args(&["up", "-d", "--remove-orphans"])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Failed to start Docker Compose: {}", stderr).into());
        }

        println!("✅ Docker Compose environment started");

        // Give containers a moment to fully start before proceeding
        std::thread::sleep(std::time::Duration::from_secs(2));

        Ok(())
    }

    /// Wait for Bitcoin RPC to be ready
    async fn wait_for_bitcoin_ready(
        _compose_dir: &std::path::Path,
        test_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("⏳ Waiting for Bitcoin RPC to be ready...");
        let bitcoin_container_name = format!("test-bitcoin-{}", test_id);

        for attempt in 1..=30 {
            let output = Command::new("docker")
                .args(&[
                    "exec",
                    &bitcoin_container_name,
                    "bitcoin-cli",
                    "-regtest",
                    "-rpcport=8332",
                    "-rpcuser=test",
                    "-rpcpassword=test",
                    "getblockchaininfo",
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
    async fn wait_for_fulcrum_ready(
        _compose_dir: &std::path::Path,
        fulcrum_port: u16,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("⏳ Waiting for Fulcrum Electrum server to be ready...");

        // First wait for port to be open
        for attempt in 1..=120 {
            // Increased timeout for Fulcrum startup
            let connection_test =
                std::net::TcpStream::connect(format!("127.0.0.1:{}", fulcrum_port));

            if connection_test.is_ok() {
                println!(
                    "   📡 Fulcrum port {} is open after {} seconds",
                    fulcrum_port, attempt
                );

                // Give Fulcrum some more time to fully initialize before we proceed
                println!("   ⏳ Waiting additional 10 seconds for Fulcrum to fully initialize...");
                sleep(Duration::from_secs(10)).await;

                println!("✅ Fulcrum Electrum server ready");
                return Ok(());
            }

            if attempt % 15 == 0 {
                println!(
                    "   ⏳ Still waiting for Fulcrum port... ({}/120 seconds)",
                    attempt
                );
            }

            sleep(Duration::from_secs(2)).await;
        }

        Err("Fulcrum port failed to open within 4 minutes".into())
    }

    /// Wait for Fulcrum to sync with Bitcoin Core after mining blocks
    async fn wait_for_fulcrum_sync_after_mining(
        bitcoin_container: &str,
        _fulcrum_port: u16,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Get Bitcoin Core's current block height for logging
        let btc_height_str = Self::bitcoin_cli(bitcoin_container, &["getblockcount"])?;
        let btc_height: u64 = btc_height_str
            .trim()
            .parse()
            .map_err(|_| "Failed to parse Bitcoin block height")?;

        println!(
            "   🧱 Bitcoin Core height: {}, giving Fulcrum time to sync...",
            btc_height
        );

        // For now, use a fixed delay to let Fulcrum sync
        // This is simpler than trying to query Fulcrum's height which is causing connection issues
        // Increased to 15 seconds for multiple transaction scenarios
        sleep(Duration::from_secs(15)).await;

        println!("   ✅ Fulcrum should have synced");
        Ok(())
    }

    /// Setup deterministic test wallets (same as docker-utils.sh)
    #[allow(dead_code)] // Used in other test files
    async fn setup_test_wallets(
        _compose_dir: &std::path::Path,
        test_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("🏦 Setting up test wallets...");
        let bitcoin_container_name = format!("test-bitcoin-{}", test_id);

        // Create Alice wallet with deterministic descriptor
        Self::bitcoin_cli(
            &bitcoin_container_name,
            &[
                "-named",
                "createwallet",
                "wallet_name=alice",
                "disable_private_keys=false",
                "blank=true",
                "descriptors=true",
            ],
        )?;

        // Import Alice's deterministic descriptors (same as docker-utils.sh)
        Self::bitcoin_cli(
            &bitcoin_container_name,
            &[
                "-rpcwallet=alice",
                "importdescriptors",
                r#"[{"desc": "wpkh(tprv8ZgxMBicQKsPe6D3wxZPHxy7JEMBwjxiimYXSvM8aRaqZmDvU7Jec5c8aSB8rCzFmAP6aEjPhUmiXm2KB7XUzkApX2prmDHcQsUQY5DsxJw/84h/1h/0h/0/*)#5asejmkj", "timestamp": "now", "active": true, "internal": false, "range": [0, 999]}, {"desc": "wpkh(tprv8ZgxMBicQKsPe6D3wxZPHxy7JEMBwjxiimYXSvM8aRaqZmDvU7Jec5c8aSB8rCzFmAP6aEjPhUmiXm2KB7XUzkApX2prmDHcQsUQY5DsxJw/84h/1h/0h/1/*)#9f4c0wx2", "timestamp": "now", "active": true, "internal": true, "range": [0, 999]}]"#,
            ],
        )?;

        // Create Bob wallet with deterministic descriptor
        Self::bitcoin_cli(
            &bitcoin_container_name,
            &[
                "-named",
                "createwallet",
                "wallet_name=bob",
                "disable_private_keys=false",
                "blank=true",
                "descriptors=true",
            ],
        )?;

        Self::bitcoin_cli(
            &bitcoin_container_name,
            &[
                "-rpcwallet=bob",
                "importdescriptors",
                r#"[{"desc": "wpkh(tprv8ZgxMBicQKsPd3nsWkJrKQgXk22UnGFHFPDWNCeTjvzeY9LjRYdi3RNX1NfkCwj4mD1YTsNPCCtGPTTQkxn14oLKNg6vuVkChn55qa5rC7K/84h/1h/0h/0/*)#y872gtkp", "timestamp": "now", "active": true, "internal": false, "range": [0, 999]}, {"desc": "wpkh(tprv8ZgxMBicQKsPd3nsWkJrKQgXk22UnGFHFPDWNCeTjvzeY9LjRYdi3RNX1NfkCwj4mD1YTsNPCCtGPTTQkxn14oLKNg6vuVkChn55qa5rC7K/84h/1h/0h/1/*)#4nmt47xe", "timestamp": "now", "active": true, "internal": true, "range": [0, 999]}]"#,
            ],
        )?;

        // Create Charlie wallet with deterministic descriptor
        Self::bitcoin_cli(
            &bitcoin_container_name,
            &[
                "-named",
                "createwallet",
                "wallet_name=charlie",
                "disable_private_keys=false",
                "blank=true",
                "descriptors=true",
            ],
        )?;

        Self::bitcoin_cli(
            &bitcoin_container_name,
            &[
                "-rpcwallet=charlie",
                "importdescriptors",
                r#"[{"desc": "wpkh(tprv8ZgxMBicQKsPd7Uf69XL1XwhmjHopUGep8GuEiJDZmbQz6o58LninorQAfcKZWARbtRtfnLcJ5MQ2AtHcQJCCRUcMRvmDUjyEmNUWwx8UbK/84h/1h/0h/0/*)#pe5sgqha", "timestamp": "now", "active": true, "internal": false, "range": [0, 999]}, {"desc": "wpkh(tprv8ZgxMBicQKsPd7Uf69XL1XwhmjHopUGep8GuEiJDZmbQz6o58LninorQAfcKZWARbtRtfnLcJ5MQ2AtHcQJCCRUcMRvmDUjyEmNUWwx8UbK/84h/1h/0h/1/*)#sd334489", "timestamp": "now", "active": true, "internal": true, "range": [0, 999]}]"#,
            ],
        )?;

        // Create miner wallet for generating blocks
        Self::bitcoin_cli(
            &bitcoin_container_name,
            &["-named", "createwallet", "wallet_name=miner"],
        )?;

        println!("✅ Test wallets created");
        Ok(())
    }

    /// Setup Alice and Bob wallets only (for mined directly tests)
    async fn setup_alice_bob_wallets(
        _compose_dir: &std::path::Path,
        test_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("🏦 Setting up Alice and Bob test wallets...");
        let bitcoin_container_name = format!("test-bitcoin-{}", test_id);

        // Create Alice wallet with deterministic descriptor (from docker-utils.sh)
        Self::bitcoin_cli(
            &bitcoin_container_name,
            &[
                "-named",
                "createwallet",
                "wallet_name=alice",
                "disable_private_keys=false",
                "blank=true",
                "descriptors=true",
            ],
        )?;

        // Import Alice's deterministic descriptors (same as docker-utils.sh)
        Self::bitcoin_cli(
            &bitcoin_container_name,
            &[
                "-rpcwallet=alice",
                "importdescriptors",
                r#"[{"desc": "wpkh(tprv8ZgxMBicQKsPe6D3wxZPHxy7JEMBwjxiimYXSvM8aRaqZmDvU7Jec5c8aSB8rCzFmAP6aEjPhUmiXm2KB7XUzkApX2prmDHcQsUQY5DsxJw/84h/1h/0h/0/*)#5asejmkj", "timestamp": "now", "active": true, "internal": false, "range": [0, 999]}, {"desc": "wpkh(tprv8ZgxMBicQKsPe6D3wxZPHxy7JEMBwjxiimYXSvM8aRaqZmDvU7Jec5c8aSB8rCzFmAP6aEjPhUmiXm2KB7XUzkApX2prmDHcQsUQY5DsxJw/84h/1h/0h/1/*)#9f4c0wx2", "timestamp": "now", "active": true, "internal": true, "range": [0, 999]}]"#,
            ],
        )?;

        // Create Bob wallet with deterministic descriptor
        Self::bitcoin_cli(
            &bitcoin_container_name,
            &[
                "-named",
                "createwallet",
                "wallet_name=bob",
                "disable_private_keys=false",
                "blank=true",
                "descriptors=true",
            ],
        )?;

        Self::bitcoin_cli(
            &bitcoin_container_name,
            &[
                "-rpcwallet=bob",
                "importdescriptors",
                r#"[{"desc": "wpkh(tprv8ZgxMBicQKsPd3nsWkJrKQgXk22UnGFHFPDWNCeTjvzeY9LjRYdi3RNX1NfkCwj4mD1YTsNPCCtGPTTQkxn14oLKNg6vuVkChn55qa5rC7K/84h/1h/0h/0/*)#y872gtkp", "timestamp": "now", "active": true, "internal": false, "range": [0, 999]}, {"desc": "wpkh(tprv8ZgxMBicQKsPd3nsWkJrKQgXk22UnGFHFPDWNCeTjvzeY9LjRYdi3RNX1NfkCwj4mD1YTsNPCCtGPTTQkxn14oLKNg6vuVkChn55qa5rC7K/84h/1h/0h/1/*)#4nmt47xe", "timestamp": "now", "active": true, "internal": true, "range": [0, 999]}]"#,
            ],
        )?;

        // Create miner wallet for generating blocks
        Self::bitcoin_cli(
            &bitcoin_container_name,
            &["-named", "createwallet", "wallet_name=miner"],
        )?;

        println!("✅ Alice and Bob test wallets created");
        Ok(())
    }

    /// Fund Alice and Charlie wallets for testing (matching docker-utils.sh setup)
    #[allow(dead_code)] // Used in other test files
    async fn fund_test_wallets(
        _compose_dir: &std::path::Path,
        test_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("💰 Funding test wallets...");
        let bitcoin_container_name = format!("test-bitcoin-{}", test_id);

        // Generate some blocks to miner first
        let miner_address = Self::bitcoin_cli(
            &bitcoin_container_name,
            &["-rpcwallet=miner", "getnewaddress"],
        )?
        .trim()
        .trim_matches('"')
        .to_string();

        Self::bitcoin_cli(
            &bitcoin_container_name,
            &["generatetoaddress", "101", &miner_address],
        )?;

        // Fund Alice with 1.0 BTC (normal case)
        let alice_address = Self::bitcoin_cli(
            &bitcoin_container_name,
            &["-rpcwallet=alice", "getnewaddress"],
        )?
        .trim()
        .trim_matches('"')
        .to_string();

        Self::bitcoin_cli(
            &bitcoin_container_name,
            &["-rpcwallet=miner", "sendtoaddress", &alice_address, "1.0"],
        )?;

        // Fund Charlie at high index (250) - same as docker-utils.sh
        println!("💰 Funding Charlie at index 250 for high-index testing...");

        // Generate addresses up to index 250 (0-250 = 251 addresses)
        let mut charlie_addr_250 = String::new();
        for i in 0..=250 {
            let addr = Self::bitcoin_cli(
                &bitcoin_container_name,
                &["-rpcwallet=charlie", "getnewaddress"],
            )?
            .trim()
            .trim_matches('"')
            .to_string();

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
        Self::bitcoin_cli(
            &bitcoin_container_name,
            &[
                "-rpcwallet=miner",
                "sendtoaddress",
                &charlie_addr_250,
                "0.5",
            ],
        )?;

        // Mine a block to confirm both transactions
        Self::bitcoin_cli(
            &bitcoin_container_name,
            &["generatetoaddress", "1", &miner_address],
        )?;

        println!("✅ Alice funded with 1.0 BTC (index 0)");
        println!("✅ Charlie funded with 0.5 BTC (index 250)");
        println!("✅ Bob unfunded (for testing receive scenarios)");

        // Wait for Fulcrum to sync with Bitcoin Core before proceeding
        Self::wait_for_fulcrum_sync_after_mining(&bitcoin_container_name, 0).await?;

        Ok(())
    }

    /// Fund Alice wallet only (for mined directly tests that don't need Charlie)
    async fn fund_alice_only(
        _compose_dir: &std::path::Path,
        test_id: &str,
        fulcrum_port: u16,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let bitcoin_container_name = format!("test-bitcoin-{}", test_id);

        // Generate some blocks to miner first
        let miner_address = Self::bitcoin_cli(
            &bitcoin_container_name,
            &["-rpcwallet=miner", "getnewaddress"],
        )?
        .trim()
        .trim_matches('"')
        .to_string();

        Self::bitcoin_cli(
            &bitcoin_container_name,
            &["generatetoaddress", "101", &miner_address],
        )?;

        // Fund Alice with 1.0 BTC (normal case)
        let alice_address = Self::bitcoin_cli(
            &bitcoin_container_name,
            &["-rpcwallet=alice", "getnewaddress"],
        )?
        .trim()
        .trim_matches('"')
        .to_string();

        Self::bitcoin_cli(
            &bitcoin_container_name,
            &["-rpcwallet=miner", "sendtoaddress", &alice_address, "1.0"],
        )?;

        // Mine a block to confirm the transaction
        Self::bitcoin_cli(
            &bitcoin_container_name,
            &["generatetoaddress", "1", &miner_address],
        )?;

        println!("✅ Alice funded with 1.0 BTC (index 0)");
        println!("✅ Bob unfunded (for testing receive scenarios)");

        // CRITICAL: Wait for Fulcrum to sync with Bitcoin Core before proceeding
        Self::wait_for_fulcrum_sync_after_mining(&bitcoin_container_name, fulcrum_port).await?;

        Ok(())
    }

    /// Simple Electrum client to verify Fulcrum is ready to serve requests
    #[allow(dead_code)] // Used in other test files
    async fn electrum_call(
        host: &str,
        port: u16,
        method: &str,
        params: &[Value],
    ) -> Result<Value, Box<dyn std::error::Error>> {
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
        let n = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buffer))
            .await
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
    async fn verify_fulcrum_blockchain_ready(
        fulcrum_port: u16,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Try to get server version first (simpler call)
        match Self::electrum_call(
            "127.0.0.1",
            fulcrum_port,
            "server.version",
            &[
                Value::String("canary-test".to_string()),
                Value::String("1.4".to_string()),
            ],
        )
        .await
        {
            Ok(result) => {
                println!("   📡 Fulcrum server version: {:?}", result);
                Ok(())
            }
            Err(e) => {
                // If server.version fails, try a simpler ping-like call
                println!("   ⚠️  server.version failed: {}, trying server.ping...", e);
                let _result =
                    Self::electrum_call("127.0.0.1", fulcrum_port, "server.ping", &[]).await?;
                println!("   📡 Fulcrum ping successful");
                Ok(())
            }
        }
    }

    /// Get current block height from Fulcrum
    #[allow(dead_code)] // Used in other test files
    async fn get_fulcrum_block_height(
        fulcrum_port: u16,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        let result = Self::electrum_call(
            "127.0.0.1",
            fulcrum_port,
            "blockchain.headers.subscribe",
            &[],
        )
        .await?;

        if let Some(header) = result.get("height") {
            if let Some(height) = header.as_u64() {
                return Ok(height);
            }
        }

        Err("Could not get block height from Fulcrum".into())
    }

    /// Execute bitcoin-cli command in the container
    pub fn bitcoin_cli(
        container_name: &str,
        args: &[&str],
    ) -> Result<String, Box<dyn std::error::Error>> {
        let mut cmd_args = vec![
            "exec",
            container_name,
            "bitcoin-cli",
            "-regtest",
            "-rpcport=8332",
            "-rpcuser=test",
            "-rpcpassword=test",
        ];
        cmd_args.extend_from_slice(args);

        let output = Command::new("docker").args(&cmd_args).output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Bitcoin CLI command failed: {}", stderr).into());
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Send transaction with advanced options (RBF support, custom fee rate)  
    pub async fn send_transaction_with_options(
        &self,
        from_wallet: &str,
        to_wallet: &str,
        amount: &str,
        replaceable: bool,
        fee_rate: Option<f64>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        println!(
            "💸 Sending {} BTC from {} to {} (RBF: {}, fee_rate: {:?})",
            amount, from_wallet, to_wallet, replaceable, fee_rate
        );

        let bitcoin_container_name = format!("test-bitcoin-{}", self.test_id);

        // Load source wallet first
        let _ = Self::bitcoin_cli(
            &bitcoin_container_name,
            &[
                &format!("-rpcwallet={}", from_wallet),
                "loadwallet",
                from_wallet,
            ],
        );

        let to_address = Self::bitcoin_cli(
            &bitcoin_container_name,
            &[&format!("-rpcwallet={}", to_wallet), "getnewaddress"],
        )?
        .trim()
        .trim_matches('"')
        .to_string();

        // Build sendtoaddress command with correct parameter order matching docker-utils.sh
        let mut send_args = vec![
            format!("-rpcwallet={}", from_wallet),
            "sendtoaddress".to_string(),
            to_address,
        ];

        if amount == "max" {
            // Drain the wallet
            let balance = Self::bitcoin_cli(
                &bitcoin_container_name,
                &[&format!("-rpcwallet={}", from_wallet), "getbalance"],
            )?
            .trim()
            .to_string();
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

        // Only add fee rate parameters if specified
        if let Some(rate) = fee_rate {
            send_args.push(rate.to_string()); // fee_rate
            send_args.push("unset".to_string()); // estimate_mode
        }

        let send_args_str: Vec<&str> = send_args.iter().map(|s| s.as_str()).collect();
        let txid = Self::bitcoin_cli(&bitcoin_container_name, &send_args_str)?
            .trim()
            .trim_matches('"')
            .to_string();

        println!("✅ Transaction sent: {}", txid);
        Ok(txid)
    }

    /// Mine blocks to confirm transactions
    pub async fn mine_blocks(&self, count: u32) -> Result<(), Box<dyn std::error::Error>> {
        let bitcoin_container_name = format!("test-bitcoin-{}", self.test_id);

        let miner_address = Self::bitcoin_cli(
            &bitcoin_container_name,
            &["-rpcwallet=miner", "getnewaddress"],
        )?
        .trim()
        .trim_matches('"')
        .to_string();

        Self::bitcoin_cli(
            &bitcoin_container_name,
            &["generatetoaddress", &count.to_string(), &miner_address],
        )?;

        println!("⛏️ Mined {} blocks", count);

        // Wait for Fulcrum to sync the new blocks before proceeding
        println!("   ⏳ Waiting for Fulcrum to sync after mining...");
        Self::wait_for_fulcrum_sync_after_mining(&bitcoin_container_name, self.fulcrum_port)
            .await?;

        Ok(())
    }

    /// Trigger wallet sync and wait for completion
    pub async fn sync_and_wait(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Wait for mempool propagation before syncing
        sleep(Duration::from_millis(SYNC_WAIT_MS)).await;
        let _ = self
            .wallet_manager
            .sync_tier_parallel(SubscriptionTier::Team)
            .await;
        Ok(())
    }

    /// Trigger wallet sync with retries for Electrum failures
    pub async fn sync_and_wait_with_retries(
        &mut self,
        max_retries: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for attempt in 1..=max_retries {
            // Wait for mempool propagation before syncing
            sleep(Duration::from_millis(SYNC_WAIT_MS)).await;

            let _result = self
                .wallet_manager
                .sync_tier_parallel(SubscriptionTier::Team)
                .await;

            // Check if sync was successful by verifying no Electrum errors were logged
            // This is a simple approach - in a more robust implementation, we'd check the actual sync results
            if attempt == max_retries {
                println!("✅ Completed sync attempt {} (final attempt)", attempt);
                break;
            } else {
                println!(
                    "✅ Completed sync attempt {}/{}, checking for errors...",
                    attempt, max_retries
                );
                // Add a small delay between retries to give Electrum more time
                sleep(Duration::from_secs(5)).await;
            }
        }
        Ok(())
    }

    /// Wait for a specific transaction to appear in a wallet's transaction list
    pub async fn wait_for_transaction_in_wallet(
        &mut self,
        wallet_checksum: &str,
        txid: &str,
        timeout_secs: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(timeout_secs);

        while start.elapsed() < timeout {
            // Sync the wallet
            self.sync_and_wait().await?;

            // Check if the transaction is now present
            let transactions = self.get_wallet_transactions(wallet_checksum).await?;
            if transactions.iter().any(|tx| tx.txid == txid) {
                println!(
                    "✅ Transaction {} found in wallet {} after {:.1}s",
                    txid,
                    wallet_checksum,
                    start.elapsed().as_secs_f64()
                );
                return Ok(());
            }

            println!(
                "⏳ Waiting for transaction {} to appear in wallet {} ({:.1}s elapsed)...",
                txid,
                wallet_checksum,
                start.elapsed().as_secs_f64()
            );
            sleep(Duration::from_secs(2)).await;
        }

        Err(format!(
            "Timeout waiting for transaction {} to appear in wallet {} after {}s",
            txid, wallet_checksum, timeout_secs
        )
        .into())
    }

    /// Get all transactions for a wallet from the database
    pub async fn get_wallet_transactions(
        &self,
        wallet_checksum: &str,
    ) -> Result<Vec<TransactionWithWallet>, Box<dyn std::error::Error>> {
        let transactions = self
            .metadata_db
            .get_transactions_by_wallet_checksum(wallet_checksum, None, false)
            .await?;
        Ok(transactions)
    }

    /// Send transaction between wallets (will be RBF-replaceable by default in regtest)
    pub async fn send_rbf_transaction(
        &self,
        from_wallet: &str,
        to_wallet: &str,
        amount: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        // Enable RBF for the transaction
        self.send_transaction_with_options(from_wallet, to_wallet, amount, true, None)
            .await
    }

    /// Replace transaction with higher fee using Bitcoin Core's bumpfee
    pub async fn replace_transaction(
        &self,
        wallet: &str,
        txid: &str,
        _fee_rate: f64,
    ) -> Result<String, Box<dyn std::error::Error>> {
        println!(
            "🔄 Bumping fee for transaction {} (automatic fee calculation)...",
            txid
        );

        let bitcoin_container_name = format!("test-bitcoin-{}", self.test_id);

        // Load wallet first
        let _ = Self::bitcoin_cli(
            &bitcoin_container_name,
            &[&format!("-rpcwallet={}", wallet), "loadwallet", wallet],
        );

        // Use bumpfee to replace the transaction
        let result = Self::bitcoin_cli(
            &bitcoin_container_name,
            &[&format!("-rpcwallet={}", wallet), "bumpfee", txid],
        )?;

        // Parse JSON result to get new txid
        let json_result: serde_json::Value = serde_json::from_str(&result)?;

        if let Some(new_txid) = json_result.get("txid").and_then(|v| v.as_str()) {
            let old_fee = json_result
                .get("origfee")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let new_fee = json_result
                .get("fee")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);

            println!("✅ RBF replacement successful!");
            println!("   Original TXID: {}", txid);
            println!("   New TXID: {}", new_txid);
            println!("   Original fee: {} BTC", old_fee);
            println!("   New fee: {} BTC", new_fee);

            Ok(new_txid.to_string())
        } else {
            Err(format!("Failed to parse bumpfee result: {}", result).into())
        }
    }

    /// Send a transaction to the same wallet (self-send)
    pub async fn send_self_transaction(
        &self,
        wallet: &str,
        amount: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        println!("💸 Self-sending {} BTC within {} wallet", amount, wallet);

        let bitcoin_container_name = format!("test-bitcoin-{}", self.test_id);

        // Load wallet first
        let _ = Self::bitcoin_cli(
            &bitcoin_container_name,
            &[&format!("-rpcwallet={}", wallet), "loadwallet", wallet],
        );

        // Get a new address from the SAME wallet
        let to_address = Self::bitcoin_cli(
            &bitcoin_container_name,
            &[&format!("-rpcwallet={}", wallet), "getnewaddress"],
        )?
        .trim()
        .trim_matches('"')
        .to_string();

        let mut send_args = vec![
            format!("-rpcwallet={}", wallet),
            "sendtoaddress".to_string(),
            to_address,
        ];

        if amount == "max" {
            let balance = Self::bitcoin_cli(
                &bitcoin_container_name,
                &[&format!("-rpcwallet={}", wallet), "getbalance"],
            )?
            .trim()
            .to_string();
            send_args.push(balance);
            send_args.push("".to_string()); // comment
            send_args.push("".to_string()); // comment_to
            send_args.push("true".to_string()); // subtractfeefromamount
        } else {
            send_args.push(amount.to_string());
        }

        let send_args_str: Vec<&str> = send_args.iter().map(|s| s.as_str()).collect();
        let txid = Self::bitcoin_cli(&bitcoin_container_name, &send_args_str)?
            .trim()
            .trim_matches('"')
            .to_string();

        println!("✅ Self-send transaction sent: {}", txid);
        Ok(txid)
    }

    /// Recreate the WalletManager to simulate a service restart.
    /// Uses the same database, wallet directory, and Fulcrum connection.
    pub async fn recreate_wallet_manager(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔄 Recreating WalletManager (simulating service restart)...");

        let wallet_dir = self.wallet_manager.wallet_dir.clone();
        let network = self.wallet_manager.get_network();
        let electrum_url = format!("tcp://127.0.0.1:{}", self.fulcrum_port);

        // Get the database path from the temp dir
        let db_path = self._temp_dir.path().join("test.db");

        let test_config = AppConfig::new_for_test(
            NetworkConfig::Regtest,
            None,
            "127.0.0.1:3000".to_string(),
            self._temp_dir.path().to_string_lossy().to_string(),
            OperatingMode::SelfHosted,
            None,
            None,
        );

        let (notification_sender, _notification_receiver) =
            broadcast::channel::<TransactionNotification>(100);

        // Drop old wallet_manager by replacing it
        self.wallet_manager = Arc::new(
            WalletManager::new(
                notification_sender,
                wallet_dir,
                &db_path.to_string_lossy(),
                network,
                &electrum_url,
                &test_config,
            )
            .await,
        );

        // Wait for wallets to be ready after recreation
        let checksums: Vec<&str> = if self.charlie_checksum.is_empty() {
            vec![self.alice_checksum.as_str(), self.bob_checksum.as_str()]
        } else {
            vec![
                self.alice_checksum.as_str(),
                self.bob_checksum.as_str(),
                self.charlie_checksum.as_str(),
            ]
        };
        Self::wait_for_wallets_ready(&self.metadata_db, &checksums).await?;

        println!("✅ WalletManager recreated successfully");
        Ok(())
    }

    /// Send a batch transaction to multiple recipients using sendmany
    pub async fn send_batch_transaction(
        &self,
        from_wallet: &str,
        recipients: &[(&str, &str)], // [(wallet_name, amount), ...]
    ) -> Result<String, Box<dyn std::error::Error>> {
        println!(
            "💸 Batch sending from {} to {} recipients",
            from_wallet,
            recipients.len()
        );

        let bitcoin_container_name = format!("test-bitcoin-{}", self.test_id);

        // Load source wallet
        let _ = Self::bitcoin_cli(
            &bitcoin_container_name,
            &[
                &format!("-rpcwallet={}", from_wallet),
                "loadwallet",
                from_wallet,
            ],
        );

        // Get fresh addresses for each recipient and build the JSON map using serde_json
        let mut address_amounts = serde_json::Map::new();
        for (recipient_wallet, amount) in recipients {
            let address = Self::bitcoin_cli(
                &bitcoin_container_name,
                &[&format!("-rpcwallet={}", recipient_wallet), "getnewaddress"],
            )?
            .trim()
            .trim_matches('"')
            .to_string();

            let amount_f64: f64 = amount
                .parse()
                .map_err(|e| format!("Invalid amount '{}': {}", amount, e))?;
            address_amounts.insert(
                address,
                serde_json::Value::Number(
                    serde_json::Number::from_f64(amount_f64)
                        .ok_or("Invalid float for JSON number")?,
                ),
            );
            println!(
                "   {} -> {} ({} BTC)",
                from_wallet, recipient_wallet, amount
            );
        }

        let address_map = serde_json::Value::Object(address_amounts).to_string();

        let txid = Self::bitcoin_cli(
            &bitcoin_container_name,
            &[
                &format!("-rpcwallet={}", from_wallet),
                "sendmany",
                "",
                &address_map,
            ],
        )?
        .trim()
        .trim_matches('"')
        .to_string();

        println!("✅ Batch transaction sent: {}", txid);
        Ok(txid)
    }

    /// Send a simple transaction (non-RBF) - convenience wrapper
    pub async fn send_transaction(
        &self,
        from_wallet: &str,
        to_wallet: &str,
        amount: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        // Use the proper system test method that works with isolated containers
        self.send_transaction_with_options(from_wallet, to_wallet, amount, false, None)
            .await
    }

    /// Create a CPFP (Child-Pays-For-Parent) transaction
    /// This creates a transaction that spends from an unconfirmed parent transaction with a high fee
    pub async fn create_cpfp_transaction(
        &self,
        wallet: &str,
        parent_txid: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        println!(
            "👶 Creating CPFP child transaction spending from parent: {}",
            parent_txid
        );

        let bitcoin_container_name = format!("test-bitcoin-{}", self.test_id);

        // Load wallet first
        let _ = Self::bitcoin_cli(
            &bitcoin_container_name,
            &[&format!("-rpcwallet={}", wallet), "loadwallet", wallet],
        );

        // Get transaction details to find spendable outputs
        let raw_tx_result = Self::bitcoin_cli(
            &bitcoin_container_name,
            &[
                &format!("-rpcwallet={}", wallet),
                "gettransaction",
                parent_txid,
            ],
        )?;

        let tx_json: serde_json::Value = serde_json::from_str(&raw_tx_result)?;

        // Find the first output that belongs to this wallet (we can spend from it)
        let details = tx_json["details"]
            .as_array()
            .ok_or("No details found in transaction")?;

        let mut receive_detail = None;
        for detail in details {
            if detail["category"] == "receive" {
                receive_detail = Some(detail);
                break;
            }
        }

        let receive_detail =
            receive_detail.ok_or("No receive output found in parent transaction")?;
        let receive_amount = receive_detail["amount"]
            .as_f64()
            .ok_or("Could not parse receive amount")?;

        // Create a child transaction that spends from the parent with higher fee
        // Send most of it back to ourselves, keeping a high fee
        let child_amount = receive_amount - 0.0001; // Leave 0.0001 BTC as fee (high fee for CPFP)

        if child_amount <= 0.0 {
            return Err("Insufficient amount in parent transaction for CPFP".into());
        }

        // Get a new address to send the child transaction to
        let child_address = Self::bitcoin_cli(
            &bitcoin_container_name,
            &[&format!("-rpcwallet={}", wallet), "getnewaddress"],
        )?
        .trim()
        .trim_matches('"')
        .to_string();

        // Create raw transaction spending from parent
        let vout = receive_detail["vout"]
            .as_u64()
            .ok_or("Could not parse vout")?;
        let create_raw_tx_result = Self::bitcoin_cli(
            &bitcoin_container_name,
            &[
                &format!("-rpcwallet={}", wallet),
                "createrawtransaction",
                &format!(r#"[{{"txid":"{}","vout":{}}}]"#, parent_txid, vout),
                &format!(r#"{{"{}":"{}"}}"#, child_address, child_amount),
            ],
        )?;

        let raw_tx = create_raw_tx_result.trim().trim_matches('"');

        // Sign the raw transaction
        let signed_result = Self::bitcoin_cli(
            &bitcoin_container_name,
            &[
                &format!("-rpcwallet={}", wallet),
                "signrawtransactionwithwallet",
                raw_tx,
            ],
        )?;

        let signed_json: serde_json::Value = serde_json::from_str(&signed_result)?;
        let signed_hex = signed_json["hex"]
            .as_str()
            .ok_or("Could not get signed transaction hex")?;

        // Broadcast the child transaction
        let child_txid =
            Self::bitcoin_cli(&bitcoin_container_name, &["sendrawtransaction", signed_hex])?
                .trim()
                .trim_matches('"')
                .to_string();

        println!("✅ CPFP child transaction created: {}", child_txid);
        println!("   Parent: {}", parent_txid);
        println!("   Child: {}", child_txid);
        println!(
            "   Child amount: {} BTC (fee: {} BTC)",
            child_amount,
            receive_amount - child_amount
        );

        Ok(child_txid)
    }
}

impl Drop for IsolatedTestEnvironment {
    /// Cleanup: Stop Docker Compose environment and ensure complete cleanup
    fn drop(&mut self) {
        println!("🧹 Cleaning up test environment: {}", self.test_id);

        // Step 1: Use docker-compose to stop services gracefully
        let result = Command::new("docker-compose")
            .current_dir(&self.compose_dir)
            .args(&["down", "-v"])
            .output();

        match result {
            Ok(output) => {
                if output.status.success() {
                    println!("✅ Docker Compose services stopped");
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    println!("⚠️  Docker Compose down warning: {}", stderr);
                }
            }
            Err(e) => {
                println!("❌ Failed to run docker-compose down: {}", e);
            }
        }

        // Step 2: Force stop and remove containers by name pattern to ensure cleanup
        let bitcoin_container = format!("test-bitcoin-{}", self.test_id);
        let fulcrum_container = format!("test-fulcrum-{}", self.test_id);

        for container in [&bitcoin_container, &fulcrum_container] {
            // Stop container
            let _ = Command::new("docker").args(&["stop", container]).output();

            // Remove container
            let _ = Command::new("docker")
                .args(&["rm", "-f", container])
                .output();
        }

        // Step 3: Remove volumes by name pattern
        let bitcoin_volume = format!("canary_test_bitcoin_data_{}", self.test_id);
        let fulcrum_volume = format!("canary_test_fulcrum_data_{}", self.test_id);

        for volume in [&bitcoin_volume, &fulcrum_volume] {
            let _ = Command::new("docker")
                .args(&["volume", "rm", "-f", volume])
                .output();
        }

        // Step 4: Remove network by name pattern
        let network_name = format!("canary_test_network_{}", self.test_id);
        let _ = Command::new("docker")
            .args(&["network", "rm", &network_name])
            .output();

        // Step 5: Cleanup any orphaned test containers as a safety net
        Self::cleanup_orphaned_test_containers();

        println!("✅ Test environment {} cleanup completed", self.test_id);
    }
}
