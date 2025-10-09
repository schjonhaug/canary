use crate::config::AppConfig;
use crate::config::NetworkConfig;
use crate::electrum::ElectrumClient;
use crate::metadata::{MetadataDb, TransactionNotification, WalletMetadata};
use anyhow::{anyhow, Result};
use bdk_wallet::rusqlite::Connection;
use bdk_wallet::{bitcoin::Network, KeychainKind, PersistedWallet, Wallet};
use futures::future::join_all;
use miniscript::{Descriptor, DescriptorPublicKey};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, Mutex, Semaphore};
use tracing::{debug, error, info, warn};

/// Maximum number of wallets to sync in parallel
const MAX_PARALLEL_SYNCS: usize = 10;

/// Standalone wallet creation function that doesn't require WalletManager mutex
/// This allows wallet creation to be non-blocking and concurrent
pub struct WalletCreationService {
    wallet_dir: PathBuf,
    metadata_db: MetadataDb,
    electrum_client: Option<ElectrumClient>,
    network: Network,
    // Reference to in-memory wallet storage for adding new wallets
    wallets: Arc<Mutex<HashMap<String, Arc<Mutex<(PersistedWallet<Connection>, Connection)>>>>>,
}

impl WalletCreationService {
    pub fn new(
        wallet_dir: PathBuf,
        metadata_db: MetadataDb,
        electrum_client: Option<ElectrumClient>,
        network: Network,
        wallets: Arc<Mutex<HashMap<String, Arc<Mutex<(PersistedWallet<Connection>, Connection)>>>>>,
    ) -> Self {
        Self {
            wallet_dir,
            metadata_db,
            electrum_client,
            network,
            wallets,
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

        debug!("Creating wallet from multipath descriptor:");
        debug!(" Name: {}", name);
        debug!(" Input descriptor: {}", descriptor_str);

        // Validate network compatibility (defense-in-depth)
        XpubConverter::validate_descriptor_network(descriptor_str, self.network)?;

        // Check if input is an XPUB with known script type
        if XpubConverter::is_xpub(descriptor_str) && !is_fresh_wallet {
            if let Some(script_type_str) = script_type {
                if script_type_str != "auto" {
                    // Fast path: XPUB + known script type = skip probing
                    debug!(
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
            debug!("Detected XPUB format - background task will handle script type detection");
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
        debug!(
            "[{}] Wallet filename: {}",
            checksum, wallet_filename_with_ext
        );

        // Create wallet file path
        let wallet_path = self.wallet_dir.join(&wallet_filename_with_ext);
        debug!("[{}] Wallet file path: {}", checksum, wallet_path.display());

        // Check if wallet file already exists
        if wallet_path.exists() {
            return Err(anyhow!("Wallet file already exists"));
        }

        // PHASE 1: Save wallet metadata immediately (synchronous)
        let wallet_checksum = self
            .metadata_db
            .insert_wallet(name, &normalized_descriptor, user_id)
            .await?;
        debug!(
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
        let wallets_clone = self.wallets.clone();
        let wallet_path_clone = wallet_path.clone();

        tokio::spawn(async move {
            debug!(
                "[{}] Starting background wallet creation with stop gap: {:?}",
                checksum_clone, stop_gap_clone
            );
            if let Err(e) = WalletManager::complete_wallet_creation_with_stop_gap(
                wallet_path_clone.clone(),
                receive_descriptor,
                change_descriptor,
                network,
                electrum_client_clone,
                metadata_db_clone,
                checksum_clone.clone(),
                is_fresh_wallet,
                stop_gap_clone.as_deref(),
                wallets_clone,
            )
            .await
            {
                error!(
                    "[{}] Background wallet creation failed: {}",
                    wallet_checksum, e
                );
            } else {
                debug!(
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

        debug!(
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

        debug!("Generated descriptor: {}", descriptor);

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
        debug!(
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
        let wallets_clone = self.wallets.clone();
        let wallet_path_clone = wallet_path.clone();

        tokio::spawn(async move {
            debug!(
                "[{}] Starting background wallet creation with stop gap: {:?}",
                checksum_clone, stop_gap_clone
            );
            if let Err(e) = WalletManager::complete_wallet_creation_with_stop_gap(
                wallet_path_clone.clone(),
                receive_descriptor,
                change_descriptor,
                network,
                electrum_client_clone,
                metadata_db_clone,
                checksum_clone.clone(),
                true, // Fresh wallet for XPUB with known type
                stop_gap_clone.as_deref(),
                wallets_clone,
            )
            .await
            {
                error!(
                    "[{}] Background wallet creation failed: {}",
                    wallet_checksum, e
                );
            } else {
                debug!(
                    "[{}] Background wallet creation with scan depth completed",
                    wallet_checksum
                );
            }
        });

        Ok(wallet_metadata)
    }
}

pub struct WalletManager {
    // Thread-safe HashMap for in-memory wallet storage
    // Each wallet has its own mutex for parallel access
    pub wallets: Arc<Mutex<HashMap<String, Arc<Mutex<(PersistedWallet<Connection>, Connection)>>>>>,
    pub wallet_dir: PathBuf,
    pub electrum_client: Option<ElectrumClient>,
    pub metadata_db: MetadataDb,
    pub notification_sender: broadcast::Sender<TransactionNotification>,
    network: Network,
    config: AppConfig,
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
            warn!("Failed to create wallet directory: {}", e);
        }

        // Initialize electrum client
        let electrum_client = match ElectrumClient::new(electrum_url) {
            Ok(client) => {
                info!("Connected to Electrum server: {}", electrum_url);
                Some(client)
            }
            Err(e) => {
                error!(
                    "❌ Failed to connect to Electrum server {}: {}",
                    electrum_url, e
                );
                info!("Wallet sync will not work without Electrum connection!");
                None
            }
        };

        // Initialize metadata database
        let metadata_db = match MetadataDb::new(metadata_db_path, config).await {
            Ok(db) => db,
            Err(e) => {
                warn!("Failed to create metadata database: {}", e);
                panic!("Cannot create WalletManager without metadata database");
            }
        };

        // Initialize thread-safe wallet storage
        let wallets = Arc::new(Mutex::new(HashMap::new()));

        let mut manager = WalletManager {
            wallets: wallets.clone(),
            wallet_dir: wallet_dir.clone(),
            electrum_client,
            metadata_db,
            notification_sender,
            network,
            config: config.clone(),
        };

        // Load active wallets on startup (only wallets with active subscriptions)
        info!("Loading active wallets into memory...");
        let load_start = Instant::now();

        // Get wallets with active subscriptions from the database
        match manager.load_active_wallets().await {
            Ok(count) => {
                debug!(
                    "✅ Loaded {} active wallets into memory in {:?}",
                    count,
                    load_start.elapsed()
                );
            }
            Err(e) => {
                warn!("Failed to load wallets on startup: {}", e);
                info!("Wallets will be loaded on-demand during sync");
            }
        }

        manager
    }

    /// Get the network configuration used by all wallets
    pub fn get_network(&self) -> Network {
        self.network
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
            debug!(" Stripped key origin: {} -> {}", without_checksum, result);
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
        debug!(" Final normalized descriptor: {}", final_descriptor);

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

        debug!(" Receive descriptor: {}", receive_descriptor);
        debug!(" Change descriptor: {}", change_descriptor);

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
        wallets: Arc<Mutex<HashMap<String, Arc<Mutex<(PersistedWallet<Connection>, Connection)>>>>>,
    ) -> Result<()> {
        debug!(
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
                    debug!(
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
                        error!(
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

            if let Err(e) = client.full_scan_wallet(&mut wallet, custom_stop_gap).await {
                error!(
                    "[{}] Warning: Failed to full scan wallet during background creation: {}",
                    checksum, e
                );
            } else {
                // Persist after sync
                if let Err(e) = wallet.persist(&mut db) {
                    error!(
                        "[{}] Warning: Failed to persist wallet after sync: {}",
                        checksum, e
                    );
                }

                // Deep scanning for existing wallets with no funds (only if stop_gap is auto)
                if !is_fresh_wallet
                    && wallet.balance().total().to_sat() == 0
                    && stop_gap.unwrap_or("auto") == "auto"
                {
                    debug!(
                        "[{}] No funds found in initial scan, starting deep scan...",
                        checksum
                    );

                    // Deep scan in batches up to 500 addresses (only for auto mode)
                    for batch in 1..=5 {
                        let reveal_to = batch * 100;
                        debug!(
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

                        debug!(
                            "[{}] Revealed {} external, {} internal addresses (total: {} each)",
                            checksum,
                            ext_revealed.len(),
                            int_revealed.len(),
                            reveal_to + 1
                        );

                        // Sync the newly revealed addresses
                        if let Err(e) = client.sync_wallet(&mut wallet).await {
                            error!(
                                "[{}] Warning: Failed to sync during deep scan batch {}: {}",
                                checksum, batch, e
                            );
                            break;
                        }

                        // Check if we found any activity - if so, we should continue scanning
                        let current_balance = wallet.balance().total().to_sat();
                        if current_balance > 0 {
                            debug!(
                                "[{}] Found activity during deep scan! Balance: {} sats",
                                checksum, current_balance
                            );
                            // Continue scanning to find all transactions
                        }
                    }

                    // Final persistence after deep scanning
                    if let Err(e) = wallet.persist(&mut db) {
                        error!(
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
            error!(
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
            error!(
                "[{}] Warning: Failed to extract historical transactions: {}",
                checksum, e
            );
        }

        // Update last synced timestamp
        if let Err(e) = metadata_db.update_wallet_last_synced(&checksum).await {
            error!(
                "[{}] Warning: Failed to update wallet last synced: {}",
                checksum, e
            );
        }

        // Mark wallet as ready after deep scan and transaction extraction is complete
        if let Err(e) = metadata_db.update_wallet_status(&checksum, "ready").await {
            error!(
                "[{}] Warning: Failed to mark wallet as ready: {}",
                checksum, e
            );
        } else {
            debug!(
                "[{}] ✅ Wallet marked as ready - available for frontend display",
                checksum
            );
        }

        // Add wallet to in-memory storage after it's fully set up and marked as ready
        if let Ok((wallet, conn)) = Self::load_wallet_from_disk(&wallet_path, network).await {
            let mut wallets_map = wallets.lock().await;
            wallets_map.insert(checksum.clone(), Arc::new(Mutex::new((wallet, conn))));
            debug!(
                "[{}] Added newly created wallet to in-memory storage after full setup",
                checksum
            );
        } else {
            error!(
                "[{}] Failed to load wallet into memory after creation",
                checksum
            );
        }

        debug!(
            "[{}] Background wallet creation with scan depth completed",
            checksum
        );
        Ok(())
    }

    /// Clean up deleted wallets - remove from memory, disk, and database
    async fn cleanup_deleted_wallets(&mut self) -> Result<()> {
        use std::collections::HashSet;

        // Get ready wallets from database (source of truth)
        let ready_wallets = self.metadata_db.get_ready_wallets().await?;

        // Get wallets marked as deleted in database
        let deleted_wallets = self.metadata_db.get_deleted_wallets().await?;

        // Create set of valid checksums from database
        let valid_checksums: HashSet<String> =
            ready_wallets.iter().map(|w| w.checksum.clone()).collect();

        // Collect wallets that need to be removed
        let mut wallets_to_remove = Vec::new();

        // Check wallets in memory against valid checksums
        {
            let wallets_map = self.wallets.lock().await;
            for checksum in wallets_map.keys() {
                if !valid_checksums.contains(checksum) {
                    wallets_to_remove.push(checksum.clone());
                }
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
            debug!("Cleaning up wallet {} (deleted or expired)", checksum);

            // Remove from memory (if it was loaded)
            {
                let mut wallets_map = self.wallets.lock().await;
                if let Some(wallet_arc) = wallets_map.remove(&checksum) {
                    // Persist final state before removal
                    let mut wallet_data = wallet_arc.lock().await;
                    let (wallet, conn) = &mut *wallet_data;
                    if let Err(e) = wallet.persist(conn) {
                        warn!("Failed to persist wallet before removal: {}", e);
                    }
                    debug!("Removed wallet from memory");
                }
            }

            // Delete wallet file from disk (whether it was in memory or not)
            let wallet_filename = format!("{}.sqlite", checksum);
            let wallet_path = self.wallet_dir.join(&wallet_filename);
            if wallet_path.exists() {
                if let Err(e) = std::fs::remove_file(&wallet_path) {
                    error!(
                        "  Warning: Failed to delete wallet file {}: {}",
                        wallet_path.display(),
                        e
                    );
                } else {
                    debug!("Deleted wallet file: {}", wallet_path.display());
                }
            }

            // Hard delete from database (if it was marked as deleted)
            if let Err(e) = self
                .metadata_db
                .hard_delete_wallet_by_checksum(&checksum)
                .await
            {
                error!(
                    "  Warning: Failed to hard delete wallet {} from database: {}",
                    checksum, e
                );
            } else {
                debug!("Hard deleted wallet {} from database", checksum);
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

        let sync_start = Instant::now();

        // First, perform wallet cleanup (remove deleted wallets from memory and disk)
        self.cleanup_deleted_wallets().await?;

        // Convert Network to NetworkConfig for the query
        let network_config = NetworkConfig::from_network(self.network);

        // Get wallets for this tier from metadata
        let tier_wallets = self
            .metadata_db
            .get_wallets_for_tier_sync(&tier, &network_config)
            .await?;

        // Get summary of non-syncing wallets
        if let Ok(non_syncing_summary) = self.metadata_db.get_non_syncing_wallets_summary().await {
            if non_syncing_summary.total_non_syncing > 0 {
                let mut reasons = Vec::new();
                if non_syncing_summary.expired_trials > 0 {
                    reasons.push(format!("{} expired trials", non_syncing_summary.expired_trials));
                }
                if non_syncing_summary.cancelled_subscriptions > 0 {
                    reasons.push(format!("{} cancelled", non_syncing_summary.cancelled_subscriptions));
                }
                if non_syncing_summary.expired_subscriptions > 0 {
                    reasons.push(format!("{} expired", non_syncing_summary.expired_subscriptions));
                }
                if non_syncing_summary.past_due_subscriptions > 0 {
                    reasons.push(format!("{} past_due", non_syncing_summary.past_due_subscriptions));
                }
                if non_syncing_summary.inactive_wallets > 0 {
                    reasons.push(format!("{} inactive", non_syncing_summary.inactive_wallets));
                }

                info!(
                    "🔒 Subscription status: {} wallets not syncing ({})",
                    non_syncing_summary.total_non_syncing,
                    reasons.join(", ")
                );
            }
        }

        if tier_wallets.is_empty() {
            debug!("No {:?} tier wallets to sync", tier);
            return Ok(());
        }

        debug!(
            "🔄 Starting parallel sync for {} {:?} tier wallets (in-memory)",
            tier_wallets.len(),
            tier
        );

        // Ensure all tier wallets are loaded in memory
        {
            let wallets_map = self.wallets.lock().await;
            let mut missing_wallets = Vec::new();

            for wallet_metadata in &tier_wallets {
                if !wallets_map.contains_key(&wallet_metadata.checksum) {
                    missing_wallets.push(wallet_metadata.clone());
                }
            }

            drop(wallets_map); // Release lock before loading

            // Load any missing wallets into memory
            if !missing_wallets.is_empty() {
                info!(
                    " Loading {} missing wallets into memory",
                    missing_wallets.len()
                );
                for wallet_metadata in missing_wallets {
                    let wallet_path = self
                        .wallet_dir
                        .join(format!("{}.sqlite", wallet_metadata.checksum));
                    if wallet_path.exists() {
                        match Self::load_wallet_from_disk(&wallet_path, self.network).await {
                            Ok((wallet, conn)) => {
                                let mut wallets_map = self.wallets.lock().await;
                                wallets_map.insert(
                                    wallet_metadata.checksum.clone(),
                                    Arc::new(Mutex::new((wallet, conn))),
                                );
                                debug!(
                                    " Loaded wallet: {} ({})",
                                    wallet_metadata.name, wallet_metadata.checksum
                                );
                            }
                            Err(e) => {
                                error!(
                                    "Failed to load wallet {} into memory: {}",
                                    wallet_metadata.name, e
                                );
                            }
                        }
                    } else {
                        error!("Wallet file not found: {}", wallet_path.display());
                    }
                }
            }
        }

        // Get wallet references from in-memory storage
        let wallet_refs: Vec<_> = {
            let wallets_map = self.wallets.lock().await;

            tier_wallets
                .iter()
                .filter_map(|metadata| {
                    wallets_map
                        .get(&metadata.checksum)
                        .map(|wallet_arc| (metadata.clone(), wallet_arc.clone()))
                })
                .collect()
        };

        if wallet_refs.is_empty() {
            warn!("No wallets found in memory for sync");
            return Ok(());
        }

        // Create semaphore to limit concurrent syncs
        let semaphore = Arc::new(Semaphore::new(MAX_PARALLEL_SYNCS));

        // Prepare shared resources
        let metadata_db = self.metadata_db.clone();
        let notification_sender = self.notification_sender.clone();
        let electrum_client = self.electrum_client.clone();
        let config = self.config.clone();

        // Create parallel sync tasks using in-memory wallets
        let sync_tasks: Vec<_> = wallet_refs
            .into_iter()
            .map(|(wallet_metadata, wallet_arc)| {
                let semaphore = semaphore.clone();
                let metadata_db = metadata_db.clone();
                let notification_sender = notification_sender.clone();
                let electrum_client = electrum_client.clone();
                let config = config.clone();

                tokio::spawn(async move {
                    // Acquire semaphore permit
                    let _permit = semaphore
                        .acquire()
                        .await
                        .map_err(|e| anyhow!("Failed to acquire semaphore: {}", e))?;

                    let wallet_start = Instant::now();

                    // Lock the specific wallet for sync
                    let mut wallet_data = wallet_arc.lock().await;
                    let (wallet, conn) = &mut *wallet_data;

                    // Create sync service with config for mode-based retry logic
                    let sync_service =
                        WalletSyncService::new(metadata_db, notification_sender, config);

                    // Perform sync
                    match sync_service
                        .sync_wallet_by_checksum(
                            wallet,
                            &wallet_metadata.checksum,
                            electrum_client.as_ref(),
                        )
                        .await
                    {
                        Ok(_) => {
                            // Persist wallet changes back to disk (wallet stays in memory)
                            if let Err(e) = wallet.persist(conn) {
                                error!(
                                    "⚠️ Failed to persist wallet {} after sync: {}",
                                    wallet_metadata.name, e
                                );
                            }

                            let sync_duration = wallet_start.elapsed();
                            debug!(
                                "✅ Synced wallet {} in {:.2}s (from memory)",
                                wallet_metadata.name,
                                sync_duration.as_secs_f64()
                            );
                            Ok(wallet_metadata.name)
                        }
                        Err(e) => {
                            warn!("Failed to sync wallet {}: {}", wallet_metadata.name, e);
                            Err(e)
                        }
                    }
                })
            })
            .collect();

        // Wait for all sync tasks to complete
        let results = join_all(sync_tasks).await;

        // Count successes and failures
        let mut success_count = 0;
        let mut failure_count = 0;

        for result in results {
            match result {
                Ok(Ok(_)) => success_count += 1,
                _ => failure_count += 1,
            }
        }

        let total_duration = sync_start.elapsed();
        debug!(
            "🏁 Parallel in-memory sync completed in {:.2}s - Success: {}, Failed: {}",
            total_duration.as_secs_f64(),
            success_count,
            failure_count
        );

        Ok(())
    }

    /// Apply subscription tier limits by setting is_active status on wallets and contacts
    pub async fn apply_subscription_limits(
        &self,
        user_id: &str,
        tier: &str,
        subscription_status: &str,
        is_admin: bool,
        trial_ends_at: Option<String>,
    ) -> Result<(), anyhow::Error> {
        // Check if subscription has expired or failed payment
        let is_subscription_active = crate::saas::subscription::is_subscription_active(
            subscription_status,
            trial_ends_at.as_deref(),
        );

        if is_admin {
            tracing::info!("🎯 Applying unlimited limits for admin user {}", user_id);
        } else if !is_subscription_active {
            tracing::info!("🎯 Deactivating all wallets for user {} (status: {})", user_id, subscription_status);
        } else {
            tracing::info!("🎯 Applying {} tier limits for user {} (status: {})", tier, user_id, subscription_status);
        }

        // Get all wallets for this user ordered by creation time (oldest first)
        let wallets = self
            .metadata_db
            .get_wallets_for_user_oldest_first(user_id)
            .await?;

        // Determine wallet limit based on subscription status, tier, and admin status
        let wallet_limit = if is_admin {
            usize::MAX // Unlimited for admin
        } else if !is_subscription_active {
            0 // No active wallets for expired/past_due/canceled subscriptions
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
        } else if !is_subscription_active {
            0 // No active contacts for expired/past_due/canceled subscriptions
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

    /// Load all active wallets from disk into memory
    /// This is called on startup to pre-load wallets with active subscriptions
    async fn load_active_wallets(&mut self) -> Result<usize> {
        // Get ready wallets from the database (wallets with active subscriptions)
        let active_wallets = self.metadata_db.get_ready_wallets().await?;

        let mut loaded_count = 0;
        let mut wallets_map = self.wallets.lock().await;

        for wallet_metadata in active_wallets {
            let wallet_path = self
                .wallet_dir
                .join(format!("{}.sqlite", wallet_metadata.checksum));

            if wallet_path.exists() {
                match Self::load_wallet_from_disk(&wallet_path, self.network).await {
                    Ok((wallet, conn)) => {
                        wallets_map.insert(
                            wallet_metadata.checksum.clone(),
                            Arc::new(Mutex::new((wallet, conn))),
                        );
                        loaded_count += 1;
                        debug!(
                            " Loaded wallet: {} ({})",
                            wallet_metadata.name, wallet_metadata.checksum
                        );
                    }
                    Err(e) => {
                        warn!(
                            " Failed to load wallet {} from {}: {}",
                            wallet_metadata.name,
                            wallet_path.display(),
                            e
                        );
                    }
                }
            } else {
                warn!(
                    " Wallet file not found for {}: {}",
                    wallet_metadata.name,
                    wallet_path.display()
                );
            }
        }

        Ok(loaded_count)
    }

    /// Load a single wallet from disk
    /// Returns the wallet and its database connection
    async fn load_wallet_from_disk(
        wallet_path: &PathBuf,
        network: Network,
    ) -> Result<(PersistedWallet<Connection>, Connection)> {
        // Run blocking I/O in a separate thread
        let path = wallet_path.clone();
        let result = tokio::task::spawn_blocking(move || {
            let conn = Connection::open(&path)
                .map_err(|e| anyhow!("Failed to open wallet database: {}", e))?;

            let mut conn = conn;
            let wallet = Wallet::load()
                .extract_keys()
                .check_network(network)
                .load_wallet(&mut conn)
                .map_err(|e| anyhow!("Failed to load wallet from database: {}", e))?;

            let wallet_opt = wallet.ok_or_else(|| anyhow!("No wallet data found in file"))?;
            Ok::<(PersistedWallet<Connection>, Connection), anyhow::Error>((wallet_opt, conn))
        })
        .await??;

        Ok(result)
    }
}
