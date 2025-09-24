use crate::config::AppConfig;
use crate::electrum::ElectrumClient;
use crate::metadata::{
    EventType, MetadataDb, Transaction, TransactionInsert, TransactionNotification,
};
use anyhow::Result;
use bdk_wallet::{rusqlite::Connection, PersistedWallet};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Transaction-based wallet sync service
/// This replaces the old balance-based sync logic with proper transaction tracking
pub struct WalletSyncService {
    metadata_db: MetadataDb,
    notification_sender: broadcast::Sender<TransactionNotification>,
    config: AppConfig,
}

#[derive(Debug, Default)]
struct TransactionProcessSummary {
    has_changes: bool,
    new_transactions: usize,
    confirmation_updates: usize,
    conflicts_marked: usize,
}

impl WalletSyncService {
    pub fn new(
        metadata_db: MetadataDb,
        notification_sender: broadcast::Sender<TransactionNotification>,
        config: AppConfig,
    ) -> Self {
        Self {
            metadata_db,
            notification_sender,
            config,
        }
    }

    /// Sync a single wallet using transaction-based approach
    pub async fn sync_wallet_by_checksum(
        &self,
        wallet: &mut PersistedWallet<Connection>,
        wallet_checksum: &str,
        electrum_client: Option<&ElectrumClient>,
    ) -> Result<bool> {
        let sync_start = Instant::now();
        info!("[{}] Starting transaction-based sync", wallet_checksum);
        let mut electrum_duration = Duration::ZERO;
        let mut electrum_attempts: u32 = 0;

        // Perform the actual sync with Electrum with mode-based retry logic
        if let Some(client) = electrum_client {
            let max_retries: u32 = if self.config.is_saas_mode() { 3 } else { 1 };
            let use_exponential_backoff = self.config.is_saas_mode();

            let mut last_error = None;

            for attempt in 1..=max_retries {
                let attempt_start = Instant::now();
                let result = client.sync_wallet(wallet).await;
                let attempt_elapsed = attempt_start.elapsed();
                electrum_duration += attempt_elapsed;
                electrum_attempts = attempt;

                match result {
                    Ok(()) => {
                        debug!(
                            "[{}] Electrum sync attempt {} succeeded in {:.2?}",
                            wallet_checksum, attempt, attempt_elapsed
                        );
                        if attempt > 1 {
                            debug!(
                                "[{}] Sync succeeded on attempt {}/{}",
                                wallet_checksum, attempt, max_retries
                            );
                        }
                        last_error = None;
                        break;
                    }
                    Err(e) => {
                        let error_message = e.to_string();
                        warn!(
                            "[{}] Electrum sync attempt {} failed in {:.2?}: {}",
                            wallet_checksum, attempt, attempt_elapsed, error_message
                        );
                        last_error = Some(e);

                        // Enhanced error categorization for SAAS mode
                        if self.config.is_saas_mode() && attempt < max_retries {
                            let error_type = Self::categorize_error(&error_message);

                            let delay_secs = if use_exponential_backoff {
                                // Exponential backoff: 5s, 10s, 20s
                                5 * (2u64.pow(attempt - 1))
                            } else {
                                0
                            };

                            warn!(
                                "[{}] Sync attempt {}/{} failed ({}), retrying in {}s: {}",
                                wallet_checksum,
                                attempt,
                                max_retries,
                                error_type,
                                delay_secs,
                                error_message
                            );

                            if delay_secs > 0 {
                                sleep(Duration::from_secs(delay_secs)).await;
                            }
                        } else if attempt < max_retries {
                            // FOSS mode - simple retry without categorization
                            warn!(
                                "[{}] Sync attempt {}/{} failed: {}",
                                wallet_checksum, attempt, max_retries, error_message
                            );
                        }
                    }
                }
            }

            // If all retries failed, handle the error
            if let Some(error) = last_error {
                if self.config.is_saas_mode() {
                    error!(
                        "[{}] Failed to sync with Electrum after {} attempts: {}",
                        wallet_checksum, max_retries, error
                    );
                } else {
                    error!(
                        "[{}] Failed to sync with Electrum: {}",
                        wallet_checksum, error
                    );
                }
                return Ok(false);
            }
        }

        // Update last_synced_at timestamp
        let last_synced_start = Instant::now();
        let _ = self
            .metadata_db
            .update_wallet_last_synced(wallet_checksum)
            .await;
        debug!(
            "[{}] Metadata last_synced_at update took {:.2?}",
            wallet_checksum,
            last_synced_start.elapsed()
        );

        // Process all transactions and detect changes
        let tx_process_start = Instant::now();
        let summary = self
            .process_wallet_transactions(wallet, wallet_checksum, electrum_client)
            .await?;
        debug!(
            "[{}] Transaction processing took {:.2?}",
            wallet_checksum,
            tx_process_start.elapsed()
        );

        // Update wallet balance in metadata
        let balance_update_start = Instant::now();
        let current_balance = wallet.balance().total();
        self.metadata_db
            .update_wallet_balance_by_checksum(wallet_checksum, current_balance.to_sat() as i64)
            .await?;
        debug!(
            "[{}] Wallet balance metadata update took {:.2?}",
            wallet_checksum,
            balance_update_start.elapsed()
        );

        // Check balance alerts only if wallet has changes
        if summary.has_changes {
            let balance_alert_start = Instant::now();
            if let Err(e) = self.check_balance_alerts(wallet_checksum, current_balance.to_sat() as i64).await.map(|_| ()) {
                warn!(
                    "[{}] Balance alert checking failed: {}",
                    wallet_checksum, e
                );
            }
            debug!(
                "[{}] Balance alert checking took {:.2?}",
                wallet_checksum,
                balance_alert_start.elapsed()
            );
        }

        let sync_duration = sync_start.elapsed();
        info!(
            "[{}] Sync complete in {:.2}s (electrum {:.2}s across {} attempt(s)); changes={}, new_transactions={}, confirmations={}, conflicts_marked={}",
            wallet_checksum,
            sync_duration.as_secs_f64(),
            electrum_duration.as_secs_f64(),
            electrum_attempts,
            summary.has_changes,
            summary.new_transactions,
            summary.confirmation_updates,
            summary.conflicts_marked
        );

        // Log warning for unusually long syncs (SAAS mode only)
        if self.config.is_saas_mode() && sync_duration.as_secs() > 120 {
            warn!(
                "[{}] WARNING: Sync took {:.1}s (>120s), potential performance issue",
                wallet_checksum,
                sync_duration.as_secs_f64()
            );
        }
        Ok(summary.has_changes)
    }

