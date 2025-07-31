
mod api;

use api::create_router;
use canary_core::{AppConfig, BlockHeader, TransactionEvent, DashboardUpdate, NotificationManager, NtfyProvider, WalletManager};
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast};
use tokio::time::{Duration, interval};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load configuration
    let config = AppConfig::load()?;

    println!("Starting Canary with configuration:");
    println!("  Network: {:?}", config.network);
    println!("  Electrum URL: {}", config.electrum_url());
    println!("  Bind address: {}", config.bind_address);
    println!("  Wallet directory: {}", config.effective_wallet_dir());
    println!("  Metadata DB: {}", config.effective_metadata_db());

    // Create wallet manager with sync worker
    println!("Creating wallet sync worker...");
    
    let (event_tx, _event_rx) = broadcast::channel::<TransactionEvent>(100);

    // Create broadcast channel for block headers
    let (block_header_tx, _block_header_rx) = broadcast::channel::<BlockHeader>(10);

    // Create broadcast channel for dashboard updates
    let (dashboard_tx, _dashboard_rx) = broadcast::channel::<DashboardUpdate>(100);

    // Create shared state for current block header
    let current_block_header = Arc::new(Mutex::new(None::<BlockHeader>));

    let wallet_manager = Arc::new(Mutex::new(
        WalletManager::new(
            event_tx.clone(),
            dashboard_tx.clone(),
            config.effective_wallet_dir().into(),
            &config.effective_metadata_db(),
            config.network(),
            &config.electrum_url(),
        )
        .await,
    ));

    // Create notification manager and register ntfy provider
    let mut notification_manager = NotificationManager::new();
    notification_manager.register_provider(Arc::new(NtfyProvider::new()));
    let notification_manager = Arc::new(Mutex::new(notification_manager));

    // Fetch and store initial block header
    {
        let manager = wallet_manager.lock().await;
        if let Some(ref electrum_client) = manager.electrum_client {
            match electrum_client.get_current_block_height() {
                Ok(height) => {
                    match electrum_client.get_block_header(height) {
                        Ok(block_header) => {
                            println!("📦 Initial block header: height={}", 
                                   block_header.height);
                            
                            // Store in database
                            if let Err(e) = manager.metadata_db.upsert_current_block_header(&block_header).await {
                                eprintln!("Failed to store initial block header: {}", e);
                            }
                            
                            // Update shared state
                            let mut current_header = current_block_header.lock().await;
                            *current_header = Some(block_header.clone());
                            
                            // Broadcast to SSE clients
                            if let Err(e) = block_header_tx.send(block_header) {
                                eprintln!("Failed to broadcast initial block header: {}", e);
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to get initial block header: {}", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to get current block height: {}", e);
                }
            }
        }
    }

    // Spawn wallet sync worker
    let sync_wallet_manager = Arc::clone(&wallet_manager);
    let sync_current_block_header = Arc::clone(&current_block_header);
    let sync_dashboard_tx = dashboard_tx.clone();
    let block_header_tx_sync = block_header_tx.clone();
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(4));
        
        loop {
            interval.tick().await;
            
            let mut manager = sync_wallet_manager.lock().await;
            if let Err(e) = manager.sync_all_wallets().await {
                eprintln!("Sync failed: {}", e);
            }

            // Broadcast dashboard update after sync
            match manager.get_current_dashboard_state().await {
                Ok(dashboard_update) => {
                    if let Err(e) = sync_dashboard_tx.send(dashboard_update) {
                        eprintln!("Failed to send dashboard update: {}", e);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to get dashboard state for broadcast: {}", e);
                }
            }

            // Check for new block headers
            if let Some(ref electrum_client) = manager.electrum_client {
                match electrum_client.get_current_block_height() {
                    Ok(current_height) => {
                        let stored_header = sync_current_block_header.lock().await;
                        let stored_height = stored_header.as_ref().map(|h| h.height).unwrap_or(0);
                        
                        if current_height > stored_height {
                            drop(stored_header); // Release the lock before the blocking operation
                            
                            match electrum_client.get_block_header(current_height) {
                                Ok(block_header) => {
                                    println!("📦 New block header: height={} (was {})", 
                                           block_header.height, stored_height);
                                    
                                    // Store in database
                                    if let Err(e) = manager.metadata_db.upsert_current_block_header(&block_header).await {
                                        eprintln!("Failed to store block header: {}", e);
                                    }
                                    
                                    // Update shared state
                                    let mut current_header = sync_current_block_header.lock().await;
                                    *current_header = Some(block_header.clone());
                                    drop(current_header);
                                    
                                    // Broadcast to SSE clients
                                    if let Err(e) = block_header_tx_sync.send(block_header) {
                                        eprintln!("Failed to broadcast block header: {}", e);
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Failed to get block header for height {}: {}", current_height, e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to get current block height: {}", e);
                    }
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
            if let Ok(Some(wallet_info)) = wallet_manager_lock.get_wallet_by_id(event.wallet_id).await {
                // Get contacts for this wallet
                if let Ok(contacts) = wallet_manager_lock.metadata_db.get_contacts_for_wallet(event.wallet_id).await {
                    if !contacts.is_empty() {
                        println!("🔔 Triggering notifications for {} contacts on wallet '{}'", contacts.len(), wallet_info.name);
                        
                        // Try to send notifications using available providers
                        for provider_name in ["ntfy"] {
                            if let Ok(results) = manager.send_notifications(
                                provider_name,
                                &event,
                                &wallet_info.name,
                                &contacts,
                            ).await {
                                for (contact, result, message) in results {
                                    // Log the notification attempt to database
                                    if let Some(contact_id) = contact.id {
                                        if let Some(event_id) = event.id {
                                            let status = if result.success { "sent" } else { "failed" };
                                            let _ = wallet_manager_lock.metadata_db.insert_notification_log(
                                                event_id,
                                                contact_id,
                                                provider_name,
                                                result.provider_id.as_deref(),
                                                status,
                                                result.error_message.as_deref(),
                                                &message,
                                            ).await;
                                        }
                                    }
                                    
                                    if result.success {
                                        println!("✅ Notification sent to {} via {}", contact.name, provider_name);
                                    } else {
                                        println!("❌ Failed to notify {} via {}: {}", 
                                            contact.name, provider_name, 
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

    let app = create_router(wallet_manager, notification_manager, block_header_tx, dashboard_tx);

    let listener = tokio::net::TcpListener::bind(&config.bind_address).await?;
    println!("Server running on http://{}", config.bind_address);

    axum::serve(listener, app).await?;

    Ok(())
}