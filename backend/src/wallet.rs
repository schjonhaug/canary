use crate::config::AppConfig;
use crate::electrum::ElectrumClient;
use crate::metadata::{
    EventInsert, EventType, MetadataDb, TransactionEvent, WalletMetadata,
};
use crate::subscription::SubscriptionTier;
use anyhow::{anyhow, Result};
use bdk_wallet::rusqlite::Connection;
use bdk_wallet::{bitcoin::Network, PersistedWallet, Wallet, KeychainKind};
use miniscript::{Descriptor, DescriptorPublicKey};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use tokio::sync::broadcast;

pub struct WalletManager {
    pub wallets: Vec<(String, PersistedWallet<Connection>)>, // (checksum, wallet)
    pub wallet_dir: PathBuf,
    pub electrum_client: Option<ElectrumClient>,
    pub metadata_db: MetadataDb,
    pub event_sender: broadcast::Sender<TransactionEvent>,
    network: Network,
    // Sync tracking for periodic summaries
    sync_counter: u32,
    syncs_with_changes: u32,
    sync_errors: u32,
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
            Ok(db) => {
                db
            }
            Err(e) => {
                eprintln!("Warning: Failed to create metadata database: {}", e);
                panic!("Cannot create WalletManager without metadata database");
            }
        };

        let manager = WalletManager {
            wallets: Vec::new(),
            wallet_dir,
            electrum_client,
            metadata_db,
            event_sender,
            network,
            sync_counter: 0,
            syncs_with_changes: 0,
            sync_errors: 0,
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



    async fn load_all_wallets(&mut self) -> Result<()> {
        let start_time = Instant::now();
        
        // Get only ready wallets from database (source of truth)
        let ready_wallets = self.metadata_db.get_ready_wallets().await?;
        
        let wallets_before = self.wallets.len();
        let mut missing = 0;
        
        println!("⏱️ Loading {} ready wallets from disk...", ready_wallets.len());
        
        for wallet_metadata in ready_wallets {
            let wallet_path = self.wallet_dir.join(format!("{}.sqlite", wallet_metadata.checksum));
            
            if wallet_path.exists() {
                if let Err(e) = self.load_wallet_from_file(&wallet_path).await {
                    eprintln!(
                        "Warning: Failed to load wallet {} from {}: {}",
                        wallet_metadata.checksum,
                        wallet_path.display(),
                        e
                    );
                }
            } else {
                eprintln!(
                    "Warning: Wallet file missing for {} ({}). Expected at: {}",
                    wallet_metadata.name,
                    wallet_metadata.checksum,
                    wallet_path.display()
                );
                missing += 1;
            }
        }
        
        // Only log if new wallets were actually loaded from disk
        let newly_loaded = self.wallets.len() - wallets_before;
        let load_duration = start_time.elapsed();
        
        if newly_loaded > 0 {
            println!("📂 Loaded {} ready wallets from disk in {:?} (avg: {:?}/wallet)", 
                     newly_loaded, load_duration, load_duration / newly_loaded as u32);
        } else {
            println!("⏱️ Wallet loading check completed in {:?} (no new wallets)", load_duration);
        }
        
        if missing > 0 {
            eprintln!("⚠️  {} wallet files were missing", missing);
        }
        
        // Clean up expired sessions on startup
        match self.metadata_db.cleanup_expired_sessions().await {
            Ok(deleted) => {
                if deleted > 0 {
                    println!("Cleaned up {} expired sessions on startup", deleted);
                }
            }
            Err(e) => {
                eprintln!("Failed to cleanup expired sessions on startup: {}", e);
            }
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
                let filename = path.file_stem()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                
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
    /// Converts [fingerprint/path]xpub to just xpub, handles script-wrapped descriptors
    pub fn strip_key_origin(&self, descriptor_str: &str) -> Result<String> {
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

    /// Parse and validate multipath descriptor
    pub fn parse_multipath_descriptor(&self, descriptor_str: &str) -> Result<(String, String)> {
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

    pub async fn create_from_multipath(
        &mut self,
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

        // Check if input is an XPUB
        if XpubConverter::is_xpub(descriptor_str) && !is_fresh_wallet {
            // Check if script type was provided (fast path)
            if let Some(script_type_str) = script_type {
                if script_type_str != "auto" {
                    // Fast path: XPUB + known script type = skip probing
                    println!("Fast path: XPUB with known script type '{}'", script_type_str);
                    return self.create_from_xpub_with_known_type(name, descriptor_str, user_id, script_type_str, stop_gap).await;
                }
            }
            
            // Auto script type: need to probe for the correct script type
            return self.create_from_xpub_with_probing(name, descriptor_str, user_id, stop_gap).await;
        }

        // Strip key origin to prevent duplicate wallets with same XPUB
        let normalized_descriptor = self.strip_key_origin(descriptor_str)?;
        
        // Check if normalized descriptor already exists
        if self.metadata_db.descriptor_exists(&normalized_descriptor).await? {
            let checksum = self.metadata_db.extract_checksum(&normalized_descriptor);
            return Err(anyhow!("This wallet has already been added with ID: {}. Ask the wallet owner to add you as a contact for notifications.", checksum));
        }

        // Parse and validate the normalized multipath descriptor
        let (receive_descriptor, change_descriptor) =
            self.parse_multipath_descriptor(&normalized_descriptor)?;

        // Extract checksum from the normalized descriptor for consistent filename
        let checksum = self.metadata_db.extract_checksum(&normalized_descriptor);
        let wallet_filename_with_ext = format!("{}.sqlite", checksum);
        println!("[{}] Wallet filename: {}", checksum, wallet_filename_with_ext);

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
        println!("[{}] Metadata saved with checksum: {}", checksum, wallet_checksum);

        // Get wallet metadata to return immediately
        let wallet_metadata = self
            .metadata_db
            .get_wallet_by_descriptor(&normalized_descriptor)
            .await?
            .ok_or_else(|| anyhow!("Failed to retrieve created wallet metadata"))?;

        // PHASE 2: Spawn background task for slow operations
        let electrum_client_clone = self.electrum_client.clone();
        let metadata_db_clone = self.metadata_db.clone();
        let network = self.get_network();
        let checksum_clone = checksum.clone();
        let stop_gap_clone = stop_gap.map(|s| s.to_string());
        
        tokio::spawn(async move {
            if let Err(e) = Self::complete_wallet_creation_with_stop_gap(
                wallet_path,
                receive_descriptor,
                change_descriptor,
                network,
                electrum_client_clone,
                metadata_db_clone,
                checksum_clone,
                is_fresh_wallet,
                stop_gap_clone.as_deref(),
            ).await {
                eprintln!("[{}] Background wallet creation failed: {}", wallet_checksum, e);
            }
        });

        Ok(wallet_metadata)
    }

    /// Fast path: Create wallet from XPUB with known script type (skip probing)
    pub async fn create_from_xpub_with_known_type(
        &mut self,
        name: &str,
        xpub: &str,
        user_id: &str,
        script_type_str: &str,
        stop_gap: Option<&str>,
    ) -> Result<WalletMetadata> {
        use crate::xpub_converter::{XpubConverter, ScriptType};
        
        println!("Creating XPUB wallet with known script type: {}", script_type_str);
        
        // Parse script type
        let script_type = match script_type_str {
            "p2wpkh" => ScriptType::P2WPKH,
            "p2sh" => ScriptType::P2SH,
            "p2pkh" => ScriptType::P2PKH,
            "p2tr" => ScriptType::P2TR,
            _ => return Err(anyhow!("Invalid script type: {}", script_type_str)),
        };
        
        // Create converter and generate descriptor
        let network = self.get_network();
        let converter = XpubConverter::new(network, self.electrum_client.as_ref());
        let descriptor = converter.generate_descriptor_for_type(xpub, &script_type)?;
        
        println!("Generated descriptor: {}", descriptor);
        
        // Strip key origin for consistency
        let normalized_descriptor = self.strip_key_origin(&descriptor)?;
        
        // Check if this descriptor already exists
        if self.metadata_db.descriptor_exists(&normalized_descriptor).await? {
            let checksum = self.metadata_db.extract_checksum(&normalized_descriptor);
            return Err(anyhow!("This wallet has already been added with ID: {}. Ask the wallet owner to add you as a contact for notifications.", checksum));
        }
        
        // Parse multipath descriptor
        let (receive_descriptor, change_descriptor) = self.parse_multipath_descriptor(&normalized_descriptor)?;
        
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
        
        // Get wallet metadata to return immediately
        let wallet_metadata = self
            .metadata_db
            .get_wallet_by_descriptor(&normalized_descriptor)
            .await?
            .ok_or_else(|| anyhow!("Failed to retrieve created wallet metadata"))?;
        
        // Create wallet and apply scan depth
        let electrum_client_clone = self.electrum_client.clone();
        let metadata_db_clone = self.metadata_db.clone();
        let network = self.get_network();
        let checksum_clone = checksum.clone();
        let stop_gap_clone = stop_gap.map(|s| s.to_string());
        
        tokio::spawn(async move {
            if let Err(e) = Self::complete_wallet_creation_with_stop_gap(
                wallet_path,
                receive_descriptor,
                change_descriptor,
                network,
                electrum_client_clone,
                metadata_db_clone,
                checksum_clone,
                false, // not fresh wallet
                stop_gap_clone.as_deref(),
            ).await {
                eprintln!("[{}] Background wallet creation failed: {}", wallet_checksum, e);
            }
        });
        
        Ok(wallet_metadata)
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
        println!("[{}] Starting background wallet creation with stop gap: {:?}", checksum, stop_gap);
        
        // Create SQLite connection
        let mut db = Connection::open(&wallet_path)
            .map_err(|e| anyhow!("Failed to create connection to {}: {}", wallet_path.display(), e))?;
        
        // Parse descriptors
        let receive_desc: Descriptor<DescriptorPublicKey> = receive_descriptor.parse()
            .map_err(|e| anyhow!("Failed to parse receive descriptor: {}", e))?;
        let change_desc: Descriptor<DescriptorPublicKey> = change_descriptor.parse() 
            .map_err(|e| anyhow!("Failed to parse change descriptor: {}", e))?;
        
        // Create new wallet
        let mut wallet = Wallet::create(receive_desc, change_desc)
            .network(network)
            .create_wallet(&mut db)
            .map_err(|e| anyhow!("Failed to create wallet: {}", e))?;
        
        // Persist initial wallet state
        wallet.persist(&mut db)
            .map_err(|e| anyhow!("Failed to persist wallet: {}", e))?;
        
        // Apply stop gap if specified
        if let Some(stop_gap_str) = stop_gap {
            if stop_gap_str != "auto" {
                if let Ok(max_index) = stop_gap_str.parse::<u32>() {
                    println!("[{}] Applying custom stop gap: {} addresses", checksum, max_index);
                    
                    // Reveal addresses up to the specified index
                    let _external_addresses: Vec<_> = wallet.reveal_addresses_to(KeychainKind::External, max_index).collect();
                    let _internal_addresses: Vec<_> = wallet.reveal_addresses_to(KeychainKind::Internal, max_index).collect();
                    
                    // Persist the revealed addresses
                    if let Err(e) = wallet.persist(&mut db) {
                        eprintln!("[{}] Warning: Failed to persist revealed addresses: {}", checksum, e);
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
                eprintln!("[{}] Warning: Failed to full scan wallet during background creation: {}", checksum, e);
            } else {
                // Persist after sync
                if let Err(e) = wallet.persist(&mut db) {
                    eprintln!("[{}] Warning: Failed to persist wallet after sync: {}", checksum, e);
                }
                
                // Deep scanning for existing wallets with no funds (only if stop_gap is auto)
                if !is_fresh_wallet && wallet.balance().total().to_sat() == 0 && stop_gap.unwrap_or("auto") == "auto" {
                    println!("[{}] No funds found in initial scan, starting deep scan...", checksum);
                    
                    // Deep scan in batches up to 500 addresses (only for auto mode)
                    for batch in 1..=5 {
                        let reveal_to = batch * 100;
                        println!("[{}] Deep scan batch {}: checking addresses up to index {}", checksum, batch, reveal_to);
                        
                        // Reveal more addresses for both keychains
                        let ext_revealed: Vec<_> = wallet
                            .reveal_addresses_to(bdk_wallet::KeychainKind::External, reveal_to)
                            .collect();
                        let int_revealed: Vec<_> = wallet
                            .reveal_addresses_to(bdk_wallet::KeychainKind::Internal, reveal_to)
                            .collect();
                        
                        println!("[{}] Revealed {} external, {} internal addresses (total: {} each)", 
                                checksum, ext_revealed.len(), int_revealed.len(), reveal_to + 1);
                        
                        // Sync the newly revealed addresses
                        if let Err(e) = client.sync_wallet(&mut wallet) {
                            eprintln!("[{}] Warning: Failed to sync during deep scan batch {}: {}", checksum, batch, e);
                            break;
                        }
                        
                        // Check if we found any activity - if so, we should continue scanning
                        let current_balance = wallet.balance().total().to_sat();
                        if current_balance > 0 {
                            println!("[{}] Found activity during deep scan! Balance: {} sats", checksum, current_balance);
                            // Continue scanning to find all transactions
                        }
                    }
                    
                    // Final persistence after deep scanning
                    if let Err(e) = wallet.persist(&mut db) {
                        eprintln!("[{}] Warning: Failed to persist after deep scan: {}", checksum, e);
                    }
                }
            }
        }
        
        // Update balance in metadata database
        let balance = wallet.balance().total().to_sat() as i64;
        if let Err(e) = metadata_db.update_wallet_balance_by_checksum(&checksum, balance).await {
            eprintln!("[{}] Warning: Failed to update wallet balance: {}", checksum, e);
        }
        
        // Extract historical transactions after sync
        if let Err(e) = Self::extract_historical_transactions_for_background(
            &wallet, 
            &checksum, 
            &metadata_db,
            electrum_client.as_ref()
        ).await {
            eprintln!("[{}] Warning: Failed to extract historical transactions: {}", checksum, e);
        }

        // Update last synced timestamp
        if let Err(e) = metadata_db.update_wallet_last_synced(&checksum).await {
            eprintln!("[{}] Warning: Failed to update wallet last synced: {}", checksum, e);
        }
        
        // Mark wallet as ready after deep scan and transaction extraction is complete
        if let Err(e) = metadata_db.update_wallet_sync_status(&checksum, "ready").await {
            eprintln!("[{}] Warning: Failed to mark wallet as ready: {}", checksum, e);
        } else {
            println!("[{}] ✅ Wallet marked as ready - available for frontend display", checksum);
        }
        
        println!("[{}] Background wallet creation with scan depth completed", checksum);
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
                    bdk_wallet::chain::ChainPosition::Confirmed { anchor: anchor_a, .. },
                    bdk_wallet::chain::ChainPosition::Confirmed { anchor: anchor_b, .. },
                ) => anchor_a.block_id.height.cmp(&anchor_b.block_id.height),
                // Both unconfirmed: sort by first_seen timestamp if available
                (
                    bdk_wallet::chain::ChainPosition::Unconfirmed { first_seen: first_a, .. },
                    bdk_wallet::chain::ChainPosition::Unconfirmed { first_seen: first_b, .. },
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

        println!("[{}] Found {} historical transactions to process", wallet_checksum, all_transactions.len());

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

        // Collect all events first for batch insertion
        let mut events_to_insert = Vec::new();
        
        // Process each transaction chronologically
        for tx in all_transactions {
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

            let (event_type, amount_sats) = if net_amount > 0 {
                (EventType::Receive, net_amount)
            } else {
                (EventType::Send, net_amount.abs())
            };

            // Determine transaction timestamp
            let transaction_time = match &tx.chain_position {
                bdk_wallet::chain::ChainPosition::Confirmed { anchor, .. } => {
                    // Fetch actual block timestamp from Electrum
                    if let Some(electrum_client) = electrum_client {
                        match electrum_client.get_block_header(anchor.block_id.height) {
                            Ok(header) => header.timestamp,
                            Err(e) => {
                                eprintln!("[{}] Failed to fetch block header for height {}: {}", 
                                         wallet_checksum, anchor.block_id.height, e);
                                // Fallback to current time only if fetch fails
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs()
                            }
                        }
                    } else {
                        // No electrum client available, use current time as fallback
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs()
                    }
                }
                bdk_wallet::chain::ChainPosition::Unconfirmed { first_seen, .. } => {
                    first_seen.unwrap_or_else(|| {
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs()
                    })
                }
            };

            // Create event
            let event_insert = EventInsert {
                wallet_checksum: wallet_checksum.to_string(),
                event_type,
                amount_sats,
                is_confirmed,
                is_rbf: false,
                is_cpfp: false,
                balance_total: Some(running_balance),
                transaction_time,
            };

            // Collect event for batch insertion
            events_to_insert.push(event_insert);
        }
        
        // Batch insert all events
        if !events_to_insert.is_empty() {
            if let Err(e) = metadata_db.insert_events_batch(events_to_insert).await {
                eprintln!("[{}] Failed to batch insert historical events: {}", wallet_checksum, e);
            }
        }

        println!("[{}] Historical transaction extraction completed", wallet_checksum);
        Ok(())
    }

    pub async fn sync_wallet_by_checksum(&mut self, wallet_checksum: &str) -> Result<bool> {
        // Similar to sync_all_wallets but for a single wallet
        let metadata_db = &self.metadata_db;
        let event_sender = &self.event_sender;
        let mut has_changes = false;

        // Create wallet path for persistence (used later in persist function)

        if let Some((_, wallet)) = self
            .wallets
            .iter_mut()
            .find(|(checksum, _)| checksum == wallet_checksum)
        {
            // Extract electrum client reference before mutable operations
            let electrum_client = self.electrum_client.as_ref();
            
            // Get latest transaction timestamp for new events
            let latest_tx_timestamp = Self::get_latest_transaction_timestamp_static(electrum_client, wallet);
            // Get balance before sync
            let balance_before = wallet.balance();
            let trusted_pending_before = balance_before.trusted_pending;
            let untrusted_pending_before = balance_before.untrusted_pending;
            let confirmed_before = balance_before.confirmed;
            let total_before = balance_before.total();

            let unconfirmed_sends_before: Vec<(String, i64)> = wallet
                .transactions()
                .filter_map(|tx| {
                    if !tx.chain_position.is_confirmed() {
                        let sent = wallet.sent_and_received(&tx.tx_node).0;
                        let received = wallet.sent_and_received(&tx.tx_node).1;
                        let net_amount = received.to_sat() as i64 - sent.to_sat() as i64;
                        if net_amount < 0 {
                            Some((tx.tx_node.txid.to_string(), net_amount.abs()))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect();

            let unconfirmed_receives_before: Vec<(String, i64)> = wallet
                .transactions()
                .filter_map(|tx| {
                    if !tx.chain_position.is_confirmed() {
                        let sent = wallet.sent_and_received(&tx.tx_node).0;
                        let received = wallet.sent_and_received(&tx.tx_node).1;
                        let net_amount = received.to_sat() as i64 - sent.to_sat() as i64;
                        if net_amount > 0 {
                            Some((tx.tx_node.txid.to_string(), net_amount))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect();

            // Perform the sync
            if let Some(ref client) = self.electrum_client {
                let sync_result = client.sync_wallet(wallet);
                if let Err(e) = sync_result {
                    eprintln!("[{}] Failed to sync wallet: {}", wallet_checksum, e);
                    return Ok(false);
                }
            }

            // Update last_synced_at timestamp
            let _ = metadata_db.update_wallet_last_synced(wallet_checksum).await;

            // Get balance after sync
            let balance_after = wallet.balance();
            let trusted_pending_after = balance_after.trusted_pending;
            let untrusted_pending_after = balance_after.untrusted_pending;
            let confirmed_after = balance_after.confirmed;
            let total_after = balance_after.total();

            // Update balance in metadata
            metadata_db
                .update_wallet_balance_by_checksum(wallet_checksum, total_after.to_sat() as i64)
                .await?;

            // Check which sends have now been confirmed
            let mut total_confirmed_send_amount = 0i64;
            let mut confirmed_send_txid: Option<String> = None;
            for (txid, send_amount) in &unconfirmed_sends_before {
                // Check if this transaction is now confirmed
                if let Some(tx) = wallet
                    .transactions()
                    .find(|tx| tx.tx_node.txid.to_string() == *txid)
                {
                    if tx.chain_position.is_confirmed() {
                        total_confirmed_send_amount += send_amount;
                        confirmed_send_txid = Some(txid.clone());
                    }
                }
            }

            // Check which receives have now been confirmed
            let mut total_confirmed_receive_amount = 0i64;
            let mut confirmed_receive_txid: Option<String> = None;
            for (txid, receive_amount) in &unconfirmed_receives_before {
                // Check if this transaction is now confirmed
                if let Some(tx) = wallet
                    .transactions()
                    .find(|tx| tx.tx_node.txid.to_string() == *txid)
                {
                    if tx.chain_position.is_confirmed() {
                        total_confirmed_receive_amount += receive_amount;
                        confirmed_receive_txid = Some(txid.clone());
                    }
                }
            }

            // Check if any balance component changed
            has_changes = trusted_pending_before != trusted_pending_after
                || untrusted_pending_before != untrusted_pending_after
                || confirmed_before != confirmed_after
                || total_before != total_after;

            if has_changes {
                // Get the user-friendly wallet name (wallet_checksum is now the checksum)
                let wallet_metadata = metadata_db
                    .get_wallet_by_checksum(wallet_checksum)
                    .await
                    .expect(&format!(
                        "Failed to get wallet for checksum '{}'",
                        wallet_checksum
                    ))
                    .expect(&format!(
                        "Wallet with checksum '{}' should exist in metadata database",
                        wallet_checksum
                    ));
                let wallet_name = wallet_metadata.name;

                // Print debug table showing balance changes
                println!(); // Add blank line before wallet output
                println!("{:-<85}", "");
                println!(
                    "{:>22} | {:<18} | {:<18} | {:<18}",
                    format!("Wallet {}", wallet_name),
                    "Before",
                    "After",
                    "Diff"
                );
                println!("{:-<85}", "");
                let fmt = |amt: bdk_wallet::bitcoin::Amount| {
                    let btc = amt.to_sat() as f64 / 100_000_000.0;
                    if btc == 0.0 {
                        "".to_string()
                    } else {
                        format!("{:>13.8} BTC", btc)
                    }
                };
                let fmt_diff =
                    |before: bdk_wallet::bitcoin::Amount,
                     after: bdk_wallet::bitcoin::Amount| {
                        let diff_sats = after.to_sat() as i64 - before.to_sat() as i64;
                        let diff_btc = diff_sats as f64 / 100_000_000.0;
                        if diff_btc == 0.0 {
                            "".to_string()
                        } else {
                            format!("{:>+13.8} BTC", diff_btc)
                        }
                    };

                // Only print non-zero values
                if trusted_pending_before.to_sat() > 0 || trusted_pending_after.to_sat() > 0
                {
                    println!(
                        "{:>22} | {:<18} | {:<18} | {:<18}",
                        "Trusted pending",
                        fmt(trusted_pending_before),
                        fmt(trusted_pending_after),
                        fmt_diff(trusted_pending_before, trusted_pending_after)
                    );
                } else {
                    println!(
                        "{:>22} | {:<18} | {:<18} | {:<18}",
                        "Trusted pending", "", "", ""
                    );
                }
                if untrusted_pending_before.to_sat() > 0
                    || untrusted_pending_after.to_sat() > 0
                {
                    println!(
                        "{:>22} | {:<18} | {:<18} | {:<18}",
                        "Unconfirmed pending",
                        fmt(untrusted_pending_before),
                        fmt(untrusted_pending_after),
                        fmt_diff(untrusted_pending_before, untrusted_pending_after)
                    );
                } else {
                    println!(
                        "{:>22} | {:<18} | {:<18} | {:<18}",
                        "Unconfirmed pending", "", "", ""
                    );
                }
                if confirmed_before.to_sat() > 0 || confirmed_after.to_sat() > 0 {
                    println!(
                        "{:>22} | {:<18} | {:<18} | {:<18}",
                        "Confirmed",
                        fmt(confirmed_before),
                        fmt(confirmed_after),
                        fmt_diff(confirmed_before, confirmed_after)
                    );
                } else {
                    println!("{:>22} | {:<18} | {:<18} | {:<18}", "Confirmed", "", "", "");
                }

                // Add separator before Total
                println!("{:-<85}", "");
                println!(
                    "{:>22} | {:<18} | {:<18} | {:<18}",
                    "Total",
                    fmt(total_before),
                    fmt(total_after),
                    fmt_diff(total_before, total_after)
                );
                println!("{:-<85}", "");

                // Detect transaction types and broadcast events
                let trusted_pending_increase =
                    trusted_pending_after.to_sat() > trusted_pending_before.to_sat();
                let trusted_pending_decrease =
                    trusted_pending_after.to_sat() < trusted_pending_before.to_sat();
                let untrusted_pending_increase =
                    untrusted_pending_after.to_sat() > untrusted_pending_before.to_sat();
                let untrusted_pending_decrease =
                    untrusted_pending_after.to_sat() < untrusted_pending_before.to_sat();
                let confirmed_increase =
                    confirmed_after.to_sat() > confirmed_before.to_sat();
                let confirmed_decrease =
                    confirmed_after.to_sat() < confirmed_before.to_sat();
                let total_increase = total_after.to_sat() > total_before.to_sat();
                let total_decrease = total_after.to_sat() < total_before.to_sat();
                let total_same = total_after.to_sat() == total_before.to_sat();
                let confirmed_same = confirmed_after.to_sat() == confirmed_before.to_sat();

                // First check for special transaction types (takes precedence over regular sending)
                let mut is_special_tx = false;

                // Check for CPFP (Child-Pays-For-Parent)
                if !is_special_tx {
                    if untrusted_pending_decrease && confirmed_same && total_decrease {
                        let fee_paid = total_before.to_sat() - total_after.to_sat();
                        let message = format!("🚀 CPFP fee: {:.8} BTC", fee_paid as f64 / 100_000_000.0);
                        println!("[{}] {}", wallet_checksum, message);

                        // Insert CPFP event to database and broadcast
                        if let Err(e) = Self::insert_and_broadcast_event_helper(
                            metadata_db,
                            event_sender,
                            &EventInsert {
                                wallet_checksum: wallet_checksum.to_string(),
                                event_type: EventType::Send,
                                amount_sats: fee_paid as i64,
                                is_confirmed: false,
                                is_rbf: false,
                                is_cpfp: true,
                                balance_total: Some(total_after.to_sat() as i64),
                                transaction_time: latest_tx_timestamp,
                            },
                        )
                        .await
                        {
                            eprintln!("[{}] Failed to insert CPFP event: {}", wallet_checksum, e);
                        }

                        is_special_tx = true;
                    }
                }

                // Check if this might be RBF by looking for existing unconfirmed transactions
                let has_unconfirmed = wallet.transactions().any(|tx| {
                    matches!(
                        tx.chain_position,
                        bdk_wallet::chain::ChainPosition::Unconfirmed { .. }
                    )
                });

                // RBF detection: small amount change (just fee difference) with existing unconfirmed tx
                if has_unconfirmed && total_decrease && !is_special_tx {
                    let fee_increase = total_before.to_sat() - total_after.to_sat();

                    // RBF pattern: trusted pending decreases (spending from change) with existing unconfirmed
                    if trusted_pending_decrease && !confirmed_decrease {
                        let message = format!("📤 RBF fee bump: +{:.8} BTC", fee_increase as f64 / 100_000_000.0);
                        println!("[{}] {}", wallet_checksum, message);

                        // Insert RBF event to database and broadcast
                        if let Err(e) = Self::insert_and_broadcast_event_helper(
                            metadata_db,
                            event_sender,
                            &EventInsert {
                                wallet_checksum: wallet_checksum.to_string(),
                                event_type: EventType::Send,
                                amount_sats: fee_increase as i64,
                                is_confirmed: false,
                                is_rbf: true,
                                is_cpfp: false,
                                balance_total: Some(total_after.to_sat() as i64),
                                transaction_time: latest_tx_timestamp,
                            },
                        )
                        .await
                        {
                            eprintln!("[{}] Failed to insert RBF event: {}", wallet_checksum, e);
                        }
                    } else {
                        // Regular sending logic continues below
                        // Case 1: Spending from confirmed balance (first transaction)
                        if trusted_pending_increase && confirmed_decrease {
                            let confirmed_spent =
                                confirmed_before.to_sat() - confirmed_after.to_sat();
                            let change_received = trusted_pending_after.to_sat()
                                - trusted_pending_before.to_sat();
                            let sending_amount = confirmed_spent - change_received;

                            let message = format!("📤 Sending {:.8} BTC", sending_amount as f64 / 100_000_000.0);
                            println!("[{}] {}", wallet_checksum, message);

                            // Insert sending event to database and broadcast
                            if let Err(e) = Self::insert_and_broadcast_event_helper(
                                metadata_db,
                                event_sender,
                                &EventInsert {
                                    wallet_checksum: wallet_checksum.to_string(),
                                    event_type: EventType::Send,
                                    amount_sats: sending_amount as i64,
                                    is_confirmed: false,
                                    is_rbf: false,
                                    is_cpfp: false,
                                    balance_total: Some(total_after.to_sat() as i64),
                                    transaction_time: latest_tx_timestamp,
                                },
                            )
                            .await
                            {
                                eprintln!("[{}] Failed to insert sending event: {}", wallet_checksum, e);
                            }
                        }
                        // Case 2: Spending from trusted pending balance (subsequent transactions)
                        else if trusted_pending_decrease && confirmed_decrease {
                            let trusted_spent = trusted_pending_before.to_sat()
                                - trusted_pending_after.to_sat();
                            let confirmed_spent =
                                confirmed_before.to_sat() - confirmed_after.to_sat();
                            let total_spent = trusted_spent + confirmed_spent;

                            let message = format!("📤 Sending {:.8} BTC", total_spent as f64 / 100_000_000.0);
                            println!("[{}] {}", wallet_checksum, message);

                            // Insert sending event to database and broadcast
                            if let Err(e) = Self::insert_and_broadcast_event_helper(
                                metadata_db,
                                event_sender,
                                &EventInsert {
                                    wallet_checksum: wallet_checksum.to_string(),
                                    event_type: EventType::Send,
                                    amount_sats: total_spent as i64,
                                    is_confirmed: false,
                                    is_rbf: false,
                                    is_cpfp: false,
                                    balance_total: Some(total_after.to_sat() as i64),
                                    transaction_time: latest_tx_timestamp,
                                },
                            )
                            .await
                            {
                                eprintln!("[{}] Failed to insert sending event: {}", wallet_checksum, e);
                            }
                        }
                        // Case 3: Spending only from trusted pending (no confirmed funds used)
                        else if trusted_pending_decrease && !confirmed_decrease {
                            let trusted_spent = trusted_pending_before.to_sat()
                                - trusted_pending_after.to_sat();
                            let message = format!("📤 Sending {:.8} BTC", trusted_spent as f64 / 100_000_000.0);
                            println!("[{}] {}", wallet_checksum, message);

                            // Insert sending event to database and broadcast
                            if let Err(e) = Self::insert_and_broadcast_event_helper(
                                metadata_db,
                                event_sender,
                                &EventInsert {
                                    wallet_checksum: wallet_checksum.to_string(),
                                    event_type: EventType::Send,
                                    amount_sats: trusted_spent as i64,
                                    is_confirmed: false,
                                    is_rbf: false,
                                    is_cpfp: false,
                                    balance_total: Some(total_after.to_sat() as i64),
                                    transaction_time: latest_tx_timestamp,
                                },
                            )
                            .await
                            {
                                eprintln!("[{}] Failed to insert sending event: {}", wallet_checksum, e);
                            }
                        }
                    }
                } else if !is_special_tx {
                    // Regular sending logic (no existing unconfirmed transactions)
                    // Case 1: Spending from confirmed balance (first transaction)
                    if trusted_pending_increase && confirmed_decrease && total_decrease {
                        let confirmed_spent =
                            confirmed_before.to_sat() - confirmed_after.to_sat();
                        let change_received = trusted_pending_after.to_sat()
                            - trusted_pending_before.to_sat();
                        let sending_amount = confirmed_spent - change_received;

                        let message = format!("📤 Sending {:.8} BTC", sending_amount as f64 / 100_000_000.0);
                        println!("[{}] {}", wallet_checksum, message);

                        // Insert sending event to database and broadcast
                        if let Err(e) = Self::insert_and_broadcast_event_helper(
                            metadata_db,
                            event_sender,
                            &EventInsert {
                                wallet_checksum: wallet_checksum.to_string(),
                                event_type: EventType::Send,
                                amount_sats: sending_amount as i64,
                                is_confirmed: false,
                                is_rbf: false,
                                is_cpfp: false,
                                balance_total: Some(total_after.to_sat() as i64),
                                transaction_time: latest_tx_timestamp,
                            },
                        )
                        .await
                        {
                            eprintln!("Failed to insert sending event: {}", e);
                        }
                    }
                    // Case 2: Spending from trusted pending balance (subsequent transactions)
                    else if trusted_pending_decrease
                        && confirmed_decrease
                        && total_decrease
                    {
                        let trusted_spent = trusted_pending_before.to_sat()
                            - trusted_pending_after.to_sat();
                        let confirmed_spent =
                            confirmed_before.to_sat() - confirmed_after.to_sat();
                        let total_spent = trusted_spent + confirmed_spent;

                        let message = format!("📤 Sending {:.8} BTC", total_spent as f64 / 100_000_000.0);
                        println!("[{}] {}", wallet_checksum, message);

                        // Insert sending event to database and broadcast
                        if let Err(e) = Self::insert_and_broadcast_event_helper(
                            metadata_db,
                            event_sender,
                            &EventInsert {
                                wallet_checksum: wallet_checksum.to_string(),
                                event_type: EventType::Send,
                                amount_sats: total_spent as i64,
                                is_confirmed: false,
                                is_rbf: false,
                                is_cpfp: false,
                                balance_total: Some(total_after.to_sat() as i64),
                                transaction_time: latest_tx_timestamp,
                            },
                        )
                        .await
                        {
                            eprintln!("Failed to insert sending event: {}", e);
                        }
                    }
                    // Case 3: Spending only from trusted pending (no confirmed funds used)
                    else if trusted_pending_decrease
                        && !confirmed_decrease
                        && total_decrease
                    {
                        let trusted_spent = trusted_pending_before.to_sat()
                            - trusted_pending_after.to_sat();
                        let message = format!("📤 Sending {:.8} BTC", trusted_spent as f64 / 100_000_000.0);
                        println!("[{}] {}", wallet_checksum, message);

                        // Insert sending event to database and broadcast
                        if let Err(e) = Self::insert_and_broadcast_event_helper(
                            metadata_db,
                            event_sender,
                            &EventInsert {
                                wallet_checksum: wallet_checksum.to_string(),
                                event_type: EventType::Send,
                                amount_sats: trusted_spent as i64,
                                is_confirmed: false,
                                is_rbf: false,
                                is_cpfp: false,
                                balance_total: Some(total_after.to_sat() as i64),
                                transaction_time: latest_tx_timestamp,
                            },
                        )
                        .await
                        {
                            eprintln!("Failed to insert sending event: {}", e);
                        }
                    }
                }

                // Detect if this is a receiving transaction
                if untrusted_pending_increase && confirmed_same && total_increase {
                    let receiving_amount = untrusted_pending_after.to_sat()
                        - untrusted_pending_before.to_sat();
                    let message = format!("📥 Receiving {:.8} BTC", receiving_amount as f64 / 100_000_000.0);
                    println!("[{}] {}", wallet_checksum, message);

                    // Insert receiving event to database and broadcast
                    if let Err(e) = Self::insert_and_broadcast_event_helper(
                        metadata_db,
                        event_sender,
                        &EventInsert {
                            wallet_checksum: wallet_checksum.to_string(),
                            event_type: EventType::Receive,
                            amount_sats: receiving_amount as i64,
                            is_confirmed: false,
                            is_rbf: false,
                            is_cpfp: false,
                            balance_total: Some(total_after.to_sat() as i64),
                            transaction_time: latest_tx_timestamp,
                        },
                    )
                    .await
                    {
                        eprintln!("[{}] Failed to insert receiving event: {}", wallet_checksum, e);
                    }
                }

                // Detect if this is a sent transaction being confirmed
                if trusted_pending_decrease && confirmed_increase && total_same {
                    // Use transaction-level analysis result for send confirmation amount
                    if total_confirmed_send_amount > 0 {
                        let message = format!("✅ Sent confirmed: {:.8} BTC", total_confirmed_send_amount as f64 / 100_000_000.0);
                        println!("[{}] {}", wallet_checksum, message);

                        // Get the proper transaction timestamp
                        let transaction_time = if let Some(ref txid) = confirmed_send_txid {
                            Self::get_transaction_timestamp_static(electrum_client, wallet, txid)
                        } else {
                            latest_tx_timestamp
                        };

                        // Insert sent confirmation event to database and broadcast
                        if let Err(e) = Self::insert_and_broadcast_event_helper(
                            metadata_db,
                            event_sender,
                            &EventInsert {
                                wallet_checksum: wallet_checksum.to_string(),
                                event_type: EventType::Send,
                                amount_sats: total_confirmed_send_amount,
                                is_confirmed: true,
                                is_rbf: false,
                                is_cpfp: false,
                                balance_total: Some(total_after.to_sat() as i64),
                                transaction_time,
                            },
                        )
                        .await
                        {
                            eprintln!("[{}] Failed to insert sent confirmation event: {}", wallet_checksum, e);
                        }
                    }
                }

                // Detect if this is a received transaction being confirmed
                if untrusted_pending_decrease && confirmed_increase && total_same {
                    // Use transaction-level analysis result for receive confirmation amount
                    if total_confirmed_receive_amount > 0 {
                        let message = format!("✅ Received confirmed: {:.8} BTC", total_confirmed_receive_amount as f64 / 100_000_000.0);
                        println!("[{}] {}", wallet_checksum, message);

                        // Get the proper transaction timestamp
                        let transaction_time = if let Some(ref txid) = confirmed_receive_txid {
                            Self::get_transaction_timestamp_static(electrum_client, wallet, txid)
                        } else {
                            latest_tx_timestamp
                        };

                        // Insert received confirmation event to database and broadcast
                        if let Err(e) = Self::insert_and_broadcast_event_helper(
                            metadata_db,
                            event_sender,
                            &EventInsert {
                                wallet_checksum: wallet_checksum.to_string(),
                                event_type: EventType::Receive,
                                amount_sats: total_confirmed_receive_amount,
                                is_confirmed: true,
                                is_rbf: false,
                                is_cpfp: false,
                                balance_total: Some(total_after.to_sat() as i64),
                                transaction_time,
                            },
                        )
                        .await
                        {
                            eprintln!("[{}] Failed to insert received confirmation event: {}", wallet_checksum, e);
                        }
                    }
                }

                println!(); // Add spacing between wallets
            }
        }

        // Persist wallet changes to the database after sync and event processing
        self.persist_wallet_by_checksum(wallet_checksum).await?;

        Ok(has_changes)
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

    pub async fn sync_wallets_due_for_sync(&mut self) -> Result<()> {
        let sync_start_time = Instant::now();
        
        // First, check for expired subscriptions and downgrade users
        if let Err(e) = self.process_expired_subscriptions().await {
            tracing::error!("Failed to process expired subscriptions: {}", e);
        }

        // Get wallets that are due for sync based on their owner's tier
        let due_wallets = self.metadata_db.get_wallets_due_for_sync().await?;

        if due_wallets.is_empty() {
            return Ok(());
        }

        // Count wallets by tier for summary
        let mut personal_count = 0;
        let mut team_count = 0;
        for (_, tier) in &due_wallets {
            match tier {
                SubscriptionTier::Personal => personal_count += 1,
                SubscriptionTier::Team => team_count += 1,
            }
        }

        let _tier_summary = if personal_count > 0 && team_count > 0 {
            format!("{}P/{}T", personal_count, team_count)
        } else if personal_count > 0 {
            format!("{}P", personal_count)
        } else {
            format!("{}T", team_count)
        };

        println!("🔄 Starting sync cycle for {} wallets ({}P/{}T)", 
                 due_wallets.len(), personal_count, team_count);

        // Ensure all wallets are loaded first
        let load_start = Instant::now();
        if let Err(e) = self.load_all_wallets().await {
            eprintln!("Failed to load wallets: {}", e);
            return Ok(());
        }
        let load_duration = load_start.elapsed();
        println!("⏱️ Wallet loading phase completed in {:?}", load_duration);

        let mut _synced = 0;
        let mut failed = 0;
        let mut had_changes = false;

        let sync_wallets_start = Instant::now();
        
        // For now, process wallets in parallel by spawning concurrent tasks
        // Note: This is still limited by the fact that each wallet sync needs mutable access to self
        // Future improvement: extract sync logic to avoid mutable self dependency
        
        println!("🔄 Starting sequential sync of {} wallets", due_wallets.len());
        
        // Create a vector to store sync tasks - but for now process in batches to avoid mutable borrow issues
        const MAX_CONCURRENT: usize = 3; // Limit concurrent syncs to avoid overwhelming the system
        
        for batch in due_wallets.chunks(MAX_CONCURRENT) {
            let batch_start = Instant::now();
            // Remove unused sync_futures vector for now
            
            // Create futures for each wallet sync in this batch
            for (wallet_metadata, _tier) in batch.iter() {
                let checksum = wallet_metadata.checksum.clone();
                let name = wallet_metadata.name.clone();
                
                // For now, we still need to sync sequentially due to mutable borrow issues
                // TODO: Refactor sync logic to be truly parallel
                let wallet_sync_start = Instant::now();
                match self.sync_wallet_by_checksum(&checksum).await {
                    Ok(wallet_had_changes) => {
                        let wallet_sync_duration = wallet_sync_start.elapsed();
                        println!("  ⏱️ Synced {} in {:?} {}", 
                                 checksum, 
                                 wallet_sync_duration,
                                 if wallet_had_changes { "(had changes)" } else { "(no changes)" });
                        
                        _synced += 1;
                        if wallet_had_changes {
                            had_changes = true;
                        }
                    }
                    Err(e) => {
                        failed += 1;
                        eprintln!("  [{}] ❌ Failed to sync {} in {:?}: {}", 
                                 checksum, name, wallet_sync_start.elapsed(), e);
                    }
                }
            }
            
            let batch_duration = batch_start.elapsed();
            println!("✅ Processed batch of {} wallets in {:?}", batch.len(), batch_duration);
        }
        
        let sync_wallets_duration = sync_wallets_start.elapsed();
        let total_sync_duration = sync_start_time.elapsed();
        println!("⏱️ Wallet sync phase completed in {:?} (total cycle: {:?})", 
                 sync_wallets_duration, total_sync_duration);

        // Update counters
        self.sync_counter += 1;
        self.sync_errors += failed;
        if had_changes {
            self.syncs_with_changes += 1;
        }
        
        // Show periodic summary every 10 sync cycles
        if self.sync_counter % 10 == 0 {
            println!(
                "📊 Sync summary: {} cycles completed, {} with changes, {} errors",
                self.sync_counter, self.syncs_with_changes, self.sync_errors
            );
        }

        Ok(())
    }

    /// Process expired subscriptions and mark users as expired (but keep their tier)
    async fn process_expired_subscriptions(&mut self) -> Result<()> {
        match self.metadata_db.process_expired_subscriptions().await {
            Ok(count) if count > 0 => {
                tracing::info!("📉 Processed {} expired subscriptions", count);
            }
            Ok(_) => {
                // No expired subscriptions to process (normal case)
            }
            Err(e) => {
                tracing::error!("Failed to process expired subscriptions: {}", e);
                return Err(e);
            }
        }
        Ok(())
    }


    pub async fn get_wallet_by_checksum(&self, checksum: &str) -> Result<Option<WalletMetadata>> {
        self.metadata_db
            .get_wallet_by_checksum(checksum)
            .await
            .map_err(|e| anyhow!("Failed to get wallet by checksum: {}", e))
    }

    pub async fn delete_wallet_by_checksum(&mut self, checksum: &str) -> Result<()> {
        println!("[{}] Deleting wallet", checksum);

        // Get the descriptor and filename for this wallet checksum and delete from metadata
        let (descriptor, wallet_filename) =
            match self.metadata_db.delete_wallet_by_checksum(checksum).await? {
                Some((desc, filename)) => (desc, filename),
                None => return Err(anyhow!("Wallet not found")),
            };

        println!("[{}] Found descriptor: {}", checksum, descriptor);
        println!("[{}] Wallet filename: {}", checksum, wallet_filename);

        // Find and remove wallet from in-memory manager (checksum is the key now)
        let wallet_index = self
            .wallets
            .iter()
            .position(|(stored_checksum, _)| stored_checksum == checksum);

        if let Some(index) = wallet_index {
            // Remove wallet from in-memory storage (this unloads it from BDK)
            self.wallets.remove(index);
            println!("[{}] Unloaded wallet from memory", checksum);
        } else {
            println!("[{}] Warning: Wallet not found in memory (may have been manually removed)", checksum);
        }

        // Delete wallet database file from disk
        let wallet_path = self.wallet_dir.join(&wallet_filename);
        if wallet_path.exists() {
            fs::remove_file(&wallet_path).map_err(|e| {
                anyhow!(
                    "Failed to delete wallet file {}: {}",
                    wallet_path.display(),
                    e
                )
            })?;
            println!("[{}] Deleted wallet file: {}", checksum, wallet_path.display());
        } else {
            println!(
                "[{}] Warning: Wallet file not found on disk: {}",
                checksum, wallet_path.display()
            );
        }

        println!("[{}] Wallet deletion completed successfully", checksum);

        Ok(())
    }

    pub async fn update_wallet(&self, checksum: &str, name: &str) -> Result<()> {
        println!("[{}] Updating wallet", checksum);

        // Update wallet name in metadata database
        let updated = self
            .metadata_db
            .update_wallet_by_checksum(checksum, name)
            .await?;
        if !updated {
            return Err(anyhow!("Wallet not found"));
        }

        println!("[{}] Updated wallet name to: {}", checksum, name);

        Ok(())
    }

    /// Helper function to get current timestamp
    fn get_current_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    /// Helper function to get transaction timestamp from BDK transaction data
    fn get_transaction_timestamp_static(
        electrum_client: Option<&crate::electrum::ElectrumClient>,
        wallet: &PersistedWallet<Connection>,
        txid: &str,
    ) -> u64 {
        // Find the transaction in the wallet
        if let Some(tx) = wallet.transactions().find(|tx| tx.tx_node.txid.to_string() == txid) {
            match &tx.chain_position {
                bdk_wallet::chain::ChainPosition::Confirmed { anchor, .. } => {
                    // For confirmed transactions, fetch block timestamp from Electrum
                    if let Some(electrum_client) = electrum_client {
                        match electrum_client.get_block_header(anchor.block_id.height) {
                            Ok(header) => header.timestamp,
                            Err(_) => {
                                // Fallback to current time if we can't fetch block header
                                Self::get_current_timestamp()
                            }
                        }
                    } else {
                        // No Electrum client, use current time as fallback
                        Self::get_current_timestamp()
                    }
                }
                bdk_wallet::chain::ChainPosition::Unconfirmed { first_seen, .. } => {
                    // For unconfirmed transactions, use first_seen if available
                    first_seen.unwrap_or_else(|| Self::get_current_timestamp())
                }
            }
        } else {
            // Transaction not found, use current time as fallback
            Self::get_current_timestamp()
        }
    }

    /// Helper function to find the most recent transaction that could have caused a balance change
    fn get_latest_transaction_timestamp_static(
        electrum_client: Option<&crate::electrum::ElectrumClient>,
        wallet: &PersistedWallet<Connection>,
    ) -> u64 {
        // Find the most recent transaction (by timestamp) that affects our balance
        let mut latest_timestamp = 0u64;
        
        for tx in wallet.transactions() {
            let sent = wallet.sent_and_received(&tx.tx_node).0;
            let received = wallet.sent_and_received(&tx.tx_node).1;
            let net_amount = received.to_sat() as i64 - sent.to_sat() as i64;
            
            // Skip transactions that don't affect our balance
            if net_amount == 0 {
                continue;
            }
            
            let tx_timestamp = match &tx.chain_position {
                bdk_wallet::chain::ChainPosition::Confirmed { anchor, .. } => {
                    // For confirmed transactions, get block timestamp
                    if let Some(electrum_client) = electrum_client {
                        if let Ok(header) = electrum_client.get_block_header(anchor.block_id.height) {
                            header.timestamp
                        } else {
                            continue; // Skip if we can't get block header
                        }
                    } else {
                        continue; // Skip if no electrum client
                    }
                }
                bdk_wallet::chain::ChainPosition::Unconfirmed { first_seen, .. } => {
                    // For unconfirmed transactions, use first_seen timestamp
                    first_seen.unwrap_or_else(|| Self::get_current_timestamp())
                }
            };
            
            // Keep track of the latest transaction timestamp
            if tx_timestamp > latest_timestamp {
                latest_timestamp = tx_timestamp;
            }
        }
        
        // If no transactions found or no valid timestamps, use current time
        if latest_timestamp == 0 {
            latest_timestamp = Self::get_current_timestamp();
        }
        
        latest_timestamp
    }

    /// Helper function to insert event and broadcast it
    async fn insert_and_broadcast_event_helper(
        metadata_db: &MetadataDb,
        event_sender: &broadcast::Sender<TransactionEvent>,
        event_insert: &EventInsert,
    ) -> Result<()> {
        // Insert to database and get the generated event ID
        let event_id = metadata_db.insert_event(event_insert).await?;

        // Create event for broadcasting using the database event ID
        let event = TransactionEvent {
            id: Some(event_id),
            wallet_checksum: event_insert.wallet_checksum.clone(),
            event_type: event_insert.event_type.clone(),
            amount_sats: event_insert.amount_sats,
            is_confirmed: event_insert.is_confirmed,
            is_rbf: event_insert.is_rbf,
            is_cpfp: event_insert.is_cpfp,
            balance_total: event_insert.balance_total,
            transaction_time: event_insert.transaction_time,
            notification_status: Vec::new(),
        };

        // Broadcast event
        let _ = event_sender.send(event);
        Ok(())
    }

    /// Apply subscription tier limits by setting is_active status on wallets and contacts
    pub async fn apply_subscription_limits(
        &self,
        user_id: &str,
        tier: &str,
        is_admin: bool,
    ) -> Result<()> {
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

        // Update wallet active status
        for (index, wallet) in wallets.iter().enumerate() {
            let should_be_active = index < wallet_limit;

            if let Err(e) = self
                .metadata_db
                .update_wallet_active_status(&wallet.checksum, should_be_active)
                .await
            {
                tracing::error!(
                    "Failed to update wallet {} active status: {}",
                    wallet.checksum,
                    e
                );
            } else if !should_be_active {
                tracing::info!(
                    "📵 Deactivated wallet '{}' (#{}) - exceeds {} tier limit",
                    wallet.name,
                    index + 1,
                    tier
                );
            }
        }

        // Handle contacts for each wallet
        for wallet in &wallets {
            let contacts = self
                .metadata_db
                .get_contacts_oldest_first_for_limits(&wallet.checksum)
                .await?;

            // Determine contact limit based on tier or admin status
            let contact_limit = if is_admin {
                usize::MAX // Unlimited for admin
            } else {
                match tier {
                    "personal" => 1,
                    "team" => 5,
                    _ => 1, // Default to personal limits
                }
            };

            for (index, contact) in contacts.iter().enumerate() {
                let within_count_limit = index < contact_limit;

                let should_be_active = within_count_limit;

                if let Some(contact_id) = &contact.id {
                    tracing::debug!("🔍 Contact '{}' (index: {}, created_at: {:?}) - within_limit: {}, should_be_active: {}", 
                        contact.name, index, contact.created_at, within_count_limit, should_be_active);

                    if let Err(e) = self
                        .metadata_db
                        .update_contact_active_status(contact_id, should_be_active)
                        .await
                    {
                        tracing::error!(
                            "Failed to update contact {} active status: {}",
                            contact_id,
                            e
                        );
                    } else if !should_be_active {
                        let reason =
                            format!("exceeds {} tier limit of {} contacts", tier, contact_limit);
                        tracing::info!(
                            "📵 Deactivated contact '{}' in wallet '{}' - {}",
                            contact.name,
                            wallet.name,
                            reason
                        );
                    } else {
                        tracing::info!(
                            "✅ Activated contact '{}' in wallet '{}' (within {} limit)",
                            contact.name,
                            wallet.name,
                            tier
                        );
                    }
                }
            }
        }

        tracing::info!(
            "✅ Applied {} tier limits: {} wallets, checking contacts per wallet",
            tier,
            wallet_limit
        );
        Ok(())
    }

    /// Create wallet from XPUB with intelligent script type probing
    pub async fn create_from_xpub_with_probing(
        &mut self,
        name: &str,
        xpub: &str,
        user_id: &str,
        stop_gap: Option<&str>,
    ) -> Result<WalletMetadata> {
        use crate::xpub_converter::{XpubConverter, ScriptType};
        use std::collections::HashMap;
        
        // Create meaningful probe ID using wallet name + XPUB prefix
        let xpub_prefix = if xpub.len() >= 10 { &xpub[..10] } else { xpub };
        let probe_id = format!("{}_{}", name, xpub_prefix);
        
        println!("[{}] Creating wallet from XPUB with intelligent script type probing", probe_id);
        println!("[{}] Name: {}", probe_id, name);
        println!("[{}] XPUB: {}", probe_id, xpub);

        // Create XPUB converter
        let network = self.get_network();
        let converter = XpubConverter::new(network, self.electrum_client.as_ref());

        // Script types to try, in popularity order
        let script_types = [
            ScriptType::P2WPKH,  // Native SegWit ~60% (most popular)
            ScriptType::P2SH,    // Nested SegWit ~25%
            ScriptType::P2PKH,   // Legacy ~15%
            ScriptType::P2TR,    // Taproot (future-proofing)
        ];

        let mut probe_wallets: HashMap<ScriptType, (PersistedWallet<Connection>, Connection, String, String)> = HashMap::new(); // (wallet, db, path, descriptor)
        let mut winning_wallet: Option<(ScriptType, String)> = None; // (script_type, temp_wallet_path)

        println!("[{}] === Phase 1: Quick Script Type Detection ===", probe_id);
        
        // Try each script type with limited scanning
        for script_type in script_types {
            println!("[{}] 🔍 Trying {} ({:?})...", probe_id, script_type.as_str(), script_type);
            
            // Generate descriptor for this script type
            let descriptor = match converter.generate_descriptor_for_type(xpub, &script_type) {
                Ok(desc) => desc,
                Err(e) => {
                    println!("[{}] ❌ Failed to generate descriptor for {:?}: {}", probe_id, script_type, e);
                    continue;
                }
            };

            // Parse multipath descriptor
            let (receive_descriptor, change_descriptor) = match self.parse_multipath_descriptor(&descriptor) {
                Ok((recv, change)) => (recv, change),
                Err(e) => {
                    println!("[{}] ❌ Failed to parse descriptor for {:?}: {}", probe_id, script_type, e);
                    continue;
                }
            };

            // Create in-memory BDK wallet
            let receive_desc: Descriptor<DescriptorPublicKey> = match receive_descriptor.parse() {
                Ok(desc) => desc,
                Err(e) => {
                    println!("[{}] ❌ Failed to parse receive descriptor for {:?}: {}", probe_id, script_type, e);
                    continue;
                }
            };
            let change_desc: Descriptor<DescriptorPublicKey> = match change_descriptor.parse() {
                Ok(desc) => desc,
                Err(e) => {
                    println!("[{}] ❌ Failed to parse change descriptor for {:?}: {}", probe_id, script_type, e);
                    continue;
                }
            };

            // Create persistent wallet in /tmp for probing
            let temp_wallet_path = format!("/tmp/canary_probe_{}_{:?}.sqlite", probe_id, script_type);
            let mut db = match Connection::open(&temp_wallet_path) {
                Ok(connection) => connection,
                Err(e) => {
                    println!("[{}] ❌ Failed to create database at {}: {}", probe_id, temp_wallet_path, e);
                    continue;
                }
            };

            let temp_wallet = match Wallet::create(receive_desc, change_desc)
                .network(network)
                .create_wallet(&mut db)
            {
                Ok(wallet) => wallet,
                Err(e) => {
                    println!("[{}] ❌ Failed to create persistent wallet for {:?}: {}", probe_id, script_type, e);
                    let _ = std::fs::remove_file(&temp_wallet_path); // Clean up
                    continue;
                }
            };

            // Store probe wallet with its database connection and path
            probe_wallets.insert(script_type, (temp_wallet, db, temp_wallet_path.clone(), descriptor.clone()));
            
            // Quick scan with limited addresses (50)
            let (temp_wallet, temp_db, temp_path, _) = probe_wallets.get_mut(&script_type).unwrap();
            if let Some(ref electrum_client) = self.electrum_client {
                // Scan with small stop gap for quick detection
                let request = temp_wallet.start_full_scan();
                match electrum_client.client.full_scan(request, 10, 50, false) {
                    Ok(update) => {
                        if let Err(e) = temp_wallet.apply_update(update) {
                            println!("[{}] ❌ Failed to apply update for {:?}: {}", probe_id, script_type, e);
                            continue;
                        }
                        
                        // Persist the updated wallet
                        if let Err(e) = temp_wallet.persist(temp_db) {
                            println!("[{}] ❌ Failed to persist wallet for {:?}: {}", probe_id, script_type, e);
                        }
                        
                        // Check for any activity
                        let has_transactions = temp_wallet.transactions().count() > 0;
                        let has_balance = temp_wallet.balance().total().to_sat() > 0;
                        
                        if has_transactions || has_balance {
                            println!("[{}] ✅ Found activity! Script type: {}", probe_id, script_type.as_str());
                            println!("[{}] Transactions: {}", probe_id, temp_wallet.transactions().count());
                            println!("[{}] Balance: {} sats", probe_id, temp_wallet.balance().total());
                            
                            // Winner found!
                            winning_wallet = Some((script_type, temp_path.clone()));
                            break;
                        } else {
                            println!("[{}] ⚪ No activity found in first 50 addresses", probe_id);
                        }
                    }
                    Err(e) => {
                        println!("[{}] ❌ Failed to scan for {:?}: {}", probe_id, script_type, e);
                        continue;
                    }
                }
            }
        }

        // No deep scanning during probing - scan depth will be applied after winner is found

        // Determine final descriptor and handle winning wallet
        let final_descriptor = if let Some((winning_script_type, _winning_temp_path)) = &winning_wallet {
            // Get the descriptor from the winning wallet
            let (_, _, _, descriptor) = probe_wallets.get(winning_script_type).unwrap();
            println!("[{}] ✅ Winner found: {} with activity!", probe_id, winning_script_type.as_str());
            descriptor.clone()
        } else {
            // No activity found anywhere - create fresh P2WPKH wallet
            println!("[{}] ⚠️ No activity found in any script type. Creating fresh P2WPKH wallet.", probe_id);
            converter.generate_descriptor_for_type(xpub, &ScriptType::P2WPKH)?
        };

        // Strip key origin from final descriptor for consistency
        let normalized_descriptor = self.strip_key_origin(&final_descriptor)?;
        
        // Check if this wallet already exists
        if self.metadata_db.descriptor_exists(&normalized_descriptor).await? {
            let checksum = self.metadata_db.extract_checksum(&normalized_descriptor);
            return Err(anyhow!("This wallet has already been added with ID: {}. Ask the wallet owner to add you as a contact for notifications.", checksum));
        }

        // Extract checksum and create metadata
        let checksum = self.metadata_db.extract_checksum(&normalized_descriptor);
        
        // Show transition from probe ID to real wallet checksum
        println!("[{}] → [{}] Script type determined, using wallet checksum", probe_id, checksum);
        
        let _wallet_checksum = self
            .metadata_db
            .insert_wallet(name, &normalized_descriptor, user_id)
            .await?;
        
        println!("[{}] ✅ Wallet metadata saved", checksum);

        // Handle winning wallet: move from /tmp to final location and clean up
        if let Some((_winning_script_type, winning_temp_path)) = &winning_wallet {
            // Move winning wallet from /tmp to final location
            let final_wallet_path = self.wallet_dir.join(format!("{}.sqlite", checksum));
            
            println!("[{}] Moving winning wallet from {} to {}", checksum, winning_temp_path, final_wallet_path.display());
            if let Err(e) = std::fs::rename(winning_temp_path, &final_wallet_path) {
                println!("[{}] ❌ Failed to move winning wallet: {}", checksum, e);
                // Clean up all probe wallets on error
                for (_, (_, _, path, _)) in probe_wallets {
                    let _ = std::fs::remove_file(path);
                }
                return Err(anyhow!("Failed to move winning wallet to final location: {}", e));
            }
            
            println!("[{}] ✅ Winning wallet moved to final location", checksum);
        }
        
        // Clean up non-winning wallets from /tmp
        for (script_type, (_, _, path, _)) in probe_wallets {
            // Skip the winning wallet (already moved)
            if let Some((winning_script_type, _)) = &winning_wallet {
                if script_type == *winning_script_type {
                    continue;
                }
            }
            
            if let Err(e) = std::fs::remove_file(&path) {
                println!("[{}] ⚠️ Failed to clean up probe wallet {}: {}", checksum, path, e);
            } else {
                println!("[{}] 🗑️ Cleaned up probe wallet: {}", checksum, path);
            }
        }

        // Get wallet metadata to return
        let wallet_metadata = self
            .metadata_db
            .get_wallet_by_descriptor(&normalized_descriptor)
            .await?
            .ok_or_else(|| anyhow!("Failed to retrieve created wallet metadata"))?;

        // If we have a winning wallet, apply scan depth and extract transactions before marking ready
        if winning_wallet.is_some() {
            println!("[{}] Processing winning wallet with stop gap: {:?}", checksum, stop_gap);
            
            // Load the wallet from its final location
            let wallet_path = self.wallet_dir.join(format!("{}.sqlite", checksum));
            let mut db = Connection::open(&wallet_path)
                .map_err(|e| anyhow!("Failed to open wallet database at {}: {}", wallet_path.display(), e))?;
            
            let mut wallet = Wallet::load()
                .extract_keys()
                .check_network(self.get_network())
                .load_wallet(&mut db)
                .map_err(|e| anyhow!("Failed to load wallet: {}", e))?
                .ok_or_else(|| anyhow!("Wallet not found in database"))?;
            
            // Apply stop gap if specified (this is where we solve the original problem!)
            if let Some(stop_gap_str) = stop_gap {
                if stop_gap_str != "auto" {
                    if let Ok(max_index) = stop_gap_str.parse::<u32>() {
                        println!("[{}] Applying custom stop gap: {} addresses", checksum, max_index);
                        
                        // Reveal addresses up to the specified index
                        let _external_addresses: Vec<_> = wallet.reveal_addresses_to(KeychainKind::External, max_index).collect();
                        let _internal_addresses: Vec<_> = wallet.reveal_addresses_to(KeychainKind::Internal, max_index).collect();
                        
                        // Persist the revealed addresses
                        if let Err(e) = wallet.persist(&mut db) {
                            println!("[{}] ⚠️ Failed to persist revealed addresses: {}", checksum, e);
                        }
                        
                        // Full scan the newly revealed addresses with custom stop gap
                        if let Some(ref client) = self.electrum_client {
                            let custom_stop_gap = Some(max_index as usize);
                            if let Err(e) = client.full_scan_wallet(&mut wallet, custom_stop_gap) {
                                println!("[{}] ⚠️ Failed to full scan with custom stop gap: {}", checksum, e);
                            } else {
                                // Persist after sync
                                if let Err(e) = wallet.persist(&mut db) {
                                    println!("[{}] ⚠️ Failed to persist after custom scan: {}", checksum, e);
                                }
                            }
                        }
                    }
                }
            }
            
            // Extract historical transactions
            if let Err(e) = Self::extract_historical_transactions_for_background(
                &wallet,
                &checksum,
                &self.metadata_db,
                self.electrum_client.as_ref(),
            ).await {
                println!("[{}] ⚠️ Failed to extract historical transactions: {}", checksum, e);
            } else {
                println!("[{}] ✅ Historical transactions extracted", checksum);
            }
            
            // Update wallet balance in metadata
            let current_balance = wallet.balance().total().to_sat() as i64;
            if let Err(e) = self.metadata_db.update_wallet_balance_by_checksum(&checksum, current_balance).await {
                println!("[{}] ⚠️ Failed to update wallet balance in metadata: {}", checksum, e);
            } else {
                println!("[{}] 📊 Updated wallet metadata: balance={} sats", checksum, current_balance);
            }
            
            // Mark wallet as ready after transaction extraction
            if let Err(e) = self.metadata_db.mark_wallet_ready(&checksum).await {
                println!("[{}] ⚠️ Failed to mark wallet as ready: {}", checksum, e);
            } else {
                println!("[{}] ✅ Wallet marked as ready - available for frontend display", checksum);
            }

            return Ok(wallet_metadata);
        }

        // No winning wallet found - spawn background task to create fresh P2WPKH wallet
        let wallet_dir = self.wallet_dir.clone();
        let electrum_client_clone = self.electrum_client.clone();
        let metadata_db_clone = self.metadata_db.clone();
        let network = self.get_network();
        let checksum_clone = checksum.clone();
        let final_descriptor_clone = normalized_descriptor.clone();
        
        let has_activity = winning_wallet.is_some();
        tokio::spawn(async move {
            let checksum_for_error = checksum_clone.clone();
            if let Err(e) = Self::complete_wallet_creation_from_probed_xpub(
                wallet_dir,
                final_descriptor_clone,
                network,
                electrum_client_clone,
                metadata_db_clone,
                checksum_clone,
                has_activity,
            ).await {
                eprintln!("[{}] Background wallet creation failed: {}", checksum_for_error, e);
            }
        });

        Ok(wallet_metadata)
    }

    /// Background task to complete wallet creation from probed XPUB
    async fn complete_wallet_creation_from_probed_xpub(
        wallet_dir: PathBuf,
        descriptor: String,
        network: Network,
        electrum_client: Option<ElectrumClient>,
        metadata_db: MetadataDb,
        checksum: String,
        has_activity: bool,
    ) -> Result<()> {
        
        println!("[{}] Starting background wallet creation from probed XPUB", checksum);
        
        let wallet_filename = format!("{}.sqlite", checksum);
        let wallet_path = wallet_dir.join(&wallet_filename);
        
        // Create SQLite connection
        let mut db = Connection::open(&wallet_path)
            .map_err(|e| anyhow!("Failed to create connection to {}: {}", wallet_path.display(), e))?;
        
        // Parse descriptor
        let (receive_descriptor, change_descriptor) = 
            WalletManager::parse_multipath_descriptor_static(&descriptor)?;
        
        let receive_desc: Descriptor<DescriptorPublicKey> = receive_descriptor.parse()
            .map_err(|e| anyhow!("Failed to parse receive descriptor: {}", e))?;
        let change_desc: Descriptor<DescriptorPublicKey> = change_descriptor.parse()
            .map_err(|e| anyhow!("Failed to parse change descriptor: {}", e))?;
        
        // Create persistent wallet
        let mut wallet = Wallet::create(receive_desc, change_desc)
            .network(network)
            .create_wallet(&mut db)
            .map_err(|e| anyhow!("Failed to create persistent wallet: {}", e))?;
        
        // For wallets with activity, we've already done the probing work
        // Use larger stop gap since probing found activity
        
        // Persist initial wallet state
        wallet.persist(&mut db)
            .map_err(|e| anyhow!("Failed to persist wallet: {}", e))?;
        
        // Full scan with electrum (use larger stop gap if activity was found during probing)
        if let Some(ref client) = electrum_client {
            let stop_gap = if has_activity {
                // Probing found activity, so use larger stop gap to ensure we catch transactions at higher indices
                Some(100)
            } else {
                // No activity found during probing, use default stop gap
                None
            };
            
            if let Err(e) = client.full_scan_wallet(&mut wallet, stop_gap) {
                eprintln!("[{}] Warning: Failed to full scan wallet during background creation: {}", checksum, e);
            } else {
                // Persist after sync
                if let Err(e) = wallet.persist(&mut db) {
                    eprintln!("[{}] Warning: Failed to persist wallet after sync: {}", checksum, e);
                }
            }
            
            // For existing wallets (those that had activity), extract historical transactions
            if has_activity {
                if let Err(e) = Self::extract_historical_transactions_for_background(
                    &wallet,
                    &checksum,
                    &metadata_db,
                    electrum_client.as_ref(),
                ).await {
                    eprintln!("[{}] Warning: Failed to extract historical transactions: {}", checksum, e);
                }
            }
        }
        
        // Update wallet metadata with current balance and activity before marking ready
        if has_activity {
            let current_balance = wallet.balance().total().to_sat() as i64;
            if let Err(e) = metadata_db.update_wallet_balance_by_checksum(&checksum, current_balance).await {
                eprintln!("[{}] Warning: Failed to update wallet balance in metadata: {}", checksum, e);
            } else {
                println!("[{}] 📊 Updated wallet metadata: balance={} sats", checksum, current_balance);
            }
        }
        
        // Mark wallet as ready (only after balance and transactions are fully processed)
        if let Err(e) = metadata_db.mark_wallet_ready(&checksum).await {
            eprintln!("[{}] Warning: Failed to mark wallet as ready: {}", checksum, e);
        } else {
            println!("[{}] ✅ Wallet marked as ready - available for frontend display", checksum);
        }
        
        Ok(())
    }

    /// Static version of parse_multipath_descriptor for background tasks
    fn parse_multipath_descriptor_static(descriptor_str: &str) -> Result<(String, String)> {
        use miniscript::{Descriptor, DescriptorPublicKey};
        
        // Parse the multipath descriptor
        let descriptor: Descriptor<DescriptorPublicKey> = descriptor_str.parse()
            .map_err(|e| anyhow!("Failed to parse multipath descriptor: {}", e))?;

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

        Ok((receive_descriptor, change_descriptor))
    }
}
