mod electrum;
mod wallet;
mod api;
use electrum::ElectrumClient;
use wallet::WalletManager;
use api::create_router;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let electrum_client = ElectrumClient::new_regtest()?;
    let features = electrum_client.server_features()?;
    println!("Connected to Electrum server: {}", features);
    
    let wallet_manager = WalletManager::new()?;
    
    let app = create_router(wallet_manager);
    
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    println!("Server running on http://127.0.0.1:3000");
    
    axum::serve(listener, app).await?;
    
    Ok(())
}
