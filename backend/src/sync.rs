use crate::admin_notifications::AdminNotifications;
use crate::config::AppConfig;
use crate::electrum::{ElectrumClient, ElectrumClientManager};
use crate::metadata::{
    BalanceAlertTriggerParams, EventType, MetadataDb, Transaction, TransactionInsert,
    TransactionNotification,
};
use crate::utils::{
    current_unix_timestamp, extract_address_from_descriptor, extract_pubkey_from_descriptor,
};
use anyhow::{anyhow, Result};
use bdk_wallet::bitcoin::{
    Address, Network, OutPoint, PublicKey, ScriptBuf, Transaction as BitcoinTransaction, Txid,
};
use bdk_wallet::{rusqlite::Connection, PersistedWallet};
use std::str::FromStr;
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Number of consecutive reconnection failures before sending an alert
const ALERT_FAILURE_THRESHOLD: u32 = 3;

/// The Bitcoin mainnet genesis coinbase txid — the only confirmed transaction at block height 0.
/// Electrum returns height=0 for both mempool transactions and genesis block transactions,
/// so mainnet needs a typed txid special-case while non-mainnet networks treat height=0 as
/// unconfirmed.
const MAINNET_GENESIS_COINBASE_TXID_HEX: &str =
    "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b";
static MAINNET_GENESIS_COINBASE_TXID: LazyLock<Txid> = LazyLock::new(|| {
    Txid::from_str(MAINNET_GENESIS_COINBASE_TXID_HEX)
        .expect("mainnet genesis coinbase txid must be valid")
});

/// Bitcoin mainnet genesis block timestamp (2009-01-03T18:15:05Z) as a fallback when the
/// Electrum server cannot serve the block 0 header.
const MAINNET_GENESIS_BLOCK_TIMESTAMP: u64 = 1231006505;

fn current_unix_timestamp_or(default: u64, context: &str) -> u64 {
    current_unix_timestamp().unwrap_or_else(|error| {
        warn!(
            "Failed to read current UNIX timestamp for {}: {}. Falling back to {}.",
            context, error, default
        );
        default
    })
}

/// Check if a transaction is confirmed based on Electrum's height convention.
/// height > 0: confirmed at that block height
/// height == 0: unconfirmed (mempool), EXCEPT for Bitcoin mainnet genesis coinbase
/// height < 0: unconfirmed with unconfirmed parents
fn is_tx_confirmed(network: Network, height: i32, txid: &Txid) -> bool {
    height > 0
        || (height == 0
            && matches!(network, Network::Bitcoin)
            && txid == &*MAINNET_GENESIS_COINBASE_TXID)
}

/// Convert a confirmed Electrum height into the persisted unsigned block height.
/// Callers must only use this after `is_tx_confirmed(...)` returns true.
fn confirmed_block_height(height: i32) -> u32 {
    debug_assert!(
        height >= 0,
        "confirmed transactions must have a non-negative block height"
    );
    u32::try_from(height).expect("confirmed transactions must have a non-negative block height")
}

