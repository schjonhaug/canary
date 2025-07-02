#[cfg(test)]
mod tests;

mod api;
mod config;
mod electrum;
mod metadata;
mod sms;
mod wallet;
use api::create_router;
use config::AppConfig;
use electrum::ElectrumClient;
use metadata::TransactionEvent;
use sms::SmsService;
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast};
use tokio::time::{Duration, interval};
use wallet::WalletManager;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load configuration
    let config = AppConfig::load()?;
    
    println!("Starting TxRay with configuration:");
    println!("  Network: {:?}", config.network);
    println!("  Electrum URL: {}", config.electrum_url());
    println!("  Bind address: {}", config.bind_address);
    println!("  Wallet directory: {}", config.wallet_dir_path());
    println!("  Metadata database: {}", config.metadata_db_path());
    
    // Test Electrum connection
    let electrum_client = ElectrumClient::new(&config.electrum_url())?;
    let features = electrum_client.server_features()?;
    println!("Connected to Electrum server: {}", features);

    // Create broadcast channel for transaction events
    let (event_tx, _event_rx) = broadcast::channel::<TransactionEvent>(1000);

    // Create SMS worker subscriber
    let sms_rx = event_tx.subscribe();

    let wallet_manager = Arc::new(Mutex::new(
        WalletManager::new(
            event_tx,
            config.wallet_dir_path().into(),
            &config.metadata_db_path(),
            config.network(),
            &config.electrum_url(),
        ).await,
    ));

    // Spawn background task for wallet syncing
    let wallet_manager_sync = Arc::clone(&wallet_manager);
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(4));

        loop {
            interval.tick().await;

            let mut manager = wallet_manager_sync.lock().await;
            if let Err(e) = manager.sync_all_wallets().await {
                eprintln!("Error syncing wallets: {}", e);
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
                                println!("📱 SMS Alert (no contacts): {}", event.message);
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
                                    println!(
                                        "📱 SMS Alert (Twilio not configured): {}",
                                        event.message
                                    );
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

    let app = create_router(wallet_manager);

    let listener = tokio::net::TcpListener::bind(&config.bind_address).await?;
    println!("Server running on http://{}", config.bind_address);

    axum::serve(listener, app).await?;

    Ok(())
}
