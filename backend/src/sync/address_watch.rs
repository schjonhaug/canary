//! Address-watch sync: single-address `addr()` and `pk()` watches queried
//! directly over Electrum, one watch at a time or grouped by shared descriptor.

use super::genesis::{
    balance_with_mainnet_genesis_coinbase, confirmed_block_height, genesis_block_timestamp,
    is_tx_confirmed, mainnet_genesis_coinbase_tx_state, MAINNET_GENESIS_BLOCK_TIMESTAMP,
};
use super::{current_unix_timestamp_or, AddressWatchSyncResult, WalletSyncService};
use crate::electrum::{ElectrumClient, ElectrumClientManager};
use crate::metadata::{EventType, TransactionInsert, TransactionStatus};
use crate::utils::amount::sats_to_i64;
use crate::utils::{extract_address_from_descriptor, extract_pubkey_from_descriptor};
use anyhow::{anyhow, Result};
use bdk_wallet::bitcoin::{
    Address, Network, PublicKey, ScriptBuf, Transaction as BitcoinTransaction, Txid,
};
use std::str::FromStr;
use std::time::Instant;
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
pub(super) struct AddressWatchTxState {
    pub(super) tx: BitcoinTransaction,
    pub(super) transaction_type: EventType,
    pub(super) amount_sats: i64,
    pub(super) is_confirmed: bool,
    pub(super) block_height: Option<u32>,
    pub(super) first_seen_at: u64,
    pub(super) confirmed_at: Option<u64>,
}

