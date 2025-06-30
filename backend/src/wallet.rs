use bdk_wallet::{bitcoin::Network, Wallet, PersistedWallet, wallet_name_from_descriptor};
use bdk_wallet::bitcoin::secp256k1::Secp256k1;
use rusqlite::Connection;
use miniscript::{Descriptor, DescriptorPublicKey};
use std::path::PathBuf;
use std::fs;
use crate::electrum::ElectrumClient;
use crate::metadata::{MetadataDb, WalletMetadata};
use anyhow::{Result, anyhow};

pub struct WalletManager {
    wallets: Vec<(String, PersistedWallet<Connection>)>, // (checksum, wallet)
    wallet_dir: PathBuf,
    electrum_client: ElectrumClient,
    metadata_db: MetadataDb,
}

impl WalletManager {
    pub async fn new() -> Self {
        let wallet_dir = PathBuf::from("./wallets");
        // Create wallet directory if it doesn't exist
        if let Err(e) = std::fs::create_dir_all(&wallet_dir) {
            eprintln!("Warning: Failed to create wallet directory: {}", e);
        }
        
        // Initialize electrum client
        let electrum_client = match ElectrumClient::new_regtest() {
            Ok(client) => client,
            Err(e) => {
                eprintln!("Warning: Failed to create electrum client: {}", e);
                // We still need to return a manager, so we'll panic for now
                // In production, you might want to handle this more gracefully
                panic!("Cannot create WalletManager without electrum client");
            }
        };
        
        // Initialize metadata database
        let metadata_db = match MetadataDb::new("txray.sqlite") {
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
        };
        
        // Load all existing wallets
        if let Err(e) = manager.load_all_wallets().await {
            eprintln!("Warning: Failed to load existing wallets: {}", e);
        }
        
        manager
    }

    /// Get the network configuration used by all wallets
    fn get_network() -> Network {
        Network::Regtest
    }

    /// Create or load a SQLite connection for a wallet
    fn create_sqlite_connection(&self, wallet_path: &PathBuf) -> Result<Connection> {
        let conn = Connection::open(wallet_path)
            .map_err(|e| anyhow!("Failed to create/load wallet database: {}", e))?;
        
        Ok(conn)
    }

    /// Persist wallet changes to the database
    fn persist_wallet(&self, wallet: &mut PersistedWallet<Connection>, db: &mut Connection) -> Result<bool> {
        wallet.persist(db)
            .map_err(|e| anyhow!("Failed to persist wallet: {}", e))
    }

