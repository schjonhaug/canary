use bdk_wallet::{bitcoin::Network, KeychainKind, Wallet};
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

    pub fn get_new_address(&mut self) -> Result<String, Box<dyn Error>> {
        // Get a new address to receive bitcoin.
        let receive_address = self.wallet.reveal_next_address(KeychainKind::External);
        Ok(receive_address.address.to_string())
    }
}