use bdk_wallet::{bitcoin::Network, KeychainKind, Wallet};
use miniscript::{Descriptor, DescriptorPublicKey};
use std::error::Error;

pub struct WalletManager {
    wallet: Wallet,
}

impl WalletManager {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        // Create a simple in-memory wallet for now
        let network = Network::Regtest;
        let receive_descriptor = "wpkh(tprv8ZgxMBicQKsPdcAqYBpzAFwU5yxBUo88ggoBqu1qPcHUfSbKK1sKMLmC7EAk438btHQrSdu3jGGQa6PA71nvH5nkDexhLteJqkM4dQmWF9g/84'/1'/0'/0/*)";
        let change_descriptor = "wpkh(tprv8ZgxMBicQKsPdcAqYBpzAFwU5yxBUo88ggoBqu1qPcHUfSbKK1sKMLmC7EAk438btHQrSdu3jGGQa6PA71nvH5nkDexhLteJqkM4dQmWF9g/84'/1'/0'/1/*)";
        
        let wallet = Wallet::create(receive_descriptor, change_descriptor)
            .network(network)
            .create_wallet_no_persist()?;

        Ok(WalletManager { wallet })
    }

    pub async fn create_from_multipath(&self, descriptor_str: &str) -> Result<String, Box<dyn Error>> {
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

        // Create wallet with the split descriptors
        let network = Network::Regtest;
        let mut wallet = Wallet::create(receive_descriptor, change_descriptor)
            .network(network)
            .create_wallet_no_persist()?;

        // Get the first address
        let first_address = wallet.reveal_next_address(KeychainKind::External);
        
        Ok(first_address.address.to_string())
    }
}