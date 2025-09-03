use crate::electrum::ElectrumClient;
use crate::metadata::{EventType, MetadataDb, Transaction, TransactionEvent, TransactionInsert};
use anyhow::{anyhow, Result};
use bdk_wallet::{chain::ChainPosition, rusqlite::Connection, PersistedWallet};
use bdk_wallet::chain::ConfirmationBlockTime;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;

/// Transaction-based wallet sync service
/// This replaces the old balance-based sync logic with proper transaction tracking
pub struct WalletSyncService {
    metadata_db: MetadataDb,
    event_sender: broadcast::Sender<TransactionEvent>,
}

impl WalletSyncService {
    pub fn new(metadata_db: MetadataDb, event_sender: broadcast::Sender<TransactionEvent>) -> Self {
        Self {
            metadata_db,
            event_sender,
        }
    }

    /// Sync a single wallet using transaction-based approach
    pub async fn sync_wallet_by_checksum(
        &self,
        wallet: &mut PersistedWallet<Connection>,
        wallet_checksum: &str,
        electrum_client: Option<&ElectrumClient>,
    ) -> Result<bool> {
        println!("[{}] Starting transaction-based sync", wallet_checksum);

        // Perform the actual sync with Electrum
        if let Some(client) = electrum_client {
            if let Err(e) = client.sync_wallet(wallet) {
                eprintln!("[{}] Failed to sync with Electrum: {}", wallet_checksum, e);
                return Ok(false);
            }
        }

        // Update last_synced_at timestamp
        let _ = self.metadata_db.update_wallet_last_synced(wallet_checksum).await;

        // Process all transactions and detect changes
        let has_changes = self.process_wallet_transactions(wallet, wallet_checksum).await?;

        // Update wallet balance in metadata
        let current_balance = wallet.balance().total();
        self.metadata_db
            .update_wallet_balance_by_checksum(wallet_checksum, current_balance.to_sat() as i64)
            .await?;

        println!("[{}] Sync complete, changes: {}", wallet_checksum, has_changes);
        Ok(has_changes)
    }

    /// Process all transactions in the wallet and sync with database
    async fn process_wallet_transactions(
        &self,
        wallet: &PersistedWallet<Connection>,
        wallet_checksum: &str,
    ) -> Result<bool> {
        let mut has_changes = false;

        // Get current wallet balance for balance_after calculations
        let current_balance = wallet.balance().total().to_sat() as i64;

        // Collect transaction data first to avoid lifetime issues across await
        let transactions_data: Vec<_> = wallet.transactions().map(|tx_item| {
            let txid = tx_item.tx_node.txid.to_string();
            let (sent, received) = wallet.sent_and_received(&tx_item.tx_node);
            let net_amount = received.to_sat() as i64 - sent.to_sat() as i64;
            let block_height = tx_item.chain_position.confirmation_height_upper_bound();
            let is_confirmed = tx_item.chain_position.is_confirmed();
            
            // Use current timestamp for new transactions
            let first_seen_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
                
            let confirmed_at = if is_confirmed {
                Some(first_seen_at) // Use same timestamp for confirmed_at in this context
            } else {
                None
            };
            
            (txid, net_amount, block_height, is_confirmed, first_seen_at, confirmed_at)
        }).collect();

        // Process each transaction with collected data
        for (txid, net_amount, block_height, is_confirmed, first_seen_at, confirmed_at) in transactions_data {
            // Check if we already know about this transaction
            let existing_tx = self
                .metadata_db
                .get_transaction_by_txid(wallet_checksum, &txid)
                .await?;

            match existing_tx {
                None => {
                    // New transaction - inline creation using pre-collected data
                    // Determine transaction type and amount
                    let (transaction_type, amount_sats, fee_sats) = if net_amount < 0 {
                        // Outgoing transaction (send)
                        // For now, don't calculate fees - we can add this later
                        (EventType::Send, (-net_amount) as i64, None)
                    } else {
                        // Incoming transaction (receive)
                        (EventType::Receive, net_amount, None)
                    };

                    let transaction = TransactionInsert {
                        txid: txid.clone(),
                        wallet_checksum: wallet_checksum.to_string(),
                        transaction_type,
                        amount_sats,
                        fee_sats,
                        block_height,
                        first_seen_at,
                        confirmed_at,
                        is_rbf: false, // TODO: Implement RBF detection later
                        is_cpfp: false, // TODO: Implement CPFP detection later
                        balance_after: Some(current_balance),
                    };

                    let transaction_id = self.metadata_db.insert_transaction(&transaction).await?;

                    // Send notifications for new transaction
                    self.send_transaction_notification(&transaction, &transaction_id)
                        .await?;

                    has_changes = true;
                    println!(
                        "[{}] New transaction: {} {} ({:.8} BTC)",
                        wallet_checksum,
                        if transaction.block_height.is_some() {
                            "✅ Confirmed"
                        } else {
                            "⏳ Pending"
                        },
                        transaction.transaction_type.as_str(),
                        transaction.amount_sats as f64 / 100_000_000.0
                    );
                }
                Some(existing) => {
                    // Check if transaction status changed (mempool -> confirmed)
                    let is_now_confirmed = is_confirmed;
                    let was_confirmed = existing.block_height.is_some();

                    if is_now_confirmed && !was_confirmed {
                        // Transaction just confirmed!
                        let block_height_value = block_height.unwrap_or(0);
                        let confirmed_at_value = confirmed_at.unwrap_or(first_seen_at);

                        self.metadata_db
                            .update_transaction_confirmation(
                                wallet_checksum,
                                &txid,
                                block_height_value,
                                confirmed_at_value,
                            )
                            .await?;

                        // Send confirmation notification
                        self.send_confirmation_notification(&existing, block_height_value, confirmed_at_value)
                            .await?;

                        has_changes = true;
                        println!(
                            "[{}] Transaction confirmed: {} at height {}",
                            wallet_checksum, &txid, block_height_value
                        );
                    }
                }
            }
        }

        Ok(has_changes)
    }

