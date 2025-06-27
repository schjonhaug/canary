mod electrum;
mod wallet;
use electrum::ElectrumClient;
use wallet::WalletManager; 

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let electrum_client = ElectrumClient::new_regtest()?;
    let features = electrum_client.server_features()?;
    println!("Connected to Electrum server: {}", features);
    
    let mut wallet = WalletManager::new()?;
    let address = wallet.get_new_address()?;
    println!("New wallet address: {}", address);
    
    Ok(())
}
