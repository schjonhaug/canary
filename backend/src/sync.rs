use crate::admin_notifications::AdminNotifications;
use crate::config::AppConfig;
use crate::electrum::{ElectrumClient, ElectrumClientManager};
use crate::metadata::{
    BalanceAlertTriggerParams, EventType, MetadataDb, Transaction, TransactionInsert,
    TransactionNotification,
};
use crate::utils::{extract_address_from_descriptor, extract_pubkey_from_descriptor};
use anyhow::{anyhow, Result};
use bdk_wallet::bitcoin::{Address, Network, PublicKey, ScriptBuf, Txid};
use bdk_wallet::{rusqlite::Connection, PersistedWallet};
use std::str::FromStr;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Number of consecutive reconnection failures before sending an alert
const ALERT_FAILURE_THRESHOLD: u32 = 3;

/// The genesis coinbase txid — the only confirmed transaction at block height 0.
/// Electrum returns height=0 for both mempool transactions and genesis block transactions,
/// so we special-case this txid to correctly mark it as confirmed.
const GENESIS_COINBASE_TXID: &str =
    "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b";

/// Genesis block timestamp (2009-01-03T18:15:05Z) as a fallback when the Electrum
/// server cannot serve the block 0 header.
const GENESIS_BLOCK_TIMESTAMP: u64 = 1231006505;

/// Check if a transaction is confirmed based on Electrum's height convention.
/// height > 0: confirmed at that block height
/// height == 0: unconfirmed (mempool), EXCEPT for the genesis coinbase
/// height < 0: unconfirmed with unconfirmed parents
fn is_tx_confirmed(height: i32, txid: &str) -> bool {
    height > 0 || (height == 0 && txid == GENESIS_COINBASE_TXID)
}