    /// Process all transactions in the wallet and sync with database
    async fn process_wallet_transactions(
        &self,
        wallet: &PersistedWallet<Connection>,
        wallet_checksum: &str,
        electrum_client: Option<&ElectrumClient>,
    ) -> Result<TransactionProcessSummary> {
        let mut has_changes = false;

        let fetch_existing_start = Instant::now();

        // Get existing transactions sorted chronologically (oldest first for balance calculation)
        let existing_transactions = self
            .metadata_db
            .get_transactions_by_wallet_checksum(wallet_checksum, None)
            .await?;
        debug!(
            "[{}] Loaded {} existing transactions from metadata in {:.2?}",
            wallet_checksum,
            existing_transactions.len(),
            fetch_existing_start.elapsed()
        );

        // Create HashMap for O(1) transaction lookups to avoid individual database queries
        let existing_tx_map: std::collections::HashMap<
            String,
            &crate::metadata::TransactionWithWallet,
        > = existing_transactions
            .iter()
            .map(|tx| (tx.txid.clone(), tx))
            .collect();

        // Sort existing transactions by first_seen_at ASC (oldest first) for proper balance calculation
        let mut existing_txs_sorted = existing_transactions
            .iter()
            .map(|tx| {
                (
                    tx.txid.clone(),
                    tx.first_seen_at,
                    tx.amount_sats,
                    tx.transaction_type,
                )
            })
            .collect::<Vec<_>>();
        existing_txs_sorted.sort_by_key(|(_, first_seen_at, _, _)| *first_seen_at);

        // We no longer calculate balances - they are computed on-demand by the frontend

        // Get canonical (non-conflicting) transactions from BDK
        let canonical_build_start = Instant::now();
        let mut canonical_transactions_data: Vec<_> = wallet
            .transactions()
            .map(|tx_item| {
                let txid = tx_item.tx_node.txid.to_string();
                let (sent, received) = wallet.sent_and_received(&tx_item.tx_node);
                let net_amount = received.to_sat() as i64 - sent.to_sat() as i64;
                let block_height = tx_item.chain_position.confirmation_height_upper_bound();
                let is_confirmed = tx_item.chain_position.is_confirmed();

                // Preserve existing timestamp if transaction already exists, otherwise use current time
                let first_seen_at = existing_transactions
                    .iter()
                    .find(|tx| tx.txid == txid)
                    .map(|tx| tx.first_seen_at)
                    .unwrap_or_else(|| {
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs()
                    });

                (
                    txid,
                    net_amount,
                    block_height,
                    is_confirmed,
                    first_seen_at,
                    None as Option<u64>, // Will be filled in next step
                )
            })
            .collect();
        debug!(
            "[{}] Built canonical transaction snapshot ({} items) in {:.2?}",
            wallet_checksum,
            canonical_transactions_data.len(),
            canonical_build_start.elapsed()
        );

        // Fetch block timestamps ONLY for NEW confirmed transactions
        let mut block_header_fetch_count = 0usize;
        let mut block_header_fetch_duration = Duration::ZERO;

        for tx_data in &mut canonical_transactions_data {
            let (
                txid,
                _net_amount,
                block_height,
                is_confirmed,
                first_seen_at,
                ref mut confirmed_at,
            ) = tx_data;

            // Check if this is an existing transaction
            let existing_tx = existing_transactions.iter().find(|tx| tx.txid == *txid);

            if let Some(existing) = existing_tx {
                // Existing transaction - check if it's transitioning from pending to confirmed
                if *is_confirmed && existing.confirmed_at.is_none() {
                    // Transaction is now confirmed but wasn't before - fetch new block timestamp
                    *confirmed_at = if let Some(client) = electrum_client {
                        if let Some(height) = block_height {
                            let header_fetch_start = Instant::now();
                            let header_result = match client.get_block_header(*height).await {
                                Ok(header) => Some(header.timestamp),
                                Err(e) => {
                                    warn!(
                                        "[{}] Failed to fetch block header for newly confirmed tx {} at height {}: {}",
                                        wallet_checksum,
                                        txid,
                                        height,
                                        e
                                    );
                                    Some(*first_seen_at) // Fallback
                                }
                            };
                            block_header_fetch_count += 1;
                            block_header_fetch_duration += header_fetch_start.elapsed();
                            header_result
                        } else {
                            Some(*first_seen_at)
                        }
                    } else {
                        Some(*first_seen_at)
                    }
                } else {
                    // Preserve existing confirmed_at if already confirmed
                    *confirmed_at = existing.confirmed_at;
                }
            } else {
                // NEW transaction - determine appropriate timestamp
                *confirmed_at = if *is_confirmed {
                    // New confirmed transaction - fetch block timestamp
                    if let Some(client) = electrum_client {
                        if let Some(height) = block_height {
                            let header_fetch_start = Instant::now();
                            let header_result = match client.get_block_header(*height).await {
                                Ok(header) => Some(header.timestamp),
                                Err(e) => {
                                    warn!(
                                        "[{}] Failed to fetch block header for new tx {} at height {}: {}",
                                        wallet_checksum,
                                        txid,
                                        height,
                                        e
                                    );
                                    Some(*first_seen_at) // Fallback
                                }
                            };
                            block_header_fetch_count += 1;
                            block_header_fetch_duration += header_fetch_start.elapsed();
                            header_result
                        } else {
                            Some(*first_seen_at)
                        }
                    } else {
                        Some(*first_seen_at)
                    }
                } else {
                    None // Mempool transaction
                };
            }
        }

        if block_header_fetch_count > 0 {
            debug!(
                "[{}] Block header lookups: {} in {:.2?}",
                wallet_checksum, block_header_fetch_count, block_header_fetch_duration
            );
        }

        // Get ALL transactions (including non-canonical/conflicted ones) for RBF detection
        let all_txs_from_bdk: Vec<String> = wallet
            .tx_graph()
            .full_txs()
            .map(|tx| tx.txid.to_string())
            .collect();
        let canonical_txids: Vec<String> = canonical_transactions_data
            .iter()
            .map(|(txid, _, _, _, _, _)| txid.clone())
            .collect();

        // Find transactions that exist in full graph but NOT in canonical set (these are conflicted/replaced)
        let conflicted_txids: Vec<String> = all_txs_from_bdk
            .into_iter()
            .filter(|txid| !canonical_txids.contains(txid))
            .collect();

        // Sort canonical transactions by timestamp for progressive balance calculation
        let sort_start = Instant::now();
        let mut all_transactions = canonical_transactions_data;
        all_transactions.sort_by_key(|(_, _, _, _, first_seen_at, _)| *first_seen_at);
        debug!(
            "[{}] Sorted canonical transactions in {:.2?}",
            wallet_checksum,
            sort_start.elapsed()
        );

        // Detect CPFP relationships for unconfirmed transactions
        let cpfp_start = Instant::now();
        let cpfp_relationships =
            self.detect_cpfp_relationships(wallet, wallet_checksum, &all_transactions)?;
        debug!(
            "[{}] CPFP detection completed in {:.2?}",
            wallet_checksum,
            cpfp_start.elapsed()
        );

        // Process each canonical transaction
        let processing_loop_start = Instant::now();
        let mut new_tx_count = 0usize;
        let mut confirmation_updates = 0usize;
        for (txid, net_amount, block_height, is_confirmed, first_seen_at, confirmed_at) in
            &all_transactions
        {
            // Check if we already know about this transaction using in-memory lookup
            let existing_tx = existing_tx_map.get(txid);

            match existing_tx {
                None => {
                    // New transaction found

                    // Create new transaction record using pre-collected data
                    // Determine transaction type and amount
                    let (transaction_type, amount_sats, fee_sats) = if *net_amount < 0 {
                        // Outgoing transaction (send)
                        // For now, don't calculate fees - we can add this later
                        (EventType::Send, (-*net_amount) as i64, None)
                    } else {
                        // Incoming transaction (receive)
                        (EventType::Receive, *net_amount, None)
                    };

                    let transaction = TransactionInsert {
                        txid: txid.clone(),
                        wallet_checksum: wallet_checksum.to_string(),
                        transaction_type,
                        amount_sats,
                        fee_sats,
                        block_height: *block_height,
                        first_seen_at: *first_seen_at,
                        confirmed_at: *confirmed_at,
                        parent_txid: cpfp_relationships.get(txid).cloned(),
                        transaction_status: if *is_confirmed {
                            "confirmed".to_string()
                        } else {
                            "pending".to_string()
                        },
                        replaced_by_txid: None,
                        replaced_at: None,
                    };

                    let _transaction_id = self.metadata_db.insert_transaction(&transaction).await?;

                    // Send notifications for new transaction
                    self.send_new_transaction_notification(&transaction).await?;

                    has_changes = true;
                    new_tx_count += 1;
                    debug!(
                        "[{}] New transaction recorded: status={}, type={}, amount_sats={}",
                        wallet_checksum,
                        if transaction.block_height.is_some() {
                            "confirmed"
                        } else {
                            "pending"
                        },
                        transaction.transaction_type.as_str(),
                        transaction.amount_sats
                    );
                }
                Some(existing) => {
                    // Check if transaction status changed (mempool -> confirmed)
                    let is_now_confirmed = *is_confirmed;
                    let was_confirmed = existing.block_height.is_some();

                    if is_now_confirmed && !was_confirmed {
                        // Transaction just confirmed!
                        let block_height_value = block_height.unwrap_or(0);
                        let confirmed_at_value = confirmed_at.unwrap_or(*first_seen_at);

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
                        if let Some(updated_tx) = self
                            .metadata_db
                            .get_transaction_by_txid(wallet_checksum, &txid)
                            .await?
                        {
                            self.send_confirmed_transaction_notification(&updated_tx)
                                .await?;
                        }

                        has_changes = true;
                        confirmation_updates += 1;
                        debug!(
                            "[{}] Transaction confirmed: {} at height {}",
                            wallet_checksum, &txid, block_height_value
                        );
                    }
                }
            }
        }
        let processing_loop_duration = processing_loop_start.elapsed();

        let conflict_detection_start = Instant::now();
        let mut conflicts_marked = 0usize;

        // Handle RBF replacements using BDK's conflict detection
        // BDK has already identified conflicted transactions - these are the ones that got replaced

        if !conflicted_txids.is_empty() {
            debug!(
                "[{}] Found {} conflicted transactions from BDK",
                wallet_checksum,
                conflicted_txids.len()
            );

            // Get all transactions from BDK with full details for input comparison
            let all_bdk_txs: Vec<_> = wallet.tx_graph().full_txs().collect();

            for conflicted_txid in &conflicted_txids {
                // Check if this conflicted transaction is in our pending transactions using in-memory lookup
                if let Some(pending_tx) = existing_tx_map.get(conflicted_txid) {
                    if pending_tx.transaction_status == "pending" {
                        // Find the conflicted transaction's inputs
                        let conflicted_tx_inputs: Option<Vec<_>> = all_bdk_txs
                            .iter()
                            .find(|tx| tx.txid.to_string() == *conflicted_txid)
                            .map(|tx| {
                                tx.tx
                                    .input
                                    .iter()
                                    .map(|input| input.previous_output)
                                    .collect()
                            });

                        if let Some(conflicted_inputs) = conflicted_tx_inputs {
                            let mut found_replacement = false;

                            // Find canonical transaction that shares inputs with the conflicted transaction
                            for (canonical_txid, net_amount, _, _, canonical_first_seen, _) in
                                &all_transactions
                            {
                                // Skip if not newer than conflicted transaction
                                if *canonical_first_seen <= pending_tx.first_seen_at {
                                    continue;
                                }

                                // Check if this canonical transaction shares any inputs with the conflicted one
                                if let Some(canonical_tx) = all_bdk_txs
                                    .iter()
                                    .find(|tx| tx.txid.to_string() == *canonical_txid)
                                {
                                    let canonical_inputs: Vec<_> = canonical_tx
                                        .tx
                                        .input
                                        .iter()
                                        .map(|input| input.previous_output)
                                        .collect();

                                    // Check for shared inputs (RBF transactions spend the same UTXOs)
                                    let has_shared_inputs =
                                        conflicted_inputs.iter().any(|conflicted_input| {
                                            canonical_inputs.contains(conflicted_input)
                                        });

                                    // Also verify it's the same transaction type (send/receive)
                                    let same_type = (*net_amount < 0
                                        && pending_tx.transaction_type == EventType::Send)
                                        || (*net_amount > 0
                                            && pending_tx.transaction_type == EventType::Receive);

                                    if has_shared_inputs && same_type {
                                        debug!(
                                            "[{}] BDK conflict detected: {} replaced by {} (shared {} inputs)",
                                            wallet_checksum,
                                            conflicted_txid,
                                            canonical_txid,
                                            conflicted_inputs
                                                .iter()
                                                .filter(|input| canonical_inputs.contains(input))
                                                .count()
                                        );

                                        let replacement_marked = self
                                            .metadata_db
                                            .mark_transaction_replaced(
                                                wallet_checksum,
                                                conflicted_txid,
                                                canonical_txid,
                                            )
                                            .await?;

                                        if replacement_marked {
                                            has_changes = true;
                                            conflicts_marked += 1;
                                            found_replacement = true;
                                            break;
                                        }
                                    }
                                }
                            }

                            if !found_replacement {
                                debug!(
                                    "[{}] BDK detected conflict for {} but couldn't find canonical replacement with shared inputs",
                                    wallet_checksum,
                                    conflicted_txid
                                );
                            }
                        } else {
                            debug!(
                                "[{}] Could not find conflicted transaction {} in BDK's full transaction set",
                                wallet_checksum,
                                conflicted_txid
                            );
                        }
                    }
                } else {
                    debug!(
                        "[{}] Conflicted transaction {} not in our database - may be historical",
                        wallet_checksum, conflicted_txid
                    );
                }
            }
        }

        let conflict_detection_duration = conflict_detection_start.elapsed();

        debug!(
            "[{}] Transaction processing loop took {:.2?}",
            wallet_checksum, processing_loop_duration
        );
        debug!(
            "[{}] Conflict handling duration: {:.2?} (conflicts_marked={})",
            wallet_checksum, conflict_detection_duration, conflicts_marked
        );
        debug!(
            "[{}] Transaction changes summary: new={}, confirmations={}",
            wallet_checksum, new_tx_count, confirmation_updates
        );

        debug!(
            "[{}] End-to-end transaction processing time {:.2?}",
            wallet_checksum,
            fetch_existing_start.elapsed()
        );

        Ok(TransactionProcessSummary {
            has_changes,
            new_transactions: new_tx_count,
            confirmation_updates,
            conflicts_marked,
        })
    }

