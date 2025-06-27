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
}

impl WalletManager {
    pub async fn new() -> Self {
        let wallet_dir = PathBuf::from("./wallets");
        // Create wallet directory if it doesn't exist
        if let Err(e) = std::fs::create_dir_all(&wallet_dir) {
            eprintln!("Warning: Failed to create wallet directory: {}", e);
        }
        
        let mut manager = WalletManager {
            wallets: Vec::new(),
            wallet_dir,
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
        // Sync with electrum
        let electrum_client = ElectrumClient::new_regtest()
            .map_err(|e| format!("Failed to create electrum client: {}", e))?;
        
        electrum_client.sync_wallet(wallet)
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
        println!("");
        println!("🔄 Syncing {} wallets...", self.wallets.len());
        
        let electrum_client = ElectrumClient::new_regtest()
            .map_err(|e| format!("Failed to create electrum client: {}", e))?;
        
        for (checksum, wallet) in self.wallets.iter_mut() {
            println!("\n═══ Wallet {} ═══", checksum);
            
            let balance_before = wallet.balance().total();
            
            match electrum_client.sync_wallet_incremental(wallet) {
                Ok(()) => {
                    let balance_after = wallet.balance().total();
                    
                    if balance_before != balance_after {
                        println!("💰 Balance changed {} -> {}", 
                               balance_before, balance_after);
                        
                        // Show only unconfirmed transactions (these are likely the cause of balance change)
                        println!("📋 Unconfirmed transactions:");
                        let unconfirmed_txs: Vec<_> = wallet.transactions()
                            .filter(|tx| matches!(tx.chain_position, bdk_wallet::chain::ChainPosition::Unconfirmed { .. }))
                            .take(5)
                            .collect();
                        
                        if unconfirmed_txs.is_empty() {
                            println!("  No unconfirmed transactions found");
                            // If no unconfirmed transactions but balance changed, show recent confirmed ones
                            println!("📋 Recent confirmed transactions:");
                            let recent_confirmed: Vec<_> = wallet.transactions()
                                .filter(|tx| matches!(tx.chain_position, bdk_wallet::chain::ChainPosition::Confirmed { .. }))
                                .take(2)
                                .collect();
                            
                            for tx_details in recent_confirmed {
                                let tx = &tx_details.tx_node.tx;
                                let txid = tx.compute_txid();
                                
                                // Calculate received and sent amounts for this wallet
                                let mut received = 0u64;
                                let mut sent = 0u64;
                                
                                // Check outputs for received amounts
                                for output in &tx.output {
                                    if wallet.is_mine(output.script_pubkey.clone()) {
                                        received += output.value.to_sat();
                                    }
                                }
                                
                                // Check inputs for sent amounts
                                for input in &tx.input {
                                    if let Some(prev_tx) = wallet.get_tx(input.previous_output.txid) {
                                        if let Some(prev_output) = prev_tx.tx_node.tx.output.get(input.previous_output.vout as usize) {
                                            if wallet.is_mine(prev_output.script_pubkey.clone()) {
                                                sent += prev_output.value.to_sat();
                                            }
                                        }
                                    }
                                }
                                
                                let net_change = received as i64 - sent as i64;
                                
                                println!("  📄 TXID: {}", txid);
                                println!("     Received: {} sats", received);
                                println!("     Sent: {} sats", sent);
                                println!("     Net change: {} sats", net_change);
                                
                                match &tx_details.chain_position {
                                    bdk_wallet::chain::ChainPosition::Confirmed { anchor, .. } => {
                                        println!("     Confirmed at height: {}", anchor.block_id.height);
                                    }
                                    bdk_wallet::chain::ChainPosition::Unconfirmed { .. } => {
                                        println!("     Status: Unconfirmed");
                                    }
                                }
                                println!();
                            }
                        } else {
                            for tx_details in unconfirmed_txs {
                                let tx = &tx_details.tx_node.tx;
                                let txid = tx.compute_txid();
                                
                                // Calculate received and sent amounts for this wallet
                                let mut received = 0u64;
                                let mut sent = 0u64;
                                
                                // Check outputs for received amounts
                                for output in &tx.output {
                                    if wallet.is_mine(output.script_pubkey.clone()) {
                                        received += output.value.to_sat();
                                    }
                                }
                                
                                // Check inputs for sent amounts
                                for input in &tx.input {
                                    if let Some(prev_tx) = wallet.get_tx(input.previous_output.txid) {
                                        if let Some(prev_output) = prev_tx.tx_node.tx.output.get(input.previous_output.vout as usize) {
                                            if wallet.is_mine(prev_output.script_pubkey.clone()) {
                                                sent += prev_output.value.to_sat();
                                            }
                                        }
                                    }
                                }
                                
                                let net_change = received as i64 - sent as i64;
                                
                                println!("  📄 TXID: {}", txid);
                                println!("     Received: {} sats", received);
                                println!("     Sent: {} sats", sent);
                                println!("     Net change: {} sats", net_change);
                                
                                match &tx_details.chain_position {
                                    bdk_wallet::chain::ChainPosition::Confirmed { anchor, .. } => {
                                        println!("     Confirmed at height: {}", anchor.block_id.height);
                                    }
                                    bdk_wallet::chain::ChainPosition::Unconfirmed { .. } => {
                                        println!("     Status: Unconfirmed");
                                    }
                                }
                                println!();
                            }
                        }
                    } else {
                        println!("📊 Balance {} (no change)", 
                               balance_after);
                    }
                }
                Err(e) => {
                    eprintln!("❌ Sync failed - {}", e);
                }
            }
        }
        
        Ok(())
    }
}