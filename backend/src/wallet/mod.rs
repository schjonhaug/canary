//! Descriptor wallets and address watches: creating them, keeping the loaded
//! BDK wallets in memory and running the background sync schedules.
//!
//! [`WalletCreationService`] adds wallets for the HTTP layer. [`WalletManager`]
//! owns the in-memory BDK wallets, the self-hosted fair sync queue, the
//! cloud tier-parallel sync, subscription limit enforcement and the cleanup of
//! deleted wallets.

mod cleanup;
mod creation;
#[cfg(unix)]
mod fs;
mod limits;
mod manager;
mod persistence;
mod scheduler;
mod tier_sync;

#[cfg(test)]
mod tests;

pub use creation::WalletCreationService;
pub use manager::WalletManager;

use crate::metadata::{WalletMetadata, WalletStatus};
use bdk_wallet::rusqlite::Connection;
use bdk_wallet::PersistedWallet;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// A loaded BDK wallet together with the `SQLite` connection it persists to
type WalletState = (PersistedWallet<Connection>, Connection);

/// Individual wallet entry containing BDK wallet and its `SQLite` connection
type WalletEntry = Arc<Mutex<WalletState>>;

/// Thread-safe map of wallet checksums to wallet entries
type WalletMap = Arc<Mutex<HashMap<String, WalletEntry>>>;

fn address_watch_sync_targets(watchers: &[WalletMetadata]) -> Vec<(String, bool)> {
    watchers
        .iter()
        .map(|watcher| {
            (
                watcher.checksum.clone(),
                watcher.status == WalletStatus::Pending,
            )
        })
        .collect()
}
