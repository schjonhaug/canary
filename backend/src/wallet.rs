use bdk_wallet::{bitcoin::Network, Wallet, ChangeSet, PersistedWallet};
use bdk_wallet::file_store::Store;
use miniscript::{Descriptor, DescriptorPublicKey, descriptor::checksum::desc_checksum};
use std::error::Error;
use std::path::PathBuf;
use std::fs;
use crate::electrum::ElectrumClient;

pub struct WalletManager {
    wallets: Vec<(String, PersistedWallet<Store<ChangeSet>>)>, // (checksum, wallet)
    wallet_dir: PathBuf,
    electrum_client: ElectrumClient,
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
        
        let mut manager = WalletManager {
            wallets: Vec::new(),
            wallet_dir,
            electrum_client,
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

    /// Create or load a file store for a wallet
    fn create_file_store(&self, wallet_path: &PathBuf) -> Result<Store<ChangeSet>, Box<dyn Error>> {
        let (db, _changeset) = Store::<ChangeSet>::load_or_create(
            b"magic_bytes", 
            wallet_path
        ).map_err(|e| format!("Failed to create/load wallet store: {}", e))?;
        
        Ok(db)
    }

    /// Persist wallet changes to the database
    fn persist_wallet(&self, wallet: &mut PersistedWallet<Store<ChangeSet>>, db: &mut Store<ChangeSet>) -> Result<bool, Box<dyn Error>> {
        wallet.persist(db)
            .map_err(|e| format!("Failed to persist wallet: {}", e).into())
    }

    /// Sync wallet with electrum and persist changes
    async fn sync_and_persist_wallet(&self, wallet: &mut PersistedWallet<Store<ChangeSet>>, db: &mut Store<ChangeSet>) -> Result<(), Box<dyn Error>> {
        // Sync with electrum using shared client
        self.electrum_client.sync_wallet(wallet)
            .map_err(|e| format!("Failed to sync wallet: {}", e))?;
        
        // Persist wallet changes after sync
        self.persist_wallet(wallet, db)?;
        
        Ok(())
    }

    async fn load_all_wallets(&mut self) -> Result<(), Box<dyn Error>> {
        let entries = fs::read_dir(&self.wallet_dir)?;
        
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            
            // Only process .db files
            if path.extension().and_then(|s| s.to_str()) == Some("db") {
                if let Err(e) = self.load_wallet_from_file(&path).await {
                    eprintln!("Warning: Failed to load wallet from {}: {}", path.display(), e);
                }
            }
        }
        
        println!("Loaded {} wallets from disk", self.wallets.len());
        Ok(())
    }

    async fn load_wallet_from_file(&mut self, wallet_path: &PathBuf) -> Result<(), Box<dyn Error>> {
        let filename = wallet_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        
        println!("Loading wallet from file: {}", filename);
        
        // Open the file store
        let mut db = self.create_file_store(wallet_path)?;

        // Try to load the wallet (we don't know the descriptors, so we let BDK figure it out)
        let wallet_opt = Wallet::load()
            .extract_keys()
            .check_network(Self::get_network())
            .load_wallet(&mut db)
            .map_err(|e| format!("Failed to load wallet: {}", e))?;

        if let Some(mut wallet) = wallet_opt {
            println!("    - Network: {:?}", wallet.network());
            
            // Sync wallet with electrum and persist changes
            if let Err(e) = self.sync_and_persist_wallet(&mut wallet, &mut db).await {
                eprintln!("Warning: Failed to sync wallet {}: {}", filename, e);
            } else {
                println!("    - Synced with electrum");
            }
            
            // Extract checksum from filename (remove .db extension)
            let checksum = filename.strip_suffix(".db").unwrap_or(filename).to_string();
            self.wallets.push((checksum, wallet));
        } else {
            println!("  ⚠ No wallet data found in file");
        }
        
        Ok(())
    }

    /// Extract checksum from descriptor if present, or compute a new one
    fn get_descriptor_checksum(&self, descriptor_str: &str) -> Result<String, Box<dyn Error>> {
        // Check if descriptor already has a checksum (format: descriptor#checksum)
        if let Some(hash_pos) = descriptor_str.rfind('#') {
            let checksum = &descriptor_str[hash_pos + 1..];
            println!("Found existing checksum in descriptor: {}", checksum);
            return Ok(checksum.to_string());
        }
        
        // No existing checksum, compute a new one
        let checksum = desc_checksum(descriptor_str)
            .map_err(|e| format!("Failed to generate checksum: {}", e))?;
        println!("Computed new checksum: {}", checksum);
        Ok(checksum)
    }

