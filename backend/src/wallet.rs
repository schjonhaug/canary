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
    /// Get current Unix timestamp
    fn get_current_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

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

    /// Helper function to insert event and broadcast using extracted components
    pub async fn insert_and_broadcast_event_helper(
        metadata_db: &MetadataDb,
        event_sender: &broadcast::Sender<TransactionEvent>,
        event_insert: &EventInsert,
    ) -> Result<()> {
        // First, insert to database (write-through pattern)
        let event_id = metadata_db.insert_event(event_insert).await?;

        // Create TransactionEvent for broadcasting
        let event = TransactionEvent {
            id: Some(event_id),
            wallet_checksum: event_insert.wallet_checksum.clone(),
            event_type: event_insert.event_type,
            amount_sats: event_insert.amount_sats,
            is_confirmed: event_insert.is_confirmed,
            is_rbf: event_insert.is_rbf,
            is_cpfp: event_insert.is_cpfp,
            balance_total: event_insert.balance_total,
            transaction_time: event_insert.transaction_time,
            notification_status: Vec::new(), // Will be populated by notification worker
        };

        // Broadcast to notification worker (non-blocking)
        if let Err(e) = event_sender.send(event) {
            eprintln!("Failed to broadcast event: {}", e);
        }

        Ok(())
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

    pub async fn sync_all_wallets(&mut self) -> Result<()> {
        // Extract needed components to avoid borrowing issues
        let metadata_db = &self.metadata_db;
        let event_sender = &self.event_sender;

        if self.wallets.is_empty() {
            // No wallets to sync, return early without sending update
            return Ok(());
        }

        for (wallet_key, wallet) in self.wallets.iter_mut() {
            // Get balance before sync
            let balance_before = wallet.balance();
            let trusted_pending_before = balance_before.trusted_pending;
            let untrusted_pending_before = balance_before.untrusted_pending;
            let confirmed_before = balance_before.confirmed;
            let total_before = balance_before.total();

            // For send confirmations, we need transaction-level analysis because balance diffs are insufficient.
            // When a send confirms, balance changes are: trusted_pending(-X) + confirmed(+X) = 0 total change.
            // The balance diff only shows pending→confirmed movement, not the original send amount
            // (which was already deducted when the transaction was created).
            // For other events (receive, send initiation), balance diffs alone provide the amounts.
            let unconfirmed_sends_before: Vec<(String, i64)> = wallet
                .transactions()
                .filter_map(|tx| {
                    // Only track unconfirmed send transactions
                    if !tx.chain_position.is_confirmed() {
                        let sent = wallet.sent_and_received(&tx.tx_node).0;
                        let received = wallet.sent_and_received(&tx.tx_node).1;
                        let net_amount = received.to_sat() as i64 - sent.to_sat() as i64;

                        if net_amount < 0 {
                            // This is a send transaction - calculate amount sent to external addresses
                            // net_amount is negative, so we take its absolute value to get the actual send amount
                            let actual_send_amount = net_amount.abs();
                            Some((tx.tx_node.txid.to_string(), actual_send_amount))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect();

            match self
                .electrum_client
                .as_ref()
                .map(|client| client.sync_wallet_incremental(wallet))
            {
                Some(Ok(())) => {
                    // Persist wallet changes after successful incremental sync
                    let wallet_filename = format!("{}.sqlite", wallet_key);
                    let wallet_path = self.wallet_dir.join(&wallet_filename);

                    match Connection::open(&wallet_path) {
                        Ok(mut db) => {
                            if let Err(e) = wallet.persist(&mut db) {
                                eprintln!(
                                    "❌ Failed to persist wallet {} after sync: {}",
                                    wallet_key, e
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!(
                                "❌ Failed to create database connection for wallet {}: {}",
                                wallet_key, e
                            );
                        }
                    }
                    // Get balance after sync
                    let balance_after = wallet.balance();
                    let trusted_pending_after = balance_after.trusted_pending;
                    let untrusted_pending_after = balance_after.untrusted_pending;
                    let confirmed_after = balance_after.confirmed;
                    let total_after = balance_after.total();

                    // Check which sends have now been confirmed
                    let mut total_confirmed_send_amount = 0i64;
                    for (txid, send_amount) in &unconfirmed_sends_before {
                        // Check if this transaction is now confirmed
                        if let Some(tx) = wallet
                            .transactions()
                            .find(|tx| tx.tx_node.txid.to_string() == *txid)
                        {
                            if tx.chain_position.is_confirmed() {
                                total_confirmed_send_amount += send_amount;
                            }
                        }
                    }

                    // Check if any balance component changed
                    let has_changes = trusted_pending_before != trusted_pending_after
                        || untrusted_pending_before != untrusted_pending_after
                        || confirmed_before != confirmed_after
                        || total_before != total_after;

                    if has_changes {
                        // Get the user-friendly wallet name (wallet_key is now the checksum)
                        let wallet_checksum = wallet_key;
                        let wallet_name = self
                            .metadata_db
                            .get_wallet_name_by_checksum(wallet_checksum)
                            .await
                            .expect(&format!(
                                "Wallet name for checksum '{}' should exist in metadata database",
                                wallet_checksum
                            ));

                        // Update the wallet balance in the metadata database
                        if let Err(e) = self
                            .metadata_db
                            .update_wallet_balance_by_checksum(
                                wallet_checksum,
                                total_after.to_sat() as i64,
                            )
                            .await
                        {
                            eprintln!("Failed to update wallet balance in metadata: {}", e);
                        }

                        // 22 for label, 18 for each value, 3 for separators
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

                        // Detect if this is a sending transaction
                        let trusted_pending_increase =
                            trusted_pending_after.to_sat() > trusted_pending_before.to_sat();
                        let trusted_pending_decrease =
                            trusted_pending_after.to_sat() < trusted_pending_before.to_sat();
                        let confirmed_decrease =
                            confirmed_after.to_sat() < confirmed_before.to_sat();
                        let total_decrease = total_after.to_sat() < total_before.to_sat();

                        // First check for special transaction types (takes precedence over regular sending)
                        let mut is_special_tx = false;

                        // Check for CPFP (Child-Pays-For-Parent)
                        if !is_special_tx {
                            let untrusted_pending_decrease = untrusted_pending_after.to_sat()
                                < untrusted_pending_before.to_sat();
                            let confirmed_same =
                                confirmed_after.to_sat() == confirmed_before.to_sat();

                            if untrusted_pending_decrease && confirmed_same && total_decrease {
                                let fee_paid = total_before.to_sat() - total_after.to_sat();
                                let fee_paid_btc = fee_paid as f64 / 100_000_000.0;

                                let message = format!("🚀 CPFP fee: {:.8} BTC", fee_paid_btc);
                                println!("{}", message);

                                // Insert CPFP event to database and broadcast
                                if let Err(e) = Self::insert_and_broadcast_event_helper(
                                    metadata_db,
                                    event_sender,
                                    &EventInsert {
                                        wallet_checksum: wallet_checksum.clone(),
                                        event_type: EventType::Send,
                                        amount_sats: fee_paid as i64,
                                        is_confirmed: false,
                                        is_rbf: false,
                                        is_cpfp: true,
                                        balance_total: Some(total_after.to_sat() as i64),
                                        transaction_time: Self::get_current_timestamp(),
                                    },
                                )
                                .await
                                {
                                    eprintln!("Failed to insert CPFP event: {}", e);
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
                            let fee_increase_btc = fee_increase as f64 / 100_000_000.0;

                            // RBF pattern: trusted pending decreases (spending from change) with existing unconfirmed
                            if trusted_pending_decrease && !confirmed_decrease {
                                let message =
                                    format!("📤 RBF fee bump: +{:.8} BTC", fee_increase_btc);
                                println!("{}", message);

                                // Insert RBF event to database and broadcast
                                if let Err(e) = Self::insert_and_broadcast_event_helper(
                                    metadata_db,
                                    event_sender,
                                    &EventInsert {
                                        wallet_checksum: wallet_checksum.clone(),
                                        event_type: EventType::Send,
                                        amount_sats: fee_increase as i64,
                                        is_confirmed: false,
                                        is_rbf: true,
                                        is_cpfp: false,
                                        balance_total: Some(total_after.to_sat() as i64),
                                        transaction_time: Self::get_current_timestamp(),
                                    },
                                )
                                .await
                                {
                                    eprintln!("Failed to insert RBF event: {}", e);
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

                                    let sending_btc = sending_amount as f64 / 100_000_000.0;
                                    let message = format!("📤 Sending {:.8} BTC", sending_btc);
                                    println!("{}", message);

                                    // Insert sending event to database and broadcast
                                    if let Err(e) = Self::insert_and_broadcast_event_helper(
                                        metadata_db,
                                        event_sender,
                                        &EventInsert {
                                            wallet_checksum: wallet_checksum.clone(),
                                            event_type: EventType::Send,
                                            amount_sats: sending_amount as i64,
                                            is_confirmed: false,
                                            is_rbf: false,
                                            is_cpfp: false,
                                            balance_total: Some(total_after.to_sat() as i64),
                                            transaction_time: Self::get_current_timestamp(),
                                        },
                                    )
                                    .await
                                    {
                                        eprintln!("Failed to insert sending event: {}", e);
                                    }
                                }
                                // Case 2: Spending from trusted pending balance (subsequent transactions)
                                else if trusted_pending_decrease && confirmed_decrease {
                                    let trusted_spent = trusted_pending_before.to_sat()
                                        - trusted_pending_after.to_sat();
                                    let confirmed_spent =
                                        confirmed_before.to_sat() - confirmed_after.to_sat();
                                    let total_spent = trusted_spent + confirmed_spent;

                                    let sending_btc = total_spent as f64 / 100_000_000.0;
                                    let message = format!("📤 Sending {:.8} BTC", sending_btc);
                                    println!("{}", message);

                                    // Insert sending event to database and broadcast
                                    if let Err(e) = Self::insert_and_broadcast_event_helper(
                                        metadata_db,
                                        event_sender,
                                        &EventInsert {
                                            wallet_checksum: wallet_checksum.clone(),
                                            event_type: EventType::Send,
                                            amount_sats: total_spent as i64,
                                            is_confirmed: false,
                                            is_rbf: false,
                                            is_cpfp: false,
                                            balance_total: Some(total_after.to_sat() as i64),
                                            transaction_time: Self::get_current_timestamp(),
                                        },
                                    )
                                    .await
                                    {
                                        eprintln!("Failed to insert sending event: {}", e);
                                    }
                                }
                                // Case 3: Spending only from trusted pending (no confirmed funds used)
                                else if trusted_pending_decrease && !confirmed_decrease {
                                    let trusted_spent = trusted_pending_before.to_sat()
                                        - trusted_pending_after.to_sat();
                                    let sending_btc = trusted_spent as f64 / 100_000_000.0;
                                    let message = format!("📤 Sending {:.8} BTC", sending_btc);
                                    println!("{}", message);

                                    // Insert sending event to database and broadcast
                                    if let Err(e) = Self::insert_and_broadcast_event_helper(
                                        metadata_db,
                                        event_sender,
                                        &EventInsert {
                                            wallet_checksum: wallet_checksum.clone(),
                                            event_type: EventType::Send,
                                            amount_sats: trusted_spent as i64,
                                            is_confirmed: false,
                                            is_rbf: false,
                                            is_cpfp: false,
                                            balance_total: Some(total_after.to_sat() as i64),
                                            transaction_time: Self::get_current_timestamp(),
                                        },
                                    )
                                    .await
                                    {
                                        eprintln!("Failed to insert sending event: {}", e);
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

                                let sending_btc = sending_amount as f64 / 100_000_000.0;
                                let message = format!("📤 Sending {:.8} BTC", sending_btc);
                                println!("{}", message);

                                // Insert sending event to database and broadcast
                                if let Err(e) = Self::insert_and_broadcast_event_helper(
                                    metadata_db,
                                    event_sender,
                                    &EventInsert {
                                        wallet_checksum: wallet_checksum.clone(),
                                        event_type: EventType::Send,
                                        amount_sats: sending_amount as i64,
                                        is_confirmed: false,
                                        is_rbf: false,
                                        is_cpfp: false,
                                        balance_total: Some(total_after.to_sat() as i64),
                                        transaction_time: Self::get_current_timestamp(),
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

                                let sending_btc = total_spent as f64 / 100_000_000.0;
                                let message = format!("📤 Sending {:.8} BTC", sending_btc);
                                println!("{}", message);

                                // Insert sending event to database and broadcast
                                if let Err(e) = Self::insert_and_broadcast_event_helper(
                                    metadata_db,
                                    event_sender,
                                    &EventInsert {
                                        wallet_checksum: wallet_checksum.clone(),
                                        event_type: EventType::Send,
                                        amount_sats: total_spent as i64,
                                        is_confirmed: false,
                                        is_rbf: false,
                                        is_cpfp: false,
                                        balance_total: Some(total_after.to_sat() as i64),
                                        transaction_time: Self::get_current_timestamp(),
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
                                let sending_btc = trusted_spent as f64 / 100_000_000.0;
                                let message = format!("📤 Sending {:.8} BTC", sending_btc);
                                println!("{}", message);

                                // Insert sending event to database and broadcast
                                if let Err(e) = Self::insert_and_broadcast_event_helper(
                                    metadata_db,
                                    event_sender,
                                    &EventInsert {
                                        wallet_checksum: wallet_checksum.clone(),
                                        event_type: EventType::Send,
                                        amount_sats: trusted_spent as i64,
                                        is_confirmed: false,
                                        is_rbf: false,
                                        is_cpfp: false,
                                        balance_total: Some(total_after.to_sat() as i64),
                                        transaction_time: Self::get_current_timestamp(),
                                    },
                                )
                                .await
                                {
                                    eprintln!("Failed to insert sending event: {}", e);
                                }
                            }
                        }

                        // Detect if this is a receiving transaction
                        let untrusted_pending_increase =
                            untrusted_pending_after.to_sat() > untrusted_pending_before.to_sat();
                        let confirmed_same = confirmed_after.to_sat() == confirmed_before.to_sat();
                        let total_increase = total_after.to_sat() > total_before.to_sat();

                        if untrusted_pending_increase && confirmed_same && total_increase {
                            let receiving_amount = untrusted_pending_after.to_sat()
                                - untrusted_pending_before.to_sat();
                            let receiving_btc = receiving_amount as f64 / 100_000_000.0;

                            let message = format!("📥 Receiving {:.8} BTC", receiving_btc);
                            println!("{}", message);

                            // Insert receiving event to database and broadcast
                            if let Err(e) = Self::insert_and_broadcast_event_helper(
                                metadata_db,
                                event_sender,
                                &EventInsert {
                                    wallet_checksum: wallet_checksum.clone(),
                                    event_type: EventType::Receive,
                                    amount_sats: receiving_amount as i64,
                                    is_confirmed: false,
                                    is_rbf: false,
                                    is_cpfp: false,
                                    balance_total: Some(total_after.to_sat() as i64),
                                    transaction_time: Self::get_current_timestamp(),
                                },
                            )
                            .await
                            {
                                eprintln!("Failed to insert receiving event: {}", e);
                            }
                        }

                        // Detect if this is a sent transaction being confirmed
                        let trusted_pending_decrease =
                            trusted_pending_after.to_sat() < trusted_pending_before.to_sat();
                        let confirmed_increase =
                            confirmed_after.to_sat() > confirmed_before.to_sat();
                        let total_same = total_after.to_sat() == total_before.to_sat();

                        if trusted_pending_decrease && confirmed_increase && total_same {
                            // Use transaction-level analysis result for send confirmation amount
                            if total_confirmed_send_amount > 0 {
                                let confirmed_btc =
                                    total_confirmed_send_amount as f64 / 100_000_000.0;
                                let message =
                                    format!("✅ Sent confirmed: {:.8} BTC", confirmed_btc);
                                println!("{}", message);

                                // Insert sent confirmation event to database and broadcast
                                if let Err(e) = Self::insert_and_broadcast_event_helper(
                                    metadata_db,
                                    event_sender,
                                    &EventInsert {
                                        wallet_checksum: wallet_checksum.clone(),
                                        event_type: EventType::Send,
                                        amount_sats: total_confirmed_send_amount,
                                        is_confirmed: true,
                                        is_rbf: false,
                                        is_cpfp: false,
                                        balance_total: Some(total_after.to_sat() as i64),
                                        transaction_time: Self::get_current_timestamp(),
                                    },
                                )
                                .await
                                {
                                    eprintln!("Failed to insert sent confirmation event: {}", e);
                                }
                            } else {
                                // Fallback for cases where we couldn't determine the amount
                                let message = "✅ Sent confirmed".to_string();
                                println!("{}", message);

                                // Insert sent confirmation event to database and broadcast
                                if let Err(e) = Self::insert_and_broadcast_event_helper(
                                    metadata_db,
                                    event_sender,
                                    &EventInsert {
                                        wallet_checksum: wallet_checksum.clone(),
                                        event_type: EventType::Send,
                                        amount_sats: 0,
                                        is_confirmed: true,
                                        is_rbf: false,
                                        is_cpfp: false,
                                        balance_total: Some(total_after.to_sat() as i64),
                                        transaction_time: Self::get_current_timestamp(),
                                    },
                                )
                                .await
                                {
                                    eprintln!("Failed to insert sent confirmation event: {}", e);
                                }
                            }
                        }

                        // Detect if this is a received transaction being confirmed
                        let untrusted_pending_decrease =
                            untrusted_pending_after.to_sat() < untrusted_pending_before.to_sat();
                        let confirmed_increase =
                            confirmed_after.to_sat() > confirmed_before.to_sat();
                        let total_same = total_after.to_sat() == total_before.to_sat();

                        if untrusted_pending_decrease && confirmed_increase && total_same {
                            let confirmed_amount =
                                confirmed_after.to_sat() - confirmed_before.to_sat();
                            let confirmed_btc = confirmed_amount as f64 / 100_000_000.0;

                            let message =
                                format!("✅ Received confirmed: {:.8} BTC", confirmed_btc);
                            println!("{}", message);

                            // Insert received confirmation event to database and broadcast
                            if let Err(e) = Self::insert_and_broadcast_event_helper(
                                metadata_db,
                                event_sender,
                                &EventInsert {
                                    wallet_checksum: wallet_checksum.clone(),
                                    event_type: EventType::Receive,
                                    amount_sats: confirmed_amount as i64,
                                    is_confirmed: true,
                                    is_rbf: false,
                                    is_cpfp: false,
                                    balance_total: Some(total_after.to_sat() as i64),
                                    transaction_time: Self::get_current_timestamp(),
                                },
                            )
                            .await
                            {
                                eprintln!("Failed to insert received confirmation event: {}", e);
                            }
                        }

                        println!(); // Add spacing between wallets
                    }
                }
                Some(Err(e)) => {
                    eprintln!("❌ Sync failed for wallet {} - {}", wallet_key, e);
                }
                None => {
                    // No electrum client available, skip sync
                }
            }
        }

        Ok(())
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

        // Get contacts for this wallet
        let contacts = self
            .metadata_db
            .get_contacts_with_notification_methods(wallet_checksum)
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
}
