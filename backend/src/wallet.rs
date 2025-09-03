use crate::config::AppConfig;
use crate::config::NetworkConfig;
use crate::electrum::ElectrumClient;
use crate::metadata::{EventInsert, EventType, MetadataDb, TransactionEvent, WalletMetadata};
// use crate::sync::WalletSyncService; // Temporarily commented out
use anyhow::{anyhow, Result};
use bdk_wallet::rusqlite::Connection;
use bdk_wallet::{bitcoin::Network, KeychainKind, PersistedWallet, Wallet};
use miniscript::{Descriptor, DescriptorPublicKey};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;

/// Standalone wallet creation function that doesn't require WalletManager mutex
/// This allows wallet creation to be non-blocking and concurrent
pub struct WalletCreationService {
    wallet_dir: PathBuf,
    metadata_db: MetadataDb,
    electrum_client: Option<ElectrumClient>,
    network: Network,
}

impl WalletCreationService {
    pub fn new(
        wallet_dir: PathBuf,
        metadata_db: MetadataDb,
        electrum_client: Option<ElectrumClient>,
        network: Network,
    ) -> Self {
        Self {
            wallet_dir,
            metadata_db,
            electrum_client,
            network,
        }
    }

    /// Create wallet without blocking WalletManager
    /// Returns wallet metadata immediately while background task handles BDK wallet creation
    pub async fn create_wallet_non_blocking(
        &self,
        name: &str,
        descriptor_str: &str,
        user_id: &str,
        is_fresh_wallet: bool,
        script_type: Option<&str>,
        stop_gap: Option<&str>,
    ) -> Result<WalletMetadata> {
        use crate::xpub_converter::XpubConverter;

        println!("Creating wallet from multipath descriptor:");
        println!("  Name: {}", name);
        println!("  Input descriptor: {}", descriptor_str);

        // Validate network compatibility (defense-in-depth)
        XpubConverter::validate_descriptor_network(descriptor_str, self.network)?;

        // Check if input is an XPUB with known script type
        if XpubConverter::is_xpub(descriptor_str) && !is_fresh_wallet {
            if let Some(script_type_str) = script_type {
                if script_type_str != "auto" {
                    // Fast path: XPUB + known script type = skip probing
                    println!(
                        "Fast path: XPUB with known script type '{}'",
                        script_type_str
                    );
                    return self
                        .create_from_xpub_with_known_type(
                            name,
                            descriptor_str,
                            user_id,
                            script_type_str,
                            stop_gap,
                        )
                        .await;
                }
            }
            // For unknown XPUB script types, fall through to use XPUB as descriptor
            // Background task will handle script type detection
            println!("Detected XPUB format - background task will handle script type detection");
        }

        // Strip key origin to prevent duplicate wallets with same XPUB
        let normalized_descriptor = WalletManager::strip_key_origin_static(descriptor_str)?;

        // Check if normalized descriptor already exists
        if self
            .metadata_db
            .descriptor_exists(&normalized_descriptor)
            .await?
        {
            let checksum = self.metadata_db.extract_checksum(&normalized_descriptor);
            return Err(anyhow!("This wallet has already been added with ID: {}. Ask the wallet owner to add you as a contact for notifications.", checksum));
        }

        // Parse and validate the normalized multipath descriptor
        let (receive_descriptor, change_descriptor) =
            WalletManager::parse_multipath_descriptor_static(&normalized_descriptor)?;

        // Extract checksum from the normalized descriptor for consistent filename
        let checksum = self.metadata_db.extract_checksum(&normalized_descriptor);
        let wallet_filename_with_ext = format!("{}.sqlite", checksum);
        println!(
            "[{}] Wallet filename: {}",
            checksum, wallet_filename_with_ext
        );

        // Create wallet file path
        let wallet_path = self.wallet_dir.join(&wallet_filename_with_ext);
        println!("[{}] Wallet file path: {}", checksum, wallet_path.display());

        // Check if wallet file already exists
        if wallet_path.exists() {
            return Err(anyhow!("Wallet file already exists"));
        }

        // PHASE 1: Save wallet metadata immediately (synchronous)
        let wallet_checksum = self
            .metadata_db
            .insert_wallet(name, &normalized_descriptor, user_id)
            .await?;
        println!(
            "[{}] Metadata saved with checksum: {}",
            checksum, wallet_checksum
        );

        // Get wallet metadata to return immediately
        let wallet_metadata = self
            .metadata_db
            .get_wallet_by_descriptor(&normalized_descriptor)
            .await?
            .ok_or_else(|| anyhow!("Failed to retrieve created wallet metadata"))?;

        // PHASE 2: Spawn background task for slow operations
        let electrum_client_clone = self.electrum_client.clone();
        let metadata_db_clone = self.metadata_db.clone();
        let network = self.network;
        let checksum_clone = checksum.clone();
        let stop_gap_clone = stop_gap.map(|s| s.to_string());

        tokio::spawn(async move {
            println!(
                "[{}] Starting background wallet creation with stop gap: {:?}",
                checksum_clone, stop_gap_clone
            );
            if let Err(e) = WalletManager::complete_wallet_creation_with_stop_gap(
                wallet_path,
                receive_descriptor,
                change_descriptor,
                network,
                electrum_client_clone,
                metadata_db_clone,
                checksum_clone,
                is_fresh_wallet,
                stop_gap_clone.as_deref(),
            )
            .await
            {
                eprintln!(
                    "[{}] Background wallet creation failed: {}",
                    wallet_checksum, e
                );
            } else {
                eprintln!(
                    "[{}] Background wallet creation with scan depth completed",
                    wallet_checksum
                );
            }
        });

        Ok(wallet_metadata)
    }

