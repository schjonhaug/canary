//! Descriptor-wallet transaction reconciliation: records new transactions,
//! confirmations and BDK-detected replacements in the metadata database.

use super::genesis::MAINNET_GENESIS_BLOCK_TIMESTAMP;
use super::{current_unix_timestamp_or, WalletSyncService};
use crate::electrum::ElectrumClient;
use crate::metadata::{EventType, TransactionInsert, TransactionStatus};
use crate::utils::amount::sats_to_i64;
use anyhow::Result;
use bdk_wallet::bitcoin::Txid;
use bdk_wallet::{rusqlite::Connection, PersistedWallet};
use std::str::FromStr;
use std::time::{Duration, Instant};
use tracing::{debug, warn};

/// Transaction summary: (txid, `amount_sats`, `block_height`, `is_confirmed`, `first_seen_at`, `confirmed_at`)
pub(super) type TransactionSummary = (String, i64, Option<u32>, bool, u64, Option<u64>);

#[derive(Debug, Default)]
pub(super) struct TransactionProcessSummary {
    pub(super) new_transactions: usize,
    pub(super) confirmation_updates: usize,
    pub(super) conflicts_marked: usize,
}

impl TransactionProcessSummary {
    pub(super) fn has_changes(&self) -> bool {
        self.new_transactions > 0 || self.confirmation_updates > 0 || self.conflicts_marked > 0
    }
}

impl WalletSyncService {
    /// Process all transactions in the wallet and sync with database
    pub(super) async fn process_wallet_transactions(
        &self,
        wallet: &PersistedWallet<Connection>,
        wallet_checksum: &str,
        electrum_client: Option<&ElectrumClient>,
    ) -> Result<TransactionProcessSummary> {
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
                let net_amount = sats_to_i64(received.to_sat()) - sats_to_i64(sent.to_sat());
                let block_height = tx_item.chain_position.confirmation_height_upper_bound();
                let is_confirmed = tx_item.chain_position.is_confirmed();

                // Preserve existing timestamp if transaction already exists, otherwise use current time
                let first_seen_at = existing_tx_map.get(txid.as_str()).map_or_else(
                    || {
                        current_unix_timestamp_or(
                            MAINNET_GENESIS_BLOCK_TIMESTAMP,
                            "canonical transaction timestamp",
                        )
                    },
                    |tx| tx.first_seen_at,
                );

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
                        );
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
            Self::detect_cpfp_relationships(wallet, wallet_checksum, &all_transactions);
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
                        transaction_status: TransactionStatus::from_confirmed(*is_confirmed),
                        replaced_by_txid: None,
                        replaced_at: None,
                    };

                    let _transaction_id = self.metadata_db.insert_transaction(&transaction).await?;

                    // Send notifications for new transaction
                    self.send_new_transaction_notification(&transaction);

                    new_tx_count += 1;
                    debug!(
                        "[{}] New transaction recorded: status={}, type={}, amount_sats={}",
                        wallet_checksum,
                        TransactionStatus::from_confirmed(transaction.block_height.is_some()),
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
                            self.send_confirmed_transaction_notification(&updated_tx);
                        }

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
                    if pending_tx.transaction_status == TransactionStatus::Pending {
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
                                        );
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
                                            if let Some(updated_tx) = self
                                                .metadata_db
                                                .get_transaction_by_txid(
                                                    wallet_checksum,
                                                    &conflicted_txid_str,
                                                )
                                                .await?
                                            {
                                                // Descriptor wallets use BDK's conflicted tx graph;
                                                // mark_transaction_replaced returns false if another
                                                // sync path already logged this replacement.
                                                self.send_replaced_transaction_notification(
                                                    &updated_tx,
                                                );
                                            }
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
            new_transactions: new_tx_count,
            confirmation_updates,
            conflicts_marked,
        })
    }
}
