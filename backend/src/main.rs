mod admin_notifications;
mod api;
mod auth;
mod config;
mod electrum;
mod email_provider;
mod email_queue;
mod email_service;
mod exchange_rates;
mod message_formatter;
mod metadata;
mod migrations;
mod notification_failure_tracker;
mod notifications;
mod ntfy_provider;
mod stripe_billing;
mod stripe_client_service;
mod subscription;
mod sync;
mod twilio_provider;
mod wallet;
mod xpub_converter;

use config::AppConfig;
use email_provider::EmailProvider;
use metadata::TransactionNotification;
use notifications::NotificationManager;
use ntfy_provider::{NtfyAuth, NtfyProvider};
use std::sync::Arc;
use std::time::Instant;
use stripe_billing::StripeBilling;
use subscription::SubscriptionTier;
use tokio::sync::{broadcast, Mutex};
use tokio::time::{interval, Duration};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use twilio_provider::TwilioProvider;
use wallet::WalletManager;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Set up file appender - writes to logs/backend.log with daily rotation
    let log_dir = std::env::current_dir()?.join("logs");
    std::fs::create_dir_all(&log_dir)?;
    let file_appender = RollingFileAppender::new(Rotation::DAILY, &log_dir, "backend.log");

    // Create env filter for log levels
    let env_filter =
        EnvFilter::from_default_env().add_directive("canary=info".parse()?);

    // Initialize tracing with both stdout and file output
    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_writer(std::io::stdout))
        .with(fmt::layer().with_writer(file_appender).with_ansi(false))
        .init();

    // Load configuration
    let config = AppConfig::load()?;

    // Display ASCII art
    println!(
        r#"
 ▄████████    ▄████████ ███▄▄▄▄      ▄████████    ▄████████ ▄██   ▄  
███    ███   ███    ███ ███▀▀▀██▄   ███    ███   ███    ███ ███   ██▄
███    █▀    ███    ███ ███   ███   ███    ███   ███    ███ ███▄▄▄███
███          ███    ███ ███   ███   ███    ███  ▄███▄▄▄▄██▀ ▀▀▀▀▀▀███
███        ▀███████████ ███   ███ ▀███████████ ▀▀███▀▀▀▀▀   ▄██   ███
███    █▄    ███    ███ ███   ███   ███    ███ ▀███████████ ███   ███
███    ███   ███    ███ ███   ███   ███    ███   ███    ███ ███   ███
████████▀    ███    █▀   ▀█   █▀    ███    █▀    ███    ███  ▀█████▀ 
                                                 ███    ███          
    "#
    );

    println!(
        "Starting Canary v{} with configuration:",
        env!("CARGO_PKG_VERSION")
    );
    println!("  Network: {:?}", config.network);
    println!("  Electrum URL: {}", config.electrum_url());
    println!("  Bind address: {}", config.bind_address);
    println!("  Wallet directory: {}", config.effective_wallet_dir());
    println!("  Metadata DB: {}", config.effective_metadata_db());
    // Display mode-appropriate sync intervals
    if config.is_self_hosted_mode() {
        let sync_interval = config.get_sync_interval();
        println!(
            "  Sync interval: {}s (self-hosted mode, network: {:?})",
            sync_interval, config.network
        );
    } else {
        let (personal_sync, _team_sync) =
            SubscriptionTier::Personal.get_sync_intervals(&config.network);
        let (_, team_sync_team) = SubscriptionTier::Team.get_sync_intervals(&config.network);
        println!(
            "  Sync intervals: Personal={}s, Team={}s (SAAS mode, network: {:?})",
            personal_sync, team_sync_team, config.network
        );
    }

    // Log operating mode
    println!(
        "🏢 Operating mode: {}",
        config.operating_mode().to_uppercase()
    );
    if config.is_cloud_mode() {
        println!("   - Multi-user with authentication");
        println!("   - Subscription billing enabled");
        println!("   - All notification providers available");
    } else {
        println!("   - Single-user mode (no authentication)");
        println!("   - No billing/subscriptions");
        println!("   - ntfy-only notifications");
    }

    // Validate required configuration for the selected mode
    if let Err(error) = config.validate_required_config() {
        eprintln!("❌ Configuration validation failed:");
        eprintln!("{}", error);
        eprintln!();
        eprintln!("Please check your .env file and ensure all required variables are set.");
        eprintln!("See backend/.env.example for configuration examples.");
        std::process::exit(1);
    }
    println!("✅ Configuration validated successfully");

    // Create wallet manager with sync worker
    println!("Creating wallet sync worker...");

    let (notification_tx, _notification_rx) = broadcast::channel::<TransactionNotification>(100);

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

    // Create non-blocking architecture: Separate metadata access from heavy wallet operations
    let app_services = {
        // Get current electrum client from the manager for wallet creation service
        let electrum_client = wallet_manager.get_electrum_client().await;
        let wallet_creation_service = wallet::WalletCreationService::new(
            wallet_manager.wallet_dir.clone(),
            wallet_manager.metadata_db.clone(),
            electrum_client,
            wallet_manager.get_network(),
            wallet_manager.wallets.clone(), // Pass reference to in-memory wallet storage
        );
        Arc::new(api::AppServices {
            metadata_db: wallet_manager.metadata_db.clone(),
            wallet_creation_service,
        })
    };

    // Create shared state for current block header and load existing header from database
    let existing_header = wallet_manager
        .metadata_db
        .get_current_block_header()
        .await
        .unwrap_or(None);
    let current_block_header = Arc::new(Mutex::new(existing_header));

    // Initialize exchange rate service and start background refresh task
    {
        let exchange_rate_service = Arc::new(exchange_rates::ExchangeRateService::new(Arc::new(
            wallet_manager.metadata_db.clone(),
        )));

        // Start background task to refresh exchange rates every 10 minutes
        exchange_rate_service.clone().start_refresh_task();
    }

    // Create notification manager and register providers based on operating mode
    let mut notification_manager = NotificationManager::new();

    if config.is_self_hosted_mode() {
        // FOSS mode: Only ntfy provider
        let ntfy_server = config.ntfy_server_url();
        println!(
            "🔔 FOSS mode: Registering ntfy notifications (server: {})",
            ntfy_server
        );
        notification_manager.register_provider(Arc::new(NtfyProvider::new(ntfy_server)));
    } else {
        // SAAS mode: Register all configured providers
        println!("🔔 SAAS mode: Registering all notification providers");

        // Register ntfy provider (always available)
        if config.is_ntfy_enabled() {
            let ntfy_server = config.ntfy_server_url();
            println!("  - ntfy notification provider (server: {})", ntfy_server);
            notification_manager.register_provider(Arc::new(NtfyProvider::new(ntfy_server)));
        }

        // Register Twilio SMS provider if enabled and configured
        if config.is_twilio_enabled() {
            match TwilioProvider::from_env() {
                Some(twilio_provider) => {
                    println!("  - Twilio SMS notification provider");
                    notification_manager.register_provider(Arc::new(twilio_provider));
                }
                None => {
                    println!("⚠️  Twilio provider enabled but missing environment variables:");
                    println!("    - TWILIO_ACCOUNT_SID");
                    println!("    - TWILIO_AUTH_TOKEN");
                    println!("    - TWILIO_SENDER_ID");
                }
            }
        }

        // Register email provider if in SAAS mode
        if config.is_email_enabled() {
            // Start the email queue worker before registering the provider
            match email_queue::EmailQueueConfig::from_env() {
                Ok(email_queue_config) => {
                    if let Err(e) = email_queue::start_email_queue_worker(email_queue_config).await
                    {
                        println!("⚠️  Failed to start email queue worker: {}", e);
                        println!("   Email notifications will not work");
                    } else {
                        println!("  - Email notification provider (with rate-limited queue)");
                        notification_manager.register_provider(Arc::new(EmailProvider::new()));
                    }
                }
                Err(e) => {
                    println!(
                        "⚠️  Email provider enabled but missing configuration: {}",
                        e
                    );
                }
            }
        }
    }

    let notification_manager = Arc::new(Mutex::new(notification_manager));

    // Initialize Stripe billing only in cloud mode
    let stripe_billing = if config.is_cloud_mode() {
        println!("🏦 Cloud mode: Initializing Stripe billing...");
        match StripeBilling::new(Arc::new(app_services.metadata_db.clone())).await {
            Ok(billing) => {
                println!("✅ Stripe billing initialized successfully");
                Some(Arc::new(billing))
            }
            Err(e) => {
                println!("⚠️  Stripe billing initialization failed: {}", e);
                println!("   Billing endpoints will not be available");
                None
            }
        }
    } else {
        println!("💵 FOSS mode: Stripe billing disabled");
        None
    };

    // For demo user, ensure bacon wallet is created through normal wallet creation flow (ONCE only)
    // This works for regtest, testnet, and mainnet with network-specific descriptors
    {
        // Network-specific bacon wallet descriptors (watch-only XPUBs)
        let bacon_descriptor = match config.network() {
            bdk_wallet::bitcoin::Network::Regtest => {
                // Regtest bacon wallet (funded in docker-utils.sh)
                Some("wpkh([9a6a2580/84h/1h/0h]tpubDCMRAYcH71Gagskm7E5peNMYB5sKaLLwtn2c4Rb3CMUTRVUk5dkpsskhspa5MEcVZ11LwTcM7R5mzndUCG9WabYcT5hfQHbYVoaLFBZHPCi/<0;1>/*)#4laqdwct")
            }
            bdk_wallet::bitcoin::Network::Bitcoin => {
                // Mainnet bacon wallet (real wallet with transaction history)
                Some("wpkh([00000000/84h/0h/0h]xpub6DEzNop46vmxR49zYWFnMwmEfawSNmAMf6dLH5YKDY463twtvw1XD7ihwJRLPRGZJz799VPFzXHpZu6WdhT29WnaeuChS6aZHZPFmqczR5K/<0;1>/*)#4jhrljfg")
            }
            _ => None, // Skip for testnet and other networks
        };

        if let Some(descriptor) = bacon_descriptor {
            // Get demo user from database
            let demo_user_result = app_services
                .metadata_db
                .get_user_by_email("demo@canarybitcoin.com")
                .await;

            if let Ok(Some(demo_user)) = demo_user_result {
                // Check if demo user already has a "Bacon Wallet" by name
                // (checking by name instead of descriptor to avoid BDK normalization issues)
                let existing_wallets = app_services
                    .metadata_db
                    .get_wallets_for_user(Some(&demo_user.id))
                    .await
                    .unwrap_or_default();
                let bacon_exists = existing_wallets.iter().any(|w| w.name == "Bacon Wallet");

                if !bacon_exists {
                    // Create bacon wallet through normal wallet creation flow
                    match app_services
                        .wallet_creation_service
                        .create_wallet_non_blocking(
                            "Bacon Wallet",
                            descriptor,
                            &demo_user.id,
                            false, // not a fresh wallet
                            None,  // auto-detect script type
                            None,  // default stop gap
                        )
                        .await
                    {
                        Ok(wallet_metadata) => {
                            println!(
                                "✅ Created Bacon wallet for demo user (network: {:?})",
                                config.network()
                            );

                            // Add "Canary" contact with email notification
                            use metadata::{Language, ProviderType};
                            match app_services
                                .metadata_db
                                .insert_contact_with_notification_methods(
                                    &wallet_metadata.checksum,
                                    "Canary",
                                    &Language::English,
                                    vec![(
                                        ProviderType::Email,
                                        "contact@canarybitcoin.com".to_string(),
                                    )],
                                )
                                .await
                            {
                                Ok(_) => println!("✅ Added Canary contact to Bacon wallet"),
                                Err(e) => println!("❌ Failed to add Canary contact: {}", e),
                            }

                            // Create balance alert: when balance equals 0 BTC
                            use metadata::BalanceAlertType;
                            match app_services
                                .metadata_db
                                .create_balance_alert(
                                    &wallet_metadata.checksum,
                                    0, // 0 sats
                                    BalanceAlertType::Equals,
                                    None, // BTC threshold
                                    None,
                                    None, // current balance (demo setup)
                                )
                                .await
                            {
                                Ok(_) => println!("✅ Added 0 BTC balance alert to Bacon wallet"),
                                Err(e) => println!("❌ Failed to add 0 BTC balance alert: {}", e),
                            }

                            // Create balance alert: when balance is above 0.21 BTC (21,000,000 sats)
                            match app_services
                                .metadata_db
                                .create_balance_alert(
                                    &wallet_metadata.checksum,
                                    21_000_000, // 0.21 BTC in sats
                                    BalanceAlertType::Above,
                                    None, // BTC threshold
                                    None,
                                    None, // current balance (demo setup)
                                )
                                .await
                            {
                                Ok(_) => {
                                    println!("✅ Added >0.21 BTC balance alert to Bacon wallet")
                                }
                                Err(e) => {
                                    println!("❌ Failed to add >0.21 BTC balance alert: {}", e)
                                }
                            }
                        }
                        Err(e) => println!("❌ Failed to create Bacon wallet: {}", e),
                    }
                }
            }
        }
    }

    // Try to fetch initial block header in background with timeout
    {
        let initial_wallet_manager = Arc::clone(&wallet_manager);
        let initial_block_header = Arc::clone(&current_block_header);
        tokio::spawn(async move {
            // Use tokio timeout to prevent indefinite blocking
            let timeout_duration = Duration::from_secs(5);

            let electrum_manager = initial_wallet_manager.get_electrum_manager();
            let metadata_db = initial_wallet_manager.metadata_db.clone();

            if let Some(ref electrum_mgr) = electrum_manager {
                // Try to get the client, attempt reconnection if needed
                let client = match electrum_mgr.get_client().await {
                    Some(c) => c,
                    None => {
                        println!("📦 No Electrum connection, attempting reconnection...");
                        match electrum_mgr.reconnect().await {
                            Ok(true) => match electrum_mgr.get_client().await {
                                Some(c) => c,
                                None => {
                                    eprintln!("⚠️  Still no Electrum client after reconnection");
                                    return;
                                }
                            },
                            _ => {
                                eprintln!("⚠️  Failed to reconnect to Electrum server");
                                return;
                            }
                        }
                    }
                };

                println!("📦 Attempting to fetch initial block header (5s timeout)...");

                // Run the async operation with timeout
                let height_result =
                    tokio::time::timeout(timeout_duration, client.get_current_block_height()).await;

                match height_result {
                    Ok(Ok(height)) => {
                        // Successfully got height, now get the header
                        match client.get_block_header(height).await {
                            Ok(block_header) => {
                                println!(
                                    "📦 Initial block header fetched: height={}",
                                    block_header.height
                                );

                                // Store in database
                                if let Err(e) =
                                    metadata_db.upsert_current_block_header(&block_header).await
                                {
                                    eprintln!("Failed to store initial block header: {}", e);
                                }

                                // Update shared state
                                let mut current_header = initial_block_header.lock().await;
                                *current_header = Some(block_header.clone());
                            }
                            Err(e) => {
                                eprintln!("❌ Failed to get block header: {}", e);
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        eprintln!("❌ Electrum error getting block height: {}", e);
                    }
                    Err(_) => {
                        eprintln!("⏱️  Timeout getting block height from Electrum (5s exceeded)");
                        eprintln!("   Electrum may be down or unresponsive");
                    }
                }
            } else {
                eprintln!("⚠️  No Electrum client manager available - cannot fetch block headers");
            }
        });
    }

    // Mode-based sync task configuration
    if config.is_self_hosted_mode() {
        // FOSS mode: Single sync task using CANARY_SYNC_INTERVAL
        let sync_interval = config.get_sync_interval();
        let foss_wallet_manager = Arc::clone(&wallet_manager);

        println!(
            "🕐 FOSS sync interval: {}s (network: {:?})",
            sync_interval, config.network
        );

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(sync_interval));

            loop {
                interval.tick().await;

                // In FOSS mode, sync all wallets together (no tier separation)
                let sync_start = Instant::now();

                // In FOSS mode, all wallets belong to the hardcoded "team" tier user
                // No need to sync Personal tier since no FOSS wallets use that tier
                if let Err(e) = foss_wallet_manager
                    .sync_tier_parallel(SubscriptionTier::Team)
                    .await
                {
                    eprintln!("❌ Failed to sync FOSS wallets: {}", e);
                }

                let sync_duration = sync_start.elapsed();
                if sync_duration.as_millis() > 100 {
                    println!("⚡ FOSS sync completed in {:?}", sync_duration);
                }
            }
        });
    } else {
        // SAAS mode: Separate tier-based sync tasks
        let (personal_sync_interval, team_sync_interval) =
            SubscriptionTier::Personal.get_sync_intervals(&config.network);

        // Team tier sync task (more frequent)
        let team_wallet_manager = Arc::clone(&wallet_manager);
        println!(
            "🕐 Team tier sync interval: {}s (network: {:?})",
            team_sync_interval, config.network
        );
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(team_sync_interval));

            loop {
                interval.tick().await;

                // Sync Team tier wallets
                if let Err(e) = team_wallet_manager
                    .sync_tier_parallel(SubscriptionTier::Team)
                    .await
                {
                    eprintln!("Team tier sync failed: {}", e);
                }
            }
        });

        // Personal tier sync task (less frequent)
        let personal_wallet_manager = Arc::clone(&wallet_manager);
        println!(
            "🕐 Personal tier sync interval: {}s (network: {:?})",
            personal_sync_interval, config.network
        );
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(personal_sync_interval));

            loop {
                interval.tick().await;

                // Sync Personal tier wallets
                if let Err(e) = personal_wallet_manager
                    .sync_tier_parallel(SubscriptionTier::Personal)
                    .await
                {
                    eprintln!("Personal tier sync failed: {}", e);
                }
            }
        });

        // Log non-syncing wallets summary at startup (SAAS mode only)
        let startup_wallet_manager = Arc::clone(&wallet_manager);
        tokio::spawn(async move {
            // Wait a moment for sync tasks to start
            tokio::time::sleep(Duration::from_secs(2)).await;

            if let Ok(non_syncing_summary) = startup_wallet_manager
                .metadata_db
                .get_non_syncing_wallets_summary()
                .await
            {
                if non_syncing_summary.total_non_syncing > 0 {
                    let mut reasons = Vec::new();
                    if non_syncing_summary.expired_trials > 0 {
                        reasons.push(format!(
                            "{} expired trials",
                            non_syncing_summary.expired_trials
                        ));
                    }
                    if non_syncing_summary.cancelled_subscriptions > 0 {
                        reasons.push(format!(
                            "{} cancelled",
                            non_syncing_summary.cancelled_subscriptions
                        ));
                    }
                    if non_syncing_summary.expired_subscriptions > 0 {
                        reasons.push(format!(
                            "{} expired",
                            non_syncing_summary.expired_subscriptions
                        ));
                    }
                    if non_syncing_summary.past_due_subscriptions > 0 {
                        reasons.push(format!(
                            "{} past_due",
                            non_syncing_summary.past_due_subscriptions
                        ));
                    }
                    if non_syncing_summary.inactive_wallets > 0 {
                        reasons.push(format!(
                            "{} inactive (tier limits)",
                            non_syncing_summary.inactive_wallets
                        ));
                    }

                    println!(
                        "🔒 Startup subscription status: {} wallets not syncing ({})",
                        non_syncing_summary.total_non_syncing,
                        reasons.join(", ")
                    );
                } else {
                    println!("✅ All wallets have active subscriptions");
                }
            }
        });
    }

    // Block header sync task (mode-aware frequency)
    let block_sync_interval = if config.is_self_hosted_mode() {
        config.get_sync_interval()
    } else {
        let (_, team_sync_interval) =
            SubscriptionTier::Personal.get_sync_intervals(&config.network);
        team_sync_interval
    };

    let block_sync_wallet_manager = Arc::clone(&wallet_manager);
    let block_sync_current_block_header = Arc::clone(&current_block_header);
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(block_sync_interval));

        loop {
            interval.tick().await;

            // Check for new block headers with timeout
            if let Some(ref electrum_mgr) = &block_sync_wallet_manager.electrum_client_manager {
                let metadata_db = block_sync_wallet_manager.metadata_db.clone();
                let current_block_header_clone = block_sync_current_block_header.clone();
                let electrum_mgr = electrum_mgr.clone();

                // Try to get the client, attempt reconnection if needed
                let client = match electrum_mgr.get_client().await {
                    Some(c) => c,
                    None => {
                        // Attempt reconnection
                        match electrum_mgr.reconnect().await {
                            Ok(true) => match electrum_mgr.get_client().await {
                                Some(c) => c,
                                None => continue,
                            },
                            _ => continue,
                        }
                    }
                };

                // Get current stored height
                let stored_header = block_sync_current_block_header.lock().await;
                let stored_height = stored_header.as_ref().map(|h| h.height).unwrap_or(0);
                drop(stored_header);

                // Run async operation with timeout
                let height_result =
                    tokio::time::timeout(Duration::from_secs(5), client.get_current_block_height())
                        .await;

                match height_result {
                    Ok(Ok(current_height)) => {
                        if current_height > stored_height {
                            // Get the actual block header
                            match client.get_block_header(current_height).await {
                                Ok(block_header) => {
                                    println!(
                                        "📦 New block header: height={} (was {})",
                                        block_header.height, stored_height
                                    );

                                    // Store in database
                                    if let Err(e) =
                                        metadata_db.upsert_current_block_header(&block_header).await
                                    {
                                        eprintln!("Failed to store block header: {}", e);
                                    }

                                    // Update shared state
                                    let mut current_header =
                                        current_block_header_clone.lock().await;
                                    *current_header = Some(block_header.clone());
                                }
                                Err(e) => {
                                    let error_msg = e.to_string();
                                    eprintln!(
                                        "Failed to get block header for height {}: {}",
                                        current_height, error_msg
                                    );
                                    // Check if transport error and trigger reconnection
                                    if electrum::ElectrumClientManager::is_transport_error(
                                        &error_msg,
                                    ) {
                                        electrum_mgr.mark_disconnected(&error_msg).await;
                                        let _ = electrum_mgr.reconnect().await;
                                    }
                                }
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        let error_msg = e.to_string();
                        eprintln!("Electrum error checking block height: {}", error_msg);
                        // Check if transport error and trigger reconnection
                        if electrum::ElectrumClientManager::is_transport_error(&error_msg) {
                            electrum_mgr.mark_disconnected(&error_msg).await;
                            let _ = electrum_mgr.reconnect().await;
                        }
                    }
                    Err(_) => {
                        eprintln!("⏱️  Timeout checking block height, triggering reconnection");
                        electrum_mgr.mark_disconnected("Block height check timed out").await;
                        let _ = electrum_mgr.reconnect().await;
                    }
                }
            }
        }
    });

    // Create session cleanup task (runs every hour)
    let session_cleanup_manager = wallet_manager.clone();
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(3600)); // Run every hour

        loop {
            interval.tick().await;

            match session_cleanup_manager
                .metadata_db
                .cleanup_expired_sessions()
                .await
            {
                Ok(deleted) => {
                    if deleted > 0 {
                        println!("Cleaned up {} expired sessions", deleted);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to cleanup expired sessions: {}", e);
                }
            }
        }
    });

    // Create notification worker task
    let notification_worker_manager = notification_manager.clone();
    let notification_wallet_manager = wallet_manager.clone();
    let notification_event_rx = notification_tx.subscribe();
    tokio::spawn(async move {
        let mut rx = notification_event_rx;

        // Initialize failure trackers for SMS and Email providers
        let sms_failure_tracker =
            notification_failure_tracker::NotificationFailureTracker::new("twilio");
        let email_failure_tracker =
            notification_failure_tracker::NotificationFailureTracker::new("email");

        while let Ok(notification) = rx.recv().await {
            let manager = notification_worker_manager.lock().await;

            // Handle notification and extract wallet information
            let (wallet_checksum, notification_type) = match &notification {
                TransactionNotification::Pending(tx) => (&tx.wallet_checksum, "pending"),
                TransactionNotification::Confirmed(tx) => (&tx.wallet_checksum, "confirmed"),
                TransactionNotification::BalanceAlert(alert) => {
                    (&alert.wallet_checksum, "balance_alert")
                }
            };

            // Get wallet information for the notification
            if let Ok(Some(wallet_info)) = notification_wallet_manager
                .get_wallet_by_checksum(wallet_checksum)
                .await
            {
                // Get contacts for this wallet
                if let Ok(contacts) = notification_wallet_manager
                    .metadata_db
                    .get_contacts_with_notification_methods(wallet_checksum)
                    .await
                {
                    if !contacts.is_empty() {
                        // Look up user's ntfy server URL preference
                        let user_ntfy_server_url = notification_wallet_manager
                            .metadata_db
                            .get_user_ntfy_server_url(&wallet_info.user_id)
                            .await
                            .ok()
                            .flatten();

                        // Generate message content once (same for all providers)
                        let mut message_content = String::new();
                        let mut provider_counts = std::collections::HashMap::new();
                        let mut failed_count = 0;
                        let mut total_sent = 0;

                        // Try to send notifications using all available providers, filtered by subscription tier
                        let available_providers = manager.list_providers();
                        for provider_info in available_providers {
                            let provider_name = &provider_info.name;

                            // For ntfy, use user's preferred server URL and auth if set
                            let results = if provider_name == "ntfy" {
                                // Determine the ntfy server URL: user preference > env var > default
                                let ntfy_server = user_ntfy_server_url.clone().unwrap_or_else(|| {
                                    std::env::var("NTFY_SERVER_URL")
                                        .unwrap_or_else(|_| "https://ntfy.sh".to_string())
                                });

                                // Get ntfy authentication credentials
                                let ntfy_auth = match notification_wallet_manager
                                    .metadata_db
                                    .get_user_ntfy_auth(&wallet_info.user_id)
                                    .await
                                {
                                    Ok((Some(token), _, _)) => {
                                        NtfyAuth::AccessToken(token)
                                    }
                                    Ok((None, Some(username), Some(password))) => {
                                        NtfyAuth::BasicAuth { username, password }
                                    }
                                    _ => NtfyAuth::None,
                                };

                                let ntfy_provider = NtfyProvider::with_auth(ntfy_server, ntfy_auth);
                                use crate::notifications::NotificationProvider;
                                Ok(ntfy_provider
                                    .send_notification(&notification, &wallet_info.name, &contacts)
                                    .await)
                            } else {
                                manager
                                    .send_notifications(
                                        provider_name,
                                        &notification,
                                        &wallet_info.name,
                                        &contacts,
                                    )
                                    .await
                            };

                            // All notification types are now allowed for all tiers
                            if let Ok(results) = results {
                                for (notification_method, result, message) in results {
                                    // Store message content for summary (same for all providers)
                                    if message_content.is_empty() {
                                        message_content = message.clone();
                                    }

                                    // Log the notification attempt to database (keep this for audit)
                                    if let Some(ref method_id) = notification_method.id {
                                        let status = if result.success { "sent" } else { "failed" };

                                        // Handle logging based on notification type
                                        let log_result = match &notification {
                                            TransactionNotification::Pending(tx)
                                            | TransactionNotification::Confirmed(tx) => {
                                                // Log transaction notifications
                                                notification_wallet_manager
                                                    .metadata_db
                                                    .insert_notification_log_for_transaction(
                                                        &tx.txid,
                                                        &tx.wallet_checksum,
                                                        method_id,
                                                        provider_name,
                                                        result.provider_id.as_deref(),
                                                        status,
                                                        result.error_message.as_deref(),
                                                        &message,
                                                        notification_type,
                                                    )
                                                    .await
                                            }
                                            TransactionNotification::BalanceAlert(alert) => {
                                                // Use separate logging method for balance alerts
                                                notification_wallet_manager
                                                    .metadata_db
                                                    .insert_notification_log_for_balance_alert(
                                                        &alert.balance_alert_id,
                                                        &alert.wallet_checksum,
                                                        method_id,
                                                        provider_name,
                                                        result.provider_id.as_deref(),
                                                        status,
                                                        result.error_message.as_deref(),
                                                        &message,
                                                    )
                                                    .await
                                            }
                                        };

                                        if let Err(e) = log_result {
                                            eprintln!(
                                                "❌ Failed to log notification to database: {}",
                                                e
                                            );
                                        }
                                    }

                                    // Count results by provider
                                    if result.success {
                                        *provider_counts
                                            .entry(provider_name.clone())
                                            .or_insert(0) += 1;
                                        total_sent += 1;
                                    } else {
                                        failed_count += 1;
                                        // Log the actual error for debugging
                                        eprintln!(
                                            "❌ {} notification failed for {}: {}",
                                            provider_name,
                                            notification_method.notification_target,
                                            result.error_message.as_deref().unwrap_or("Unknown error")
                                        );
                                    }

                                    // Track failures for SMS and Email providers and send admin alerts
                                    if provider_name == "twilio" || provider_name == "email" {
                                        let tracker = if provider_name == "twilio" {
                                            &sms_failure_tracker
                                        } else {
                                            &email_failure_tracker
                                        };

                                        let display_name = if provider_name == "twilio" {
                                            "SMS (Twilio)"
                                        } else {
                                            "Email (Resend)"
                                        };

                                        if result.success {
                                            // Check if we should send recovery notification
                                            if tracker.record_success() {
                                                let admin =
                                                    admin_notifications::AdminNotifications::new();
                                                if admin.is_enabled() {
                                                    admin
                                                        .notify_provider_recovery(display_name)
                                                        .await;
                                                }
                                            }
                                        } else {
                                            // Record failure and check if we should alert
                                            let (should_alert, failures, category) = tracker
                                                .record_failure(result.error_message.as_deref())
                                                .await;
                                            if should_alert {
                                                let admin =
                                                    admin_notifications::AdminNotifications::new();
                                                if admin.is_enabled() {
                                                    admin
                                                        .notify_provider_failure(
                                                            display_name,
                                                            failures,
                                                            &category.to_string(),
                                                            result.error_message.as_deref(),
                                                            category.suggested_action(),
                                                        )
                                                        .await;
                                                }
                                                tracker.mark_alert_sent();
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Print concise summary
                        if total_sent > 0 || failed_count > 0 {
                            let mut provider_summary = Vec::new();
                            for (provider, count) in provider_counts.iter() {
                                provider_summary.push(format!("{}×{}", count, provider));
                            }
                            let provider_str = provider_summary.join(", ");

                            // Extract clean transaction summary from message
                            let transaction_summary = if message_content.contains("📤")
                                || message_content.contains("✅")
                                    && (message_content.contains("Sent")
                                        || message_content.contains("Sendt"))
                            {
                                // Extract amount from sending message, looking for " BTC" pattern
                                if let Some(btc_pos) = message_content.find(" BTC") {
                                    let before_btc = &message_content[..btc_pos];
                                    if let Some(space_pos) = before_btc.rfind(' ') {
                                        let amount = &before_btc[space_pos + 1..];
                                        // Determine if confirmed or unconfirmed based on message content
                                        if message_content.contains("✅")
                                            || message_content.contains("Sent")
                                            || message_content.contains("Sendt")
                                        {
                                            format!("✅ Sent {} BTC", amount)
                                        } else {
                                            format!("📤 Sending {} BTC", amount)
                                        }
                                    } else if message_content.contains("✅")
                                        || message_content.contains("Sent")
                                        || message_content.contains("Sendt")
                                    {
                                        "✅ Sent transaction".to_string()
                                    } else {
                                        "📤 Sending transaction".to_string()
                                    }
                                } else if message_content.contains("✅")
                                    || message_content.contains("Sent")
                                    || message_content.contains("Sendt")
                                {
                                    "✅ Sent transaction".to_string()
                                } else {
                                    "📤 Sending transaction".to_string()
                                }
                            } else if message_content.contains("📥")
                                || message_content.contains("💸")
                                || message_content.contains("✅")
                                    && (message_content.contains("Received")
                                        || message_content.contains("Mottatt"))
                            {
                                // Extract amount from receiving message
                                if let Some(btc_pos) = message_content.find(" BTC") {
                                    let before_btc = &message_content[..btc_pos];
                                    if let Some(space_pos) = before_btc.rfind(' ') {
                                        let amount = &before_btc[space_pos + 1..];
                                        // Determine if confirmed or unconfirmed based on message content
                                        if message_content.contains("✅")
                                            || message_content.contains("Received")
                                            || message_content.contains("Mottatt")
                                        {
                                            format!("✅ Received {} BTC", amount)
                                        } else {
                                            format!("💸 Receiving {} BTC", amount)
                                        }
                                    } else if message_content.contains("✅")
                                        || message_content.contains("Received")
                                        || message_content.contains("Mottatt")
                                    {
                                        "✅ Received transaction".to_string()
                                    } else {
                                        "💸 Receiving transaction".to_string()
                                    }
                                } else if message_content.contains("✅")
                                    || message_content.contains("Received")
                                    || message_content.contains("Mottatt")
                                {
                                    "✅ Received transaction".to_string()
                                } else {
                                    "💸 Receiving transaction".to_string()
                                }
                            } else {
                                "Transaction".to_string()
                            };

                            if failed_count == 0 {
                                println!(
                                    "🔔 Notified {} contacts for {}: {} ({})",
                                    contacts.len(),
                                    wallet_info.name,
                                    transaction_summary,
                                    provider_str
                                );
                            } else {
                                println!(
                                    "🔔 Notified {}/{} contacts for {}: {} ({}, {}×failed)",
                                    total_sent,
                                    total_sent + failed_count,
                                    wallet_info.name,
                                    transaction_summary,
                                    provider_str,
                                    failed_count
                                );
                            }
                        }
                    }
                }
            }
        }
    });

    let app = api::create_router_with_services(
        app_services.clone(),
        notification_manager,
        stripe_billing,
        config.clone(),
    );

    // Apply subscription limits for all existing users at startup (SAAS mode only)
    if config.is_cloud_mode() {
        tokio::spawn({
            let wallet_manager = wallet_manager.clone();
            async move {
                if let Err(e) = apply_startup_subscription_limits(wallet_manager).await {
                    eprintln!("❌ Failed to apply startup subscription limits: {}", e);
                }
            }
        });
    } else {
        println!("🔓 FOSS mode: Skipping subscription limit enforcement");
    }

    let listener = tokio::net::TcpListener::bind(&config.bind_address).await?;
    println!("Server running on http://{}", config.bind_address);

    axum::serve(listener, app).await?;

    Ok(())
}

async fn apply_startup_subscription_limits(
    wallet_manager: std::sync::Arc<WalletManager>,
) -> anyhow::Result<()> {
    tracing::info!("🎯 Applying subscription limits for active users at startup");

    // Get all users from the database
    let users = wallet_manager.metadata_db.get_all_users().await?;

    for user in users {
        // Apply subscription limits for ALL users
        // The function will deactivate wallets for expired/past_due/canceled users
        if let Err(e) = wallet_manager
            .apply_subscription_limits(
                &user.id,
                user.subscription_tier.as_str(),
                &user.subscription_status,
                user.is_admin,
                user.trial_ends_at.clone(),
            )
            .await
        {
            tracing::error!(
                "Failed to apply subscription limits for user {}: {}",
                user.id,
                e
            );
        } else if user.is_admin {
            tracing::info!("✅ Applied unlimited limits for admin user {}", user.id);
        } else {
            tracing::info!(
                "✅ Applied {} tier limits for user {}",
                user.subscription_tier.as_str(),
                user.id
            );
        }
    }

    Ok(())
}