    // Helper methods that mirror WalletManager functionality
    async fn create_from_xpub_with_known_type(
        &self,
        name: &str,
        xpub: &str,
        user_id: &str,
        script_type_str: &str,
        stop_gap: Option<&str>,
    ) -> Result<WalletMetadata> {
        use crate::xpub_converter::{ScriptType, XpubConverter};

        println!(
            "Creating XPUB wallet with known script type: {}",
            script_type_str
        );

        // Validate network compatibility (defense-in-depth)
        XpubConverter::validate_key_network(xpub, self.network)?;

        // Parse script type
        let script_type = match script_type_str {
            "p2wpkh" => ScriptType::P2WPKH,
            "p2sh" => ScriptType::P2SH,
            "p2pkh" => ScriptType::P2PKH,
            "p2tr" => ScriptType::P2TR,
            _ => return Err(anyhow!("Invalid script type: {}", script_type_str)),
        };

        // Create converter and generate descriptor
        let converter = XpubConverter::new(self.network, self.electrum_client.as_ref());
        let descriptor = converter.generate_descriptor_for_type(xpub, &script_type)?;

        println!("Generated descriptor: {}", descriptor);

        // Strip key origin for consistency
        let normalized_descriptor = WalletManager::strip_key_origin_static(&descriptor)?;

        // Check if this descriptor already exists
        if self
            .metadata_db
            .descriptor_exists(&normalized_descriptor)
            .await?
        {
            let checksum = self.metadata_db.extract_checksum(&normalized_descriptor);
            return Err(anyhow!("This wallet has already been added with ID: {}. Ask the wallet owner to add you as a contact for notifications.", checksum));
        }

        // Parse multipath descriptor
        let (receive_descriptor, change_descriptor) =
            WalletManager::parse_multipath_descriptor_static(&normalized_descriptor)?;

        // Extract checksum
        let checksum = self.metadata_db.extract_checksum(&normalized_descriptor);
        let wallet_filename_with_ext = format!("{}.sqlite", checksum);
        let wallet_path = self.wallet_dir.join(&wallet_filename_with_ext);

        // Check if wallet file already exists
        if wallet_path.exists() {
            return Err(anyhow!("Wallet file already exists"));
        }

        // Save wallet metadata immediately
        let wallet_checksum = self
            .metadata_db
            .insert_wallet(name, &normalized_descriptor, user_id)
            .await?;
        println!(
            "[{}] Metadata saved with checksum: {}",
            checksum, wallet_checksum
        );

        // Get wallet metadata to return immediately
        let wallet_metadata = self
            .metadata_db
            .get_wallet_by_descriptor(&normalized_descriptor)
            .await?
            .ok_or_else(|| anyhow!("Failed to retrieve created wallet metadata"))?;

        // PHASE 2: Spawn background task for slow operations
        let electrum_client_clone = self.electrum_client.clone();
        let metadata_db_clone = self.metadata_db.clone();
        let network = self.network;
        let checksum_clone = checksum.clone();
        let stop_gap_clone = stop_gap.map(|s| s.to_string());

        tokio::spawn(async move {
            println!(
                "[{}] Starting background wallet creation with stop gap: {:?}",
                checksum_clone, stop_gap_clone
            );
            if let Err(e) = WalletManager::complete_wallet_creation_with_stop_gap(
                wallet_path,
                receive_descriptor,
                change_descriptor,
                network,
                electrum_client_clone,
                metadata_db_clone,
                checksum_clone,
                true, // Fresh wallet for XPUB with known type
                stop_gap_clone.as_deref(),
            )
            .await
            {
                eprintln!(
                    "[{}] Background wallet creation failed: {}",
                    wallet_checksum, e
                );
            } else {
                eprintln!(
                    "[{}] Background wallet creation with scan depth completed",
                    wallet_checksum
                );
            }
        });

        Ok(wallet_metadata)
    }
}

pub struct WalletManager {
    pub wallets: Vec<(String, PersistedWallet<Connection>)>, // (checksum, wallet)
    pub wallet_dir: PathBuf,
    pub electrum_client: Option<ElectrumClient>,
    pub metadata_db: MetadataDb,
    pub event_sender: broadcast::Sender<TransactionEvent>,
    // pub sync_service: WalletSyncService, // Temporarily commented out
    network: Network,
    // Sync overlap protection
    sync_in_progress: Arc<AtomicBool>,
    sync_start_time: Option<Instant>,
}

impl WalletManager {
    pub async fn new(
        event_sender: broadcast::Sender<TransactionEvent>,
        wallet_dir: PathBuf,
        metadata_db_path: &str,
        network: Network,
        electrum_url: &str,
        config: &AppConfig,
    ) -> Self {
        if let Err(e) = std::fs::create_dir_all(&wallet_dir) {
            eprintln!("Warning: Failed to create wallet directory: {}", e);
        }

        // Initialize electrum client
        let electrum_client = match ElectrumClient::new(electrum_url) {
            Ok(client) => {
                println!("✅ Connected to Electrum server: {}", electrum_url);
                Some(client)
            }
            Err(e) => {
                eprintln!(
                    "❌ Failed to connect to Electrum server {}: {}",
                    electrum_url, e
                );
                eprintln!("   Wallet sync will not work without Electrum connection!");
                None
            }
        };

        // Initialize metadata database
        let metadata_db = match MetadataDb::new(metadata_db_path, config).await {
            Ok(db) => db,
            Err(e) => {
                eprintln!("Warning: Failed to create metadata database: {}", e);
                panic!("Cannot create WalletManager without metadata database");
            }
        };

        // let sync_service = WalletSyncService::new(metadata_db.clone(), event_sender.clone()); // Temporarily commented out

        let manager = WalletManager {
            wallets: Vec::new(),
            wallet_dir,
            electrum_client,
            metadata_db,
            event_sender,
            // sync_service, // Temporarily commented out
            network,
            sync_in_progress: Arc::new(AtomicBool::new(false)),
            sync_start_time: None,
        };

        // Don't load wallets at startup - this blocks the server from starting!
        // Wallets will be loaded on-demand during the first sync cycle
        println!("🚀 Non-blocking startup: Deferring wallet loading to background sync task");

        manager
    }

