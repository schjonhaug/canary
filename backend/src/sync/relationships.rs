//! CPFP and RBF relationship detection: parent/child links between unconfirmed
//! transactions and replacements of pending transactions, for descriptor
//! wallets and address watches.

use super::address_watch::AddressWatchTxState;
use super::transactions::TransactionSummary;
use super::WalletSyncService;
use crate::metadata::TransactionStatus;
use anyhow::Result;
use bdk_wallet::bitcoin::{OutPoint, Transaction as BitcoinTransaction, Txid};
use bdk_wallet::{rusqlite::Connection, PersistedWallet};
use std::str::FromStr;
use tracing::{debug, warn};

impl WalletSyncService {
    /// Detect CPFP relationships among unconfirmed transactions
    /// Returns a `HashMap` mapping `child_txid` -> `parent_txid` for transactions that are CPFP children
    pub(super) fn detect_cpfp_relationships(
        wallet: &PersistedWallet<Connection>,
        wallet_checksum: &str,
        all_transactions: &[TransactionSummary],
    ) -> std::collections::HashMap<String, String> {
        use std::collections::HashMap;

        let mut cpfp_relationships = HashMap::new();

        // Only consider unconfirmed transactions for CPFP detection
        let unconfirmed_txs: Vec<_> = all_transactions
            .iter()
            .filter(|(_, _, _, is_confirmed, _, _)| !is_confirmed)
            .collect();

        if unconfirmed_txs.len() < 2 {
            // Need at least 2 unconfirmed transactions for CPFP
            return cpfp_relationships;
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
                    warn!("[{}] Failed to parse txid {}: {}", wallet_checksum, txid, e);
                })
                .ok()
                .and_then(|t| bdk_tx_map.get(&t))
            {
                for (vout, _) in (0u32..).zip(&bdk_tx.tx.output) {
                    unconfirmed_outputs.insert((txid.clone(), vout), txid.clone());
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
                    );
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

        cpfp_relationships
    }

    pub(super) fn detect_address_watch_cpfp_relationships(
        tx_states: &std::collections::HashMap<String, AddressWatchTxState>,
    ) -> std::collections::HashMap<String, String> {
        let mut relationships = std::collections::HashMap::new();
        let mut unconfirmed_outputs = std::collections::HashMap::new();

        for (txid, state) in tx_states.iter().filter(|(_, state)| !state.is_confirmed) {
            let computed_txid = state.tx.compute_txid();
            for (vout, _) in (0u32..).zip(&state.tx.output) {
                unconfirmed_outputs.insert(
                    OutPoint {
                        txid: computed_txid,
                        vout,
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

    pub(super) fn detect_address_watch_rbf_replacements(
        current_tx_states: &std::collections::HashMap<String, AddressWatchTxState>,
        existing_transactions: &[crate::metadata::TransactionWithWallet],
        disappeared_pending_txs: &std::collections::HashMap<String, BitcoinTransaction>,
    ) -> std::collections::HashMap<String, String> {
        let mut replacements = std::collections::HashMap::new();

        let mut candidate_states: Vec<_> = current_tx_states.iter().collect();
        candidate_states.sort_by_key(|(txid, state)| (state.first_seen_at, txid.as_str()));

        for (candidate_txid, candidate_state) in candidate_states {
            if let Some((original_txid, replacement_txid)) =
                Self::find_address_watch_replacement_for_state(
                    candidate_txid,
                    candidate_state,
                    existing_transactions,
                    disappeared_pending_txs,
                )
            {
                replacements
                    .entry(original_txid)
                    .or_insert(replacement_txid);
            }
        }

        replacements
    }

    pub(super) fn find_address_watch_replacement_for_state(
        candidate_txid: &str,
        candidate_state: &AddressWatchTxState,
        existing_transactions: &[crate::metadata::TransactionWithWallet],
        disappeared_pending_txs: &std::collections::HashMap<String, BitcoinTransaction>,
    ) -> Option<(String, String)> {
        for pending_tx in existing_transactions
            .iter()
            .filter(|tx| tx.transaction_status == TransactionStatus::Pending)
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

            if candidate_state.first_seen_at < pending_tx.first_seen_at {
                continue;
            }

            if candidate_state.transaction_type != pending_tx.transaction_type {
                continue;
            }

            let has_shared_input = candidate_state
                .tx
                .input
                .iter()
                .filter(|input| !input.previous_output.is_null())
                .any(|input| disappeared_inputs.contains(&input.previous_output));

            if has_shared_input {
                return Some((pending_tx.txid.clone(), candidate_txid.to_string()));
            }
        }

        None
    }

    pub(super) async fn apply_address_watch_relationships(
        &self,
        wallet_checksum: &str,
        existing_transactions: &[crate::metadata::TransactionWithWallet],
        cpfp_relationships: &std::collections::HashMap<String, String>,
        rbf_replacements: &std::collections::HashMap<String, String>,
        suppress_notifications: bool,
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

            if tx.transaction_status == TransactionStatus::Pending {
                if let Some(replacement_txid) = rbf_replacements.get(&tx.txid) {
                    if self
                        .metadata_db
                        .mark_transaction_replaced(wallet_checksum, &tx.txid, replacement_txid)
                        .await?
                    {
                        if let Some(updated_tx) = self
                            .metadata_db
                            .get_transaction_by_txid(wallet_checksum, &tx.txid)
                            .await?
                        {
                            if !suppress_notifications {
                                // Address-watch wallets do not have BDK's wallet graph;
                                // this path uses stored pending tx inputs instead.
                                self.send_replaced_transaction_notification(&updated_tx);
                            }
                        }
                        has_changes = true;
                    }
                }
            }
        }

        Ok(has_changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::{EventType, TransactionWithWallet};
    use bdk_wallet::bitcoin::{absolute, Amount, ScriptBuf, Sequence, TxIn, TxOut, Witness};

    fn test_tx(txid_seed: u8, inputs: Vec<OutPoint>, output_count: usize) -> BitcoinTransaction {
        let outputs = (0..output_count)
            .map(|i| TxOut {
                value: Amount::from_sat(1_000 + i as u64),
                script_pubkey: ScriptBuf::from_bytes(vec![txid_seed, u8::try_from(i).unwrap()]),
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

    fn confirmed_state(
        tx: BitcoinTransaction,
        transaction_type: EventType,
        first_seen_at: u64,
    ) -> AddressWatchTxState {
        AddressWatchTxState {
            tx,
            transaction_type,
            amount_sats: 1_000,
            is_confirmed: true,
            block_height: Some(1),
            first_seen_at,
            confirmed_at: Some(first_seen_at),
        }
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
            transaction_status: TransactionStatus::Pending,
            replaced_by_txid: None,
            replaced_at: None,
            label: None,
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

    #[test]
    fn test_detect_address_watch_rbf_replacements_accepts_confirmed_candidate() {
        let shared_input = OutPoint {
            txid: test_tx(10, vec![], 1).compute_txid(),
            vout: 0,
        };
        let original = test_tx(11, vec![shared_input], 1);
        let replacement = test_tx(12, vec![shared_input], 1);
        let replacement_txid = replacement.compute_txid().to_string();

        let current_tx_states = std::collections::HashMap::from([(
            replacement_txid.clone(),
            confirmed_state(replacement, EventType::Send, 100),
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
            transaction_status: TransactionStatus::Pending,
            replaced_by_txid: None,
            replaced_at: None,
            label: None,
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
