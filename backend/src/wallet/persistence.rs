//! BDK wallet persistence: creating a wallet's `SQLite` file, writing staged
//! changes and loading a persisted wallet back from disk.

use super::WalletState;
use super::manager::WalletManager;
use anyhow::{Result, anyhow};
use bdk_wallet::rusqlite::Connection;
use bdk_wallet::{PersistedWallet, Wallet, bitcoin::Network};
use std::ops::DerefMut;
use std::path::Path;
use tokio::task::spawn_blocking;

#[cfg(unix)]
use super::fs::{create_private_file, restrict_permissions, sqlite_sidecar_path};

/// Creates the BDK wallet file and persists its initial state.
///
/// This is blocking file and `SQLite` work, so callers run it on the blocking
/// thread pool.
pub(super) fn create_wallet_file(
    wallet_path: &Path,
    descriptor: String,
    network: Network,
) -> Result<WalletState> {
    #[cfg(unix)]
    create_private_file(wallet_path)?;

    let mut db = Connection::open(wallet_path).map_err(|e| {
        anyhow!(
            "Failed to create connection to {}: {}",
            wallet_path.display(),
            e
        )
    })?;
    // If a later creation step fails, keep the partial BDK SQLite file with the
    // failed wallet record so the normal delete cleanup path can remove both.

    // Canary validates the descriptor before spawning this task. Keep the normalized
    // two-path form intact so BDK owns the receive/change split used for persistence.
    let mut wallet = Wallet::create_from_two_path_descriptor(descriptor)
        .network(network)
        .create_wallet(&mut db)
        .map_err(|e| anyhow!("Failed to create wallet: {e}"))?;

    // Persist initial wallet state
    wallet
        .persist(&mut db)
        .map_err(|e| anyhow!("Failed to persist wallet: {e}"))?;
    #[cfg(unix)]
    for path in [
        wallet_path.to_path_buf(),
        sqlite_sidecar_path(wallet_path, "-wal"),
        sqlite_sidecar_path(wallet_path, "-shm"),
    ] {
        if path.exists() {
            restrict_permissions(&path, 0o600)?;
        }
    }
    Ok((wallet, db))
}

/// Writes a wallet's staged BDK changes to its `SQLite` file on the blocking
/// thread pool and hands the wallet back.
///
/// Taking the state by value (a [`tokio::sync::OwnedMutexGuard`] for loaded
/// wallets) keeps the wallet locked for the whole write.
pub(super) async fn persist_wallet<S>(mut state: S) -> Result<S>
where
    S: DerefMut<Target = WalletState> + Send + 'static,
{
    spawn_blocking(move || {
        let (wallet, conn) = &mut *state;
        wallet.persist(conn)?;
        Ok(state)
    })
    .await?
}

impl WalletManager {
    /// Load a single wallet from disk
    /// Returns the wallet and its database connection
    pub(super) async fn load_wallet_from_disk(
        wallet_path: &Path,
        expected_descriptor: &str,
        network: Network,
    ) -> Result<(PersistedWallet<Connection>, Connection)> {
        // Run blocking I/O in a separate thread
        let path = wallet_path.to_path_buf();
        let expected_descriptor = expected_descriptor.to_string();
        let result = tokio::task::spawn_blocking(move || {
            let conn = Connection::open(&path)
                .map_err(|e| anyhow!("Failed to open wallet database: {e}"))?;

            let mut conn = conn;
            let wallet = Wallet::load()
                .two_path_descriptor(expected_descriptor)
                .check_network(network)
                .load_wallet(&mut conn)
                .map_err(|e| anyhow!("Failed to load wallet from database: {e}"))?;

            let wallet_opt = wallet.ok_or_else(|| anyhow!("No wallet data found in file"))?;
            Ok::<(PersistedWallet<Connection>, Connection), anyhow::Error>((wallet_opt, conn))
        })
        .await??;

        Ok(result)
    }
}
