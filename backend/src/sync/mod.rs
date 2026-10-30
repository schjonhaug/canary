//! Wallet synchronization. `WalletSyncService` reconciles Electrum state with
//! the metadata database and publishes transaction and balance-alert
//! notifications on the broadcast channel.
//!
//! Descriptor wallets sync through BDK (`descriptor`, `transactions` and
//! `historical`). Single-address watches query Electrum directly, one watch at
//! a time (`address_watch`) or grouped by shared descriptor
//! (`address_watch_group`). Both kinds use the shared pieces in `genesis`
//! (Electrum height conventions and the mainnet genesis coinbase),
//! `relationships` (CPFP and RBF detection), `balance_alerts` and `notify`.

mod address_watch;
mod address_watch_group;
mod balance_alerts;
mod descriptor;
mod genesis;
mod historical;
mod notify;
mod relationships;
mod transactions;

use crate::config::AppConfig;
use crate::metadata::{MetadataDb, TransactionNotification};
use crate::utils::current_unix_timestamp;
use tokio::sync::broadcast;
use tracing::warn;

/// Result of an address watch synchronization attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AddressWatchSyncResult {
    /// The sync completed successfully after querying Electrum.
    Completed { has_changes: bool },
    /// The sync was skipped because no Electrum client or manager was available.
    SkippedNoClient,
}

/// Result of a descriptor-wallet synchronization attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DescriptorWalletSyncResult {
    /// Electrum synchronization and Canary transaction reconciliation completed.
    Completed,
    /// The sync was skipped because no Electrum client manager was configured.
    SkippedNoClient,
}

fn current_unix_timestamp_or(default: u64, context: &str) -> u64 {
    current_unix_timestamp().unwrap_or_else(|error| {
        warn!(
            "Failed to read current UNIX timestamp for {}: {}. Falling back to {}.",
            context, error, default
        );
        default
    })
}

/// Syncs wallets and address watches against Electrum, records their
/// transactions in the metadata database and publishes notifications for them.
pub struct WalletSyncService {
    metadata_db: MetadataDb,
    notification_sender: broadcast::Sender<TransactionNotification>,
    config: AppConfig,
}

impl WalletSyncService {
    /// Creates a sync service that publishes notifications on `notification_sender`.
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
}