/// Transaction summary: (txid, amount_sats, block_height, is_confirmed, first_seen_at, confirmed_at)
type TransactionSummary = (String, i64, Option<u32>, bool, u64, Option<u64>);

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
        electrum_manager: Option<&ElectrumClientManager>,
    ) -> Result<bool> {
        let sync_start = Instant::now();
        debug!("[{}] Starting descriptor-based sync", wallet_checksum);
        let mut electrum_duration = Duration::ZERO;
        let mut electrum_attempts: u32 = 0;

        // Perform the actual sync with Electrum with mode-based retry logic
        if let Some(manager) = electrum_manager {
            let max_retries: u32 = if self.config.is_cloud_mode() { 3 } else { 1 };
            let use_exponential_backoff = self.config.is_cloud_mode();

            let mut last_error = None;

            for attempt in 1..=max_retries {
                // Get fresh client from manager each attempt (may have reconnected)
                let client = match manager.get_client().await {
                    Some(c) => c,
                    None => {
                        warn!(
                            "[{}] No Electrum client available (attempt {}), triggering reconnection",
                            wallet_checksum, attempt
                        );
                        match manager.reconnect().await {
                            Ok(true) => {
                                info!("[{}] Reconnection successful", wallet_checksum);
                                // Send reconnected notification if we had previous failures (atomic check)
                                if manager.should_send_reconnected_notification() {
                                    let admin_notifications = AdminNotifications::new();
                                    if admin_notifications.is_enabled() {
                                        admin_notifications
                                            .notify_electrum_reconnected(manager.url())
                                            .await;
                                    }
                                }
                                match manager.get_client().await {
                                    Some(c) => c,
                                    None => {
                                        error!(
                                            "[{}] Client still unavailable after reconnection",
                                            wallet_checksum
                                        );
                                        continue;
                                    }
                                }
                            }
                            Ok(false) => {
                                debug!(
                                    "[{}] Reconnection in progress by another task, waiting...",
                                    wallet_checksum
                                );
                                sleep(Duration::from_secs(2)).await;
                                continue;
                            }
                            Err(e) => {
                                error!("[{}] Reconnection failed: {}", wallet_checksum, e);
                                // Check if we should send an alert
                                let failures = manager.get_consecutive_failures();
                                if failures >= ALERT_FAILURE_THRESHOLD
                                    && !manager.has_alert_been_sent()
                                {
                                    let admin_notifications = AdminNotifications::new();
                                    if admin_notifications.is_enabled() {
                                        admin_notifications
                                            .notify_electrum_disconnect(
                                                manager.url(),
                                                failures,
                                                Some(&e.to_string()),
                                            )
                                            .await;
                                        manager.mark_alert_sent();
                                        warn!(
                                            "[{}] Sent admin alert for {} consecutive Electrum failures",
                                            wallet_checksum, failures
                                        );
                                    }
                                }
                                continue;
                            }
                        }
                    }
                };

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
                        let error_type = Self::categorize_error(&error_message);

                        warn!(
                            "[{}] Electrum sync attempt {} failed in {:.2?} ({}): {}",
                            wallet_checksum, attempt, attempt_elapsed, error_type, error_message
                        );
                        last_error = Some(e);

                        // If transport error or timeout, mark connection as dead and attempt reconnection
                        // Timeouts indicate a stale/unresponsive connection that needs to be recreated
                        if error_type == "TRANSPORT" || error_type == "TIMEOUT" {
                            warn!(
                                "[{}] {} error detected, triggering reconnection",
                                wallet_checksum, error_type
                            );
                            manager.mark_disconnected(&error_message).await;

                            match manager.reconnect().await {
                                Ok(true) => {
                                    info!(
                                        "[{}] Reconnection successful, will retry sync",
                                        wallet_checksum
                                    );
                                    // Send reconnected notification if we had previous failures (atomic check)
                                    if manager.should_send_reconnected_notification() {
                                        let admin_notifications = AdminNotifications::new();
                                        if admin_notifications.is_enabled() {
                                            admin_notifications
                                                .notify_electrum_reconnected(manager.url())
                                                .await;
                                        }
                                    }
                                }
                                Ok(false) => {
                                    debug!(
                                        "[{}] Reconnection already in progress by another task",
                                        wallet_checksum
                                    );
                                }
                                Err(reconnect_err) => {
                                    error!(
                                        "[{}] Reconnection failed: {}",
                                        wallet_checksum, reconnect_err
                                    );
                                    // Check if we should send an alert
                                    let failures = manager.get_consecutive_failures();
                                    if failures >= ALERT_FAILURE_THRESHOLD
                                        && !manager.has_alert_been_sent()
                                    {
                                        let admin_notifications = AdminNotifications::new();
                                        if admin_notifications.is_enabled() {
                                            admin_notifications
                                                .notify_electrum_disconnect(
                                                    manager.url(),
                                                    failures,
                                                    Some(&reconnect_err.to_string()),
                                                )
                                                .await;
                                            manager.mark_alert_sent();
                                            warn!(
                                                "[{}] Sent admin alert for {} consecutive Electrum failures",
                                                wallet_checksum, failures
                                            );
                                        }
                                    }
                                }
                            }
                        }

                        // Apply backoff delay before retry (cloud mode only)
                        if self.config.is_cloud_mode() && attempt < max_retries {
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
                            // Self-hosted mode - simple retry without categorization
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
                if self.config.is_cloud_mode() {
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
        // Get the current client for transaction processing (may be None if disconnected)
        let electrum_client = match electrum_manager {
            Some(manager) => manager.get_client().await,
            None => None,
        };
        let summary = self
            .process_wallet_transactions(wallet, wallet_checksum, electrum_client.as_ref())
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

        // Check balance alerts on every sync (for both BTC and fiat alerts)
        let balance_alert_start = Instant::now();
        if let Err(e) = self
            .check_balance_alerts(wallet_checksum, current_balance.to_sat() as i64)
            .await
            .map(|_| ())
        {
            warn!("[{}] Balance alert checking failed: {}", wallet_checksum, e);
        }
        debug!(
            "[{}] Balance alert checking took {:.2?}",
            wallet_checksum,
            balance_alert_start.elapsed()
        );

        let sync_duration = sync_start.elapsed();
        if summary.has_changes {
            info!(
                "[{}] Descriptor-based sync complete in {:.2}s (electrum {:.2}s across {} attempt(s)); changes={}, new_transactions={}, confirmations={}, conflicts_marked={}",
                wallet_checksum,
                sync_duration.as_secs_f64(),
                electrum_duration.as_secs_f64(),
                electrum_attempts,
                summary.has_changes,
                summary.new_transactions,
                summary.confirmation_updates,
                summary.conflicts_marked
            );
        } else {
            debug!(
                "[{}] Descriptor-based sync complete in {:.2}s (electrum {:.2}s across {} attempt(s)); changes=false",
                wallet_checksum,
                sync_duration.as_secs_f64(),
                electrum_duration.as_secs_f64(),
                electrum_attempts,
            );
        }

        // Log warning for unusually long syncs (cloud mode only)
        if self.config.is_cloud_mode() && sync_duration.as_secs() > 120 {
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
                        (EventType::Send, -(*net_amount), None)
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
                                txid,
                                block_height_value,
                                confirmed_at_value,
                            )
                            .await?;

                        // Send confirmation notification
                        // Need to get the updated transaction record
                        if let Some(updated_tx) = self
                            .metadata_db
                            .get_transaction_by_txid(wallet_checksum, txid)
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
        if self.notification_sender.send(notification).is_err() {
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
        if self.notification_sender.send(notification).is_err() {
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
        all_transactions: &[TransactionSummary],
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

    /// Categorize error types for better diagnostics (cloud mode)
    fn categorize_error(error_msg: &str) -> &'static str {
        // Check for transport-level failures first (these need reconnection)
        if ElectrumClientManager::is_transport_error(error_msg) {
            return "TRANSPORT";
        }

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
    pub async fn check_balance_alerts(
        &self,
        wallet_checksum: &str,
        current_balance_sats: i64,
    ) -> Result<Vec<crate::metadata::BalanceAlert>> {
        let mut triggered_alerts = Vec::new();
        debug!(
            "[{}] Checking balance alerts for balance: {} sats",
            wallet_checksum, current_balance_sats
        );

        // Get all active balance alerts for this wallet
        let active_alerts = self
            .metadata_db
            .get_active_balance_alerts_for_wallet(wallet_checksum)
            .await?;

        if active_alerts.is_empty() {
            debug!("[{}] No active balance alerts to check", wallet_checksum);
            return Ok(Vec::new());
        }

        debug!(
            "[{}] Found {} active balance alerts to check",
            wallet_checksum,
            active_alerts.len()
        );

        for alert in active_alerts {
            // Determine threshold to compare against
            // For fiat thresholds, convert current balance to fiat using current exchange rate
            let (comparison_threshold, exchange_rate_snapshot) = if let (
                Some(ref currency),
                Some(fiat_amount),
            ) =
                (&alert.threshold_currency, alert.threshold_fiat_amount)
            {
                // Fiat threshold - need to convert current balance to fiat
                match self.metadata_db.get_exchange_rates().await {
                    Ok(rates) => {
                        match rates.get(currency) {
                            Some(rate) => {
                                let rate_per_btc = rate.rate_per_btc;
                                // Convert current balance from sats to BTC to fiat
                                let balance_btc = current_balance_sats as f64 / 100_000_000.0;
                                let balance_fiat = balance_btc * rate_per_btc;

                                debug!(
                                    "[{}] Checking fiat alert: balance {} {} (from {} sats at rate {} {}/BTC), threshold {} {}",
                                    wallet_checksum, balance_fiat, currency, current_balance_sats, rate_per_btc, currency, fiat_amount, currency
                                );

                                // For fiat alerts, we compare fiat amounts directly
                                // We'll use a threshold in "fiat cents" for comparison to avoid floating point issues
                                let _balance_cents = (balance_fiat * 100.0) as i64;
                                let threshold_cents = (fiat_amount * 100.0) as i64;

                                (threshold_cents, Some(rate_per_btc))
                            }
                            None => {
                                warn!(
                                    "[{}] Exchange rate for {} not available, skipping fiat alert check",
                                    wallet_checksum, currency
                                );
                                continue;
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            "[{}] Failed to fetch exchange rates for fiat alert: {}, skipping",
                            wallet_checksum, e
                        );
                        continue;
                    }
                }
            } else {
                // BTC threshold - compare satoshis directly
                (alert.threshold_sats, None)
            };

            // Determine current value for comparison
            let current_value = if alert.threshold_currency.is_some() {
                // For fiat alerts, convert current balance to fiat cents
                if let Some(rate) = exchange_rate_snapshot {
                    let balance_btc = current_balance_sats as f64 / 100_000_000.0;
                    let balance_fiat = balance_btc * rate;
                    (balance_fiat * 100.0) as i64
                } else {
                    current_balance_sats // Fallback (should not reach here)
                }
            } else {
                current_balance_sats
            };

            // Implement threshold crossing detection
            // Only fire when balance crosses the threshold in the relevant direction
            let last_checked = alert.last_checked_balance_sats;

            let should_trigger = if let Some(last_checked_value) = last_checked {
                // For fiat alerts, last_checked_value is already in fiat cents
                // For BTC alerts, last_checked_value is in satoshis
                let previous_value = last_checked_value;

                match alert.alert_type {
                    crate::metadata::BalanceAlertType::Above => {
                        // Fire when crossing from at-or-below to above
                        previous_value <= comparison_threshold
                            && current_value > comparison_threshold
                    }
                    crate::metadata::BalanceAlertType::Below => {
                        // Fire when crossing from at-or-above to below
                        previous_value >= comparison_threshold
                            && current_value < comparison_threshold
                    }
                    crate::metadata::BalanceAlertType::Equals => {
                        // Fire when crossing to equals (from any non-equals value)
                        previous_value != comparison_threshold
                            && current_value == comparison_threshold
                    }
                }
            } else {
                // First check - initialize last_checked_balance_sats without firing
                debug!(
                    "[{}] Initializing balance alert {} with current balance: {} sats",
                    wallet_checksum, alert.id, current_balance_sats
                );
                false
            };

            // Update last_checked_balance_sats for next sync cycle
            // For fiat alerts, store the fiat value (in cents) to detect rate changes
            // For BTC alerts, store the satoshi value
            let value_to_store = if alert.threshold_currency.is_some() {
                // Store fiat value in cents for fiat alerts
                current_value
            } else {
                // Store satoshi value for BTC alerts
                current_balance_sats
            };

            if let Err(e) = self
                .metadata_db
                .update_alert_last_checked_balance(&alert.id, value_to_store)
                .await
            {
                warn!(
                    "[{}] Failed to update last_checked_balance for alert {}: {}",
                    wallet_checksum, alert.id, e
                );
            }

            if should_trigger {
                let threshold_desc = if let (Some(ref currency), Some(fiat_amount)) =
                    (&alert.threshold_currency, alert.threshold_fiat_amount)
                {
                    format!(
                        "{} {} (≈ {} sats)",
                        fiat_amount, currency, alert.threshold_sats
                    )
                } else {
                    format!("{} sats", alert.threshold_sats)
                };

                info!(
                    "[{}] Balance alert triggered: {} {} (current: {} sats)",
                    wallet_checksum,
                    alert.alert_type.as_str(),
                    threshold_desc,
                    current_balance_sats
                );

                // Add to triggered alerts list for testing
                triggered_alerts.push(alert.clone());

                // Update last_triggered_at timestamp
                if let Err(e) = self
                    .metadata_db
                    .update_balance_alert_triggered_timestamp(&alert.id)
                    .await
                {
                    warn!(
                        "[{}] Failed to update balance alert triggered timestamp: {}",
                        wallet_checksum, e
                    );
                }

                // Create notification record in balance_alert_notifications table
                let trigger_params = BalanceAlertTriggerParams {
                    threshold_sats: alert.threshold_sats,
                    current_balance_sats,
                    alert_type: alert.alert_type,
                    threshold_currency: alert.threshold_currency.clone(),
                    threshold_fiat_amount: alert.threshold_fiat_amount,
                    exchange_rate_snapshot,
                };
                if let Err(e) = self
                    .metadata_db
                    .create_balance_alert_notification(&alert.id, wallet_checksum, &trigger_params)
                    .await
                {
                    warn!(
                        "[{}] Failed to create balance alert notification record: {}",
                        wallet_checksum, e
                    );
                }

                // Send balance alert notification via existing notification system
                if let Err(e) = self
                    .send_balance_alert_notification(
                        &alert,
                        wallet_checksum,
                        current_balance_sats,
                        exchange_rate_snapshot,
                    )
                    .await
                {
                    warn!(
                        "[{}] Failed to send balance alert notification: {}",
                        wallet_checksum, e
                    );
                }

                // Alert remains active and will check for next crossing
                debug!(
                    "[{}] Balance alert {} triggered, will check for next crossing in future syncs",
                    wallet_checksum, alert.id
                );
            }
        }

        Ok(triggered_alerts)
    }

    /// Sync a single-address watch using direct Electrum script queries.
    ///
    /// Address watches use `addr()` descriptors (BIP-385) which are valid Bitcoin descriptors
    /// supported by Bitcoin Core, but not yet by `rust-miniscript` / BDK. Because of this we
    /// cannot create a BDK `Wallet` for them and instead query Electrum directly via
    /// `script_get_history` and `script_get_balance`.
    ///
    /// We evaluated BDK's `SpkTxOutIndex` / `IndexedTxGraph` from `bdk_chain` as an
    /// alternative (tracks arbitrary script pubkeys without descriptor support) and decided
    /// to keep the direct Electrum approach because:
    /// - `SpkTxOutIndex` is an in-memory index, not a sync engine — we'd still need the
    ///   same Electrum network calls to fetch transactions.
    /// - Our stateless approach (query Electrum, diff against the DB) avoids the need to
    ///   persist `TxGraph` state or accept full re-syncs on restart.
    /// - The N+1 parent-tx fetches for send detection happen either way — `SpkTxOutIndex`
    ///   only replaces who iterates the inputs (~20 lines of code).
    /// - `IndexedTxGraph` adds conflict/reorg handling we don't need for single addresses.
    ///
    /// Revisit if we support multiple addresses per watch or need RBF/reorg detection.
    ///
    /// If `rust-miniscript` adds `addr()` support in the future, we can migrate to BDK wallets
    /// without any data migration since the stored descriptors are already in standard format.
    ///
    /// Resolve an `addr()` or `pk()` descriptor to the corresponding ScriptBuf.
    fn script_from_watch_descriptor(descriptor: &str, network: Network) -> Result<ScriptBuf> {
        if let Some(address_str) = extract_address_from_descriptor(descriptor) {
            let address = Address::from_str(&address_str)
                .map_err(|e| anyhow!("Failed to parse address {}: {}", address_str, e))?;
            Ok(address
                .require_network(network)
                .map_err(|e| anyhow!("Address network mismatch for {}: {}", address_str, e))?
                .script_pubkey())
        } else if let Some(pubkey_str) = extract_pubkey_from_descriptor(descriptor) {
            let pubkey = PublicKey::from_str(&pubkey_str)
                .map_err(|e| anyhow!("Failed to parse public key {}: {}", pubkey_str, e))?;
            Ok(ScriptBuf::new_p2pk(&pubkey))
        } else {
            Err(anyhow!(
                "Invalid watch descriptor format (expected addr() or pk()): {}",
                descriptor
            ))
        }
    }

    /// See: https://github.com/rust-bitcoin/rust-miniscript/issues/294
    /// See: https://github.com/bitcoindevkit/bdk_wallet/issues/174
    pub async fn sync_address_watch(
        &self,
        wallet_checksum: &str,
        descriptor: &str,
        electrum_manager: Option<&ElectrumClientManager>,
        suppress_notifications: bool,
    ) -> Result<bool> {
        let sync_start = Instant::now();
        debug!("[{}] Starting address-based sync", wallet_checksum);

        let script = Self::script_from_watch_descriptor(
            descriptor,
            self.config.network.to_bdk_network(),
        )?;

        // Get Electrum client
        let client = match electrum_manager {
            Some(manager) => match manager.get_client().await {
                Some(c) => c,
                None => {
                    warn!(
                        "[{}] No Electrum client available for address watch",
                        wallet_checksum
                    );
                    return Ok(false);
                }
            },
            None => {
                warn!(
                    "[{}] No Electrum manager available for address watch",
                    wallet_checksum
                );
                return Ok(false);
            }
        };

        // Get transaction history for the address
        let history = client.script_get_history(&script).await?;
        debug!(
            "[{}] Address has {} transactions in history",
            wallet_checksum,
            history.len()
        );

        // Get existing transactions from our database
        let existing_transactions = self
            .metadata_db
            .get_transactions_by_wallet_checksum(wallet_checksum, None)
            .await?;
        let existing_txids: std::collections::HashSet<String> = existing_transactions
            .iter()
            .map(|tx| tx.txid.clone())
            .collect();

        let mut has_changes = false;

        // Process each transaction in history
        for hist_entry in &history {
            let txid_str = hist_entry.tx_hash.to_string();

            if existing_txids.contains(&txid_str) {
                // Check if an existing pending transaction got confirmed
                if is_tx_confirmed(hist_entry.height, &txid_str) {
                    if let Some(_existing) = existing_transactions
                        .iter()
                        .find(|tx| tx.txid == txid_str && tx.block_height.is_none())
                    {
                        // Transaction just confirmed
                        let confirmed_at =
                            match client.get_block_header(hist_entry.height as u32).await {
                                Ok(header) => header.timestamp,
                                Err(_) if hist_entry.height == 0 => GENESIS_BLOCK_TIMESTAMP,
                                Err(_) => std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs(),
                            };

                        self.metadata_db
                            .update_transaction_confirmation(
                                wallet_checksum,
                                &txid_str,
                                hist_entry.height as u32,
                                confirmed_at,
                            )
                            .await?;

                        // Send confirmation notification
                        if !suppress_notifications {
                            if let Some(updated_tx) = self
                                .metadata_db
                                .get_transaction_by_txid(wallet_checksum, &txid_str)
                                .await?
                            {
                                self.send_confirmed_transaction_notification(&updated_tx)
                                    .await?;
                            }
                        }

                        has_changes = true;
                        debug!(
                            "[{}] Address watch tx confirmed: {} at height {}",
                            wallet_checksum, txid_str, hist_entry.height
                        );
                    }
                }
                continue;
            }

            // New transaction - fetch full tx to determine amount
            let txid = Txid::from_str(&txid_str)
                .map_err(|e| anyhow!("Failed to parse txid {}: {}", txid_str, e))?;
            let full_tx = match client.transaction_get(&txid).await {
                Ok(tx) => tx,
                Err(e) => {
                    warn!(
                        "[{}] Failed to fetch tx {}: {}",
                        wallet_checksum, txid_str, e
                    );
                    continue;
                }
            };

            // Calculate received amount (outputs to our address)
            let mut received: i64 = 0;
            for output in &full_tx.output {
                if output.script_pubkey == script {
                    received += output.value.to_sat() as i64;
                }
            }

            // Calculate sent amount (inputs from our address)
            let mut sent: i64 = 0;
            for input in &full_tx.input {
                // Skip coinbase inputs (no previous transaction to fetch)
                if input.previous_output.is_null() {
                    continue;
                }
                let prev_txid = input.previous_output.txid;
                let prev_vout = input.previous_output.vout;
                match client.transaction_get(&prev_txid).await {
                    Ok(prev_tx) => {
                        if let Some(prev_output) = prev_tx.output.get(prev_vout as usize) {
                            if prev_output.script_pubkey == script {
                                sent += prev_output.value.to_sat() as i64;
                            }
                        }
                    }
                    Err(e) => {
                        debug!(
                            "[{}] Could not fetch parent tx {}: {}",
                            wallet_checksum, prev_txid, e
                        );
                    }
                }
            }

            if received == 0 && sent == 0 {
                debug!(
                    "[{}] Transaction {} has no inputs/outputs for our address, skipping",
                    wallet_checksum, txid_str
                );
                continue;
            }

            // Determine transaction type and net amount
            let (event_type, amount) = if sent > 0 && received == 0 {
                // Pure send (no change back to same address)
                (EventType::Send, sent)
            } else if sent > 0 && received > 0 {
                // Send with change back to our address; net amount sent
                (EventType::Send, (sent - received).max(0))
            } else {
                // Pure receive
                (EventType::Receive, received)
            };

            let is_confirmed = is_tx_confirmed(hist_entry.height, &txid_str);
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            let (confirmed_at, block_height) = if is_confirmed {
                let timestamp = match client.get_block_header(hist_entry.height as u32).await {
                    Ok(header) => header.timestamp,
                    Err(_) if hist_entry.height == 0 => GENESIS_BLOCK_TIMESTAMP,
                    Err(_) => now,
                };
                (Some(timestamp), Some(hist_entry.height as u32))
            } else {
                (None, None)
            };

            let first_seen_at = confirmed_at.unwrap_or(now);

            let transaction = TransactionInsert {
                txid: txid_str.clone(),
                wallet_checksum: wallet_checksum.to_string(),
                transaction_type: event_type,
                amount_sats: amount,
                fee_sats: None,
                block_height,
                first_seen_at,
                confirmed_at,
                parent_txid: None,
                transaction_status: if is_confirmed {
                    "confirmed".to_string()
                } else {
                    "pending".to_string()
                },
                replaced_by_txid: None,
                replaced_at: None,
            };

            self.metadata_db.insert_transaction(&transaction).await?;
            if !suppress_notifications {
                self.send_new_transaction_notification(&transaction).await?;
            }

            has_changes = true;
            info!(
                "[{}] New address watch tx: {} ({:?}, {} sats, {}{})",
                wallet_checksum,
                txid_str,
                event_type,
                amount,
                if is_confirmed { "confirmed" } else { "pending" },
                if suppress_notifications { ", notifications suppressed" } else { "" }
            );
        }

        // Update balance from Electrum
        let balance = client.script_get_balance(&script).await?;
        let total_balance = balance.confirmed as i64 + balance.unconfirmed as i64;
        self.metadata_db
            .update_wallet_balance_by_checksum(wallet_checksum, total_balance)
            .await?;

        // Update last_synced_at
        let _ = self
            .metadata_db
            .update_wallet_last_synced(wallet_checksum)
            .await;

        // Check balance alerts (skip during initial sync to avoid spurious alerts)
        if !suppress_notifications {
            if let Err(e) = self
                .check_balance_alerts(wallet_checksum, total_balance)
                .await
            {
                warn!("[{}] Balance alert checking failed: {}", wallet_checksum, e);
            }
        }

        let sync_duration = sync_start.elapsed();
        if has_changes {
            info!(
                "[{}] Address-based sync complete in {:.2}s; changes=true",
                wallet_checksum,
                sync_duration.as_secs_f64(),
            );
        } else {
            debug!(
                "[{}] Address-based sync complete in {:.2}s; changes=false",
                wallet_checksum,
                sync_duration.as_secs_f64(),
            );
        }

        Ok(has_changes)
    }

    /// Sync a group of address watches that share the same descriptor.
    /// Queries Electrum once and fans out the results (transactions, balance,
    /// notifications) to each watcher independently.
    pub async fn sync_address_watch_group(
        &self,
        wallet_checksums: &[String],
        descriptor: &str,
        electrum_manager: Option<&ElectrumClientManager>,
        suppress_notifications: bool,
    ) -> Result<bool> {
        let sync_start = Instant::now();
        info!(
            "Starting grouped address watch sync for {} watchers (descriptor: {})",
            wallet_checksums.len(),
            descriptor
        );

        let script = Self::script_from_watch_descriptor(
            descriptor,
            self.config.network.to_bdk_network(),
        )?;

        // Get Electrum client
        let client = match electrum_manager {
            Some(manager) => match manager.get_client().await {
                Some(c) => c,
                None => {
                    warn!("No Electrum client available for grouped address watch");
                    return Ok(false);
                }
            },
            None => {
                warn!("No Electrum manager available for grouped address watch");
                return Ok(false);
            }
        };

        // === Single Electrum query for history ===
        let history = client.script_get_history(&script).await?;
        debug!(
            "Grouped address watch: {} transactions in history",
            history.len()
        );

        // === Single Electrum query for balance ===
        let balance = client.script_get_balance(&script).await?;
        let total_balance = balance.confirmed as i64 + balance.unconfirmed as i64;

        // Cache block headers by height to avoid repeated Electrum calls
        let mut block_header_cache: std::collections::HashMap<u32, u64> =
            std::collections::HashMap::new();

        let mut any_changes = false;

        // === Fan out to each watcher ===
        for wallet_checksum in wallet_checksums {
            // Get existing transactions for THIS watcher
            let existing_transactions = self
                .metadata_db
                .get_transactions_by_wallet_checksum(wallet_checksum, None)
                .await?;
            let existing_txids: std::collections::HashSet<String> = existing_transactions
                .iter()
                .map(|tx| tx.txid.clone())
                .collect();

            let mut has_changes = false;

            for hist_entry in &history {
                let txid_str = hist_entry.tx_hash.to_string();

                if existing_txids.contains(&txid_str) {
                    // Check if an existing pending transaction got confirmed
                    if is_tx_confirmed(hist_entry.height, &txid_str) {
                        if let Some(_existing) = existing_transactions
                            .iter()
                            .find(|tx| tx.txid == txid_str && tx.block_height.is_none())
                        {
                            let confirmed_at = match block_header_cache
                                .get(&(hist_entry.height as u32))
                            {
                                Some(&ts) => ts,
                                None => {
                                    let ts = match client
                                        .get_block_header(hist_entry.height as u32)
                                        .await
                                    {
                                        Ok(header) => header.timestamp,
                                        Err(_) if hist_entry.height == 0 => {
                                            GENESIS_BLOCK_TIMESTAMP
                                        }
                                        Err(_) => std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap()
                                            .as_secs(),
                                    };
                                    block_header_cache.insert(hist_entry.height as u32, ts);
                                    ts
                                }
                            };

                            self.metadata_db
                                .update_transaction_confirmation(
                                    wallet_checksum,
                                    &txid_str,
                                    hist_entry.height as u32,
                                    confirmed_at,
                                )
                                .await?;

                            if !suppress_notifications {
                                if let Some(updated_tx) = self
                                    .metadata_db
                                    .get_transaction_by_txid(wallet_checksum, &txid_str)
                                    .await?
                                {
                                    self.send_confirmed_transaction_notification(&updated_tx)
                                        .await?;
                                }
                            }

                            has_changes = true;
                        }
                    }
                    continue;
                }

                // New transaction for this watcher - fetch full tx
                let txid = Txid::from_str(&txid_str)
                    .map_err(|e| anyhow!("Failed to parse txid {}: {}", txid_str, e))?;
                let full_tx = match client.transaction_get(&txid).await {
                    Ok(tx) => tx,
                    Err(e) => {
                        warn!(
                            "[{}] Failed to fetch tx {}: {}",
                            wallet_checksum, txid_str, e
                        );
                        continue;
                    }
                };

                // Calculate received amount
                let mut received: i64 = 0;
                for output in &full_tx.output {
                    if output.script_pubkey == script {
                        received += output.value.to_sat() as i64;
                    }
                }

                // Calculate sent amount
                let mut sent: i64 = 0;
                for input in &full_tx.input {
                    // Skip coinbase inputs (no previous transaction to fetch)
                    if input.previous_output.is_null() {
                        continue;
                    }
                    let prev_txid = input.previous_output.txid;
                    let prev_vout = input.previous_output.vout;
                    match client.transaction_get(&prev_txid).await {
                        Ok(prev_tx) => {
                            if let Some(prev_output) = prev_tx.output.get(prev_vout as usize) {
                                if prev_output.script_pubkey == script {
                                    sent += prev_output.value.to_sat() as i64;
                                }
                            }
                        }
                        Err(e) => {
                            debug!(
                                "[{}] Could not fetch parent tx {}: {}",
                                wallet_checksum, prev_txid, e
                            );
                        }
                    }
                }

                if received == 0 && sent == 0 {
                    continue;
                }

                let (event_type, amount) = if sent > 0 && received == 0 {
                    (EventType::Send, sent)
                } else if sent > 0 && received > 0 {
                    (EventType::Send, (sent - received).max(0))
                } else {
                    (EventType::Receive, received)
                };

                let is_confirmed = is_tx_confirmed(hist_entry.height, &txid_str);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();

                let (confirmed_at, block_height) = if is_confirmed {
                    let timestamp =
                        match block_header_cache.get(&(hist_entry.height as u32)) {
                            Some(&ts) => ts,
                            None => {
                                let ts =
                                    match client.get_block_header(hist_entry.height as u32).await {
                                        Ok(header) => header.timestamp,
                                        Err(_) if hist_entry.height == 0 => {
                                            GENESIS_BLOCK_TIMESTAMP
                                        }
                                        Err(_) => now,
                                    };
                                block_header_cache.insert(hist_entry.height as u32, ts);
                                ts
                            }
                        };
                    (Some(timestamp), Some(hist_entry.height as u32))
                } else {
                    (None, None)
                };

                let first_seen_at = confirmed_at.unwrap_or(now);

                let transaction = TransactionInsert {
                    txid: txid_str.clone(),
                    wallet_checksum: wallet_checksum.to_string(),
                    transaction_type: event_type,
                    amount_sats: amount,
                    fee_sats: None,
                    block_height,
                    first_seen_at,
                    confirmed_at,
                    parent_txid: None,
                    transaction_status: if is_confirmed {
                        "confirmed".to_string()
                    } else {
                        "pending".to_string()
                    },
                    replaced_by_txid: None,
                    replaced_at: None,
                };

                self.metadata_db.insert_transaction(&transaction).await?;
                if !suppress_notifications {
                    self.send_new_transaction_notification(&transaction).await?;
                }

                has_changes = true;
                info!(
                    "[{}] New address watch tx: {} ({:?}, {} sats, {}{})",
                    wallet_checksum,
                    txid_str,
                    event_type,
                    amount,
                    if is_confirmed { "confirmed" } else { "pending" },
                    if suppress_notifications { ", notifications suppressed" } else { "" }
                );
            }

            // Update balance for this watcher
            self.metadata_db
                .update_wallet_balance_by_checksum(wallet_checksum, total_balance)
                .await?;

            // Update last_synced_at
            let _ = self
                .metadata_db
                .update_wallet_last_synced(wallet_checksum)
                .await;

            // Check balance alerts for this watcher (skip during initial sync)
            if !suppress_notifications {
                if let Err(e) = self
                    .check_balance_alerts(wallet_checksum, total_balance)
                    .await
                {
                    warn!("[{}] Balance alert checking failed: {}", wallet_checksum, e);
                }
            }

            if has_changes {
                any_changes = true;
            }
        }

        let sync_duration = sync_start.elapsed();
        if any_changes {
            info!(
                "Grouped address-based sync complete in {:.2}s for {} watchers; changes=true",
                sync_duration.as_secs_f64(),
                wallet_checksums.len(),
            );
        } else {
            debug!(
                "Grouped address-based sync complete in {:.2}s for {} watchers; changes=false",
                sync_duration.as_secs_f64(),
                wallet_checksums.len(),
            );
        }

        Ok(any_changes)
    }

    /// Send balance alert notification using existing notification system
    async fn send_balance_alert_notification(
        &self,
        alert: &crate::metadata::BalanceAlert,
        wallet_checksum: &str,
        current_balance_sats: i64,
        exchange_rate_snapshot: Option<f64>,
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
            threshold_currency: alert.threshold_currency.clone(),
            threshold_fiat_amount: alert.threshold_fiat_amount,
            exchange_rate_snapshot,
        };

        // Send notification via broadcast channel (same as transaction notifications)
        if let Err(e) =
            self.notification_sender
                .send(crate::metadata::TransactionNotification::BalanceAlert(
                    balance_alert_notification,
                ))
        {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_tx_confirmed_positive_height() {
        assert!(is_tx_confirmed(1, "some_txid"));
        assert!(is_tx_confirmed(100, "some_txid"));
        assert!(is_tx_confirmed(800000, "some_txid"));
    }

    #[test]
    fn test_is_tx_confirmed_mempool() {
        assert!(!is_tx_confirmed(0, "some_mempool_txid"));
    }

    #[test]
    fn test_is_tx_confirmed_unconfirmed_parents() {
        assert!(!is_tx_confirmed(-1, "some_txid"));
    }

    #[test]
    fn test_is_tx_confirmed_genesis_coinbase() {
        assert!(is_tx_confirmed(0, GENESIS_COINBASE_TXID));
    }
}
