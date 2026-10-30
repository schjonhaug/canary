//! Bulk import of a new descriptor wallet's transaction history into the
//! metadata database, without sending notifications.

use super::genesis::MAINNET_GENESIS_BLOCK_TIMESTAMP;
use super::{current_unix_timestamp_or, WalletSyncService};
use crate::electrum::ElectrumClient;
use crate::metadata::{EventType, MetadataDb, TransactionInsert, TransactionStatus};
use crate::utils::amount::{sats_to_btc, sats_to_i64};
use anyhow::Result;
use bdk_wallet::chain::{ChainPosition, ConfirmationBlockTime};
use bdk_wallet::{rusqlite::Connection, PersistedWallet};
use std::cmp::Ordering;
use tracing::{debug, warn};

impl WalletSyncService {
    /// Records every transaction already in `wallet` once the full scan that
    /// completes wallet creation has finished, oldest first and without
    /// notifications.
    pub(crate) async fn extract_historical_transactions_for_background(
        wallet: &PersistedWallet<Connection>,
        wallet_checksum: &str,
        metadata_db: &MetadataDb,
        electrum_client: Option<&ElectrumClient>,
    ) -> Result<()> {
        debug!("[{}] Extracting historical transactions", wallet_checksum);

        // Collect all transactions and sort them chronologically
        let mut all_transactions: Vec<_> = wallet.transactions().collect();

        // Sort transactions chronologically (confirmed first, then by height/timestamp)
        all_transactions.sort_by(|a, b| chronological_order(&a.chain_position, &b.chain_position));

        debug!(
            "[{}] Found {} historical transactions to process",
            wallet_checksum,
            all_transactions.len()
        );

        // Get current wallet balance
        let current_balance = sats_to_i64(wallet.balance().total().to_sat());

        // Calculate initial balance by working backwards from current balance
        let total_net_change: i64 = all_transactions
            .iter()
            .map(|tx| {
                let sent = wallet.sent_and_received(&tx.tx_node).0;
                let received = wallet.sent_and_received(&tx.tx_node).1;
                sats_to_i64(received.to_sat()) - sats_to_i64(sent.to_sat())
            })
            .filter(|&net| net != 0) // Skip zero net amount transactions
            .sum();

        let initial_balance = current_balance - total_net_change;

        debug!(
            "[{}] Current balance: {:.8} BTC, Initial balance: {:.8} BTC",
            wallet_checksum,
            sats_to_btc(current_balance),
            sats_to_btc(initial_balance)
        );

        // Process each transaction chronologically using new transaction-based approach
        for tx in all_transactions {
            let txid = tx.tx_node.txid.to_string();
            let sent = wallet.sent_and_received(&tx.tx_node).0;
            let received = wallet.sent_and_received(&tx.tx_node).1;
            let net_amount = sats_to_i64(received.to_sat()) - sats_to_i64(sent.to_sat());
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
            let (block_height, confirmed_at) =
                confirmation_details(&tx.chain_position, wallet_checksum, electrum_client).await;

            // Get first_seen timestamp
            let first_seen_at = match &tx.chain_position {
                ChainPosition::Confirmed { .. } => confirmed_at.unwrap_or_else(|| {
                    current_unix_timestamp_or(
                        MAINNET_GENESIS_BLOCK_TIMESTAMP,
                        "confirmed first-seen timestamp",
                    )
                }),
                ChainPosition::Unconfirmed { first_seen, .. } => first_seen.unwrap_or_else(|| {
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
                transaction_status: TransactionStatus::from_confirmed(is_confirmed),
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
}

/// Orders confirmed transactions by block height ahead of unconfirmed ones,
/// which are ordered by first-seen time.
fn chronological_order(
    a: &ChainPosition<ConfirmationBlockTime>,
    b: &ChainPosition<ConfirmationBlockTime>,
) -> Ordering {
    match (a, b) {
        // Both confirmed: sort by block height
        (
            ChainPosition::Confirmed {
                anchor: anchor_a, ..
            },
            ChainPosition::Confirmed {
                anchor: anchor_b, ..
            },
        ) => anchor_a.block_id.height.cmp(&anchor_b.block_id.height),
        // Both unconfirmed: sort by first_seen timestamp if available
        (
            ChainPosition::Unconfirmed {
                first_seen: first_a,
                ..
            },
            ChainPosition::Unconfirmed {
                first_seen: first_b,
                ..
            },
        ) => first_a.unwrap_or(0).cmp(&first_b.unwrap_or(0)),
        // Confirmed comes before unconfirmed
        (ChainPosition::Confirmed { .. }, ChainPosition::Unconfirmed { .. }) => Ordering::Less,
        // Unconfirmed comes after confirmed
        (ChainPosition::Unconfirmed { .. }, ChainPosition::Confirmed { .. }) => Ordering::Greater,
    }
}

/// Block height and timestamp for a confirmed position, or `(None, None)` for
/// an unconfirmed one. The timestamp falls back to the current time when the
/// block header cannot be fetched.
async fn confirmation_details(
    chain_position: &ChainPosition<ConfirmationBlockTime>,
    wallet_checksum: &str,
    electrum_client: Option<&ElectrumClient>,
) -> (Option<u32>, Option<u64>) {
    match chain_position {
        ChainPosition::Confirmed { anchor, .. } => {
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
        ChainPosition::Unconfirmed { .. } => (None, None),
    }
}