    /// Get the network configuration used by all wallets
    pub fn get_network(&self) -> Network {
        self.network
    }

    /// Convert BDK Network to NetworkConfig for tier limit calculations
    fn bdk_network_to_config(&self) -> NetworkConfig {
        match self.network {
            Network::Bitcoin => NetworkConfig::Mainnet,
            Network::Testnet => NetworkConfig::Testnet,
            Network::Regtest => NetworkConfig::Regtest,
            Network::Signet => NetworkConfig::Testnet, // Treat signet as testnet for sync purposes
            _ => NetworkConfig::Regtest,               // Default fallback for any unknown networks
        }
    }

    /// Create or load a SQLite connection for a wallet
    pub fn create_sqlite_connection(&self, wallet_path: &PathBuf) -> Result<Connection> {
        let conn = Connection::open(wallet_path)
            .map_err(|e| anyhow!("Failed to create/load wallet database: {}", e))?;

        Ok(conn)
    }

    /// Intelligently sync wallet list with database - removes deleted/expired wallets,
    /// loads only new wallets, avoids redundant disk I/O for already-loaded wallets
    async fn sync_wallet_list(&mut self) -> Result<()> {
        let start_time = Instant::now();

        // Get ready wallets from database (source of truth)
        let ready_wallets = self.metadata_db.get_ready_wallets().await?;

        // Create set of valid checksums from database
        let valid_checksums: std::collections::HashSet<String> =
            ready_wallets.iter().map(|w| w.checksum.clone()).collect();

        // Track statistics
        let _wallets_before = self.wallets.len();
        let mut removed_count = 0;
        let mut added_count = 0;
        let mut missing_count = 0;

        // First collect wallets that need to be removed (deleted or expired users)
        let mut wallets_to_remove = Vec::new();
        for (checksum, _) in &self.wallets {
            if !valid_checksums.contains(checksum) {
                wallets_to_remove.push(checksum.clone());
            }
        }

        // Clean up each removed wallet: memory, disk, and database
        for checksum in wallets_to_remove {
            removed_count += 1;
            println!("🗑️ Cleaning up wallet {} (deleted or expired)", checksum);

            // Remove from memory
            self.wallets
                .retain(|(stored_checksum, _)| stored_checksum != &checksum);

            // Delete wallet file from disk
            let wallet_filename = format!("{}.sqlite", checksum);
            let wallet_path = self.wallet_dir.join(&wallet_filename);
            if wallet_path.exists() {
                if let Err(e) = std::fs::remove_file(&wallet_path) {
                    eprintln!(
                        "  Warning: Failed to delete wallet file {}: {}",
                        wallet_path.display(),
                        e
                    );
                } else {
                    println!("  ✅ Deleted wallet file: {}", wallet_path.display());
                }
            }

            // Hard delete from database (if it was marked as deleted)
            if let Err(e) = self
                .metadata_db
                .hard_delete_wallet_by_checksum(&checksum)
                .await
            {
                eprintln!(
                    "  Warning: Failed to hard delete wallet {} from database: {}",
                    checksum, e
                );
            } else {
                println!("  ✅ Hard deleted wallet {} from database", checksum);
            }
        }

        // Create set of already loaded wallets
        let loaded_checksums: std::collections::HashSet<String> = self
            .wallets
            .iter()
            .map(|(checksum, _)| checksum.clone())
            .collect();

        // Load only NEW wallets not in memory
        for wallet_metadata in ready_wallets {
            if !loaded_checksums.contains(&wallet_metadata.checksum) {
                let wallet_path = self
                    .wallet_dir
                    .join(format!("{}.sqlite", wallet_metadata.checksum));

                if wallet_path.exists() {
                    if let Err(e) = self.load_wallet_from_file(&wallet_path).await {
                        eprintln!(
                            "Failed to load new wallet {} from {}: {}",
                            wallet_metadata.checksum,
                            wallet_path.display(),
                            e
                        );
                    } else {
                        added_count += 1;
                        println!(
                            "✅ Loaded new wallet {} into memory",
                            wallet_metadata.checksum
                        );
                    }
                } else {
                    eprintln!(
                        "Warning: Wallet file missing for {} ({}). Expected at: {}",
                        wallet_metadata.name,
                        wallet_metadata.checksum,
                        wallet_path.display()
                    );
                    missing_count += 1;
                }
            }
        }

        let duration = start_time.elapsed();

        if removed_count > 0 || added_count > 0 {
            println!(
                "📊 Wallet sync completed in {:?}: {} in memory (+{} new, -{} removed)",
                duration,
                self.wallets.len(),
                added_count,
                removed_count
            );
        } else {
            println!(
                "⚡ Wallet list unchanged in {:?}: {} wallets in memory",
                duration,
                self.wallets.len()
            );
        }

        if missing_count > 0 {
            eprintln!("⚠️  {} wallet files were missing", missing_count);
        }

        Ok(())
    }

