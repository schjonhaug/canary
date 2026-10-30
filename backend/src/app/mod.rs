//! Application assembly: wires configuration, storage, notification providers
//! and background tasks together and serves the HTTP API.
//!
//! The binary in `main.rs` only sets up the process (TLS provider, `.env`,
//! logging) and then hands over to [`run`].

mod demo;
mod dispatcher;
mod logging;
mod providers;
mod tasks;

pub use logging::init_tracing;

use crate::api::{self, AppServices};
use crate::config::AppConfig;
use crate::exchange_rates::ExchangeRateService;
use crate::metadata::TransactionNotification;
use crate::stripe_billing::StripeBilling;
use crate::subscription::SubscriptionTier;
use crate::wallet::{WalletCreationService, WalletManager};
use anyhow::Context;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::info;

/// Capacity of the channel that carries transaction and balance alert events
/// from wallet sync to the notification dispatcher.
const NOTIFICATION_CHANNEL_CAPACITY: usize = 100;

/// Builds every service from `config`, starts the background tasks and serves
/// the HTTP API until the server stops.
pub async fn run(config: AppConfig) -> anyhow::Result<()> {
    log_startup_summary(&config);
    config.validate_required_config().map_err(|error| {
        anyhow::anyhow!(
            "Configuration validation failed:\n{error}\n\nCheck your .env file \
             (see backend/.env.example) and ensure all required variables are set."
        )
    })?;
    info!("Configuration validated");

    let (notification_tx, _) =
        broadcast::channel::<TransactionNotification>(NOTIFICATION_CHANNEL_CAPACITY);

    let wallet_manager = Arc::new(
        WalletManager::new(
            notification_tx.clone(),
            config.effective_wallet_dir().into(),
            &config.effective_metadata_db(),
            config.network(),
            &config.electrum_url(),
            &config,
        )
        .await,
    );

    // Web handlers only need metadata access and non-blocking wallet creation,
    // so they get their own service bundle instead of the full wallet manager.
    let app_services = Arc::new(AppServices {
        metadata_db: wallet_manager.metadata_db.clone(),
        wallet_creation_service: WalletCreationService::new(
            wallet_manager.wallet_dir.clone(),
            wallet_manager.metadata_db.clone(),
            wallet_manager.get_electrum_client().await,
            wallet_manager.get_network(),
            wallet_manager.clone(),
        ),
    });

    if !config.is_restore_drill() {
        let exchange_rates = ExchangeRateService::new(Arc::new(app_services.metadata_db.clone()))?;
        Arc::new(exchange_rates).start_refresh_task();
    }

    let notification_manager =
        Arc::new(providers::build_notification_manager(&config, &app_services.metadata_db).await);
    let stripe_billing = init_stripe_billing(&config, &app_services).await?;

    if !config.is_restore_drill() {
        demo::seed_demo_wallets(&app_services, config.network()).await;
    }

    tasks::spawn_block_header_sync(&config, &wallet_manager).await;
    tasks::spawn_wallet_sync(&config, &wallet_manager);
    tasks::spawn_session_cleanup(&wallet_manager);
    dispatcher::NotificationDispatcher::new(
        notification_manager.clone(),
        wallet_manager.clone(),
        config.clone(),
    )
    .spawn(notification_tx.subscribe());

    let router = api::create_router_with_services(
        &app_services,
        &notification_manager,
        &stripe_billing,
        config.clone(),
        wallet_manager.get_electrum_manager(),
    );

    if config.is_cloud_mode() {
        tasks::spawn_startup_subscription_limits(&wallet_manager);
    } else {
        info!("Self-hosted mode: skipping subscription limit enforcement");
    }

    let startup_db = app_services.metadata_db.clone();
    tokio::spawn(async move { startup_db.run_startup_checks().await });

    let listener = tokio::net::TcpListener::bind(&config.bind_address)
        .await
        .with_context(|| format!("Failed to bind {}", config.bind_address))?;
    info!("Server running on http://{}", config.bind_address);

    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;

    Ok(())
}

fn log_startup_summary(config: &AppConfig) {
    info!(
        version = env!("CARGO_PKG_VERSION"),
        network = ?config.network,
        electrum_url = %config.electrum_url(),
        bind_address = %config.bind_address,
        wallet_dir = %config.effective_wallet_dir(),
        metadata_db = %config.effective_metadata_db(),
        "Starting Canary"
    );

    if config.is_self_hosted_mode() {
        info!(
            "Per-wallet sync target: {}s (self-hosted mode)",
            config.sync_interval()
        );
    } else {
        let (personal, team) = SubscriptionTier::get_sync_intervals(&config.network);
        info!("Sync intervals: Personal={personal}s, Team={team}s (cloud mode)");
    }

    info!("Operating mode: {}", config.operating_mode());
    if config.is_restore_drill() {
        info!("Restore drill: billing and customer notifications are disabled");
    }

    match config.btcpay_url() {
        Some(url) if config.is_btcpay_enabled() => info!("BTCPay Server: enabled ({url})"),
        _ => info!("BTCPay Server: not configured (donation endpoints disabled)"),
    }
}

/// Initializes Stripe when it is the configured cloud billing provider.
///
/// A configured but unusable Stripe account is a startup error: running
/// without billing would silently stop subscription handling.
async fn init_stripe_billing(
    config: &AppConfig,
    app_services: &AppServices,
) -> anyhow::Result<Option<Arc<StripeBilling>>> {
    if !(config.is_cloud_mode() && config.is_stripe_enabled()) {
        info!("Stripe billing disabled");
        return Ok(None);
    }

    let billing = StripeBilling::new(Arc::new(app_services.metadata_db.clone()))
        .await
        .context(
            "Stripe billing initialization failed. Check that STRIPE_SECRET_KEY is valid, \
             STRIPE_WEBHOOK_SECRET is set, and Stripe products have metadata.tier set to \
             'personal' or 'team'",
        )?;
    info!("Stripe billing initialized");
    Ok(Some(Arc::new(billing)))
}
