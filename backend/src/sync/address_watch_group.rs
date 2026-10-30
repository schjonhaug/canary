//! Grouped address-watch sync: one set of Electrum queries per shared
//! descriptor, applied to each watcher of that descriptor in turn.

use super::address_watch::{AddressWatchTxState, fetch_disappeared_pending_txs};
use super::genesis::{
    balance_with_mainnet_genesis_coinbase, confirmed_block_height, genesis_block_timestamp,
    is_tx_confirmed,
};
use super::{AddressWatchSyncResult, WalletSyncService, current_unix_timestamp_or};
use crate::electrum::{ElectrumClient, ElectrumClientManager};
use crate::utils::amount::sats_to_i64;
use anyhow::Result;
use bdk_electrum::electrum_client::GetHistoryRes;
use bdk_wallet::bitcoin::{Network, ScriptBuf};
use std::collections::{HashMap, HashSet};
use std::time::Instant;
use tracing::{debug, info, warn};

/// Electrum results for one grouped address-watch sync, shared by every watcher
/// of the descriptor.
struct GroupedWatchSnapshot<'a> {
    client: &'a ElectrumClient,
    script: &'a ScriptBuf,
    network: Network,
    history: &'a [GetHistoryRes],
    history_txids: &'a HashSet<String>,
    current_tx_states: &'a HashMap<String, AddressWatchTxState>,
    cpfp_relationships: &'a HashMap<String, String>,
    total_balance: i64,
}

impl WalletSyncService {
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
        let history_txids: HashSet<String> = history
            .iter()
            .map(|entry| entry.tx_hash.to_string())
            .collect();
        let mut current_tx_states = HashMap::new();

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
        let snapshot = GroupedWatchSnapshot {
            client: &client,
            script: &script,
            network,
            history: &history,
            history_txids: &history_txids,
            current_tx_states: &current_tx_states,
            cpfp_relationships: &cpfp_relationships,
            total_balance,
        };

        // Cache block headers by height to avoid repeated Electrum calls
        let mut block_header_cache: HashMap<u32, u64> = HashMap::new();

        let mut any_changes = false;

        // === Fan out to each watcher ===
        for (wallet_checksum, suppress_notifications) in watchers {
            if self
                .sync_grouped_watcher(
                    &snapshot,
                    wallet_checksum,
                    *suppress_notifications,
                    &mut block_header_cache,
                )
                .await?
            {
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

    /// Applies one grouped sync's Electrum results to a single watcher: new and
    /// newly confirmed transactions, CPFP and RBF links, balance and alerts.
    /// Returns whether anything changed for this watcher.
    async fn sync_grouped_watcher(
        &self,
        snapshot: &GroupedWatchSnapshot<'_>,
        wallet_checksum: &str,
        suppress_notifications: bool,
        block_header_cache: &mut HashMap<u32, u64>,
    ) -> Result<bool> {
        let GroupedWatchSnapshot {
            client,
            script,
            network,
            history,
            history_txids,
            current_tx_states,
            cpfp_relationships,
            total_balance,
        } = *snapshot;

        // Get existing transactions for THIS watcher
        let existing_transactions = self
            .metadata_db
            .get_transactions_by_wallet_checksum(wallet_checksum, None, false)
            .await?;
        let existing_tx_map: HashMap<&str, &_> = existing_transactions
            .iter()
            .map(|tx| (tx.txid.as_str(), tx))
            .collect();

        let mut has_changes = false;
        let disappeared_pending_txs = fetch_disappeared_pending_txs(
            client,
            wallet_checksum,
            &existing_transactions,
            history_txids,
        )
        .await?;

        let mut rbf_replacements = Self::detect_address_watch_rbf_replacements(
            current_tx_states,
            &existing_transactions,
            &disappeared_pending_txs,
        );

        for hist_entry in history {
            let txid = hist_entry.tx_hash;
            let txid_str = hist_entry.tx_hash.to_string();

            if let Some(existing) = existing_tx_map.get(txid_str.as_str()) {
                // Check if an existing pending transaction got confirmed
                if is_tx_confirmed(network, hist_entry.height, &txid)
                    && existing.block_height.is_none()
                {
                    let confirmed_height = confirmed_block_height(hist_entry.height);
                    let confirmed_at = cached_block_timestamp(
                        client,
                        network,
                        confirmed_height,
                        block_header_cache,
                    )
                    .await;

                    self.record_address_watch_confirmation(
                        wallet_checksum,
                        &txid_str,
                        confirmed_height,
                        confirmed_at,
                        suppress_notifications,
                    )
                    .await?;

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
                        client,
                        script,
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

            self.insert_address_watch_tx(
                wallet_checksum,
                &txid_str,
                state,
                cpfp_relationships,
                suppress_notifications,
            )
            .await?;
            has_changes = true;
        }

        if self
            .apply_address_watch_relationships(
                wallet_checksum,
                &existing_transactions,
                cpfp_relationships,
                &rbf_replacements,
                suppress_notifications,
            )
            .await?
        {
            has_changes = true;
        }

        self.store_address_watch_balance(wallet_checksum, total_balance, suppress_notifications)
            .await?;

        Ok(has_changes)
    }
}

/// Block timestamp for `confirmed_height`, cached across the watchers of one
/// grouped sync.
async fn cached_block_timestamp(
    client: &ElectrumClient,
    network: Network,
    confirmed_height: u32,
    block_header_cache: &mut HashMap<u32, u64>,
) -> u64 {
    if let Some(&ts) = block_header_cache.get(&confirmed_height) {
        return ts;
    }

    let ts = match client.get_block_header(confirmed_height).await {
        Ok(header) => header.timestamp,
        // Grouped watchers can run on any configured network, so fall
        // back to a generic timestamp when there is no mainnet genesis
        // special-case to apply.
        Err(_) => genesis_block_timestamp(network, confirmed_height)
            .unwrap_or_else(|| current_unix_timestamp_or(0, "watcher confirmation timestamp")),
    };
    block_header_cache.insert(confirmed_height, ts);
    ts
}
