use crate::electrum::ElectrumClient;
use crate::metadata::{
    EventInsert, EventType, MetadataDb, TransactionEvent, WalletDetailResponse, WalletMetadata,
    WalletsListResponse,
};
use anyhow::{anyhow, Result};
use bdk_wallet::rusqlite::Connection;
use bdk_wallet::{bitcoin::Network, PersistedWallet, Wallet};
use miniscript::{Descriptor, DescriptorPublicKey};
use std::fs;
use std::path::PathBuf;
use tokio::sync::broadcast;

pub struct WalletManager {
    pub wallets: Vec<(String, PersistedWallet<Connection>)>, // (checksum, wallet)
    pub wallet_dir: PathBuf,
    pub electrum_client: Option<ElectrumClient>,
    pub metadata_db: MetadataDb,
    pub event_sender: broadcast::Sender<TransactionEvent>,
    network: Network,
}

impl WalletManager {

    pub async fn new(
        event_sender: broadcast::Sender<TransactionEvent>,
        wallet_dir: PathBuf,
        metadata_db_path: &str,
        network: Network,
        electrum_url: &str,
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
        let metadata_db = match MetadataDb::new(metadata_db_path).await {
            Ok(db) => db,
            Err(e) => {
                eprintln!("Warning: Failed to create metadata database: {}", e);
                panic!("Cannot create WalletManager without metadata database");
            }
        };

        let mut manager = WalletManager {
            wallets: Vec::new(),
            wallet_dir,
            electrum_client,
            metadata_db,
            event_sender,
            network,
        };

        // Load all existing wallets
        if let Err(e) = manager.load_all_wallets().await {
            eprintln!("Warning: Failed to load existing wallets: {}", e);
        }

        manager
    }

    /// Get the network configuration used by all wallets
    pub fn get_network(&self) -> Network {
        self.network
    }


    /// Helper function to insert historical event without broadcasting (no notifications)
    pub async fn insert_historical_event_helper(
        metadata_db: &MetadataDb,
        event_insert: &EventInsert,
    ) -> Result<()> {
        // Insert to database only, no broadcasting for historical events
        metadata_db.insert_event(event_insert).await?;
        Ok(())
    }

    /// Extract and process all historical transactions from a wallet
    pub async fn extract_historical_transactions(
        &self,
        wallet: &PersistedWallet<Connection>,
        wallet_checksum: &str,
    ) -> Result<()> {
        println!(
            "Extracting historical transactions for wallet checksum: {}",
            wallet_checksum
        );

        // Collect all transactions and sort them chronologically
        let mut all_transactions: Vec<_> = wallet.transactions().collect();

        // Sort transactions chronologically
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
            "Found {} historical transactions to process",
            all_transactions.len()
        );

        // Get current wallet balance and calculate initial balance
        let current_balance = wallet.balance().total().to_sat() as i64;