fn genesis_block_timestamp(network: Network, height: u32) -> Option<u64> {
    if matches!(network, Network::Bitcoin) && height == 0 {
        Some(MAINNET_GENESIS_BLOCK_TIMESTAMP)
    } else {
        None
    }
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

#[derive(Debug, Clone)]
struct AddressWatchTxState {
    tx: BitcoinTransaction,
    transaction_type: EventType,
    amount_sats: i64,
    is_confirmed: bool,
    block_height: Option<u32>,
    first_seen_at: u64,
    confirmed_at: Option<u64>,
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
            .get_transactions_by_wallet_checksum(wallet_checksum, None, false)
            .await?;
        debug!(
            "[{}] Loaded {} existing transactions from metadata in {:.2?}",
            wallet_checksum,
            existing_transactions.len(),
            fetch_existing_start.elapsed()
        );

        // Create HashMap for O(1) transaction lookups to avoid individual database queries
        let existing_tx_map: std::collections::HashMap<
            &str,
            &crate::metadata::TransactionWithWallet,
        > = existing_transactions
            .iter()
            .map(|tx| (tx.txid.as_str(), tx))
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
                let first_seen_at = existing_tx_map
                    .get(txid.as_str())
                    .map(|tx| tx.first_seen_at)
                    .unwrap_or_else(|| {
                        current_unix_timestamp_or(
                            MAINNET_GENESIS_BLOCK_TIMESTAMP,
                            "canonical transaction timestamp",
                        )
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
            let existing_tx = existing_tx_map.get(txid.as_str()).copied();

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
        let canonical_txids: std::collections::HashSet<Txid> = canonical_transactions_data
            .iter()
            .filter_map(|(txid, _, _, _, _, _)| {
                Txid::from_str(txid)
                    .inspect_err(|e| {
                        warn!(
                            "[{}] Failed to parse canonical txid {}: {}",
                            wallet_checksum, txid, e
                        )
                    })
                    .ok()
            })
            .collect();

        // Find transactions that exist in full graph but NOT in canonical set (these are conflicted/replaced)
        let conflicted_txids: Vec<Txid> = wallet
            .tx_graph()
            .full_txs()
            .map(|tx| tx.txid)
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
            let existing_tx = existing_tx_map.get(txid.as_str());

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
            let bdk_tx_map: std::collections::HashMap<Txid, _> = wallet
                .tx_graph()
                .full_txs()
                .map(|tx| (tx.txid, tx))
                .collect();

            for conflicted_txid in &conflicted_txids {
                let conflicted_txid_str = conflicted_txid.to_string();
                // Check if this conflicted transaction is in our pending transactions using in-memory lookup
                if let Some(pending_tx) = existing_tx_map.get(conflicted_txid_str.as_str()) {
                    if pending_tx.transaction_status == "pending" {
                        // Find the conflicted transaction's inputs
                        let conflicted_tx_inputs: Option<Vec<_>> =
                            bdk_tx_map.get(conflicted_txid).map(|tx| {
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
                                if let Some(canonical_tx) = Txid::from_str(canonical_txid)
                                    .inspect_err(|e| {
                                        warn!(
                                            "[{}] Failed to parse canonical txid {}: {}",
                                            wallet_checksum, canonical_txid, e
                                        )
                                    })
                                    .ok()
                                    .and_then(|txid| bdk_tx_map.get(&txid))
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
                                                &conflicted_txid_str,
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
                                Some(current_unix_timestamp_or(
                                    0,
                                    "confirmed transaction timestamp",
                                ))
                            }
                        }
                    } else {
                        // No electrum client available, use current time as fallback
                        Some(current_unix_timestamp_or(
                            0,
                            "confirmed transaction timestamp",
                        ))
                    };
                    (block_height, confirmed_at)
                }
                bdk_wallet::chain::ChainPosition::Unconfirmed { .. } => (None, None),
            };

            // Get first_seen timestamp
            let first_seen_at = match &tx.chain_position {
                bdk_wallet::chain::ChainPosition::Confirmed { .. } => {
                    confirmed_at.unwrap_or_else(|| {
                        current_unix_timestamp_or(
                            MAINNET_GENESIS_BLOCK_TIMESTAMP,
                            "confirmed first-seen timestamp",
                        )
                    })
                }
                bdk_wallet::chain::ChainPosition::Unconfirmed { first_seen, .. } => first_seen
                    .unwrap_or_else(|| {
                        current_unix_timestamp_or(
                            MAINNET_GENESIS_BLOCK_TIMESTAMP,
                            "unconfirmed first-seen timestamp",
                        )
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
        let bdk_tx_map: std::collections::HashMap<Txid, _> = wallet
            .tx_graph()
            .full_txs()
            .map(|tx| (tx.txid, tx))
            .collect();

        // Build a map of unconfirmed transaction outputs: (txid, vout) -> txid
        let mut unconfirmed_outputs: HashMap<(String, u32), String> = HashMap::new();
        for (txid, _, _, _, _, _) in &unconfirmed_txs {
            if let Some(bdk_tx) = Txid::from_str(txid)
                .inspect_err(|e| {
                    warn!("[{}] Failed to parse txid {}: {}", wallet_checksum, txid, e)
                })
                .ok()
                .and_then(|t| bdk_tx_map.get(&t))
            {
                for (vout, _) in bdk_tx.tx.output.iter().enumerate() {
                    unconfirmed_outputs.insert((txid.clone(), vout as u32), txid.clone());
                }
            }
        }

        // Check each unconfirmed transaction to see if it spends from another unconfirmed transaction
        for (child_txid, _, _, _, _, _) in &unconfirmed_txs {
            if let Some(bdk_tx) = Txid::from_str(child_txid)
                .inspect_err(|e| {
                    warn!(
                        "[{}] Failed to parse child txid {}: {}",
                        wallet_checksum, child_txid, e
                    )
                })
                .ok()
                .and_then(|t| bdk_tx_map.get(&t))
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

    async fn fetch_address_watch_tx_state(
        &self,
        wallet_checksum: &str,
        client: &ElectrumClient,
        script: &ScriptBuf,
        network: Network,
        txid: &Txid,
        height: i32,
    ) -> Result<Option<AddressWatchTxState>> {
        let tx = match client.transaction_get(txid).await {
            Ok(tx) => tx,
            Err(e) => {
                warn!("[{}] Failed to fetch tx {}: {}", wallet_checksum, txid, e);
                return Ok(None);
            }
        };

        let received: i64 = tx
            .output
            .iter()
            .filter(|output| output.script_pubkey == *script)
            .map(|output| output.value.to_sat() as i64)
            .sum();

        let mut sent: i64 = 0;
        for input in &tx.input {
            if input.previous_output.is_null() {
                continue;
            }

            let prev_txid = input.previous_output.txid;
            let prev_vout = input.previous_output.vout;
            match client.transaction_get(&prev_txid).await {
                Ok(prev_tx) => {
                    if let Some(prev_output) = prev_tx.output.get(prev_vout as usize) {
                        if prev_output.script_pubkey == *script {
                            sent += prev_output.value.to_sat() as i64;
                        }
                    }
                }
                Err(e) => {
                    debug!(
                        "Could not fetch parent tx {} while classifying {}: {}",
                        prev_txid, txid, e
                    );
                }
            }
        }

        if received == 0 && sent == 0 {
            return Ok(None);
        }

        let (transaction_type, amount_sats) = if sent > 0 && received == 0 {
            (EventType::Send, sent)
        } else if sent > 0 && received > 0 {
            (EventType::Send, (sent - received).max(0))
        } else {
            (EventType::Receive, received)
        };

        let is_confirmed = is_tx_confirmed(network, height, txid);
        let now = current_unix_timestamp_or(
            MAINNET_GENESIS_BLOCK_TIMESTAMP,
            "address-watch transaction timestamp",
        );

        let (confirmed_at, block_height) = if is_confirmed {
            let confirmed_height = confirmed_block_height(height);
            let timestamp = match client.get_block_header(confirmed_height).await {
                Ok(header) => header.timestamp,
                Err(_) => genesis_block_timestamp(network, confirmed_height).unwrap_or(now),
            };
            (Some(timestamp), Some(confirmed_height))
        } else {
            (None, None)
        };

        Ok(Some(AddressWatchTxState {
            tx,
            transaction_type,
            amount_sats,
            is_confirmed,
            block_height,
            first_seen_at: confirmed_at.unwrap_or(now),
            confirmed_at,
        }))
    }

    fn detect_address_watch_cpfp_relationships(
        tx_states: &std::collections::HashMap<String, AddressWatchTxState>,
    ) -> std::collections::HashMap<String, String> {
        let mut relationships = std::collections::HashMap::new();
        let mut unconfirmed_outputs = std::collections::HashMap::new();

        for (txid, state) in tx_states.iter().filter(|(_, state)| !state.is_confirmed) {
            let computed_txid = state.tx.compute_txid();
            for (vout, _) in state.tx.output.iter().enumerate() {
                unconfirmed_outputs.insert(
                    OutPoint {
                        txid: computed_txid,
                        vout: vout as u32,
                    },
                    txid.clone(),
                );
            }
        }

        for (child_txid, state) in tx_states.iter().filter(|(_, state)| !state.is_confirmed) {
            for input in &state.tx.input {
                if let Some(parent_txid) = unconfirmed_outputs.get(&input.previous_output) {
                    if parent_txid != child_txid {
                        relationships.insert(child_txid.clone(), parent_txid.clone());
                        break;
                    }
                }
            }
        }

        relationships
    }

    fn detect_address_watch_rbf_replacements(
        current_tx_states: &std::collections::HashMap<String, AddressWatchTxState>,
        existing_transactions: &[crate::metadata::TransactionWithWallet],
        disappeared_pending_txs: &std::collections::HashMap<String, BitcoinTransaction>,
    ) -> std::collections::HashMap<String, String> {
        let mut replacements = std::collections::HashMap::new();

        for pending_tx in existing_transactions
            .iter()
            .filter(|tx| tx.transaction_status == "pending")
        {
            let Some(disappeared_tx) = disappeared_pending_txs.get(&pending_tx.txid) else {
                continue;
            };

            let disappeared_inputs: std::collections::HashSet<_> = disappeared_tx
                .input
                .iter()
                .filter(|input| !input.previous_output.is_null())
                .map(|input| input.previous_output)
                .collect();

            if disappeared_inputs.is_empty() {
                continue;
            }

            for (txid, state) in current_tx_states
                .iter()
                .filter(|(_, state)| !state.is_confirmed)
            {
                if state.first_seen_at <= pending_tx.first_seen_at {
                    continue;
                }

                if state.transaction_type != pending_tx.transaction_type {
                    continue;
                }

                let has_shared_input = state
                    .tx
                    .input
                    .iter()
                    .filter(|input| !input.previous_output.is_null())
                    .any(|input| disappeared_inputs.contains(&input.previous_output));

                if has_shared_input {
                    replacements.insert(pending_tx.txid.clone(), txid.clone());
                    break;
                }
            }
        }

        replacements
    }

    async fn apply_address_watch_relationships(
        &self,
        wallet_checksum: &str,
        existing_transactions: &[crate::metadata::TransactionWithWallet],
        cpfp_relationships: &std::collections::HashMap<String, String>,
        rbf_replacements: &std::collections::HashMap<String, String>,
    ) -> Result<bool> {
        let mut has_changes = false;

        for tx in existing_transactions {
            if let Some(parent_txid) = cpfp_relationships.get(&tx.txid) {
                if tx.parent_txid.as_deref() != Some(parent_txid.as_str())
                    && self
                        .metadata_db
                        .update_transaction_parent(wallet_checksum, &tx.txid, parent_txid)
                        .await?
                {
                    has_changes = true;
                }
            }

            if tx.transaction_status == "pending" {
                if let Some(replacement_txid) = rbf_replacements.get(&tx.txid) {
                    if self
                        .metadata_db
                        .mark_transaction_replaced(wallet_checksum, &tx.txid, replacement_txid)
                        .await?
                    {
                        has_changes = true;
                    }
                }
            }
        }

        Ok(has_changes)
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
        let network = self.config.network.to_bdk_network();

        let script = Self::script_from_watch_descriptor(descriptor, network)?;

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
            .get_transactions_by_wallet_checksum(wallet_checksum, None, false)
            .await?;
        let existing_tx_map: std::collections::HashMap<&str, &_> = existing_transactions
            .iter()
            .map(|tx| (tx.txid.as_str(), tx))
            .collect();

        let mut has_changes = false;
        let history_txids: std::collections::HashSet<String> = history
            .iter()
            .map(|entry| entry.tx_hash.to_string())
            .collect();
        let mut current_tx_states = std::collections::HashMap::new();

        for hist_entry in &history {
            let txid_str = hist_entry.tx_hash.to_string();
            let should_fetch_state =
                !is_tx_confirmed(network, hist_entry.height, &hist_entry.tx_hash)
                    || !existing_tx_map.contains_key(txid_str.as_str());

            if !should_fetch_state {
                continue;
            }

            if let Some(state) = self
                .fetch_address_watch_tx_state(
                    "grouped-address-watch",
                    &client,
                    &script,
                    network,
                    &hist_entry.tx_hash,
                    hist_entry.height,
                )
                .await?
            {
                current_tx_states.insert(txid_str, state);
            }
        }

        let mut disappeared_pending_txs = std::collections::HashMap::new();
        for existing_tx in existing_transactions
            .iter()
            .filter(|tx| tx.transaction_status == "pending" && !history_txids.contains(&tx.txid))
        {
            let txid = Txid::from_str(&existing_tx.txid)
                .map_err(|e| anyhow!("Failed to parse txid {}: {}", existing_tx.txid, e))?;
            match client.transaction_get(&txid).await {
                Ok(tx) => {
                    disappeared_pending_txs.insert(existing_tx.txid.clone(), tx);
                }
                Err(e) => {
                    debug!(
                        "[{}] Could not fetch disappeared pending tx {} for conflict detection: {}",
                        wallet_checksum, existing_tx.txid, e
                    );
                }
            }
        }

        let cpfp_relationships = Self::detect_address_watch_cpfp_relationships(&current_tx_states);
        let rbf_replacements = Self::detect_address_watch_rbf_replacements(
            &current_tx_states,
            &existing_transactions,
            &disappeared_pending_txs,
        );

        // Process each transaction in history
        for hist_entry in &history {
            let txid = hist_entry.tx_hash;
            let txid_str = hist_entry.tx_hash.to_string();

            if let Some(existing) = existing_tx_map.get(txid_str.as_str()) {
                // Check if an existing pending transaction got confirmed
                if is_tx_confirmed(network, hist_entry.height, &txid)
                    && existing.block_height.is_none()
                {
                    let confirmed_height = confirmed_block_height(hist_entry.height);
                    // Transaction just confirmed
                    let confirmed_at = match client.get_block_header(confirmed_height).await {
                        Ok(header) => header.timestamp,
                        Err(_) => genesis_block_timestamp(network, confirmed_height)
                            .unwrap_or_else(|| {
                                current_unix_timestamp_or(
                                    MAINNET_GENESIS_BLOCK_TIMESTAMP,
                                    "address-watch confirmation timestamp",
                                )
                            }),
                    };

                    self.metadata_db
                        .update_transaction_confirmation(
                            wallet_checksum,
                            &txid_str,
                            confirmed_height,
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
                continue;
            }

            let Some(state) = current_tx_states.get(&txid_str) else {
                debug!(
                    "[{}] Transaction {} has no inputs/outputs for our address, skipping",
                    wallet_checksum, txid_str
                );
                continue;
            };
            let transaction = TransactionInsert {
                txid: txid_str.clone(),
                wallet_checksum: wallet_checksum.to_string(),
                transaction_type: state.transaction_type,
                amount_sats: state.amount_sats,
                fee_sats: None,
                block_height: state.block_height,
                first_seen_at: state.first_seen_at,
                confirmed_at: state.confirmed_at,
                parent_txid: cpfp_relationships.get(&txid_str).cloned(),
                transaction_status: if state.is_confirmed {
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
                state.transaction_type,
                state.amount_sats,
                if state.is_confirmed {
                    "confirmed"
                } else {
                    "pending"
                },
                if suppress_notifications {
                    ", notifications suppressed"
                } else {
                    ""
                }
            );
        }

        if self
            .apply_address_watch_relationships(
                wallet_checksum,
                &existing_transactions,
                &cpfp_relationships,
                &rbf_replacements,
            )
            .await?
        {
            has_changes = true;
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
        let network = self.config.network.to_bdk_network();
        info!(
            "Starting grouped address watch sync for {} watchers (descriptor: {})",
            wallet_checksums.len(),
            descriptor
        );

        let script = Self::script_from_watch_descriptor(descriptor, network)?;

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
        let history_txids: std::collections::HashSet<String> = history
            .iter()
            .map(|entry| entry.tx_hash.to_string())
            .collect();
        let mut current_tx_states = std::collections::HashMap::new();

        for hist_entry in &history {
            let txid_str = hist_entry.tx_hash.to_string();
            if is_tx_confirmed(network, hist_entry.height, &hist_entry.tx_hash) {
                continue;
            }

            if let Some(state) = self
                .fetch_address_watch_tx_state(
                    "grouped-address-watch",
                    &client,
                    &script,
                    network,
                    &hist_entry.tx_hash,
                    hist_entry.height,
                )
                .await?
            {
                current_tx_states.insert(txid_str, state);
            }
        }

        let cpfp_relationships = Self::detect_address_watch_cpfp_relationships(&current_tx_states);

        // Cache block headers by height to avoid repeated Electrum calls
        let mut block_header_cache: std::collections::HashMap<u32, u64> =
            std::collections::HashMap::new();

        let mut any_changes = false;

        // === Fan out to each watcher ===
        for wallet_checksum in wallet_checksums {
            // Get existing transactions for THIS watcher
            let existing_transactions = self
                .metadata_db
                .get_transactions_by_wallet_checksum(wallet_checksum, None, false)
                .await?;
            let existing_tx_map: std::collections::HashMap<&str, &_> = existing_transactions
                .iter()
                .map(|tx| (tx.txid.as_str(), tx))
                .collect();

            let mut has_changes = false;
            let mut disappeared_pending_txs = std::collections::HashMap::new();

            for existing_tx in existing_transactions.iter().filter(|tx| {
                tx.transaction_status == "pending" && !history_txids.contains(&tx.txid)
            }) {
                let txid = Txid::from_str(&existing_tx.txid)
                    .map_err(|e| anyhow!("Failed to parse txid {}: {}", existing_tx.txid, e))?;
                match client.transaction_get(&txid).await {
                    Ok(tx) => {
                        disappeared_pending_txs.insert(existing_tx.txid.clone(), tx);
                    }
                    Err(e) => {
                        debug!(
                            "[{}] Could not fetch disappeared pending tx {} for conflict detection: {}",
                            wallet_checksum, existing_tx.txid, e
                        );
                    }
                }
            }

            let rbf_replacements = Self::detect_address_watch_rbf_replacements(
                &current_tx_states,
                &existing_transactions,
                &disappeared_pending_txs,
            );

            for hist_entry in &history {
                let txid = hist_entry.tx_hash;
                let txid_str = hist_entry.tx_hash.to_string();

                if let Some(existing) = existing_tx_map.get(txid_str.as_str()) {
                    // Check if an existing pending transaction got confirmed
                    if is_tx_confirmed(network, hist_entry.height, &txid)
                        && existing.block_height.is_none()
                    {
                        let confirmed_height = confirmed_block_height(hist_entry.height);
                        let confirmed_at = match block_header_cache.get(&confirmed_height) {
                            Some(&ts) => ts,
                            None => {
                                let ts = match client.get_block_header(confirmed_height).await {
                                    Ok(header) => header.timestamp,
                                    // Grouped watchers can run on any configured network, so fall
                                    // back to a generic timestamp when there is no mainnet genesis
                                    // special-case to apply.
                                    Err(_) => genesis_block_timestamp(network, confirmed_height)
                                        .unwrap_or_else(|| {
                                            current_unix_timestamp_or(
                                                0,
                                                "watcher confirmation timestamp",
                                            )
                                        }),
                                };
                                block_header_cache.insert(confirmed_height, ts);
                                ts
                            }
                        };

                        self.metadata_db
                            .update_transaction_confirmation(
                                wallet_checksum,
                                &txid_str,
                                confirmed_height,
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
                    continue;
                }
                let Some(state) = current_tx_states.get(&txid_str) else {
                    let Some(state) = self
                        .fetch_address_watch_tx_state(
                            wallet_checksum,
                            &client,
                            &script,
                            network,
                            &hist_entry.tx_hash,
                            hist_entry.height,
                        )
                        .await?
                    else {
                        continue;
                    };

                    let transaction = TransactionInsert {
                        txid: txid_str.clone(),
                        wallet_checksum: wallet_checksum.to_string(),
                        transaction_type: state.transaction_type,
                        amount_sats: state.amount_sats,
                        fee_sats: None,
                        block_height: state.block_height,
                        first_seen_at: state.first_seen_at,
                        confirmed_at: state.confirmed_at,
                        parent_txid: cpfp_relationships.get(&txid_str).cloned(),
                        transaction_status: if state.is_confirmed {
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
                        state.transaction_type,
                        state.amount_sats,
                        if state.is_confirmed {
                            "confirmed"
                        } else {
                            "pending"
                        },
                        if suppress_notifications {
                            ", notifications suppressed"
                        } else {
                            ""
                        }
                    );
                    continue;
                };
                let transaction = TransactionInsert {
                    txid: txid_str.clone(),
                    wallet_checksum: wallet_checksum.to_string(),
                    transaction_type: state.transaction_type,
                    amount_sats: state.amount_sats,
                    fee_sats: None,
                    block_height: state.block_height,
                    first_seen_at: state.first_seen_at,
                    confirmed_at: state.confirmed_at,
                    parent_txid: cpfp_relationships.get(&txid_str).cloned(),
                    transaction_status: if state.is_confirmed {
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
                    state.transaction_type,
                    state.amount_sats,
                    if state.is_confirmed {
                        "confirmed"
                    } else {
                        "pending"
                    },
                    if suppress_notifications {
                        ", notifications suppressed"
                    } else {
                        ""
                    }
                );
            }

            if self
                .apply_address_watch_relationships(
                    wallet_checksum,
                    &existing_transactions,
                    &cpfp_relationships,
                    &rbf_replacements,
                )
                .await?
            {
                has_changes = true;
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
    use crate::metadata::TransactionWithWallet;
    use bdk_wallet::bitcoin::{absolute, Amount, Sequence, TxIn, TxOut, Witness};

    fn test_tx(txid_seed: u8, inputs: Vec<OutPoint>, output_count: usize) -> BitcoinTransaction {
        let outputs = (0..output_count)
            .map(|i| TxOut {
                value: Amount::from_sat(1_000 + i as u64),
                script_pubkey: ScriptBuf::from_bytes(vec![txid_seed, i as u8]),
            })
            .collect();

        BitcoinTransaction {
            version: bdk_wallet::bitcoin::transaction::Version(2),
            lock_time: absolute::LockTime::ZERO,
            input: inputs
                .into_iter()
                .map(|previous_output| TxIn {
                    previous_output,
                    script_sig: ScriptBuf::new(),
                    sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                    witness: Witness::default(),
                })
                .collect(),
            output: outputs,
        }
    }

    fn pending_state(
        tx: BitcoinTransaction,
        transaction_type: EventType,
        first_seen_at: u64,
    ) -> AddressWatchTxState {
        AddressWatchTxState {
            tx,
            transaction_type,
            amount_sats: 1_000,
            is_confirmed: false,
            block_height: None,
            first_seen_at,
            confirmed_at: None,
        }
    }

    #[test]
    fn test_is_tx_confirmed_positive_height() {
        let txid =
            Txid::from_str("1111111111111111111111111111111111111111111111111111111111111111")
                .unwrap();

        assert!(is_tx_confirmed(Network::Bitcoin, 1, &txid));
        assert!(is_tx_confirmed(Network::Testnet, 100, &txid));
        assert!(is_tx_confirmed(Network::Regtest, 800000, &txid));
    }

    #[test]
    fn test_is_tx_confirmed_mempool() {
        let non_genesis_txid =
            Txid::from_str("1111111111111111111111111111111111111111111111111111111111111111")
                .unwrap();
        let genesis_txid = Txid::from_str(MAINNET_GENESIS_COINBASE_TXID_HEX).unwrap();

        assert!(!is_tx_confirmed(Network::Bitcoin, 0, &non_genesis_txid));
        assert!(!is_tx_confirmed(Network::Testnet, 0, &genesis_txid));
        assert!(!is_tx_confirmed(Network::Regtest, 0, &genesis_txid));
    }

    #[test]
    fn test_is_tx_confirmed_unconfirmed_parents() {
        let txid =
            Txid::from_str("1111111111111111111111111111111111111111111111111111111111111111")
                .unwrap();

        assert!(!is_tx_confirmed(Network::Bitcoin, -1, &txid));
    }

    #[test]
    fn test_is_tx_confirmed_genesis_coinbase() {
        assert!(is_tx_confirmed(
            Network::Bitcoin,
            0,
            &MAINNET_GENESIS_COINBASE_TXID
        ));
    }

    #[test]
    fn test_is_tx_confirmed_genesis_coinbase_requires_mainnet() {
        assert!(!is_tx_confirmed(
            Network::Testnet,
            0,
            &MAINNET_GENESIS_COINBASE_TXID
        ));
        assert!(!is_tx_confirmed(
            Network::Regtest,
            0,
            &MAINNET_GENESIS_COINBASE_TXID
        ));
    }

    #[test]
    fn test_confirmed_block_height_accepts_genesis_height() {
        assert_eq!(confirmed_block_height(0), 0);
    }

    #[test]
    fn test_genesis_block_timestamp_is_mainnet_only() {
        assert_eq!(
            genesis_block_timestamp(Network::Bitcoin, 0),
            Some(MAINNET_GENESIS_BLOCK_TIMESTAMP)
        );
        assert_eq!(genesis_block_timestamp(Network::Testnet, 0), None);
        assert_eq!(genesis_block_timestamp(Network::Regtest, 0), None);
        assert_eq!(genesis_block_timestamp(Network::Bitcoin, 1), None);
    }

    #[test]
    fn test_detect_address_watch_cpfp_relationships() {
        let parent = test_tx(1, vec![], 1);
        let parent_txid = parent.compute_txid().to_string();
        let child = test_tx(
            2,
            vec![OutPoint {
                txid: parent.compute_txid(),
                vout: 0,
            }],
            1,
        );
        let child_txid = child.compute_txid().to_string();

        let tx_states = std::collections::HashMap::from([
            (
                parent_txid.clone(),
                pending_state(parent, EventType::Receive, 10),
            ),
            (
                child_txid.clone(),
                pending_state(child, EventType::Send, 20),
            ),
        ]);

        let relationships = WalletSyncService::detect_address_watch_cpfp_relationships(&tx_states);

        assert_eq!(relationships.get(&child_txid), Some(&parent_txid));
    }

    #[test]
    fn test_detect_address_watch_rbf_replacements() {
        let shared_input = OutPoint {
            txid: test_tx(9, vec![], 1).compute_txid(),
            vout: 0,
        };
        let original = test_tx(3, vec![shared_input], 1);
        let replacement = test_tx(4, vec![shared_input], 1);
        let replacement_txid = replacement.compute_txid().to_string();

        let current_tx_states = std::collections::HashMap::from([(
            replacement_txid.clone(),
            pending_state(replacement, EventType::Send, 200),
        )]);
        let existing_transactions = vec![TransactionWithWallet {
            txid: original.compute_txid().to_string(),
            wallet_checksum: "wallet".to_string(),
            wallet_name: "wallet".to_string(),
            transaction_type: EventType::Send,
            amount_sats: 1_000,
            fee_sats: None,
            block_height: None,
            first_seen_at: 100,
            confirmed_at: None,
            parent_txid: None,
            transaction_status: "pending".to_string(),
            replaced_by_txid: None,
            replaced_at: None,
            notification_status: vec![],
        }];
        let disappeared_pending_txs =
            std::collections::HashMap::from([(existing_transactions[0].txid.clone(), original)]);

        let replacements = WalletSyncService::detect_address_watch_rbf_replacements(
            &current_tx_states,
            &existing_transactions,
            &disappeared_pending_txs,
        );

        assert_eq!(
            replacements.get(&existing_transactions[0].txid),
            Some(&replacement_txid)
        );
    }
}
