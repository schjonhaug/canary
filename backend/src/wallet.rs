use bdk_wallet::{bitcoin::Network, KeychainKind, Wallet, ChangeSet, PersistedWallet};
use bdk_wallet::file_store::Store;
use miniscript::{Descriptor, DescriptorPublicKey, descriptor::checksum::desc_checksum};
use std::error::Error;
use std::path::PathBuf;

pub struct WalletManager {
    wallets: Vec<PersistedWallet<Store<ChangeSet>>>,
    wallet_dir: PathBuf,
}

impl WalletManager {
    pub fn new() -> Self {
        let wallet_dir = PathBuf::from("./wallets");
        // Create wallet directory if it doesn't exist
        if let Err(e) = std::fs::create_dir_all(&wallet_dir) {
            eprintln!("Warning: Failed to create wallet directory: {}", e);
        }
        
        WalletManager {
            wallets: Vec::new(),
            wallet_dir,
        }
    }

    pub async fn create_from_multipath(&mut self, descriptor_str: &str) -> Result<String, Box<dyn Error>> {
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

        // Generate checksum for filename using receive descriptor
        let checksum = desc_checksum(&receive_descriptor)
            .map_err(|e| format!("Failed to generate checksum: {}", e))?;
        
        // Create wallet file path using checksum
        let wallet_filename = format!("{}.db", checksum);
        let wallet_path = self.wallet_dir.join(wallet_filename);

        // Open or create file store
        let (mut db, _changeset) = Store::<ChangeSet>::load_or_create(
            b"magic_bytes", 
            &wallet_path
        ).map_err(|e| format!("Failed to create/load wallet store: {}", e))?;

        // Set network
        let network = Network::Regtest;

        // Try to load existing wallet, or create new one
        let wallet_opt = Wallet::load()
            .descriptor(KeychainKind::External, Some(receive_descriptor.clone()))
            .descriptor(KeychainKind::Internal, Some(change_descriptor.clone()))
            .extract_keys()
            .check_network(network)
            .load_wallet(&mut db)
            .map_err(|e| format!("Failed to load wallet: {}", e))?;

        let mut wallet = match wallet_opt {
            Some(wallet) => wallet,
            None => Wallet::create(receive_descriptor, change_descriptor)
                .network(network)
                .create_wallet(&mut db)
                .map_err(|e| format!("Failed to create wallet: {}", e))?,
        };

        // Get the first address
        let first_address = wallet.reveal_next_address(KeychainKind::External);
        
        // Persist wallet changes to file store
        wallet.persist(&mut db)
            .map_err(|e| format!("Failed to persist wallet: {}", e))?;
        
        // Add wallet to the in-memory manager 
        self.wallets.push(wallet);
        
        Ok(first_address.address.to_string())
    }
}