    /// Optional cleanup function to identify orphaned wallet files
    /// (files that exist but aren't registered in the database)
    #[allow(dead_code)]
    async fn cleanup_orphaned_wallet_files(&self) -> Result<()> {
        let entries = fs::read_dir(&self.wallet_dir)?;
        let mut orphaned = Vec::new();

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("sqlite") {
                let filename = path.file_stem().and_then(|n| n.to_str()).unwrap_or("");

                // Check if this wallet exists in the database
                if let Ok(None) = self.metadata_db.get_wallet_by_checksum(filename).await {
                    orphaned.push((filename.to_string(), path.clone()));
                }
            }
        }

        if !orphaned.is_empty() {
            println!("⚠️  Found {} orphaned wallet files:", orphaned.len());
            for (checksum, path) in orphaned {
                println!("  - {} at {}", checksum, path.display());
                // Optionally delete or move to backup directory
                // fs::remove_file(&path)?;
            }
        } else {
            println!("✅ No orphaned wallet files found");
        }

        Ok(())
    }

    async fn load_wallet_from_file(&mut self, wallet_path: &PathBuf) -> Result<()> {
        let start_time = Instant::now();
        let filename = wallet_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        // Open the SQLite connection
        let mut db = self.create_sqlite_connection(wallet_path)?;

        // Try to load the wallet (we don't know the descriptors, so we let BDK figure it out)
        let wallet_opt = Wallet::load()
            .extract_keys()
            .check_network(self.get_network())
            .load_wallet(&mut db)
            .map_err(|e| anyhow!("Failed to load wallet: {}", e))?;

        if let Some(wallet) = wallet_opt {
            // Extract checksum from filename (remove .sqlite extension)
            // Since we now use checksums as filenames, this is already the checksum
            let checksum = filename
                .strip_suffix(".sqlite")
                .unwrap_or(filename)
                .to_string();

            // Check if wallet already exists before adding
            if !self.wallets.iter().any(|(cs, _)| cs == &checksum) {
                let load_duration = start_time.elapsed();
                println!("  ⏱️ Loaded wallet {} in {:?}", checksum, load_duration);
                self.wallets.push((checksum, wallet));
            }
        } else {
            println!("  ⚠ No wallet data found in file");
        }

        Ok(())
    }

    /// Strip key origin information from descriptor to prevent duplicates

    /// Static version of strip_key_origin for use without WalletManager instance
    pub fn strip_key_origin_static(descriptor_str: &str) -> Result<String> {
        use regex::Regex;

        // First strip any existing checksum (everything after #)
        let without_checksum = if let Some(pos) = descriptor_str.find('#') {
            &descriptor_str[..pos]
        } else {
            descriptor_str
        };

        // Pattern to match [fingerprint/derivation/path] anywhere in the descriptor
        // This handles both bare xpubs and script-wrapped descriptors like wpkh([fingerprint/path]xpub...)
        // Supports both 'h' and '\'' for hardened paths
        let key_origin_pattern = Regex::new(r"\[([0-9a-fA-F]{8})(/\d+[h']?)*\]").unwrap();

        // Strip key origin if present
        let stripped_without_checksum = if key_origin_pattern.is_match(without_checksum) {
            let result = key_origin_pattern.replace_all(without_checksum, "");
            println!("  Stripped key origin: {} -> {}", without_checksum, result);
            result.to_string()
        } else {
            // No key origin found, return without checksum
            without_checksum.to_string()
        };

        // Parse the stripped descriptor to recalculate checksum
        let descriptor: Descriptor<DescriptorPublicKey> = stripped_without_checksum
            .parse()
            .map_err(|e| anyhow!("Invalid stripped descriptor: {}", e))?;

        // Convert back to string with new checksum
        let final_descriptor = descriptor.to_string();
        println!("  Final normalized descriptor: {}", final_descriptor);

        Ok(final_descriptor)
    }

    /// Static version of parse_multipath_descriptor for use without WalletManager instance
    pub fn parse_multipath_descriptor_static(descriptor_str: &str) -> Result<(String, String)> {
        // Parse the descriptor
        let descriptor: Descriptor<DescriptorPublicKey> = descriptor_str
            .parse()
            .map_err(|e| anyhow!("Invalid descriptor: {}", e))?;

        // Check if it's a multipath descriptor
        if !descriptor.is_multipath() {
            return Err(anyhow!("Descriptor is not a multipath descriptor"));
        }

        // Split multipath descriptor into receive and change descriptors
        let descriptors = descriptor
            .into_single_descriptors()
            .map_err(|e| anyhow!("Failed to split multipath descriptor: {}", e))?;

        if descriptors.len() != 2 {
            return Err(anyhow!(
                "Multipath descriptor must have exactly 2 paths (receive and change)"
            ));
        }

        let receive_descriptor = descriptors[0].to_string();
        let change_descriptor = descriptors[1].to_string();

        println!("  Receive descriptor: {}", receive_descriptor);
        println!("  Change descriptor: {}", change_descriptor);

        Ok((receive_descriptor, change_descriptor))
    }

    /// Background task to complete wallet creation with scan depth support
    async fn complete_wallet_creation_with_stop_gap(
        wallet_path: PathBuf,
        receive_descriptor: String,
        change_descriptor: String,
        network: Network,
        electrum_client: Option<ElectrumClient>,
        metadata_db: MetadataDb,
        checksum: String,
        is_fresh_wallet: bool,
        stop_gap: Option<&str>,
    ) -> Result<()> {
        println!(
            "[{}] Starting background wallet creation with stop gap: {:?}",
            checksum, stop_gap
        );

        // Create SQLite connection
        let mut db = Connection::open(&wallet_path).map_err(|e| {
            anyhow!(
                "Failed to create connection to {}: {}",
                wallet_path.display(),
                e
            )
        })?;

        // Parse descriptors
        let receive_desc: Descriptor<DescriptorPublicKey> = receive_descriptor
            .parse()
            .map_err(|e| anyhow!("Failed to parse receive descriptor: {}", e))?;
        let change_desc: Descriptor<DescriptorPublicKey> = change_descriptor
            .parse()
            .map_err(|e| anyhow!("Failed to parse change descriptor: {}", e))?;

        // Create new wallet
        let mut wallet = Wallet::create(receive_desc, change_desc)
            .network(network)
            .create_wallet(&mut db)
            .map_err(|e| anyhow!("Failed to create wallet: {}", e))?;

        // Persist initial wallet state
        wallet
            .persist(&mut db)
            .map_err(|e| anyhow!("Failed to persist wallet: {}", e))?;

        // Apply stop gap if specified
        if let Some(stop_gap_str) = stop_gap {
            if stop_gap_str != "auto" {
                if let Ok(max_index) = stop_gap_str.parse::<u32>() {
                    println!(
                        "[{}] Applying custom stop gap: {} addresses",
                        checksum, max_index
                    );

                    // Reveal addresses up to the specified index
                    let _external_addresses: Vec<_> = wallet
                        .reveal_addresses_to(KeychainKind::External, max_index)
                        .collect();
                    let _internal_addresses: Vec<_> = wallet
                        .reveal_addresses_to(KeychainKind::Internal, max_index)
                        .collect();

                    // Persist the revealed addresses
                    if let Err(e) = wallet.persist(&mut db) {
                        eprintln!(
                            "[{}] Warning: Failed to persist revealed addresses: {}",
                            checksum, e
                        );
                    }
                }
            }
        }

        // Full scan with electrum (using custom stop gap if specified)
        if let Some(ref client) = electrum_client {
            let custom_stop_gap = if let Some(stop_gap_str) = stop_gap {
                if stop_gap_str != "auto" {
                    stop_gap_str.parse::<usize>().ok()
                } else {
                    None
                }
            } else {
                None
            };

            if let Err(e) = client.full_scan_wallet(&mut wallet, custom_stop_gap) {
                eprintln!(
                    "[{}] Warning: Failed to full scan wallet during background creation: {}",
                    checksum, e
                );
            } else {
                // Persist after sync
                if let Err(e) = wallet.persist(&mut db) {
                    eprintln!(
                        "[{}] Warning: Failed to persist wallet after sync: {}",
                        checksum, e
                    );
                }

                // Deep scanning for existing wallets with no funds (only if stop_gap is auto)
                if !is_fresh_wallet
                    && wallet.balance().total().to_sat() == 0
                    && stop_gap.unwrap_or("auto") == "auto"
                {
                    println!(
                        "[{}] No funds found in initial scan, starting deep scan...",
                        checksum
                    );

                    // Deep scan in batches up to 500 addresses (only for auto mode)
                    for batch in 1..=5 {
                        let reveal_to = batch * 100;
                        println!(
                            "[{}] Deep scan batch {}: checking addresses up to index {}",
                            checksum, batch, reveal_to
                        );

                        // Reveal more addresses for both keychains
                        let ext_revealed: Vec<_> = wallet
                            .reveal_addresses_to(bdk_wallet::KeychainKind::External, reveal_to)
                            .collect();
                        let int_revealed: Vec<_> = wallet
                            .reveal_addresses_to(bdk_wallet::KeychainKind::Internal, reveal_to)
                            .collect();

                        println!(
                            "[{}] Revealed {} external, {} internal addresses (total: {} each)",
                            checksum,
                            ext_revealed.len(),
                            int_revealed.len(),
                            reveal_to + 1
                        );

                        // Sync the newly revealed addresses
                        if let Err(e) = client.sync_wallet(&mut wallet) {
                            eprintln!(
                                "[{}] Warning: Failed to sync during deep scan batch {}: {}",
                                checksum, batch, e
                            );
                            break;
                        }

                        // Check if we found any activity - if so, we should continue scanning
                        let current_balance = wallet.balance().total().to_sat();
                        if current_balance > 0 {
                            println!(
                                "[{}] Found activity during deep scan! Balance: {} sats",
                                checksum, current_balance
                            );
                            // Continue scanning to find all transactions
                        }
                    }

                    // Final persistence after deep scanning
                    if let Err(e) = wallet.persist(&mut db) {
                        eprintln!(
                            "[{}] Warning: Failed to persist after deep scan: {}",
                            checksum, e
                        );
                    }
                }
            }
        }

        // Update balance in metadata database
        let balance = wallet.balance().total().to_sat() as i64;
        if let Err(e) = metadata_db
            .update_wallet_balance_by_checksum(&checksum, balance)
            .await
        {
            eprintln!(
                "[{}] Warning: Failed to update wallet balance: {}",
                checksum, e
            );
        }

        // Extract historical transactions after sync
        if let Err(e) = Self::extract_historical_transactions_for_background(
            &wallet,
            &checksum,
            &metadata_db,
            electrum_client.as_ref(),
        )
        .await
        {
            eprintln!(
                "[{}] Warning: Failed to extract historical transactions: {}",
                checksum, e
            );
        }

        // Update last synced timestamp
        if let Err(e) = metadata_db.update_wallet_last_synced(&checksum).await {
            eprintln!(
                "[{}] Warning: Failed to update wallet last synced: {}",
                checksum, e
            );
        }

        // Mark wallet as ready after deep scan and transaction extraction is complete
        if let Err(e) = metadata_db.update_wallet_status(&checksum, "ready").await {
            eprintln!(
                "[{}] Warning: Failed to mark wallet as ready: {}",
                checksum, e
            );
        } else {
            println!(
                "[{}] ✅ Wallet marked as ready - available for frontend display",
                checksum
            );
        }

        println!(
            "[{}] Background wallet creation with scan depth completed",
            checksum
        );
        Ok(())
    }

    /// Extract historical transactions for background task (static version)
    async fn extract_historical_transactions_for_background(
        wallet: &PersistedWallet<Connection>,
        wallet_checksum: &str,
        metadata_db: &MetadataDb,
        electrum_client: Option<&crate::electrum::ElectrumClient>,
    ) -> Result<()> {
        println!("[{}] Extracting historical transactions", wallet_checksum);

        // Collect all transactions and sort them chronologically
        let mut all_transactions: Vec<_> = wallet.transactions().collect();

        // Sort transactions chronologically (confirmed first, then by height/timestamp)
        all_transactions.sort_by(|a, b| {
            match (&a.chain_position, &b.chain_position) {
                // Both confirmed: sort by block height
                (
                    bdk_wallet::chain::ChainPosition::Confirmed {
                        anchor: anchor_a, ..
                    },
                    bdk_wallet::chain::ChainPosition::Confirmed {
                        anchor: anchor_b, ..
                    },
                ) => anchor_a.block_id.height.cmp(&anchor_b.block_id.height),
                // Both unconfirmed: sort by first_seen timestamp if available
                (
                    bdk_wallet::chain::ChainPosition::Unconfirmed {
                        first_seen: first_a,
                        ..
                    },
                    bdk_wallet::chain::ChainPosition::Unconfirmed {
                        first_seen: first_b,
                        ..
                    },
                ) => first_a.unwrap_or(0).cmp(&first_b.unwrap_or(0)),
                // Confirmed comes before unconfirmed
                (
                    bdk_wallet::chain::ChainPosition::Confirmed { .. },
                    bdk_wallet::chain::ChainPosition::Unconfirmed { .. },
                ) => std::cmp::Ordering::Less,
                // Unconfirmed comes after confirmed
                (
                    bdk_wallet::chain::ChainPosition::Unconfirmed { .. },
                    bdk_wallet::chain::ChainPosition::Confirmed { .. },
                ) => std::cmp::Ordering::Greater,
            }
        });

        println!(
            "[{}] Found {} historical transactions to process",
            wallet_checksum,
            all_transactions.len()
        );

        // Get current wallet balance
        let current_balance = wallet.balance().total().to_sat() as i64;

        // Calculate initial balance by working backwards from current balance
        let total_net_change: i64 = all_transactions
            .iter()
            .map(|tx| {
                let sent = wallet.sent_and_received(&tx.tx_node).0;
                let received = wallet.sent_and_received(&tx.tx_node).1;
                received.to_sat() as i64 - sent.to_sat() as i64
            })
            .filter(|&net| net != 0) // Skip zero net amount transactions
            .sum();

        let initial_balance = current_balance - total_net_change;
        let mut running_balance = initial_balance;

        println!(
            "[{}] Current balance: {:.8} BTC, Initial balance: {:.8} BTC",
            wallet_checksum,
            current_balance as f64 / 100_000_000.0,
            initial_balance as f64 / 100_000_000.0
        );

        // Process each transaction chronologically using new transaction-based approach
        for tx in all_transactions {
            let txid = tx.tx_node.txid.to_string();
            let sent = wallet.sent_and_received(&tx.tx_node).0;
            let received = wallet.sent_and_received(&tx.tx_node).1;
            let net_amount = received.to_sat() as i64 - sent.to_sat() as i64;
            let is_confirmed = tx.chain_position.is_confirmed();

            // Skip transactions with zero net amount
            if net_amount == 0 {
                continue;
            }

            // Update running balance
            running_balance += net_amount;

            let (transaction_type, amount_sats) = if net_amount > 0 {
                (EventType::Receive, net_amount)
            } else {
                (EventType::Send, net_amount.abs())
            };

            // Get block height and confirmation details
            let (block_height, confirmed_at) = match &tx.chain_position {
                bdk_wallet::chain::ChainPosition::Confirmed { anchor, .. } => {
                    let block_height = Some(anchor.block_id.height);
                    // Fetch actual block timestamp from Electrum
                    let confirmed_at = if let Some(electrum_client) = electrum_client {
                        match electrum_client.get_block_header(anchor.block_id.height) {
                            Ok(header) => Some(header.timestamp),
                            Err(e) => {
                                eprintln!(
                                    "[{}] Failed to fetch block header for height {}: {}",
                                    wallet_checksum, anchor.block_id.height, e
                                );
                                // Fallback to current time only if fetch fails
                                Some(std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs())
                            }
                        }
                    } else {
                        // No electrum client available, use current time as fallback
                        Some(std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs())
                    };
                    (block_height, confirmed_at)
                }
                bdk_wallet::chain::ChainPosition::Unconfirmed { first_seen, .. } => {
                    (None, None)
                }
            };

            // Get first_seen timestamp
            let first_seen_at = match &tx.chain_position {
                bdk_wallet::chain::ChainPosition::Confirmed { anchor, .. } => {
                    confirmed_at.unwrap_or_else(|| {
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs()
                    })
                }
                bdk_wallet::chain::ChainPosition::Unconfirmed { first_seen, .. } => first_seen
                    .unwrap_or_else(|| {
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs()
                    }),
            };

            // Create transaction insert using new schema
            let transaction_insert = crate::metadata::TransactionInsert {
                txid,
                wallet_checksum: wallet_checksum.to_string(),
                transaction_type,
                amount_sats,
                fee_sats: None, // TODO: Calculate fees for historical transactions
                block_height,
                first_seen_at,
                confirmed_at,
                is_rbf: false, // TODO: Detect RBF for historical transactions
                is_cpfp: false, // TODO: Detect CPFP for historical transactions
                balance_after: Some(running_balance),
            };

            // Insert individual transaction
            if let Err(e) = metadata_db.insert_transaction(&transaction_insert).await {
                eprintln!(
                    "[{}] Failed to insert historical transaction {}: {}",
                    wallet_checksum, transaction_insert.txid, e
                );
            }
        }

        println!(
            "[{}] Historical transaction extraction completed",
            wallet_checksum
        );
        Ok(())
    }

    /// Helper function to get timestamp of new sending transactions
    fn get_new_send_transaction_timestamp(
        wallet: &PersistedWallet<Connection>,
        unconfirmed_sends_before: &Vec<(String, i64)>,
    ) -> u64 {
        wallet
            .transactions()
            .filter_map(|tx| {
                if !tx.chain_position.is_confirmed() {
                    let sent = wallet.sent_and_received(&tx.tx_node).0;
                    let received = wallet.sent_and_received(&tx.tx_node).1;
                    let net_amount = received.to_sat() as i64 - sent.to_sat() as i64;
                    if net_amount < 0 {
                        let txid = tx.tx_node.txid.to_string();
                        // Check if this is a NEW transaction (not in the before list)
                        if !unconfirmed_sends_before.iter().any(|(id, _)| id == &txid) {
                            // Get the first_seen timestamp
                            if let bdk_wallet::chain::ChainPosition::Unconfirmed { first_seen, .. } = &tx.chain_position {
                                return Some(first_seen.unwrap_or_else(|| Self::get_current_timestamp()));
                            }
                        }
                    }
                }
                None
            })
            .next()
            .unwrap_or_else(|| Self::get_current_timestamp())
    }

    pub async fn sync_wallet_by_checksum(&mut self, wallet_checksum: &str) -> Result<bool> {
        println!("[{}] Starting transaction-based sync", wallet_checksum);

        // Find the wallet
        if let Some((_, wallet)) = self
            .wallets
            .iter_mut()
            .find(|(checksum, _)| checksum == wallet_checksum)
        {
            // Use the new transaction-based sync service
            // TODO: Implement new sync logic
            let has_changes = false; // Temporarily disabled

            // Persist wallet changes to disk
            self.persist_wallet_by_checksum(wallet_checksum).await?;

            Ok(has_changes)
        } else {
            // Wallet not found in memory - this shouldn't happen during normal operation
            eprintln!("[{}] Wallet not found in memory", wallet_checksum);
            Ok(false)
        }
    }

    /// Helper function to persist a specific wallet by checksum
    async fn persist_wallet_by_checksum(&mut self, wallet_checksum: &str) -> Result<()> {
        let wallet_filename = format!("{}.sqlite", wallet_checksum);
        let wallet_path = self.wallet_dir.join(&wallet_filename);
        let mut db = self.create_sqlite_connection(&wallet_path)?;

        if let Some((_, wallet)) = self
            .wallets
            .iter_mut()
            .find(|(checksum, _)| checksum == wallet_checksum)
        {
            wallet
                .persist(&mut db)
                .map_err(|e| anyhow!("Failed to persist wallet: {}", e))?;
        }

        Ok(())
    }

    /// Clean up deleted wallets - remove from memory, disk, and database
    async fn cleanup_deleted_wallets(&mut self) -> Result<()> {
        // Get ready wallets from database (source of truth)
        let ready_wallets = self.metadata_db.get_ready_wallets().await?;

        // Create set of valid checksums from database
        let valid_checksums: std::collections::HashSet<String> =
            ready_wallets.iter().map(|w| w.checksum.clone()).collect();

        // Collect wallets that need to be removed (deleted or expired users)
        let mut wallets_to_remove = Vec::new();
        for (checksum, _) in &self.wallets {
            if !valid_checksums.contains(checksum) {
                wallets_to_remove.push(checksum.clone());
            }
        }

        // Clean up each removed wallet: memory, disk, and database
        for checksum in wallets_to_remove {
            println!("🗑️ Cleaning up wallet {} (deleted or expired)", checksum);

            // Remove from memory
            self.wallets
                .retain(|(stored_checksum, _)| stored_checksum != &checksum);

            // Delete wallet file from disk
            let wallet_filename = format!("{}.sqlite", checksum);
            let wallet_path = self.wallet_dir.join(&wallet_filename);
            if wallet_path.exists() {
                if let Err(e) = std::fs::remove_file(&wallet_path) {
                    eprintln!(
                        "  Warning: Failed to delete wallet file {}: {}",
                        wallet_path.display(),
                        e
                    );
                } else {
                    println!("  ✅ Deleted wallet file: {}", wallet_path.display());
                }
            }

            // Hard delete from database (if it was marked as deleted)
            if let Err(e) = self
                .metadata_db
                .hard_delete_wallet_by_checksum(&checksum)
                .await
            {
                eprintln!(
                    "  Warning: Failed to hard delete wallet {} from database: {}",
                    checksum, e
                );
            } else {
                println!("  ✅ Hard deleted wallet {} from database", checksum);
            }
        }

        Ok(())
    }

    /// Sync all wallets for a specific subscription tier in parallel
    pub async fn sync_tier_parallel(
        &mut self,
        tier: crate::subscription::SubscriptionTier,
    ) -> Result<()> {
        use crate::sync::WalletSyncService;
        
        // First, perform wallet cleanup (remove deleted wallets)
        self.cleanup_deleted_wallets().await?;
        
        // Convert Network to NetworkConfig for the query
        let network_config = NetworkConfig::from_network(self.network);
        
        // Get wallets for this tier from metadata
        let wallets = self.metadata_db.get_wallets_for_tier_sync(&tier, &network_config).await?;
        
        if wallets.is_empty() {
            println!("📭 No {:?} tier wallets to sync", tier);
            return Ok(());
        }
        
        println!("🔄 Starting sync for {} {:?} tier wallets", wallets.len(), tier);
        
        // Create sync service with proper parameters
        let sync_service = WalletSyncService::new(
            self.metadata_db.clone(),
            self.event_sender.clone(),
        );
        
        // Process each wallet with new transaction-based sync
        for wallet_metadata in wallets {
            // Find the wallet in our Vec<(String, PersistedWallet)>
            if let Some((_checksum, persisted_wallet)) = self.wallets
                .iter_mut()
                .find(|(checksum, _)| checksum == &wallet_metadata.checksum) 
            {
                match sync_service.sync_wallet_by_checksum(
                    persisted_wallet,
                    &wallet_metadata.checksum,
                    self.electrum_client.as_ref(),
                ).await {
                    Ok(_) => {
                        println!("✅ Synced wallet: {}", wallet_metadata.name);
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to sync wallet {}: {}", wallet_metadata.name, e);
                    }
                }
            }
        }
        
        Ok(())
    }

    // TODO: Clean up and implement other methods below using new sync system
    
    fn get_current_timestamp() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    /// Apply subscription tier limits by setting is_active status on wallets and contacts
    pub async fn apply_subscription_limits(
        &self,
        user_id: &str,
        tier: &str,
        is_admin: bool,
    ) -> Result<(), anyhow::Error> {
        if is_admin {
            tracing::info!("🎯 Applying unlimited limits for admin user {}", user_id);
        } else {
            tracing::info!("🎯 Applying {} tier limits for user {}", tier, user_id);
        }

        // Get all wallets for this user ordered by creation time (oldest first)
        let wallets = self
            .metadata_db
            .get_wallets_for_user_oldest_first(user_id)
            .await?;

        // Determine wallet limit based on tier or admin status
        let wallet_limit = if is_admin {
            usize::MAX // Unlimited for admin
        } else {
            match tier {
                "personal" => 1,
                "team" => 5,
                _ => 1, // Default to personal limits for unknown tiers
            }
        };

        // Apply wallet limits (activate oldest wallets, deactivate newer ones)
        for (i, wallet) in wallets.iter().enumerate() {
            let should_be_active = i < wallet_limit;

            // Update is_active status only if it changed
            if wallet.is_active != should_be_active {
                if let Err(e) = self
                    .metadata_db
                    .update_wallet_active_status(&wallet.checksum, should_be_active)
                    .await
                {
                    tracing::error!(
                        "Failed to set wallet {} active status to {}: {}",
                        wallet.checksum,
                        should_be_active,
                        e
                    );
                } else {
                    tracing::info!(
                        "Set wallet {} active status to {} (position: {})",
                        wallet.checksum,
                        should_be_active,
                        i + 1
                    );
                }
            }
        }

        // For active wallets, apply contact limits
        let contact_limit = if is_admin {
            usize::MAX // Unlimited for admin
        } else {
            match tier {
                "personal" => 1,
                "team" => 5,
                _ => 1, // Default to personal limits
            }
        };

        for wallet in wallets.iter() {
            if wallet.is_active {
                // Get contacts ordered by creation time (oldest first)
                let contacts = self
                    .metadata_db
                    .get_contacts_oldest_first_for_limits(&wallet.checksum)
                    .await?;

                // Apply contact limits
                for (i, contact) in contacts.iter().enumerate() {
                    let should_be_active = i < contact_limit;

                    // Update is_active status only if it changed
                    if contact.is_active != should_be_active {
                        if let Some(contact_id) = &contact.id {
                            if let Err(e) = self
                                .metadata_db
                                .update_contact_active_status(contact_id, should_be_active)
                                .await
                            {
                                tracing::error!(
                                    "Failed to set contact {} active status to {}: {}",
                                    contact_id,
                                    should_be_active,
                                    e
                                );
                            } else {
                                tracing::info!(
                                    "Set contact {} active status to {} (position: {})",
                                    contact.name,
                                    should_be_active,
                                    i + 1
                                );
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Get wallet metadata by checksum
    pub async fn get_wallet_by_checksum(
        &self,
        checksum: &str,
    ) -> Result<Option<crate::metadata::WalletMetadata>, anyhow::Error> {
        self.metadata_db.get_wallet_by_checksum(checksum).await
    }
}

