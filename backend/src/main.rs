mod api;
mod auth;
mod config;
mod electrum;
mod email_provider;
mod email_service;
mod message_formatter;
mod metadata;
mod migrations;
mod notifications;
mod ntfy_provider;
mod stripe_billing;
mod stripe_client_service;
mod subscription;
mod twilio_provider;
mod wallet;
mod xpub_converter;

use config::AppConfig;
use subscription::SubscriptionTier;
use email_provider::EmailProvider;
use metadata::TransactionEvent;
use notifications::NotificationManager;
use ntfy_provider::NtfyProvider;
use std::sync::Arc;
use std::time::Instant;
use stripe_billing::StripeBilling;
use tokio::sync::{broadcast, Mutex};
use tokio::time::{interval, Duration};
use tracing_subscriber;
use twilio_provider::TwilioProvider;
use wallet::WalletManager;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env().add_directive("canary=info".parse()?),
        )
        .init();

    // Load configuration
    let config = AppConfig::load()?;

    // Display ASCII art
    println!(r#"
 ▄████████    ▄████████ ███▄▄▄▄      ▄████████    ▄████████ ▄██   ▄  
███    ███   ███    ███ ███▀▀▀██▄   ███    ███   ███    ███ ███   ██▄
███    █▀    ███    ███ ███   ███   ███    ███   ███    ███ ███▄▄▄███
███          ███    ███ ███   ███   ███    ███  ▄███▄▄▄▄██▀ ▀▀▀▀▀▀███
███        ▀███████████ ███   ███ ▀███████████ ▀▀███▀▀▀▀▀   ▄██   ███
███    █▄    ███    ███ ███   ███   ███    ███ ▀███████████ ███   ███
███    ███   ███    ███ ███   ███   ███    ███   ███    ███ ███   ███
████████▀    ███    █▀   ▀█   █▀    ███    █▀    ███    ███  ▀█████▀ 
                                                 ███    ███          
    "#);

    println!(
        "Starting Canary v{} with configuration:",
        env!("CARGO_PKG_VERSION")
    );
    println!("  Network: {:?}", config.network);
    println!("  Electrum URL: {}", config.electrum_url());
    println!("  Bind address: {}", config.bind_address);
    println!("  Wallet directory: {}", config.effective_wallet_dir());
    println!("  Metadata DB: {}", config.effective_metadata_db());
    // Display network-appropriate sync intervals
    let (personal_sync, _team_sync) = SubscriptionTier::Personal.get_sync_intervals(&config.network);
    let (_, team_sync_team) = SubscriptionTier::Team.get_sync_intervals(&config.network);
    println!("  Sync intervals: Personal={}s, Team={}s (network: {:?})", personal_sync, team_sync_team, config.network);

    // Log operating mode
    println!("🏢 Operating mode: {}", config.operating_mode().to_uppercase());
    if config.is_saas_mode() {
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
        eprintln!("");
        eprintln!("Please check your .env file and ensure all required variables are set.");
        eprintln!("See backend/.env.example for configuration examples.");
        std::process::exit(1);
    }
    println!("✅ Configuration validated successfully");


    // Create wallet manager with sync worker
    println!("Creating wallet sync worker...");

    let (event_tx, _event_rx) = broadcast::channel::<TransactionEvent>(100);

    let wallet_manager = Arc::new(Mutex::new(
        WalletManager::new(
            event_tx.clone(),
            config.effective_wallet_dir().into(),
            &config.effective_metadata_db(),
            config.network(),
            &config.electrum_url(),
            &config,
        )
        .await,
    ));
    
    // Create new non-blocking architecture: Separate metadata access from heavy wallet operations
    let app_services = {
        let manager = wallet_manager.lock().await;
        let wallet_creation_service = wallet::WalletCreationService::new(
            manager.wallet_dir.clone(),
            manager.metadata_db.clone(),
            manager.electrum_client.clone(),
            manager.get_network(),
        );
        Arc::new(api::AppServices {
            metadata_db: manager.metadata_db.clone(),
            wallet_creation_service,
        })
    };

    // Create shared state for current block header and load existing header from database
    let existing_header = {
        let manager = wallet_manager.lock().await;
        manager.metadata_db.get_current_block_header().await.unwrap_or(None)
    };
    let current_block_header = Arc::new(Mutex::new(existing_header));

    // Create notification manager and register providers based on operating mode
    let mut notification_manager = NotificationManager::new();

    if config.is_foss_mode() {
        // FOSS mode: Only ntfy provider
        println!("🔔 FOSS mode: Registering ntfy-only notifications");
        notification_manager.register_provider(Arc::new(NtfyProvider::new()));
    } else {
        // SAAS mode: Register all configured providers
        println!("🔔 SAAS mode: Registering all notification providers");
        
        // Register ntfy provider (always available)
        if config.is_ntfy_enabled() {
            println!("  - ntfy notification provider");
            notification_manager.register_provider(Arc::new(NtfyProvider::new()));
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
                    println!("    - TWILIO_MESSAGING_SERVICE_SID");
                }
            }
        }

        // Register email provider if in SAAS mode
        if config.is_email_enabled() {
            println!("  - Email notification provider");
            notification_manager.register_provider(Arc::new(EmailProvider::new()));
        }
    }

    let notification_manager = Arc::new(Mutex::new(notification_manager));

    // Initialize Stripe billing only in SAAS mode
    let stripe_billing = if config.is_saas_mode() {
        println!("🏦 SAAS mode: Initializing Stripe billing...");
        match StripeBilling::new().await {
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

    // Try to fetch initial block header in background with timeout
    {
        let initial_wallet_manager = Arc::clone(&wallet_manager);
        let initial_block_header = Arc::clone(&current_block_header);
        tokio::spawn(async move {
            // Use tokio timeout to prevent indefinite blocking
            let timeout_duration = Duration::from_secs(5);

            let manager = initial_wallet_manager.lock().await;
            if let Some(ref electrum_client) = manager.electrum_client {
                println!("📦 Attempting to fetch initial block header (5s timeout)...");

                // Clone what we need before the blocking operation
                let client = electrum_client.clone();
                let metadata_db = manager.metadata_db.clone();
                drop(manager); // Release the lock before potential blocking

                // Run the blocking operation in a separate thread with timeout
                let height_result = tokio::time::timeout(
                    timeout_duration,
                    tokio::task::spawn_blocking(move || client.get_current_block_height()),
                )
                .await;

                match height_result {
                    Ok(Ok(Ok(height))) => {
                        // Successfully got height, now get the header
                        let manager = initial_wallet_manager.lock().await;
                        if let Some(ref electrum_client) = manager.electrum_client {
                            match electrum_client.get_block_header(height) {
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
                    }
                    Ok(Ok(Err(e))) => {
                        eprintln!("❌ Electrum error getting block height: {}", e);
                    }
                    Ok(Err(e)) => {
                        eprintln!("❌ Task error getting block height: {}", e);
                    }
                    Err(_) => {
                        eprintln!("⏱️  Timeout getting block height from Electrum (5s exceeded)");
                        eprintln!("   Electrum may be down or unresponsive");
                    }
                }
            } else {
                eprintln!("⚠️  No Electrum client available - cannot fetch block headers");
            }
        });
    }

    // Spawn wallet sync worker with tier-based intervals
    let sync_wallet_manager = Arc::clone(&wallet_manager);
    let sync_current_block_header = Arc::clone(&current_block_header);
    // Set sync check interval based on network to ensure responsive checking
    // This should be faster than the fastest wallet sync interval to ensure timely checks
    let (_, team_sync_interval) = SubscriptionTier::Team.get_sync_intervals(&config.network);
    let sync_check_interval = std::cmp::min(team_sync_interval / 2, 30); // Check at least every 30 seconds
    
    println!("🕐 Sync checker interval: {}s (network: {:?})", sync_check_interval, config.network);
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(sync_check_interval));

        loop {
            interval.tick().await;

            let mutex_wait_start = Instant::now();
            let mut manager = sync_wallet_manager.lock().await;
            let mutex_wait_time = mutex_wait_start.elapsed();
            
            if mutex_wait_time.as_millis() > 10 {
                println!("🔒 Sync task waited {:?} for wallet manager mutex", mutex_wait_time);
            }
            
            // Use tier-based sync instead of syncing all wallets
            if let Err(e) = manager.sync_wallets_due_for_sync().await {
                eprintln!("Tier-based sync failed: {}", e);
            }

            // Check for new block headers with timeout
            if let Some(ref electrum_client) = manager.electrum_client {
                let client = electrum_client.clone();
                let metadata_db = manager.metadata_db.clone();
                let current_block_header_clone = sync_current_block_header.clone();

                // Get current stored height
                let stored_header = sync_current_block_header.lock().await;
                let stored_height = stored_header.as_ref().map(|h| h.height).unwrap_or(0);
                drop(stored_header);

                // Run blocking operation in separate thread with timeout
                let height_result = tokio::time::timeout(
                    Duration::from_secs(5),
                    tokio::task::spawn_blocking(move || client.get_current_block_height()),
                )
                .await;

                match height_result {
                    Ok(Ok(Ok(current_height))) => {
                        if current_height > stored_height {
                            // Get the actual block header
                            if let Some(ref electrum_client) = manager.electrum_client {
                                match electrum_client.get_block_header(current_height) {
                                    Ok(block_header) => {
                                        println!(
                                            "📦 New block header: height={} (was {})",
                                            block_header.height, stored_height
                                        );

                                        // Store in database
                                        if let Err(e) = metadata_db
                                            .upsert_current_block_header(&block_header)
                                            .await
                                        {
                                            eprintln!("Failed to store block header: {}", e);
                                        }

                                        // Update shared state
                                        let mut current_header =
                                            current_block_header_clone.lock().await;
                                        *current_header = Some(block_header.clone());
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "Failed to get block header for height {}: {}",
                                            current_height, e
                                        );
                                    }
                                }
                            }
                        }
                    }
                    Ok(Ok(Err(e))) => {
                        eprintln!("Electrum error checking block height: {}", e);
                    }
                    Ok(Err(e)) => {
                        eprintln!("Task error checking block height: {}", e);
                    }
                    Err(_) => {
                        eprintln!("⏱️  Timeout checking block height (sync task)");
                    }
                }
            }
            
            // Explicitly release the mutex and log timing
            let mutex_hold_duration = mutex_wait_start.elapsed();
            drop(manager);
            println!("🔓 Released wallet manager mutex after {:?} (sync + block check)", mutex_hold_duration);
        }
    });

    // Create session cleanup task (runs every hour)
    let session_cleanup_manager = wallet_manager.clone();
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(3600)); // Run every hour

        loop {
            interval.tick().await;

            let manager = session_cleanup_manager.lock().await;
            match manager.metadata_db.cleanup_expired_sessions().await {
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
    let notification_event_rx = event_tx.subscribe();
    tokio::spawn(async move {
        let mut rx = notification_event_rx;

        while let Ok(event) = rx.recv().await {
            let manager = notification_worker_manager.lock().await;

            // Get wallet information for the event
            let wallet_manager_lock = notification_wallet_manager.lock().await;
            if let Ok(Some(wallet_info)) = wallet_manager_lock
                .get_wallet_by_checksum(&event.wallet_checksum)
                .await
            {
                // Get contacts for this wallet
                if let Ok(contacts) = wallet_manager_lock
                    .metadata_db
                    .get_contacts_with_notification_methods(&event.wallet_checksum)
                    .await
                {
                    if !contacts.is_empty() {
                        // Generate message content once (same for all providers)
                        let mut message_content = String::new();
                        let mut provider_counts = std::collections::HashMap::new();
                        let mut failed_count = 0;
                        let mut total_sent = 0;

                        // Try to send notifications using all available providers, filtered by subscription tier
                        let available_providers = manager.list_providers();
                        for provider_info in available_providers {
                            let provider_name = &provider_info.name;

                            // All notification types are now allowed for all tiers
                            if let Ok(results) = manager
                                .send_notifications(
                                    provider_name,
                                    &event,
                                    &wallet_info.name,
                                    &contacts,
                                )
                                .await
                            {
                                for (notification_method, result, message) in results {
                                    // Store message content for summary (same for all providers)
                                    if message_content.is_empty() {
                                        message_content = message.clone();
                                    }

                                    // Log the notification attempt to database (keep this for audit)
                                    if let Some(ref method_id) = notification_method.id {
                                        if let Some(ref event_id) = event.id {
                                            let status =
                                                if result.success { "sent" } else { "failed" };
                                            if let Err(e) = wallet_manager_lock
                                                .metadata_db
                                                .insert_notification_log_for_method(
                                                    event_id,
                                                    method_id,
                                                    provider_name,
                                                    result.provider_id.as_deref(),
                                                    status,
                                                    result.error_message.as_deref(),
                                                    &message,
                                                )
                                                .await
                                            {
                                                eprintln!("❌ Failed to log notification to database: {}", e);
                                            }
                                        }
                                    }

                                    // Count results by provider
                                    if result.success {
                                        *provider_counts.entry(provider_name.clone()).or_insert(0) += 1;
                                        total_sent += 1;
                                    } else {
                                        failed_count += 1;
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
                            let transaction_summary = if message_content.contains("📤") {
                                // Extract amount from sending message, looking for " BTC" pattern
                                if let Some(btc_pos) = message_content.find(" BTC") {
                                    let before_btc = &message_content[..btc_pos];
                                    if let Some(space_pos) = before_btc.rfind(' ') {
                                        let amount = &before_btc[space_pos + 1..];
                                        format!("📤 Sending {} BTC", amount)
                                    } else {
                                        "📤 Sending".to_string()
                                    }
                                } else {
                                    "📤 Sending".to_string()
                                }
                            } else if message_content.contains("📥") {
                                // Extract amount from receiving message
                                if let Some(btc_pos) = message_content.find(" BTC") {
                                    let before_btc = &message_content[..btc_pos];
                                    if let Some(space_pos) = before_btc.rfind(' ') {
                                        let amount = &before_btc[space_pos + 1..];
                                        format!("📥 Received {} BTC", amount)
                                    } else {
                                        "📥 Received".to_string()
                                    }
                                } else {
                                    "📥 Received".to_string()
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
        config.clone()
    );

    // Apply subscription limits for all existing users at startup (SAAS mode only)
    if config.is_saas_mode() {
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
    wallet_manager: std::sync::Arc<tokio::sync::Mutex<WalletManager>>,
) -> anyhow::Result<()> {
    tracing::info!("🎯 Applying subscription limits for active users at startup");

    let manager = wallet_manager.lock().await;

    // Get all users from the database
    let users = manager.metadata_db.get_all_users().await?;

    for user in users {
        // Only apply limits to users who are eligible for syncing
        // Skip users in 'pending', 'expired', 'past_due' states
        let should_apply_limits = match user.subscription_status.as_str() {
            "trialing" | "active" => true,
            _ => false, // Skip pending, expired, past_due, canceled
        };

        if !should_apply_limits {
            tracing::debug!("⏭️  Skipping limits for user {} (status: {})", user.id, user.subscription_status);
            continue;
        }

        if let Err(e) = manager
            .apply_subscription_limits(&user.id, &user.subscription_tier.as_str(), user.is_admin)
            .await
        {
            tracing::error!(
                "Failed to apply subscription limits for user {}: {}",
                user.id,
                e
            );
        } else {
            if user.is_admin {
                tracing::info!("✅ Applied unlimited limits for admin user {}", user.id);
            } else {
                tracing::info!(
                    "✅ Applied {} tier limits for user {}",
                    user.subscription_tier.as_str(),
                    user.id
                );
            }
        }
    }

    Ok(())
}
