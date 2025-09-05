use crate::config::AppConfig;
use crate::config::NetworkConfig;
use crate::electrum::ElectrumClient;
use crate::metadata::{MetadataDb, TransactionNotification, WalletMetadata};
use anyhow::{anyhow, Result};
use bdk_wallet::rusqlite::Connection;
use bdk_wallet::{bitcoin::Network, KeychainKind, PersistedWallet, Wallet};
use miniscript::{Descriptor, DescriptorPublicKey};
use std::fs;
use std::path::PathBuf;
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
    pub notification_sender: broadcast::Sender<TransactionNotification>,
    network: Network,
}

impl WalletManager {
    pub async fn new(
        notification_sender: broadcast::Sender<TransactionNotification>,
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
            notification_sender,
            network,
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

    /// Create or load a SQLite connection for a wallet
    pub fn create_sqlite_connection(&self, wallet_path: &PathBuf) -> Result<Connection> {
        let conn = Connection::open(wallet_path)
            .map_err(|e| anyhow!("Failed to create/load wallet database: {}", e))?;

        Ok(conn)
    }

    /// Intelligently sync wallet list with database - removes deleted/expired wallets,
    /// loads only new wallets, avoids redundant disk I/O for already-loaded wallets
    pub async fn sync_wallet_list(&mut self) -> Result<()> {
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
        if let Err(e) =
            crate::sync::WalletSyncService::extract_historical_transactions_for_background(
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

    /// Clean up deleted wallets - remove from memory, disk, and database
    async fn cleanup_deleted_wallets(&mut self) -> Result<()> {
        // Get ready wallets from database (source of truth)
        let ready_wallets = self.metadata_db.get_ready_wallets().await?;

        // Get wallets marked as deleted in database
        let deleted_wallets = self.metadata_db.get_deleted_wallets().await?;

        // Create set of valid checksums from database
        let valid_checksums: std::collections::HashSet<String> =
            ready_wallets.iter().map(|w| w.checksum.clone()).collect();

        // Collect wallets that need to be removed from memory (deleted or expired users)
        let mut wallets_to_remove = Vec::new();
        for (checksum, _) in &self.wallets {
            if !valid_checksums.contains(checksum) {
                wallets_to_remove.push(checksum.clone());
            }
        }

        // Also add wallets marked as deleted in database (even if not in memory)
        for deleted_wallet in &deleted_wallets {
            if !wallets_to_remove.contains(&deleted_wallet.checksum) {
                wallets_to_remove.push(deleted_wallet.checksum.clone());
            }
        }

        // Clean up each removed wallet: memory, disk, and database
        for checksum in wallets_to_remove {
            println!("🗑️ Cleaning up wallet {} (deleted or expired)", checksum);

            // Remove from memory (if it was loaded)
            self.wallets
                .retain(|(stored_checksum, _)| stored_checksum != &checksum);

            // Delete wallet file from disk (whether it was in memory or not)
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
        let wallets = self
            .metadata_db
            .get_wallets_for_tier_sync(&tier, &network_config)
            .await?;

        if wallets.is_empty() {
            println!("📭 No {:?} tier wallets to sync", tier);
            return Ok(());
        }

        println!(
            "🔄 Starting sync for {} {:?} tier wallets",
            wallets.len(),
            tier
        );

        // Create sync service with proper parameters
        let sync_service =
            WalletSyncService::new(self.metadata_db.clone(), self.notification_sender.clone());

        // Process each wallet with new transaction-based sync
        for wallet_metadata in wallets {
            // Check if wallet is loaded in memory
            let wallet_exists_in_memory = self
                .wallets
                .iter()
                .any(|(checksum, _)| checksum == &wallet_metadata.checksum);

            // If wallet is not in memory but is ready in DB, load it from disk
            if !wallet_exists_in_memory {
                let wallet_filename = format!("{}.sqlite", wallet_metadata.checksum);
                let wallet_path = self.wallet_dir.join(&wallet_filename);

                if wallet_path.exists() {
                    println!(
                        "📥 Loading wallet {} from disk for sync",
                        wallet_metadata.name
                    );
                    if let Err(e) = self.load_wallet_from_file(&wallet_path).await {
                        eprintln!(
                            "❌ Failed to load wallet {} from disk: {}",
                            wallet_metadata.name, e
                        );
                        continue; // Skip this wallet if loading fails
                    }
                } else {
                    eprintln!(
                        "❌ Wallet file not found for {}: {}",
                        wallet_metadata.name,
                        wallet_path.display()
                    );
                    continue;
                }
            }

            // Find the wallet in our Vec<(String, PersistedWallet)> (should exist now)
            if let Some((_checksum, persisted_wallet)) = self
                .wallets
                .iter_mut()
                .find(|(checksum, _)| checksum == &wallet_metadata.checksum)
            {
                match sync_service
                    .sync_wallet_by_checksum(
                        persisted_wallet,
                        &wallet_metadata.checksum,
                        self.electrum_client.as_ref(),
                    )
                    .await
                {
                    Ok(_) => {
                        println!("✅ Synced wallet: {}", wallet_metadata.name);
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to sync wallet {}: {}", wallet_metadata.name, e);
                    }
                }
            } else {
                eprintln!(
                    "❌ Wallet {} still not found in memory after loading attempt",
                    wallet_metadata.name
                );
            }
        }

        Ok(())
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