impl WalletSyncService {
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
    /// Resolve an `addr()` or `pk()` descriptor to the corresponding `ScriptBuf`.
    pub(crate) fn script_from_watch_descriptor(
        descriptor: &str,
        network: Network,
    ) -> Result<ScriptBuf> {
        if let Some(address_str) = extract_address_from_descriptor(descriptor) {
            let address = Address::from_str(&address_str)
                .map_err(|e| anyhow!("Failed to parse address {address_str}: {e}"))?;
            Ok(address
                .require_network(network)
                .map_err(|e| anyhow!("Address network mismatch for {address_str}: {e}"))?
                .script_pubkey())
        } else if let Some(pubkey_str) = extract_pubkey_from_descriptor(descriptor) {
            let pubkey = PublicKey::from_str(&pubkey_str)
                .map_err(|e| anyhow!("Failed to parse public key {pubkey_str}: {e}"))?;
            Ok(ScriptBuf::new_p2pk(&pubkey))
        } else {
            Err(anyhow!(
                "Invalid watch descriptor format (expected addr() or pk()): {descriptor}"
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
        if let Some(state) = mainnet_genesis_coinbase_tx_state(network, height, txid, script) {
            return Ok(Some(state));
        }

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
            .map(|output| sats_to_i64(output.value.to_sat()))
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
                            sent += sats_to_i64(prev_output.value.to_sat());
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

    /// See: <https://github.com/rust-bitcoin/rust-miniscript/issues/294>
    /// See: <https://github.com/bitcoindevkit/bdk_wallet/issues/174>
    pub(crate) async fn sync_address_watch(
        &self,
        wallet_checksum: &str,
        descriptor: &str,
        electrum_manager: Option<&ElectrumClientManager>,
        suppress_notifications: bool,
    ) -> Result<AddressWatchSyncResult> {
        let sync_start = Instant::now();
        debug!("[{}] Starting address-based sync", wallet_checksum);
        let network = self.config.network.to_bdk_network();

        let script = Self::script_from_watch_descriptor(descriptor, network)?;

        // Get Electrum client
        let client = if let Some(manager) = electrum_manager {
            if let Some(c) = manager.get_client().await {
                c
            } else {
                warn!(
                    "[{}] No Electrum client available for address watch",
                    wallet_checksum
                );
                return Ok(AddressWatchSyncResult::SkippedNoClient);
            }
        } else {
            warn!(
                "[{}] No Electrum manager available for address watch",
                wallet_checksum
            );
            return Ok(AddressWatchSyncResult::SkippedNoClient);
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
        for existing_tx in existing_transactions.iter().filter(|tx| {
            tx.transaction_status == TransactionStatus::Pending && !history_txids.contains(&tx.txid)
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
                            self.send_confirmed_transaction_notification(&updated_tx);
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
                transaction_status: TransactionStatus::from_confirmed(state.is_confirmed),
                replaced_by_txid: None,
                replaced_at: None,
            };

            self.metadata_db.insert_transaction(&transaction).await?;
            if !suppress_notifications {
                self.send_new_transaction_notification(&transaction);
            }

            has_changes = true;
            info!(
                "[{}] New address watch tx: {} ({:?}, {} sats, {}{})",
                wallet_checksum,
                txid_str,
                state.transaction_type,
                state.amount_sats,
                TransactionStatus::from_confirmed(state.is_confirmed),
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
                suppress_notifications,
            )
            .await?
        {
            has_changes = true;
        }

        // Update balance from Electrum
        let balance = client.script_get_balance(&script).await?;
        let electrum_balance = sats_to_i64(balance.confirmed) + balance.unconfirmed as i64;
        let total_balance =
            balance_with_mainnet_genesis_coinbase(network, &script, electrum_balance, &history);
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

        Ok(AddressWatchSyncResult::Completed { has_changes })
    }

    /// Sync a group of address watches that share the same descriptor.
    /// Queries Electrum once and fans out the results (transactions, balance,
    /// notifications) to each watcher independently.
    pub(crate) async fn sync_address_watch_group(
        &self,
        watchers: &[(String, bool)],
        descriptor: &str,
        electrum_manager: Option<&ElectrumClientManager>,
    ) -> Result<AddressWatchSyncResult> {
        let sync_start = Instant::now();
        let network = self.config.network.to_bdk_network();
        info!(
            "Starting grouped address watch sync for {} watchers (descriptor: {})",
            watchers.len(),
            descriptor
        );

        let script = Self::script_from_watch_descriptor(descriptor, network)?;

        // Get Electrum client
        let client = if let Some(manager) = electrum_manager {
            if let Some(c) = manager.get_client().await {
                c
            } else {
                warn!("No Electrum client available for grouped address watch");
                return Ok(AddressWatchSyncResult::SkippedNoClient);
            }
        } else {
            warn!("No Electrum manager available for grouped address watch");
            return Ok(AddressWatchSyncResult::SkippedNoClient);
        };

        // === Single Electrum query for history ===
        let history = client.script_get_history(&script).await?;
        debug!(
            "Grouped address watch: {} transactions in history",
            history.len()
        );

        // === Single Electrum query for balance ===
        let balance = client.script_get_balance(&script).await?;
        let electrum_balance = sats_to_i64(balance.confirmed) + balance.unconfirmed as i64;
        let total_balance =
            balance_with_mainnet_genesis_coinbase(network, &script, electrum_balance, &history);
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
        for (wallet_checksum, suppress_notifications) in watchers {
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
                tx.transaction_status == TransactionStatus::Pending
                    && !history_txids.contains(&tx.txid)
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

            let mut rbf_replacements = Self::detect_address_watch_rbf_replacements(
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
                        let confirmed_at =
                            if let Some(&ts) = block_header_cache.get(&confirmed_height) {
                                ts
                            } else {
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
                            };

                        self.metadata_db
                            .update_transaction_confirmation(
                                wallet_checksum,
                                &txid_str,
                                confirmed_height,
                                confirmed_at,
                            )
                            .await?;

                        if !*suppress_notifications {
                            if let Some(updated_tx) = self
                                .metadata_db
                                .get_transaction_by_txid(wallet_checksum, &txid_str)
                                .await?
                            {
                                self.send_confirmed_transaction_notification(&updated_tx);
                            }
                        }

                        has_changes = true;
                    }
                    continue;
                }
                let owned_state;
                let state = if let Some(state) = current_tx_states.get(&txid_str) {
                    state
                } else {
                    let Some(fetched_state) = self
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

                    if let Some((original_txid, replacement_txid)) =
                        Self::find_address_watch_replacement_for_state(
                            &txid_str,
                            &fetched_state,
                            &existing_transactions,
                            &disappeared_pending_txs,
                        )
                    {
                        rbf_replacements.insert(original_txid, replacement_txid);
                    }

                    owned_state = fetched_state;
                    &owned_state
                };

                let transaction = TransactionInsert {
                    txid: txid_str.clone(),
                    wallet_checksum: wallet_checksum.clone(),
                    transaction_type: state.transaction_type,
                    amount_sats: state.amount_sats,
                    fee_sats: None,
                    block_height: state.block_height,
                    first_seen_at: state.first_seen_at,
                    confirmed_at: state.confirmed_at,
                    parent_txid: cpfp_relationships.get(&txid_str).cloned(),
                    transaction_status: TransactionStatus::from_confirmed(state.is_confirmed),
                    replaced_by_txid: None,
                    replaced_at: None,
                };

                self.metadata_db.insert_transaction(&transaction).await?;
                if !*suppress_notifications {
                    self.send_new_transaction_notification(&transaction);
                }

                has_changes = true;
                info!(
                    "[{}] New address watch tx: {} ({:?}, {} sats, {}{})",
                    wallet_checksum,
                    txid_str,
                    state.transaction_type,
                    state.amount_sats,
                    TransactionStatus::from_confirmed(state.is_confirmed),
                    if *suppress_notifications {
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
                    *suppress_notifications,
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
            if !*suppress_notifications {
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
                watchers.len(),
            );
        } else {
            debug!(
                "Grouped address-based sync complete in {:.2}s for {} watchers; changes=false",
                sync_duration.as_secs_f64(),
                watchers.len(),
            );
        }

        Ok(AddressWatchSyncResult::Completed {
            has_changes: any_changes,
        })
    }
}