    /// Extract block height and timestamp from chain position
    fn get_confirmation_details(&self, chain_position: &ChainPosition<ConfirmationBlockTime>) -> Result<(i64, u64)> {
        match chain_position {
            ChainPosition::Confirmed { anchor, .. } => {
                let block_height = anchor.block_id.height as i64;
                let confirmed_at = anchor.confirmation_time as u64;
                Ok((block_height, confirmed_at))
            }
            ChainPosition::Unconfirmed { .. } => {
                Err(anyhow!("Transaction is not confirmed"))
            }
        }
    }

    /// Send notification for a new transaction (mempool or confirmed)
    async fn send_transaction_notification(
        &self,
        transaction: &TransactionInsert,
        transaction_id: &str,
    ) -> Result<()> {
        // Get all contacts for this wallet
        let contacts = self
            .metadata_db
            .get_contacts_by_wallet_checksum(&transaction.wallet_checksum)
            .await?;

        // Send notifications to all contacts
        for contact in contacts {
            for notification_method in contact.notification_methods {
                let message_content = if transaction.block_height.is_some() {
                    // Already confirmed (direct mining)
                    format!(
                        "✅ {} {:.8} BTC confirmed",
                        match transaction.transaction_type {
                            EventType::Send => "Sent",
                            EventType::Receive => "Received",
                        },
                        transaction.amount_sats as f64 / 100_000_000.0
                    )
                } else {
                    // In mempool
                    format!(
                        "⏳ {} {:.8} BTC pending",
                        match transaction.transaction_type {
                            EventType::Send => "Sending",
                            EventType::Receive => "Receiving",
                        },
                        transaction.amount_sats as f64 / 100_000_000.0
                    )
                };

                // TODO: Send actual notification through provider
                println!(
                    "[{}] Notification to {}: {}",
                    transaction.wallet_checksum, contact.name, message_content
                );
            }
        }

        Ok(())
    }

    /// Send confirmation notification for a transaction that just confirmed
    async fn send_confirmation_notification(
        &self,
        transaction: &Transaction,
        block_height: u32,
        _confirmed_at: u64,
    ) -> Result<()> {
        // Get all contacts for this wallet
        let contacts = self
            .metadata_db
            .get_contacts_by_wallet_checksum(&transaction.wallet_checksum)
            .await?;

        // Send confirmation notifications to all contacts
        for contact in contacts {
            for notification_method in contact.notification_methods {
                let message_content = format!(
                    "✅ {} {:.8} BTC confirmed at block {}",
                    match transaction.transaction_type {
                        EventType::Send => "Sent",
                        EventType::Receive => "Received",
                    },
                    transaction.amount_sats as f64 / 100_000_000.0,
                    block_height
                );

                // TODO: Send actual notification through provider
                println!(
                    "[{}] Confirmation notification to {}: {}",
                    transaction.wallet_checksum, contact.name, message_content
                );
            }
        }

        Ok(())
    }
}