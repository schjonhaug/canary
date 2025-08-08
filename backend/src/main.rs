
mod api;
mod auth;
mod config;
mod electrum;
mod email_service;
mod message_formatter;
mod metadata;
mod migrations;
mod notifications;
mod wallet;
mod ntfy_provider;
mod twilio_provider;

use api::create_router;
use config::AppConfig;
use metadata::TransactionEvent;
use notifications::{NotificationManager};
use wallet::WalletManager;
use ntfy_provider::NtfyProvider;
use twilio_provider::TwilioProvider;
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast};
use tokio::time::{Duration, interval};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load configuration
    let config = AppConfig::load()?;

    println!("Starting Canary v{} with configuration:", env!("CARGO_PKG_VERSION"));
    println!("  Network: {:?}", config.network);
    println!("  Electrum URL: {}", config.electrum_url());
    println!("  Bind address: {}", config.bind_address);
    println!("  Wallet directory: {}", config.effective_wallet_dir());
    println!("  Metadata DB: {}", config.effective_metadata_db());
    println!("  Sync interval: {} seconds", config.sync_interval_secs());
    
    // Log authentication status
    let auth_enabled = std::env::var("CANARY_ENABLE_AUTH")
        .unwrap_or_else(|_| "false".to_string())
        .to_lowercase() == "true";
    println!("🔐 Authentication: {}", if auth_enabled { "ENABLED" } else { "DISABLED" });

    // Create wallet manager with sync worker
    println!("Creating wallet sync worker...");
    
    let (event_tx, _event_rx) = broadcast::channel::<TransactionEvent>(100);

    // Create shared state for current block header
    let current_block_header = Arc::new(Mutex::new(None::<electrum::BlockHeader>));

    let wallet_manager = Arc::new(Mutex::new(
        WalletManager::new(
            event_tx.clone(),
            config.effective_wallet_dir().into(),
            &config.effective_metadata_db(),
            config.network(),
            &config.electrum_url(),
        )
        .await,
    ));

    // Create notification manager and register providers based on configuration
    let mut notification_manager = NotificationManager::new();
    
    // Register ntfy provider if enabled
    if config.is_ntfy_enabled() {
        println!("🔔 Registering ntfy notification provider");
        notification_manager.register_provider(Arc::new(NtfyProvider::new()));
    }
    
    // Register Twilio SMS provider if enabled and configured
    if config.is_twilio_enabled() {
        match TwilioProvider::from_env() {
            Some(twilio_provider) => {
                println!("📱 Registering Twilio SMS notification provider");
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
    
    let notification_manager = Arc::new(Mutex::new(notification_manager));

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
                    tokio::task::spawn_blocking(move || {
                        client.get_current_block_height()
                    })
                ).await;
                
                match height_result {
                    Ok(Ok(Ok(height))) => {
                        // Successfully got height, now get the header
                        let manager = initial_wallet_manager.lock().await;
                        if let Some(ref electrum_client) = manager.electrum_client {
                            match electrum_client.get_block_header(height) {
                                Ok(block_header) => {
                                    println!("📦 Initial block header fetched: height={}", 
                                           block_header.height);
                                    
                                    // Store in database
                                    if let Err(e) = metadata_db.upsert_current_block_header(&block_header).await {
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

    // Spawn wallet sync worker
    let sync_wallet_manager = Arc::clone(&wallet_manager);
    let sync_current_block_header = Arc::clone(&current_block_header);
    let sync_interval_secs = config.sync_interval_secs();
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(sync_interval_secs));
        
        loop {
            interval.tick().await;
            
            let mut manager = sync_wallet_manager.lock().await;
            if let Err(e) = manager.sync_all_wallets().await {
                eprintln!("Sync failed: {}", e);
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
                    tokio::task::spawn_blocking(move || {
                        client.get_current_block_height()
                    })
                ).await;
                
                match height_result {
                    Ok(Ok(Ok(current_height))) => {
                        if current_height > stored_height {
                            // Get the actual block header
                            if let Some(ref electrum_client) = manager.electrum_client {
                                match electrum_client.get_block_header(current_height) {
                                    Ok(block_header) => {
                                        println!("📦 New block header: height={} (was {})", 
                                               block_header.height, stored_height);
                                        
                                        // Store in database
                                        if let Err(e) = metadata_db.upsert_current_block_header(&block_header).await {
                                            eprintln!("Failed to store block header: {}", e);
                                        }
                                        
                                        // Update shared state
                                        let mut current_header = current_block_header_clone.lock().await;
                                        *current_header = Some(block_header.clone());
                                    }
                                    Err(e) => {
                                        eprintln!("Failed to get block header for height {}: {}", current_height, e);
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
            if let Ok(Some(wallet_info)) = wallet_manager_lock.get_wallet_by_checksum(&event.wallet_checksum).await {
                // Get contacts for this wallet
                if let Ok(contacts) = wallet_manager_lock.metadata_db.get_contacts_with_notification_methods(&event.wallet_checksum).await {
                    if !contacts.is_empty() {
                        println!("🔔 Triggering notifications for {} contacts on wallet '{}'", contacts.len(), wallet_info.name);
                        
                        // Generate message content once (same for all providers)
                        let mut message_printed = false;
                        
                        // Try to send notifications using all available providers
                        let available_providers = manager.list_providers();
                        for provider_info in available_providers {
                            let provider_name = &provider_info.name;
                            if let Ok(results) = manager.send_notifications(
                                provider_name,
                                &event,
                                &wallet_info.name,
                                &contacts,
                            ).await {
                                for (notification_method, result, message) in results {
                                    // Print message content only once
                                    if !message_printed {
                                        println!("   📄 Message: {}", message);
                                        message_printed = true;
                                    }
                                    
                                    // Log the notification attempt to database
                                    if let Some(method_id) = notification_method.id {
                                        if let Some(event_id) = event.id {
                                            let status = if result.success { "sent" } else { "failed" };
                                            let _ = wallet_manager_lock.metadata_db.insert_notification_log_for_method(
                                                event_id,
                                                method_id,
                                                provider_name,
                                                result.provider_id.as_deref(),
                                                status,
                                                result.error_message.as_deref(),
                                                &message,
                                            ).await;
                                        }
                                    }
                                    
                                    // Find the contact name for logging (need to find the contact that owns this method)
                                    let contact_name = contacts.iter()
                                        .find(|c| c.notification_methods.iter().any(|m| m.id == notification_method.id))
                                        .map(|c| c.name.as_str())
                                        .unwrap_or("Unknown");
                                    
                                    if result.success {
                                        let display_target = notification_method.display_target.as_ref()
                                            .unwrap_or(&notification_method.notification_target);
                                        println!("   ✅ {} → {} via {}", contact_name, display_target, provider_name);
                                    } else {
                                        let display_target = notification_method.display_target.as_ref()
                                            .unwrap_or(&notification_method.notification_target);
                                        println!("   ❌ {} → {} via {} - {}", 
                                            contact_name, display_target, provider_name,
                                            result.error_message.unwrap_or_else(|| "Unknown error".to_string()));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    let app = create_router(wallet_manager, notification_manager);

    let listener = tokio::net::TcpListener::bind(&config.bind_address).await?;
    println!("Server running on http://{}", config.bind_address);

    axum::serve(listener, app).await?;

    Ok(())
}