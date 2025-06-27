use sqlx::{PgPool, Row};

mod electrum;
use electrum::ElectrumClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://schjonhaug@localhost:5432/txray".to_string());
    
    let _pool = PgPool::connect(&database_url).await?;
    println!("Connected to PostgreSQL database!");
    
    let mut electrum_client = ElectrumClient::new_regtest()?;
    let features = electrum_client.server_features()?;
    println!("Connected to Electrum server: {}", features);
    
    Ok(())
}