        // Calculate what the initial balance was by working backwards from current balance
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
            "Current balance: {:.8} BTC, Initial balance: {:.8} BTC",
            current_balance as f64 / 100_000_000.0,
            initial_balance as f64 / 100_000_000.0
        );

        // Process each transaction chronologically
        for tx in all_transactions {
            let sent = wallet.sent_and_received(&tx.tx_node).0;
            let received = wallet.sent_and_received(&tx.tx_node).1;
            let net_amount = received.to_sat() as i64 - sent.to_sat() as i64;
            let is_confirmed = tx.chain_position.is_confirmed();

            // Skip transactions with zero net amount (likely internal or dust)
            if net_amount == 0 {
                continue;
            }

            // Update running balance
            running_balance += net_amount;

            let (event_type, amount_sats) = if net_amount > 0 {
                // Receiving transaction
                (EventType::Receive, net_amount)
            } else {
                // Sending transaction - use absolute value
                (EventType::Send, net_amount.abs())
            };

            // Determine transaction timestamp
            let transaction_time = match &tx.chain_position {
                bdk_wallet::chain::ChainPosition::Confirmed { anchor, .. } => {
                    // For confirmed transactions, fetch block timestamp from Electrum
                    if let Some(electrum_client) = &self.electrum_client {
                        match electrum_client.get_block_header(anchor.block_id.height) {
                            Ok(header) => header.timestamp,
                            Err(e) => {
                                eprintln!("Failed to fetch block header for height {}: {}. Using current time.", 
                                    anchor.block_id.height, e);
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs()
                            }
                        }
                    } else {
                        // No Electrum client, use current time as fallback
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs()
                    }
                }
                bdk_wallet::chain::ChainPosition::Unconfirmed { first_seen, .. } => {
                    // For unconfirmed transactions, use first_seen if available
                    first_seen.unwrap_or_else(|| {
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs()
                    })
                }
            };

            // Create historical event with balance_total and transaction_time
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

            // Insert historical event (no notification broadcasting)
            if let Err(e) =
                Self::insert_historical_event_helper(&self.metadata_db, &event_insert).await
            {
                eprintln!("Failed to insert historical event: {}", e);
            } else {
                println!(
                    "  ✅ Processed {}: {} {:.8} BTC (Balance: {:.8} BTC)",
                    if event_type == EventType::Receive {
                        "Receive"
                    } else {
                        "Send"
                    },
                    if is_confirmed {
                        "Confirmed"
                    } else {
                        "Unconfirmed"
                    },
                    amount_sats as f64 / 100_000_000.0,
                    running_balance as f64 / 100_000_000.0
                );
            }
        }

        println!("Historical transaction extraction completed");
        Ok(())
    }

    /// Create or load a SQLite connection for a wallet
    pub fn create_sqlite_connection(&self, wallet_path: &PathBuf) -> Result<Connection> {
        let conn = Connection::open(wallet_path)
            .map_err(|e| anyhow!("Failed to create/load wallet database: {}", e))?;

        Ok(conn)
    }

    /// Persist wallet changes to the database
    fn persist_wallet(
        &self,
        wallet: &mut PersistedWallet<Connection>,
        db: &mut Connection,
    ) -> Result<bool> {
        wallet
            .persist(db)
            .map_err(|e| anyhow!("Failed to persist wallet: {}", e))
    }

    /// Sync wallet with electrum and persist changes
    async fn sync_and_persist_wallet(
        &self,
        wallet: &mut PersistedWallet<Connection>,
        db: &mut Connection,
    ) -> Result<()> {
        // Sync with electrum using shared client
        if let Some(client) = &self.electrum_client {
            client
                .sync_wallet(wallet)
                .map_err(|e| anyhow!("Failed to sync wallet: {}", e))?;
        }

        // Persist wallet changes after sync
        self.persist_wallet(wallet, db)?;

        Ok(())
    }

    async fn load_all_wallets(&mut self) -> Result<()> {
        let entries = fs::read_dir(&self.wallet_dir)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            // Only process .sqlite files
            if path.extension().and_then(|s| s.to_str()) == Some("sqlite") {
                if let Err(e) = self.load_wallet_from_file(&path).await {
                    eprintln!(
                        "Warning: Failed to load wallet from {}: {}",
                        path.display(),
                        e
                    );
                }
            }
        }

        println!("Loaded {} wallets from disk", self.wallets.len());

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

    async fn load_wallet_from_file(&mut self, wallet_path: &PathBuf) -> Result<()> {
        let filename = wallet_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        println!("Loading wallet from file: {}", filename);

        // Open the SQLite connection
        let mut db = self.create_sqlite_connection(wallet_path)?;

        // Try to load the wallet (we don't know the descriptors, so we let BDK figure it out)
        let wallet_opt = Wallet::load()
            .extract_keys()
            .check_network(self.get_network())
            .load_wallet(&mut db)
            .map_err(|e| anyhow!("Failed to load wallet: {}", e))?;

        if let Some(wallet) = wallet_opt {
            println!("    - Network: {:?}", wallet.network());
            println!("    - Loaded from disk (sync will happen in background loop)");

            // Extract checksum from filename (remove .sqlite extension)
            // Since we now use checksums as filenames, this is already the checksum
            let checksum = filename
                .strip_suffix(".sqlite")
                .unwrap_or(filename)
                .to_string();
            
            // Check if wallet already exists before adding
            if !self.wallets.iter().any(|(cs, _)| cs == &checksum) {
                self.wallets.push((checksum, wallet));
            }
        } else {
            println!("  ⚠ No wallet data found in file");
        }

        Ok(())
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
    ) -> Result<WalletMetadata> {
        println!("Creating wallet from multipath descriptor:");
        println!("  Name: {}", name);
        println!("  Input descriptor: {}", descriptor_str);

        // Check if descriptor already exists
        if self.metadata_db.descriptor_exists(descriptor_str).await? {
            return Err(anyhow!("This wallet has already been added. Ask the wallet owner to add you as a contact for notifications."));
        }

        // Parse and validate the multipath descriptor first
        let (receive_descriptor, change_descriptor) =
            self.parse_multipath_descriptor(descriptor_str)?;

        // Extract checksum from the descriptor for consistent filename
        let checksum = self.metadata_db.extract_checksum(descriptor_str);
        let wallet_filename_with_ext = format!("{}.sqlite", checksum);
        println!("  Wallet filename: {}", wallet_filename_with_ext);

        // Create wallet file path
        let wallet_path = self.wallet_dir.join(&wallet_filename_with_ext);
        println!("  Wallet file path: {}", wallet_path.display());

        // Check if wallet file already exists
        if wallet_path.exists() {
            return Err(anyhow!("Wallet file already exists"));
        }

        // Open or create SQLite connection
        let mut db = self.create_sqlite_connection(&wallet_path)?;

        // Create new wallet
        let mut wallet = Wallet::create(receive_descriptor.clone(), change_descriptor.clone())
            .network(self.get_network())
            .create_wallet(&mut db)
            .map_err(|e| anyhow!("Failed to create wallet: {}", e))?;

        // Persist initial wallet state
        self.persist_wallet(&mut wallet, &mut db)?;

        // Sync with electrum and persist changes (optional for tests)
        if let Err(e) = self.sync_and_persist_wallet(&mut wallet, &mut db).await {
            eprintln!("Warning: Failed to sync wallet during creation: {}", e);
        }

        // Save wallet metadata (checksum used directly as filename)
        let wallet_checksum = self
            .metadata_db
            .insert_wallet(name, descriptor_str, user_id)
            .await?;
        println!("  Metadata saved with checksum: {}", wallet_checksum);

        // Extract historical transactions BEFORE enabling real-time tracking
        // This ensures chronological order: historical events → real-time events
        println!("  Extracting historical transactions...");
        if let Err(e) = self
            .extract_historical_transactions(&wallet, &wallet_checksum)
            .await
        {
            eprintln!("Warning: Failed to extract historical transactions: {}", e);
        }

        // Set initial balance in metadata database (after full scan and historical extraction)
        let initial_balance = wallet.balance().total().to_sat() as i64;
        let initial_balance_btc = initial_balance as f64 / 100_000_000.0;
        println!(
            "  Initial balance: {:.8} BTC ({} sats)",
            initial_balance_btc, initial_balance
        );

        if let Err(e) = self
            .metadata_db
            .update_wallet_balance_by_checksum(&wallet_checksum, initial_balance)
            .await
        {
            eprintln!("Warning: Failed to set initial wallet balance: {}", e);
        } else {
            println!("  Balance saved to metadata database");
        }

        // Add wallet to the in-memory manager (using checksum as key)
        self.wallets.push((checksum.clone(), wallet));

        // Retrieve and return the created wallet metadata
        let wallet_metadata = self
            .metadata_db
            .get_wallet_by_descriptor(descriptor_str)
            .await?
            .ok_or_else(|| anyhow!("Failed to retrieve created wallet metadata"))?;

        Ok(wallet_metadata)
    }


    pub async fn sync_wallet_by_checksum(&mut self, wallet_checksum: &str) -> Result<()> {
        // Similar to sync_all_wallets but for a single wallet
        let metadata_db = &self.metadata_db;
        let event_sender = &self.event_sender;

        if let Some((_, wallet)) = self.wallets.iter_mut().find(|(checksum, _)| checksum == wallet_checksum) {
            // Get balance before sync
            let balance_before = wallet.balance();
            let _trusted_pending_before = balance_before.trusted_pending;
            let _untrusted_pending_before = balance_before.untrusted_pending;
            let _confirmed_before = balance_before.confirmed;
            let _total_before = balance_before.total();

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

            // Perform the sync
            if let Some(ref client) = self.electrum_client {
                let sync_result = client.sync_wallet_incremental(wallet);
                if let Err(e) = sync_result {
                    eprintln!("Failed to sync wallet {}: {}", wallet_checksum, e);
                    return Ok(());
                }
            }

            // Update last_synced_at timestamp
            let _ = metadata_db.update_wallet_last_synced(wallet_checksum).await;

            // Get balance after sync
            let balance_after = wallet.balance();
            let total_after = balance_after.total();

            // Update balance in metadata
            metadata_db
                .update_wallet_balance_by_checksum(wallet_checksum, total_after.to_sat() as i64)
                .await?;

            // Check for confirmed transactions and send events
            // (Similar logic to sync_all_wallets but for this single wallet)
            for tx in wallet.transactions() {
                if tx.chain_position.is_confirmed() {
                    let txid = tx.tx_node.txid.to_string();
                    
                    // Check if this was an unconfirmed send that just got confirmed
                    if let Some((_, original_amount)) = unconfirmed_sends_before
                        .iter()
                        .find(|(stored_txid, _)| stored_txid == &txid) 
                    {
                        // This is a send confirmation
                        let transaction_time = match &tx.chain_position {
                            bdk_wallet::chain::ChainPosition::Confirmed { anchor, .. } => {
                                // For confirmed transactions, fetch block timestamp from Electrum
                                if let Some(electrum_client) = &self.electrum_client {
                                    match electrum_client.get_block_header(anchor.block_id.height) {
                                        Ok(block_header) => block_header.timestamp,
                                        Err(_) => {
                                            // Fallback to current time if we can't fetch block header
                                            std::time::SystemTime::now()
                                                .duration_since(std::time::UNIX_EPOCH)
                                                .unwrap()
                                                .as_secs()
                                        }
                                    }
                                } else {
                                    // Fallback to current time if no Electrum client
                                    std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap()
                                        .as_secs()
                                }
                            }
                            _ => {
                                // This shouldn't happen since we already checked is_confirmed()
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs()
                            }
                        };
                        let event = TransactionEvent {
                            id: Some(uuid::Uuid::new_v4().to_string()),
                            wallet_checksum: wallet_checksum.to_string(),
                            event_type: EventType::Send,
                            amount_sats: *original_amount,
                            is_confirmed: true,
                            is_rbf: false,
                            is_cpfp: false,
                            balance_total: Some(total_after.to_sat() as i64),
                            transaction_time,
                            notification_status: Vec::new(),
                        };
                        
                        let _ = event_sender.send(event);
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn sync_wallets_due_for_sync(&mut self) -> Result<()> {
        // First, check for expired subscriptions and downgrade users
        if let Err(e) = self.process_expired_subscriptions().await {
            tracing::error!("Failed to process expired subscriptions: {}", e);
        }

        // Get wallets that are due for sync based on their owner's tier
        let due_wallets = self.metadata_db.get_wallets_due_for_sync().await?;
        
        if due_wallets.is_empty() {
            return Ok(());
        }

        println!("🔄 Syncing {} wallets due for sync", due_wallets.len());
        
        // Ensure all wallets are loaded first
        if let Err(e) = self.load_all_wallets().await {
            eprintln!("Failed to load wallets: {}", e);
            return Ok(());
        }

        for (wallet_metadata, tier) in due_wallets {
            // Sync the wallet
            match self.sync_wallet_by_checksum(&wallet_metadata.checksum).await {
                Ok(_) => println!("   ✅ Synced {} ({})", wallet_metadata.name, tier.as_str()),
                Err(e) => eprintln!("   ❌ Failed to sync {}: {}", wallet_metadata.name, e),
            }
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

    pub async fn get_wallets_list_for_user(
        &self,
        user_id: &str,
        is_admin: bool,
    ) -> Result<WalletsListResponse> {
        // Get current timestamp
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Get wallets based on user permissions
        let wallets = if is_admin {
            self.metadata_db.get_all_wallets().await?
        } else {
            self.metadata_db.get_wallets_for_user(Some(user_id)).await?
        };

        Ok(WalletsListResponse { timestamp, wallets })
    }

    pub async fn get_wallet_detail_for_user(
        &self,
        wallet_checksum: &str,
        user_id: &str,
        is_admin: bool,
    ) -> Result<WalletDetailResponse> {
        // Get current timestamp
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Get the specific wallet
        let wallet = self
            .metadata_db
            .get_wallet_by_checksum(wallet_checksum)
            .await?
            .ok_or_else(|| anyhow!("Wallet not found"))?;

        // Check if user has permission to access this wallet
        if !is_admin {
            let user_wallets = self.metadata_db.get_wallets_for_user(Some(user_id)).await?;
            let user_wallet_checksums: Vec<&str> =
                user_wallets.iter().map(|w| w.checksum.as_str()).collect();

            if !user_wallet_checksums.contains(&wallet_checksum) {
                return Err(anyhow!("Access denied to wallet"));
            }
        }

        // Get transaction events for this specific wallet
        let mut events = self.metadata_db.get_all_events_with_wallets().await?;
        events.retain(|event| event.wallet_checksum == wallet_checksum);

        // Limit to recent events for performance (already ordered by ID desc in SQL)
        events.truncate(100);

        // Get contacts for this wallet (including inactive ones for UI)
        let contacts = self
            .metadata_db
            .get_contacts_with_notification_methods_filtered(wallet_checksum, true)
            .await?;

        Ok(WalletDetailResponse {
            timestamp,
            wallet,
            events,
            contacts,
        })
    }

    pub async fn get_wallet_by_checksum(&self, checksum: &str) -> Result<Option<WalletMetadata>> {
        self.metadata_db
            .get_wallet_by_checksum(checksum)
            .await
            .map_err(|e| anyhow!("Failed to get wallet by checksum: {}", e))
    }

    pub async fn delete_wallet_by_checksum(&mut self, checksum: &str) -> Result<()> {
        println!("Deleting wallet with checksum: {}", checksum);

        // Get the descriptor and filename for this wallet checksum and delete from metadata
        let (descriptor, wallet_filename) =
            match self.metadata_db.delete_wallet_by_checksum(checksum).await? {
                Some((desc, filename)) => (desc, filename),
                None => return Err(anyhow!("Wallet not found")),
            };

        println!("  Found descriptor: {}", descriptor);
        println!("  Wallet filename: {}", wallet_filename);

        // Find and remove wallet from in-memory manager (checksum is the key now)
        let wallet_index = self
            .wallets
            .iter()
            .position(|(stored_checksum, _)| stored_checksum == checksum);

        if let Some(index) = wallet_index {
            // Remove wallet from in-memory storage (this unloads it from BDK)
            self.wallets.remove(index);
            println!("  Unloaded wallet from memory");
        } else {
            println!("  Warning: Wallet not found in memory (may have been manually removed)");
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
            println!("  Deleted wallet file: {}", wallet_path.display());
        } else {
            println!(
                "  Warning: Wallet file not found on disk: {}",
                wallet_path.display()
            );
        }

        println!("Wallet deletion completed successfully");

        Ok(())
    }

    pub async fn update_wallet(&self, checksum: &str, name: &str) -> Result<()> {
        println!("Updating wallet with checksum: {}", checksum);

        // Update wallet name in metadata database
        let updated = self
            .metadata_db
            .update_wallet_by_checksum(checksum, name)
            .await?;
        if !updated {
            return Err(anyhow!("Wallet not found"));
        }

        println!("  Updated wallet name to: {}", name);

        Ok(())
    }

    /// Apply subscription tier limits by setting is_active status on wallets and contacts
    pub async fn apply_subscription_limits(&self, user_id: &str, tier: &str, is_admin: bool) -> Result<()> {
        if is_admin {
            tracing::info!("🎯 Applying unlimited limits for admin user {}", user_id);
        } else {
            tracing::info!("🎯 Applying {} tier limits for user {}", tier, user_id);
        }
        
        // Get all wallets for this user ordered by creation time (oldest first)
        let wallets = self.metadata_db.get_wallets_for_user_oldest_first(user_id).await?;
        
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
            
            if let Err(e) = self.metadata_db.update_wallet_active_status(&wallet.checksum, should_be_active).await {
                tracing::error!("Failed to update wallet {} active status: {}", wallet.checksum, e);
            } else if !should_be_active {
                tracing::info!("📵 Deactivated wallet '{}' (#{}) - exceeds {} tier limit", 
                    wallet.name, index + 1, tier);
            }
        }
        
        // Handle contacts for each wallet
        for wallet in &wallets {
            let contacts = self.metadata_db.get_contacts_oldest_first_for_limits(&wallet.checksum).await?;
            
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
                    
                    if let Err(e) = self.metadata_db.update_contact_active_status(contact_id, should_be_active).await {
                        tracing::error!("Failed to update contact {} active status: {}", contact_id, e);
                    } else if !should_be_active {
                        let reason = format!("exceeds {} tier limit of {} contacts", tier, contact_limit);
                        tracing::info!("📵 Deactivated contact '{}' in wallet '{}' - {}", 
                            contact.name, wallet.name, reason);
                    } else {
                        tracing::info!("✅ Activated contact '{}' in wallet '{}' (within {} limit)", 
                            contact.name, wallet.name, tier);
                    }
                }
            }
        }
        
        tracing::info!("✅ Applied {} tier limits: {} wallets, checking contacts per wallet", 
            tier, wallet_limit);
        Ok(())
    }
}