    /// Send notification for a new transaction (either pending or directly confirmed)
    async fn send_new_transaction_notification(
        &self,
        transaction: &TransactionInsert,
    ) -> Result<()> {
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
            parent_txid: transaction.parent_txid.clone(),
            transaction_status: transaction.transaction_status.clone(),
            replaced_by_txid: transaction.replaced_by_txid.clone(),
            replaced_at: transaction.replaced_at,
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
            debug!(
                "[{}] No notification listeners active",
                transaction.wallet_checksum
            );
        }

        Ok(())
    }

    /// Send confirmation notification for a transaction that just got confirmed
    async fn send_confirmed_transaction_notification(
        &self,
        transaction: &Transaction,
    ) -> Result<()> {
        let notification = TransactionNotification::Confirmed(transaction.clone());

        // Send through broadcast channel
        if let Err(_) = self.notification_sender.send(notification) {
            // Log but don't fail sync if no one is listening
            debug!(
                "[{}] No notification listeners active",
                transaction.wallet_checksum
            );
        }

        Ok(())
    }

    /// Extract historical transactions for background task (moved from wallet.rs)
    pub async fn extract_historical_transactions_for_background(
        wallet: &PersistedWallet<Connection>,
        wallet_checksum: &str,
        metadata_db: &MetadataDb,
        electrum_client: Option<&crate::electrum::ElectrumClient>,
    ) -> Result<()> {
        debug!("[{}] Extracting historical transactions", wallet_checksum);

        // Collect all transactions and sort them chronologically
        let mut all_transactions: Vec<_> = wallet.transactions().collect();

        // Sort transactions chronologically (confirmed first, then by height/timestamp)
        all_transactions.sort_by(|a, b| {
            match (&a.chain_position, &b.chain_position) {
                // Both confirmed: sort by block height
                (
                    bdk_wallet::chain::ChainPosition::Confirmed {
                        anchor: anchor_a, ..
                    },
                    bdk_wallet::chain::ChainPosition::Confirmed {
                        anchor: anchor_b, ..
                    },
                ) => anchor_a.block_id.height.cmp(&anchor_b.block_id.height),
                // Both unconfirmed: sort by first_seen timestamp if available
                (
                    bdk_wallet::chain::ChainPosition::Unconfirmed {
                        first_seen: first_a,
                        ..
                    },
                    bdk_wallet::chain::ChainPosition::Unconfirmed {
                        first_seen: first_b,
                        ..
                    },
                ) => first_a.unwrap_or(0).cmp(&first_b.unwrap_or(0)),
                // Confirmed comes before unconfirmed
                (
                    bdk_wallet::chain::ChainPosition::Confirmed { .. },
                    bdk_wallet::chain::ChainPosition::Unconfirmed { .. },
                ) => std::cmp::Ordering::Less,
                // Unconfirmed comes after confirmed
                (
                    bdk_wallet::chain::ChainPosition::Unconfirmed { .. },
                    bdk_wallet::chain::ChainPosition::Confirmed { .. },
                ) => std::cmp::Ordering::Greater,
            }
        });

        debug!(
            "[{}] Found {} historical transactions to process",
            wallet_checksum,
            all_transactions.len()
        );

        // Get current wallet balance
        let current_balance = wallet.balance().total().to_sat() as i64;

        // Calculate initial balance by working backwards from current balance
        let total_net_change: i64 = all_transactions
            .iter()
            .map(|tx| {
                let sent = wallet.sent_and_received(&tx.tx_node).0;
                let received = wallet.sent_and_received(&tx.tx_node).1;
                received.to_sat() as i64 - sent.to_sat() as i64
            })
            .filter(|&net| net != 0) // Skip zero net amount transactions
            .sum();

        let initial_balance = current_balance - total_net_change;

        debug!(
            "[{}] Current balance: {:.8} BTC, Initial balance: {:.8} BTC",
            wallet_checksum,
            current_balance as f64 / 100_000_000.0,
            initial_balance as f64 / 100_000_000.0
        );

        // Process each transaction chronologically using new transaction-based approach
        for tx in all_transactions {
            let txid = tx.tx_node.txid.to_string();
            let sent = wallet.sent_and_received(&tx.tx_node).0;
            let received = wallet.sent_and_received(&tx.tx_node).1;
            let net_amount = received.to_sat() as i64 - sent.to_sat() as i64;
            let _is_confirmed = tx.chain_position.is_confirmed();

            // Skip transactions with zero net amount
            if net_amount == 0 {
                continue;
            }

            let (transaction_type, amount_sats) = if net_amount > 0 {
                (EventType::Receive, net_amount)
            } else {
                (EventType::Send, net_amount.abs())
            };

            // Get block height and confirmation details
            let (block_height, confirmed_at) = match &tx.chain_position {
                bdk_wallet::chain::ChainPosition::Confirmed { anchor, .. } => {
                    let block_height = Some(anchor.block_id.height);
                    // Fetch actual block timestamp from Electrum
                    let confirmed_at = if let Some(electrum_client) = electrum_client {
                        match electrum_client
                            .get_block_header(anchor.block_id.height)
                            .await
                        {
                            Ok(header) => Some(header.timestamp),
                            Err(e) => {
                                warn!(
                                    "[{}] Failed to fetch block header for height {}: {}",
                                    wallet_checksum, anchor.block_id.height, e
                                );
                                // Fallback to current time only if fetch fails
                                Some(
                                    std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap()
                                        .as_secs(),
                                )
                            }
                        }
                    } else {
                        // No electrum client available, use current time as fallback
                        Some(
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs(),
                        )
                    };
                    (block_height, confirmed_at)
                }
                bdk_wallet::chain::ChainPosition::Unconfirmed { .. } => (None, None),
            };

            // Get first_seen timestamp
            let first_seen_at = match &tx.chain_position {
                bdk_wallet::chain::ChainPosition::Confirmed { .. } => {
                    confirmed_at.unwrap_or_else(|| {
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs()
                    })
                }
                bdk_wallet::chain::ChainPosition::Unconfirmed { first_seen, .. } => first_seen
                    .unwrap_or_else(|| {
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs()
                    }),
            };

            // Create transaction insert using new schema
            let is_confirmed = block_height.is_some();
            let transaction_insert = TransactionInsert {
                txid,
                wallet_checksum: wallet_checksum.to_string(),
                transaction_type,
                amount_sats,
                fee_sats: None, // TODO: Calculate fees for historical transactions
                block_height,
                first_seen_at,
                confirmed_at,
                parent_txid: None, // Historical transactions don't have CPFP relationship anymore
                transaction_status: if is_confirmed {
                    "confirmed".to_string()
                } else {
                    "pending".to_string()
                },
                replaced_by_txid: None,
                replaced_at: None,
            };

            // Insert individual transaction
            if let Err(e) = metadata_db.insert_transaction(&transaction_insert).await {
                warn!(
                    "[{}] Failed to insert historical transaction {}: {}",
                    wallet_checksum, transaction_insert.txid, e
                );
            }
        }

        debug!(
            "[{}] Historical transaction extraction completed",
            wallet_checksum
        );
        Ok(())
    }

    /// Detect CPFP relationships among unconfirmed transactions
    /// Returns a HashMap mapping child_txid -> parent_txid for transactions that are CPFP children
    fn detect_cpfp_relationships(
        &self,
        wallet: &PersistedWallet<Connection>,
        wallet_checksum: &str,
        all_transactions: &[(String, i64, Option<u32>, bool, u64, Option<u64>)],
    ) -> Result<std::collections::HashMap<String, String>> {
        use std::collections::HashMap;

        let mut cpfp_relationships = HashMap::new();

        // Only consider unconfirmed transactions for CPFP detection
        let unconfirmed_txs: Vec<_> = all_transactions
            .iter()
            .filter(|(_, _, _, is_confirmed, _, _)| !is_confirmed)
            .collect();

        if unconfirmed_txs.len() < 2 {
            // Need at least 2 unconfirmed transactions for CPFP
            return Ok(cpfp_relationships);
        }

        // Get all BDK transactions with full details
        let all_bdk_txs: Vec<_> = wallet.tx_graph().full_txs().collect();

        // Build a map of unconfirmed transaction outputs: (txid, vout) -> txid
        let mut unconfirmed_outputs: HashMap<(String, u32), String> = HashMap::new();
        for (txid, _, _, _, _, _) in &unconfirmed_txs {
            if let Some(bdk_tx) = all_bdk_txs.iter().find(|tx| tx.txid.to_string() == *txid) {
                for (vout, _) in bdk_tx.tx.output.iter().enumerate() {
                    unconfirmed_outputs.insert((txid.clone(), vout as u32), txid.clone());
                }
            }
        }

        // Check each unconfirmed transaction to see if it spends from another unconfirmed transaction
        for (child_txid, _, _, _, _, _) in &unconfirmed_txs {
            if let Some(bdk_tx) = all_bdk_txs
                .iter()
                .find(|tx| tx.txid.to_string() == *child_txid)
            {
                // Check each input of this transaction
                for input in &bdk_tx.tx.input {
                    let prev_txid = input.previous_output.txid.to_string();
                    let prev_vout = input.previous_output.vout;

                    // Check if this input references an output from another unconfirmed transaction
                    if let Some(parent_txid) =
                        unconfirmed_outputs.get(&(prev_txid.clone(), prev_vout))
                    {
                        if parent_txid != child_txid {
                            // Found CPFP relationship: child spends from parent
                            cpfp_relationships.insert(child_txid.clone(), parent_txid.clone());
                            debug!(
                                "[{}] CPFP detected: {} (child) spends from {} (parent) output {}:{}",
                                wallet_checksum,
                                child_txid,
                                parent_txid,
                                prev_txid,
                                prev_vout
                            );
                            break; // One parent relationship is enough
                        }
                    }
                }
            }
        }

        Ok(cpfp_relationships)
    }

    /// Categorize error types for better diagnostics (SAAS mode)
    fn categorize_error(error_msg: &str) -> &'static str {
        let msg_lower = error_msg.to_lowercase();

        if msg_lower.contains("timeout") || msg_lower.contains("timed out") {
            "TIMEOUT"
        } else if msg_lower.contains("connection") || msg_lower.contains("connect") {
            "CONNECTION"
        } else if msg_lower.contains("network") || msg_lower.contains("dns") {
            "NETWORK"
        } else if msg_lower.contains("server") || msg_lower.contains("electrum") {
            "SERVER"
        } else if msg_lower.contains("task") || msg_lower.contains("spawn") {
            "TASK"
        } else {
            "UNKNOWN"
        }
    }

    /// Check balance alerts and send notifications for triggered thresholds
    pub async fn check_balance_alerts(&self, wallet_checksum: &str, current_balance_sats: i64) -> Result<Vec<crate::metadata::BalanceAlert>> {
        let mut triggered_alerts = Vec::new();
        debug!("[{}] Checking balance alerts for balance: {} sats", wallet_checksum, current_balance_sats);

        // Get all active balance alerts for this wallet
        let active_alerts = self
            .metadata_db
            .get_active_balance_alerts_for_wallet(wallet_checksum)
            .await?;

        if active_alerts.is_empty() {
            debug!("[{}] No active balance alerts to check", wallet_checksum);
            return Ok(Vec::new());
        }

        debug!("[{}] Found {} active balance alerts to check", wallet_checksum, active_alerts.len());

        for alert in active_alerts {
            let should_trigger = match alert.alert_type {
                crate::metadata::BalanceAlertType::Above => current_balance_sats > alert.threshold_sats,
                crate::metadata::BalanceAlertType::Below => current_balance_sats < alert.threshold_sats,
                crate::metadata::BalanceAlertType::Equals => current_balance_sats == alert.threshold_sats,
            };

            if should_trigger {
                info!(
                    "[{}] Balance alert triggered: {} {} {} sats (current: {} sats)",
                    wallet_checksum,
                    alert.alert_type.as_str(),
                    alert.threshold_sats,
                    alert.alert_type.as_str(),
                    current_balance_sats
                );

                // Add to triggered alerts list for testing
                triggered_alerts.push(alert.clone());

                // Create notification record in balance_alert_notifications table
                if let Err(e) = self
                    .metadata_db
                    .create_balance_alert_notification(
                        &alert.id,
                        wallet_checksum,
                        alert.threshold_sats,
                        current_balance_sats,
                        alert.alert_type,
                    )
                    .await
                {
                    warn!(
                        "[{}] Failed to create balance alert notification record: {}",
                        wallet_checksum, e
                    );
                }

                // Send balance alert notification via existing notification system
                if let Err(e) = self.send_balance_alert_notification(&alert, wallet_checksum, current_balance_sats).await {
                    warn!(
                        "[{}] Failed to send balance alert notification: {}",
                        wallet_checksum, e
                    );
                }

                // Disable the alert after triggering (requires manual reactivation)
                if let Err(e) = self.metadata_db.disable_balance_alert_after_trigger(&alert.id).await {
                    warn!(
                        "[{}] Failed to disable balance alert after trigger: {}",
                        wallet_checksum, e
                    );
                } else {
                    debug!(
                        "[{}] Disabled balance alert {} after triggering",
                        wallet_checksum, alert.id
                    );
                }
            }
        }

        Ok(triggered_alerts)
    }

    /// Send balance alert notification using existing notification system
    async fn send_balance_alert_notification(
        &self,
        alert: &crate::metadata::BalanceAlert,
        wallet_checksum: &str,
        current_balance_sats: i64,
    ) -> Result<()> {
        // Create a balance alert notification that mimics transaction notifications
        let balance_alert_notification = crate::metadata::BalanceAlertNotification {
            id: uuid::Uuid::new_v4().to_string(),
            balance_alert_id: alert.id.clone(),
            wallet_checksum: wallet_checksum.to_string(),
            threshold_sats: alert.threshold_sats,
            current_balance_sats,
            alert_type: alert.alert_type,
            notification_sent_at: chrono::Utc::now().timestamp() as u64,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        // Send notification via broadcast channel (same as transaction notifications)
        if let Err(e) = self.notification_sender.send(
            crate::metadata::TransactionNotification::BalanceAlert(balance_alert_notification)
        ) {
            warn!(
                "[{}] Failed to send balance alert notification via broadcast: {}",
                wallet_checksum, e
            );
        } else {
            debug!(
                "[{}] Sent balance alert notification via broadcast channel",
                wallet_checksum
            );
        }

        Ok(())
    }
}
