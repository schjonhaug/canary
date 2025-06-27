use sqlx::PgPool;

mod electrum;
mod wallet;
use electrum::ElectrumClient;
use wallet::WalletManager; 

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://schjonhaug@localhost:5432/txray".to_string());
    
    let _pool = PgPool::connect(&database_url).await?;
    println!("Connected to PostgreSQL database!");
    
    let electrum_client = ElectrumClient::new_regtest()?;
    let features = electrum_client.server_features().await?;
    println!("Connected to Electrum server: {}", features);
    
    let mut wallet = WalletManager::new()?;
    let address = wallet.get_new_address()?;
    println!("New wallet address: {}", address);
    
    Ok(())
}
