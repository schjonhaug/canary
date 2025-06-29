mod electrum;
mod wallet;
mod api;
mod metadata;
use electrum::ElectrumClient;
use wallet::WalletManager;
use api::create_router;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let electrum_client = ElectrumClient::new_regtest()?;
    let features = electrum_client.server_features()?;
    println!("Connected to Electrum server: {}", features);
    
    let wallet_manager = Arc::new(Mutex::new(WalletManager::new().await));
    
    // Spawn background task for wallet syncing
    let wallet_manager_sync = Arc::clone(&wallet_manager);
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(4));
        
        loop {
            interval.tick().await;
            
            let mut manager = wallet_manager_sync.lock().await;
            if let Err(e) = manager.sync_all_wallets().await {
                eprintln!("Error syncing wallets: {}", e);
            }
        }
    });
    
    let app = create_router(wallet_manager);
    
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    println!("Server running on http://127.0.0.1:3000");
    
    axum::serve(listener, app).await?;
    
    Ok(())
}
