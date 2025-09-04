use crate::electrum::ElectrumClient;
use crate::metadata::{EventType, MetadataDb, Transaction, TransactionInsert, TransactionNotification};
use anyhow::{anyhow, Result};
use bdk_wallet::{chain::ChainPosition, rusqlite::Connection, PersistedWallet};
use bdk_wallet::chain::ConfirmationBlockTime;
use tokio::sync::broadcast;

/// Transaction-based wallet sync service
/// This replaces the old balance-based sync logic with proper transaction tracking
pub struct WalletSyncService {
    metadata_db: MetadataDb,
    notification_sender: broadcast::Sender<TransactionNotification>,
}

impl WalletSyncService {
    pub fn new(metadata_db: MetadataDb, notification_sender: broadcast::Sender<TransactionNotification>) -> Self {
        Self {
            metadata_db,
            notification_sender,
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


        // Get existing transactions sorted chronologically (oldest first for balance calculation)
        let existing_transactions = self.metadata_db
            .get_transactions_by_wallet_checksum(wallet_checksum, None)
            .await?;
        
        // Sort existing transactions by first_seen_at ASC (oldest first) for proper balance calculation
        let mut existing_txs_sorted = existing_transactions.iter()
            .map(|tx| (tx.txid.clone(), tx.first_seen_at, tx.amount_sats, tx.transaction_type))
            .collect::<Vec<_>>();
        existing_txs_sorted.sort_by_key(|(_, first_seen_at, _, _)| *first_seen_at);

        // We no longer calculate balances - they are computed on-demand by the frontend

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

        // Sort all transactions by timestamp for progressive balance calculation
        let mut all_transactions = transactions_data;
        all_transactions.sort_by_key(|(_, _, _, _, first_seen_at, _)| *first_seen_at);

        // No longer need balance calculations since we removed balance_after field

        // Process each transaction with progressive balance calculation
        for (txid, net_amount, block_height, is_confirmed, first_seen_at, confirmed_at) in all_transactions {
            // Check if we already know about this transaction
            let existing_tx = self
                .metadata_db
                .get_transaction_by_txid(wallet_checksum, &txid)
                .await?;

            match existing_tx {
                None => {
                    // New transaction found
                    
                    // Create new transaction record using pre-collected data
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
                    };

                    let _transaction_id = self.metadata_db.insert_transaction(&transaction).await?;

                    // Send notifications for new transaction
                    self.send_new_transaction_notification(&transaction).await?;

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
                        // Need to get the updated transaction record
                        if let Some(updated_tx) = self.metadata_db.get_transaction_by_txid(wallet_checksum, &txid).await? {
                            self.send_confirmed_transaction_notification(&updated_tx).await?;
                        }

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

    /// Send notification for a new transaction (either pending or directly confirmed)
    async fn send_new_transaction_notification(&self, transaction: &TransactionInsert) -> Result<()> {
        // Convert TransactionInsert to Transaction for notification
        let tx = Transaction {
            txid: transaction.txid.clone(),
            wallet_checksum: transaction.wallet_checksum.clone(),
            transaction_type: transaction.transaction_type,
            amount_sats: transaction.amount_sats,
            fee_sats: transaction.fee_sats,
            block_height: transaction.block_height,
            first_seen_at: transaction.first_seen_at,
            confirmed_at: transaction.confirmed_at,
            is_rbf: transaction.is_rbf,
            is_cpfp: transaction.is_cpfp,
            notification_status: vec![], // Empty for new transactions
        };

        // Send appropriate notification based on confirmation status
        let notification = if tx.block_height.is_some() {
            // Transaction mined directly - send confirmed notification
            TransactionNotification::Confirmed(tx)
        } else {
            // Transaction in mempool - send pending notification
            TransactionNotification::Pending(tx)
        };

        // Send through broadcast channel
        if let Err(_) = self.notification_sender.send(notification) {
            // Log but don't fail sync if no one is listening
            println!("[{}] No notification listeners active", transaction.wallet_checksum);
        }

        Ok(())
    }

    /// Send confirmation notification for a transaction that just got confirmed
    async fn send_confirmed_transaction_notification(&self, transaction: &Transaction) -> Result<()> {
        let notification = TransactionNotification::Confirmed(transaction.clone());

        // Send through broadcast channel
        if let Err(_) = self.notification_sender.send(notification) {
            // Log but don't fail sync if no one is listening
            println!("[{}] No notification listeners active", transaction.wallet_checksum);
        }

        Ok(())
    }
}