#[cfg(test)]
mod tests;

mod api;
mod config;
mod electrum;
mod metadata;
mod migrations;
mod sms;
mod wallet;
use api::create_router;
use config::AppConfig;
use electrum::{ElectrumClient, BlockHeader};
use metadata::{TransactionEvent, DashboardUpdate};
use sms::SmsService;
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast};
use tokio::time::{Duration, interval};
use wallet::WalletManager;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load configuration
    let config = AppConfig::load()?;

    println!("Starting Kanari with configuration:");
    println!("  Network: {:?}", config.network);
    println!("  Electrum URL: {}", config.electrum_url());
    println!("  Bind address: {}", config.bind_address);
    println!("  Wallet directory: {}", config.effective_wallet_dir());
    println!("  Metadata database: {}", config.effective_metadata_db());

    // Test Electrum connection
    let electrum_client = ElectrumClient::new(&config.electrum_url())?;
    let features = electrum_client.server_features()?;
    println!("Connected to Electrum server: {}", features);

    // Create broadcast channel for transaction events
    let (event_tx, _event_rx) = broadcast::channel::<TransactionEvent>(1000);

    // Create broadcast channel for block header events
    let (block_header_tx, _block_header_rx) = broadcast::channel::<BlockHeader>(100);

    // Create broadcast channel for dashboard updates
    let (dashboard_tx, _dashboard_rx) = broadcast::channel::<DashboardUpdate>(100);

    // Create SMS worker subscriber
    let sms_rx = event_tx.subscribe();

    // Create shared state for current block header
    let current_block_header = Arc::new(Mutex::new(None::<BlockHeader>));

    let wallet_manager = Arc::new(Mutex::new(
        WalletManager::new(
            event_tx,
            dashboard_tx.clone(),
            config.effective_wallet_dir().into(),
            &config.effective_metadata_db(),
            config.network(),
            &config.electrum_url(),
        )
        .await,
    ));

    // Fetch and store initial block header
    {
        let manager = wallet_manager.lock().await;
        if let Some(ref electrum_client) = manager.electrum_client {
            match electrum_client.get_current_block_height() {
                Ok(height) => {
                    match electrum_client.get_block_header(height) {
                        Ok(block_header) => {
                            println!("📦 Initial block header: height={}, hash={}", 
                                   block_header.height, block_header.hash);
                            
                            // Store in database
                            if let Err(e) = manager.metadata_db.upsert_current_block_header(&block_header) {
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
        } else {
            // Try to load stored block header from database
            match manager.metadata_db.get_current_block_header() {
                Ok(Some(stored_header)) => {
                    println!("📦 Loaded stored block header: height={}, hash={}", 
                           stored_header.height, stored_header.hash);
                    
                    // Update shared state
                    let mut current_header = current_block_header.lock().await;
                    *current_header = Some(stored_header.clone());
                    
                    // Broadcast to SSE clients
                    if let Err(e) = block_header_tx.send(stored_header) {
                        eprintln!("Failed to broadcast stored block header: {}", e);
                    }
                }
                Ok(None) => {
                    println!("No stored block header found");
                }
                Err(e) => {
                    eprintln!("Failed to load stored block header: {}", e);
                }
            }
        }
    }

    // Send initial dashboard update
    {
        let manager = wallet_manager.lock().await;
        if let Err(e) = manager.send_dashboard_update().await {
            eprintln!("Failed to send initial dashboard update: {}", e);
        }
    }

    // Spawn background task for wallet syncing and block header polling
    let wallet_manager_sync = Arc::clone(&wallet_manager);
    let current_block_header_sync = Arc::clone(&current_block_header);
    let block_header_tx_sync = block_header_tx.clone();
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(4));
        let mut block_header_subscribed = false;

        loop {
            interval.tick().await;

            let mut manager = wallet_manager_sync.lock().await;
            
            // Sync all wallets
            if let Err(e) = manager.sync_all_wallets().await {
                eprintln!("Error syncing wallets: {}", e);
            }

            // Initialize block header subscription on first run
            if !block_header_subscribed {
                if let Some(ref electrum_client) = manager.electrum_client {
                    if let Err(e) = electrum_client.block_headers_subscribe() {
                        eprintln!("Failed to subscribe to block headers: {}", e);
                    } else {
                        block_header_subscribed = true;
                        println!("✅ Subscribed to block headers");
                    }
                }
            }

            // Poll for new block headers
            if let Some(ref electrum_client) = manager.electrum_client {
                if let Some(notification) = electrum_client.block_headers_pop() {
                    println!("📦 New block header: height={}, hash={}", notification.height, notification.header.block_hash());
                    
                    // Get full block header details
                    if let Ok(block_header) = electrum_client.get_block_header(notification.height as u32) {
                        // Store in database
                        if let Err(e) = manager.metadata_db.upsert_current_block_header(&block_header) {
                            eprintln!("Failed to store block header: {}", e);
                        }
                        
                        // Update shared state
                        let mut current_header = current_block_header_sync.lock().await;
                        *current_header = Some(block_header.clone());
                        
                        // Broadcast to SSE clients
                        if let Err(e) = block_header_tx_sync.send(block_header) {
                            eprintln!("Failed to broadcast block header: {}", e);
                        }
                    }
                }
            }
        }
    });

    // Spawn SMS worker task
    let sms_wallet_manager = Arc::clone(&wallet_manager);
    tokio::spawn(async move {
        let mut receiver = sms_rx;
        let sms_service = SmsService::new();

        loop {
            match receiver.recv().await {
                Ok(event) => {
                    // Get contacts for this wallet and send SMS notifications
                    let manager = sms_wallet_manager.lock().await;


                    // Get contacts for the wallet
                    match manager.metadata_db.get_contacts_for_wallet(event.wallet_id) {
                        Ok(contacts) => {
                            if contacts.is_empty() {
                                // Get wallet name for message generation
                                match manager.metadata_db.get_wallet_by_id(event.wallet_id) {
                                    Ok(Some(wallet_metadata)) => {
                                        let message = sms::SmsService::create_localized_message(
                                            &event,
                                            &wallet_metadata.name,
                                            &crate::metadata::Language::Norwegian,
                                        );
                                        println!("📱 SMS Alert (no contacts): {}", message);
                                    }
                                    _ => {
                                        println!(
                                            "📱 SMS Alert (no contacts): Event for wallet {}",
                                            event.wallet_id
                                        );
                                    }
                                }
                                continue;
                            }

                            // Check if Twilio is configured
                            match manager.metadata_db.get_twilio_config() {
                                Ok(Some(twilio_config)) => {
                                    println!(
                                        "📱 Sending SMS to {} contacts for wallet {}",
                                        contacts.len(),
                                        event.wallet_id
                                    );

                                    // Get wallet name for message
                                    let wallet_name = match manager.metadata_db.get_all_wallets() {
                                        Ok(wallets) => wallets
                                            .into_iter()
                                            .find(|w| w.id == Some(event.wallet_id))
                                            .map(|w| w.name)
                                            .unwrap_or_else(|| {
                                                format!("Wallet {}", event.wallet_id)
                                            }),
                                        Err(_) => format!("Wallet {}", event.wallet_id),
                                    };

                                    // Send SMS to all contacts
                                    let results = sms_service
                                        .send_event_notifications(
                                            &event,
                                            &wallet_name,
                                            contacts.clone(),
                                            &twilio_config,
                                        )
                                        .await;

                                    // Log results to database
                                    for (contact, sms_response) in results {
                                        if let Some(event_id) = event.id {
                                            if let Some(contact_id) = contact.id {
                                                let status = if sms_response.success {
                                                    "sent"
                                                } else {
                                                    "failed"
                                                };

                                                if let Err(e) = manager.metadata_db.insert_sms_log(
                                                    event_id,
                                                    contact_id,
                                                    status,
                                                    sms_response.twilio_sid.as_deref(),
                                                    sms_response.error_message.as_deref(),
                                                ) {
                                                    eprintln!("Failed to log SMS result: {}", e);
                                                }

                                                // Log to console
                                                if sms_response.success {
                                                    println!(
                                                        "  ✅ SMS sent to {} ({})",
                                                        contact.name, contact.phone_number
                                                    );
                                                    if let Some(sid) = &sms_response.twilio_sid {
                                                        println!("     Twilio SID: {}", sid);
                                                    }
                                                } else {
                                                    println!(
                                                        "  ❌ SMS failed to {} ({}): {}",
                                                        contact.name,
                                                        contact.phone_number,
                                                        sms_response.error_message.unwrap_or_else(
                                                            || "Unknown error".to_string()
                                                        )
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                                Ok(None) => {
                                    // Get wallet name for message generation
                                    match manager.metadata_db.get_wallet_by_id(event.wallet_id) {
                                        Ok(Some(wallet_metadata)) => {
                                            let message = sms::SmsService::create_localized_message(
                                                &event,
                                                &wallet_metadata.name,
                                                &crate::metadata::Language::Norwegian,
                                            );
                                            println!(
                                                "📱 SMS Alert (Twilio not configured): {}",
                                                message
                                            );
                                        }
                                        _ => {
                                            println!(
                                                "📱 SMS Alert (Twilio not configured): Event for wallet {}",
                                                event.wallet_id
                                            );
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Failed to get Twilio config: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!(
                                "Failed to get contacts for wallet {}: {}",
                                event.wallet_id, e
                            );
                        }
                    }
                }
                Err(broadcast::error::RecvError::Closed) => {
                    println!("Event channel closed, SMS worker shutting down");
                    break;
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    eprintln!("SMS worker lagged, skipped {} events", skipped);
                    // Continue processing new events
                }
            }
        }
    });

    let app = create_router(wallet_manager, block_header_tx, dashboard_tx);

    let listener = tokio::net::TcpListener::bind(&config.bind_address).await?;
    println!("Server running on http://{}", config.bind_address);

    axum::serve(listener, app).await?;

    Ok(())
}
