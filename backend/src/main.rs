use sqlx::{PgPool, Row};

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://username:password@localhost/txray".to_string());
    
    let pool = PgPool::connect(&database_url).await?;
    
    println!("Connected to PostgreSQL database!");
    
    Ok(())
}