    /// Create wallet file path from checksum
    fn get_wallet_path(&self, checksum: &str) -> PathBuf {
        let wallet_filename = format!("{}.db", checksum);
        self.wallet_dir.join(wallet_filename)
    }

    /// Parse and validate multipath descriptor
    fn parse_multipath_descriptor(&self, descriptor_str: &str) -> Result<(String, String), Box<dyn Error>> {
        // Parse the descriptor
        let descriptor: Descriptor<DescriptorPublicKey> = descriptor_str.parse()
            .map_err(|e| format!("Invalid descriptor: {}", e))?;

        // Check if it's a multipath descriptor
        if !descriptor.is_multipath() {
            return Err("Descriptor is not a multipath descriptor".into());
        }

        // Split multipath descriptor into receive and change descriptors
        let descriptors = descriptor.into_single_descriptors()
            .map_err(|e| format!("Failed to split multipath descriptor: {}", e))?;

        if descriptors.len() != 2 {
            return Err("Multipath descriptor must have exactly 2 paths (receive and change)".into());
        }

        let receive_descriptor = descriptors[0].to_string();
        let change_descriptor = descriptors[1].to_string();
        
        println!("  Receive descriptor: {}", receive_descriptor);
        println!("  Change descriptor: {}", change_descriptor);

        Ok((receive_descriptor, change_descriptor))
    }

    pub async fn create_from_multipath(&mut self, descriptor_str: &str) -> Result<(), Box<dyn Error>> {
        println!("Creating wallet from multipath descriptor:");
        println!("  Input descriptor: {}", descriptor_str);
        
        // Get checksum (either existing or computed)
        let checksum = self.get_descriptor_checksum(descriptor_str)?;
        println!("  Final checksum: {}", checksum);
        
        // Create wallet file path using checksum
        let wallet_path = self.get_wallet_path(&checksum);
        println!("  Wallet file path: {}", wallet_path.display());

        // Check if wallet file already exists
        if wallet_path.exists() {
            return Err("Wallet already exists".into());
        }

        // Parse and validate the multipath descriptor
        let (receive_descriptor, change_descriptor) = self.parse_multipath_descriptor(descriptor_str)?;

        // Open or create file store
        let mut db = self.create_file_store(&wallet_path)?;

        // Create new wallet
        let mut wallet = Wallet::create(receive_descriptor, change_descriptor)
            .network(Self::get_network())
            .create_wallet(&mut db)
            .map_err(|e| format!("Failed to create wallet: {}", e))?;

        // Persist initial wallet state
        self.persist_wallet(&mut wallet, &mut db)?;
        
        // Sync with electrum and persist changes
        self.sync_and_persist_wallet(&mut wallet, &mut db).await?;
        
        // Add wallet to the in-memory manager 
        self.wallets.push((checksum, wallet));
        
        Ok(())
    }

    pub async fn sync_all_wallets(&mut self) -> Result<(), Box<dyn Error>> {
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
                        // 22 for label, 18 for each value, 3 for separators
                        println!("{:>22} | {:<18} | {:<18} | {:<18}", format!("Wallet {}", checksum), "Before", "After", "Diff");
                        println!("{:-<79}", "");
                        let fmt = |amt: bdk_wallet::bitcoin::Amount| format!("{:>13.8} BTC", amt.to_sat() as f64 / 100_000_000.0);
                        let fmt_diff = |before: bdk_wallet::bitcoin::Amount, after: bdk_wallet::bitcoin::Amount| {
                            let diff_sats = after.to_sat() as i64 - before.to_sat() as i64;
                            format!("{:>+13.8} BTC", diff_sats as f64 / 100_000_000.0)
                        };
                        println!("{:>22} | {:<18} | {:<18} | {:<18}", "Trusted pending", fmt(trusted_pending_before), fmt(trusted_pending_after), fmt_diff(trusted_pending_before, trusted_pending_after));
                        println!("{:>22} | {:<18} | {:<18} | {:<18}", "Unconfirmed pending", fmt(untrusted_pending_before), fmt(untrusted_pending_after), fmt_diff(untrusted_pending_before, untrusted_pending_after));
                        println!("{:>22} | {:<18} | {:<18} | {:<18}", "Confirmed", fmt(confirmed_before), fmt(confirmed_after), fmt_diff(confirmed_before, confirmed_after));
                        println!("{:>22} | {:<18} | {:<18} | {:<18}", "Total", fmt(total_before), fmt(total_after), fmt_diff(total_before, total_after));
                    }
                        
                    
                }
                Err(e) => {
                    eprintln!("❌ Sync failed for wallet {} - {}", checksum, e);
                }
            }
        }
        
        Ok(())
    }
}