    /// Sync wallet with electrum and persist changes
    async fn sync_and_persist_wallet(&self, wallet: &mut PersistedWallet<Connection>, db: &mut Connection) -> Result<()> {
        // Sync with electrum using shared client
        self.electrum_client.sync_wallet(wallet)
            .map_err(|e| anyhow!("Failed to sync wallet: {}", e))?;
        
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
                    eprintln!("Warning: Failed to load wallet from {}: {}", path.display(), e);
                }
            }
        }
        
        println!("Loaded {} wallets from disk", self.wallets.len());
        Ok(())
    }

    async fn load_wallet_from_file(&mut self, wallet_path: &PathBuf) -> Result<()> {
        let filename = wallet_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        
        println!("Loading wallet from file: {}", filename);
        
        // Open the SQLite connection
        let mut db = self.create_sqlite_connection(wallet_path)?;

        // Try to load the wallet (we don't know the descriptors, so we let BDK figure it out)
        let wallet_opt = Wallet::load()
            .extract_keys()
            .check_network(Self::get_network())
            .load_wallet(&mut db)
            .map_err(|e| anyhow!("Failed to load wallet: {}", e))?;

        if let Some(mut wallet) = wallet_opt {
            println!("    - Network: {:?}", wallet.network());
            
            // Sync wallet with electrum and persist changes
            if let Err(e) = self.sync_and_persist_wallet(&mut wallet, &mut db).await {
                eprintln!("Warning: Failed to sync wallet {}: {}", filename, e);
            } else {
                println!("    - Synced with electrum");
            }
            
            // Extract checksum from filename (remove .sqlite extension)
            let checksum = filename.strip_suffix(".sqlite").unwrap_or(filename).to_string();
            self.wallets.push((checksum, wallet));
        } else {
            println!("  ⚠ No wallet data found in file");
        }
        
        Ok(())
    }


    /// Parse and validate multipath descriptor
    fn parse_multipath_descriptor(&self, descriptor_str: &str) -> Result<(String, String)> {
        // Parse the descriptor
        let descriptor: Descriptor<DescriptorPublicKey> = descriptor_str.parse()
            .map_err(|e| anyhow!("Invalid descriptor: {}", e))?;

        // Check if it's a multipath descriptor
        if !descriptor.is_multipath() {
            return Err(anyhow!("Descriptor is not a multipath descriptor"));
        }

        // Split multipath descriptor into receive and change descriptors
        let descriptors = descriptor.into_single_descriptors()
            .map_err(|e| anyhow!("Failed to split multipath descriptor: {}", e))?;

        if descriptors.len() != 2 {
            return Err(anyhow!("Multipath descriptor must have exactly 2 paths (receive and change)"));
        }

        let receive_descriptor = descriptors[0].to_string();
        let change_descriptor = descriptors[1].to_string();
        
        println!("  Receive descriptor: {}", receive_descriptor);
        println!("  Change descriptor: {}", change_descriptor);

        Ok((receive_descriptor, change_descriptor))
    }

    pub async fn create_from_multipath(&mut self, name: &str, descriptor_str: &str) -> Result<WalletMetadata> {
        println!("Creating wallet from multipath descriptor:");
        println!("  Name: {}", name);
        println!("  Input descriptor: {}", descriptor_str);
        
        // Check if descriptor already exists
        if self.metadata_db.descriptor_exists(descriptor_str)? {
            return Err(anyhow!("Descriptor already exists"));
        }
        
        // Parse and validate the multipath descriptor first
        let (receive_descriptor, change_descriptor) = self.parse_multipath_descriptor(descriptor_str)?;
        
        // Use BDK's function to generate wallet filename
        let wallet_filename = wallet_name_from_descriptor(
            &receive_descriptor,
            Some(&change_descriptor),
            Self::get_network(),
            &Secp256k1::new()
        )?;
        let wallet_filename_with_ext = format!("{}.sqlite", wallet_filename);
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
            .network(Self::get_network())
            .create_wallet(&mut db)
            .map_err(|e| anyhow!("Failed to create wallet: {}", e))?;

        // Persist initial wallet state
        self.persist_wallet(&mut wallet, &mut db)?;
        
        // Sync with electrum and persist changes
        self.sync_and_persist_wallet(&mut wallet, &mut db).await?;
        
        // Save wallet metadata
        self.metadata_db.insert_wallet(name, descriptor_str, &wallet_filename_with_ext)?;
        println!("  Metadata saved to txray.sqlite");
        
        // Add wallet to the in-memory manager (using wallet_filename as key)
        self.wallets.push((wallet_filename, wallet));
        
        // Retrieve and return the created wallet metadata
        let wallet_metadata = self.metadata_db.get_wallet_by_descriptor(descriptor_str)?
            .ok_or_else(|| anyhow!("Failed to retrieve created wallet metadata"))?;
        
        Ok(wallet_metadata)
    }

    pub async fn sync_all_wallets(&mut self) -> Result<()> {
        if self.wallets.is_empty() {
            return Ok(());
        }
        
        for (checksum, wallet) in self.wallets.iter_mut() {
            // Get balance before sync
            let balance_before = wallet.balance();
            let trusted_pending_before = balance_before.trusted_pending;
            let untrusted_pending_before = balance_before.untrusted_pending;
            let confirmed_before = balance_before.confirmed;
            let total_before = balance_before.total();
            
            match self.electrum_client.sync_wallet_incremental(wallet) {
                Ok(()) => {
                    // Get balance after sync
                    let balance_after = wallet.balance();
                    let trusted_pending_after = balance_after.trusted_pending;
                    let untrusted_pending_after = balance_after.untrusted_pending;
                    let confirmed_after = balance_after.confirmed;
                    let total_after = balance_after.total();
                    
                    // Check if any balance component changed
                    let has_changes = trusted_pending_before != trusted_pending_after ||
                                    untrusted_pending_before != untrusted_pending_after ||
                                    confirmed_before != confirmed_after ||
                                    total_before != total_after;
                    
                    if has_changes {
                        // Get the user-friendly wallet name
                        let wallet_filename = format!("{}.sqlite", checksum);
                        let wallet_name = self.metadata_db.get_wallet_name_by_filename(&wallet_filename)
                            .expect("Wallet name should exist in metadata database");
                        
                        // 22 for label, 18 for each value, 3 for separators
                        println!("{:>22} | {:<18} | {:<18} | {:<18}", format!("Wallet {}", wallet_name), "Before", "After", "Diff");
                        println!("{:-<79}", "");
                        let fmt = |amt: bdk_wallet::bitcoin::Amount| {
                            let btc = amt.to_sat() as f64 / 100_000_000.0;
                            if btc == 0.0 {
                                "".to_string()
                            } else {
                                format!("{:>13.8} BTC", btc)
                            }
                        };
                        let fmt_diff = |before: bdk_wallet::bitcoin::Amount, after: bdk_wallet::bitcoin::Amount| {
                            let diff_sats = after.to_sat() as i64 - before.to_sat() as i64;
                            let diff_btc = diff_sats as f64 / 100_000_000.0;
                            if diff_btc == 0.0 {
                                "".to_string()
                            } else {
                                format!("{:>+13.8} BTC", diff_btc)
                            }
                        };
                        
                        // Only print non-zero values
                        if trusted_pending_before.to_sat() > 0 || trusted_pending_after.to_sat() > 0 {
                            println!("{:>22} | {:<18} | {:<18} | {:<18}", "Trusted pending", fmt(trusted_pending_before), fmt(trusted_pending_after), fmt_diff(trusted_pending_before, trusted_pending_after));
                        } else {
                            println!("{:>22} | {:<18} | {:<18} | {:<18}", "Trusted pending", "", "", "");
                        }
                        if untrusted_pending_before.to_sat() > 0 || untrusted_pending_after.to_sat() > 0 {
                            println!("{:>22} | {:<18} | {:<18} | {:<18}", "Unconfirmed pending", fmt(untrusted_pending_before), fmt(untrusted_pending_after), fmt_diff(untrusted_pending_before, untrusted_pending_after));
                        } else {
                            println!("{:>22} | {:<18} | {:<18} | {:<18}", "Unconfirmed pending", "", "", "");
                        }
                        if confirmed_before.to_sat() > 0 || confirmed_after.to_sat() > 0 {
                            println!("{:>22} | {:<18} | {:<18} | {:<18}", "Confirmed", fmt(confirmed_before), fmt(confirmed_after), fmt_diff(confirmed_before, confirmed_after));
                        } else {
                            println!("{:>22} | {:<18} | {:<18} | {:<18}", "Confirmed", "", "", "");
                        }
                        
                        // Add separator before Total
                        println!("{:-<79}", "");
                        println!("{:>22} | {:<18} | {:<18} | {:<18}", "Total", fmt(total_before), fmt(total_after), fmt_diff(total_before, total_after));
                        println!("{:-<79}", "");
                        
                        // Detect if this is a sending transaction
                        let trusted_pending_increase = trusted_pending_after.to_sat() > trusted_pending_before.to_sat();
                        let trusted_pending_decrease = trusted_pending_after.to_sat() < trusted_pending_before.to_sat();
                        let confirmed_decrease = confirmed_after.to_sat() < confirmed_before.to_sat();
                        let total_decrease = total_after.to_sat() < total_before.to_sat();
                        
                        // First check for consolidation (takes precedence over regular sending)
                        let mut is_consolidation = false;
                        if trusted_pending_increase && confirmed_decrease && total_decrease {
                            let confirmed_spent = confirmed_before.to_sat() - confirmed_after.to_sat();
                            let trusted_received = trusted_pending_after.to_sat() - trusted_pending_before.to_sat();
                            let fee_paid = total_before.to_sat() - total_after.to_sat();
                            
                            // Consolidation pattern: most of the confirmed amount comes back as trusted pending
                            // with only a small fee difference
                            if trusted_received > 0 && fee_paid > 0 && confirmed_spent == trusted_received + fee_paid {
                                let consolidated_btc = trusted_received as f64 / 100_000_000.0;
                                let fee_btc = fee_paid as f64 / 100_000_000.0;
                                
                                println!("🔄 Consolidation: {:.8} BTC (fee: {:.8} BTC)", consolidated_btc, fee_btc);
                                is_consolidation = true;
                            }
                        }
                        
                        // Check if this might be RBF by looking for existing unconfirmed transactions
                        let has_unconfirmed = wallet.transactions()
                            .any(|tx| matches!(tx.chain_position, bdk_wallet::chain::ChainPosition::Unconfirmed { .. }));
                        
                        // RBF detection: small amount change (just fee difference) with existing unconfirmed tx
                        if has_unconfirmed && total_decrease && !is_consolidation {
                            let fee_increase = total_before.to_sat() - total_after.to_sat();
                            let fee_increase_btc = fee_increase as f64 / 100_000_000.0;
                            
                            // RBF pattern: trusted pending decreases (spending from change) with existing unconfirmed
                            if trusted_pending_decrease && !confirmed_decrease {
                                println!("📤 RBF fee bump: +{:.8} BTC", fee_increase_btc);
                            } else {
                                // Regular sending logic continues below
                                // Case 1: Spending from confirmed balance (first transaction)
                                if trusted_pending_increase && confirmed_decrease {
                                    let confirmed_spent = confirmed_before.to_sat() - confirmed_after.to_sat();
                                    let change_received = trusted_pending_after.to_sat() - trusted_pending_before.to_sat();
                                    let sending_amount = confirmed_spent - change_received;
                                    
                                    let sending_btc = sending_amount as f64 / 100_000_000.0;
                                    println!("📤 Sending {:.8} BTC", sending_btc);
                                }
                                // Case 2: Spending from trusted pending balance (subsequent transactions)
                                else if trusted_pending_decrease && confirmed_decrease {
                                    let trusted_spent = trusted_pending_before.to_sat() - trusted_pending_after.to_sat();
                                    let confirmed_spent = confirmed_before.to_sat() - confirmed_after.to_sat();
                                    let total_spent = trusted_spent + confirmed_spent;
                                    
                                    let sending_btc = total_spent as f64 / 100_000_000.0;
                                    println!("📤 Sending {:.8} BTC", sending_btc);
                                }
                                // Case 3: Spending only from trusted pending (no confirmed funds used)
                                else if trusted_pending_decrease && !confirmed_decrease {
                                    let trusted_spent = trusted_pending_before.to_sat() - trusted_pending_after.to_sat();
                                    let sending_btc = trusted_spent as f64 / 100_000_000.0;
                                    println!("📤 Sending {:.8} BTC", sending_btc);
                                }
                            }
                        } else if !is_consolidation {
                            // Regular sending logic (no existing unconfirmed transactions)
                            // Case 1: Spending from confirmed balance (first transaction)
                            if trusted_pending_increase && confirmed_decrease && total_decrease {
                                let confirmed_spent = confirmed_before.to_sat() - confirmed_after.to_sat();
                                let change_received = trusted_pending_after.to_sat() - trusted_pending_before.to_sat();
                                let sending_amount = confirmed_spent - change_received;
                                
                                let sending_btc = sending_amount as f64 / 100_000_000.0;
                                println!("📤 Sending {:.8} BTC", sending_btc);
                            }
                            // Case 2: Spending from trusted pending balance (subsequent transactions)
                            else if trusted_pending_decrease && confirmed_decrease && total_decrease {
                                let trusted_spent = trusted_pending_before.to_sat() - trusted_pending_after.to_sat();
                                let confirmed_spent = confirmed_before.to_sat() - confirmed_after.to_sat();
                                let total_spent = trusted_spent + confirmed_spent;
                                
                                let sending_btc = total_spent as f64 / 100_000_000.0;
                                println!("📤 Sending {:.8} BTC", sending_btc);
                            }
                            // Case 3: Spending only from trusted pending (no confirmed funds used)
                            else if trusted_pending_decrease && !confirmed_decrease && total_decrease {
                                let trusted_spent = trusted_pending_before.to_sat() - trusted_pending_after.to_sat();
                                let sending_btc = trusted_spent as f64 / 100_000_000.0;
                                println!("📤 Sending {:.8} BTC", sending_btc);
                            }
                        }
                        
                        // Detect if this is a receiving transaction
                        let untrusted_pending_increase = untrusted_pending_after.to_sat() > untrusted_pending_before.to_sat();
                        let confirmed_same = confirmed_after.to_sat() == confirmed_before.to_sat();
                        let total_increase = total_after.to_sat() > total_before.to_sat();
                        
                        if untrusted_pending_increase && confirmed_same && total_increase {
                            let receiving_amount = untrusted_pending_after.to_sat() - untrusted_pending_before.to_sat();
                            let receiving_btc = receiving_amount as f64 / 100_000_000.0;
                            
                            println!("📥 Receiving {:.8} BTC", receiving_btc);
                        }
                        
                        // Detect if this is a sent transaction being confirmed
                        let trusted_pending_decrease = trusted_pending_after.to_sat() < trusted_pending_before.to_sat();
                        let confirmed_increase = confirmed_after.to_sat() > confirmed_before.to_sat();
                        let total_same = total_after.to_sat() == total_before.to_sat();
                        
                        if trusted_pending_decrease && confirmed_increase && total_same {
                            println!("✅ Sent confirmed");
                        }
                        
                        // Detect if this is a received transaction being confirmed
                        let untrusted_pending_decrease = untrusted_pending_after.to_sat() < untrusted_pending_before.to_sat();
                        let confirmed_increase = confirmed_after.to_sat() > confirmed_before.to_sat();
                        let total_same = total_after.to_sat() == total_before.to_sat();
                        
                        if untrusted_pending_decrease && confirmed_increase && total_same {
                            let received_amount = untrusted_pending_before.to_sat() - untrusted_pending_after.to_sat();
                            let received_btc = received_amount as f64 / 100_000_000.0;
                            
                            println!("✅ Received {:.8} BTC", received_btc);
                        }
                        
                        // Detect CPFP (Child-Pays-For-Parent)
                        let untrusted_pending_decrease = untrusted_pending_after.to_sat() < untrusted_pending_before.to_sat();
                        let confirmed_same = confirmed_after.to_sat() == confirmed_before.to_sat();
                        let total_decrease = total_after.to_sat() < total_before.to_sat();
                        
                        if untrusted_pending_decrease && confirmed_same && total_decrease {
                            let fee_paid = total_before.to_sat() - total_after.to_sat();
                            let fee_paid_btc = fee_paid as f64 / 100_000_000.0;
                            
                            println!("🚀 CPFP fee: {:.8} BTC", fee_paid_btc);
                        }
                        
                        
                        println!(); // Add spacing between wallets
                    }
                        
                    
                }
                Err(e) => {
                    eprintln!("❌ Sync failed for wallet {} - {}", checksum, e);
                }
            }
        }
        
        Ok(())
    }


    pub fn get_wallet_by_id(&self, id: i64) -> Result<Option<WalletMetadata>> {
        self.metadata_db.get_wallet_by_id(id)
            .map_err(|e| anyhow!("Failed to get wallet by ID: {}", e))
    }

    pub fn get_all_wallets(&self) -> Result<Vec<WalletMetadata>> {
        self.metadata_db.get_all_wallets()
            .map_err(|e| anyhow!("Failed to get all wallets: {}", e))
    }

    pub async fn delete_wallet_by_id(&mut self, id: i64) -> Result<()> {
        println!("Deleting wallet with ID: {}", id);
        
        // Get the descriptor and filename for this wallet ID and delete from metadata
        let (descriptor, wallet_filename) = match self.metadata_db.delete_wallet_by_id(id)? {
            Some((desc, filename)) => (desc, filename),
            None => return Err(anyhow!("Wallet not found")),
        };
        
        println!("  Found descriptor: {}", descriptor);
        println!("  Wallet filename: {}", wallet_filename);
        
        // Extract checksum from filename (remove .sqlite extension)
        let checksum = wallet_filename.strip_suffix(".sqlite").unwrap_or(&wallet_filename);
        
        // Find and remove wallet from in-memory manager
        let wallet_index = self.wallets.iter()
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
            fs::remove_file(&wallet_path)
                .map_err(|e| anyhow!("Failed to delete wallet file {}: {}", wallet_path.display(), e))?;
            println!("  Deleted wallet file: {}", wallet_path.display());
        } else {
            println!("  Warning: Wallet file not found on disk: {}", wallet_path.display());
        }
        
        println!("Wallet deletion completed successfully");
        Ok(())
    }
}