//! Canary backend server.
//!
//! Sets up the process (TLS crypto provider, `.env`, logging) and hands over
//! to [`canary::app::run`].

use canary::config::AppConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    canary::tls::install_default_rustls_crypto_provider();

    // Load .env before logging so CANARY_FILE_LOGGING can take effect. A
    // missing file is fine: production configures the environment directly.
    let _ = dotenvy::dotenv();
    canary::app::init_tracing()?;

    let config = AppConfig::load()?;
    canary::app::run(config).await
}
