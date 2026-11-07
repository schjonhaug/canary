//! Removal of deleted and expired wallets from memory, disk, the metadata
//! database and the Electrum server's script subscriptions.

use super::manager::WalletManager;
use super::persistence::persist_wallet;
use crate::metadata::{WalletMetadata, WalletStatus, WalletType};
use anyhow::Result;
use bdk_wallet::bitcoin::ScriptBuf;
use std::collections::HashSet;
use tracing::{debug, error, warn};

fn active_address_watch_descriptors(wallets: &[WalletMetadata]) -> HashSet<&str> {
    wallets
        .iter()
        .filter(|wallet| {
            wallet.wallet_type == WalletType::Address && wallet.status != WalletStatus::Deleted
        })
        .map(|wallet| wallet.descriptor.as_str())
        .collect()
}

/// Picks the wallets cleanup should remove: every soft-deleted wallet, plus
/// loaded wallets whose database row is gone. `loaded_checksums` must be read
/// before the two wallet lists.
fn wallets_to_remove(
    loaded_checksums: &[String],
    non_deleted_wallets: &[WalletMetadata],
    deleted_wallets: &[WalletMetadata],
) -> Vec<String> {
    let known: HashSet<&str> = non_deleted_wallets
        .iter()
        .chain(deleted_wallets)
        .map(|wallet| wallet.checksum.as_str())
        .collect();

    let mut to_remove: Vec<String> = deleted_wallets
        .iter()
        .map(|wallet| wallet.checksum.clone())
        .collect();
    for checksum in loaded_checksums {
        if !known.contains(checksum.as_str()) && !to_remove.contains(checksum) {
            to_remove.push(checksum.clone());
        }
    }
    to_remove
}

impl WalletManager {
    /// Removes soft-deleted wallets from memory, disk and the database, and
    /// unloads in-memory wallets whose database row no longer exists.
    ///
    /// Only the `deleted` status, which is terminal, marks a wallet for
    /// removal. A wallet that is merely not `ready` yet may be in the middle
    /// of creation, so it is left alone.
    pub(super) async fn cleanup_deleted_wallets(&self) -> Result<()> {
        // Snapshot the map before reading the database. A wallet's row is
        // inserted before it is ever registered, so any checksum in this
        // snapshot that has no row afterwards really was removed.
        let loaded_checksums: Vec<String> = self.wallets.lock().await.keys().cloned().collect();

        // Pending address watches already own the same endpoint subscription that their initial
        // sync will use. Keep shared scripts until every non-deleted watcher is gone.
        let non_deleted_wallets = self.metadata_db.get_all_wallets().await?;
        let deleted_wallets = self.metadata_db.get_deleted_wallets().await?;
        let active_watch_descriptors = active_address_watch_descriptors(&non_deleted_wallets);

        let wallets_to_remove =
            wallets_to_remove(&loaded_checksums, &non_deleted_wallets, &deleted_wallets);

        let mut scripts_to_forget = HashSet::<ScriptBuf>::new();

        // Clean up each removed wallet: memory, disk, and database
        for checksum in wallets_to_remove {
            self.remove_wallet(
                &checksum,
                &deleted_wallets,
                &active_watch_descriptors,
                &mut scripts_to_forget,
            )
            .await;
        }

        if !scripts_to_forget.is_empty() && self.electrum_work_gate.is_some() {
            let _work_permit = self.acquire_electrum_work().await?;
            if let Some(manager) = &self.electrum_client_manager
                && let Err(error) = manager
                    .forget_scripts(scripts_to_forget.into_iter().collect())
                    .await
            {
                warn!("Failed to clean up deleted Electrum subscriptions: {error}");
            }
        }

        Ok(())
    }

    /// Removes one wallet from memory, disk and the database, and collects the
    /// scripts whose Electrum subscriptions can be forgotten.
    async fn remove_wallet(
        &self,
        checksum: &str,
        deleted_wallets: &[WalletMetadata],
        active_watch_descriptors: &HashSet<&str>,
        scripts_to_forget: &mut HashSet<ScriptBuf>,
    ) {
        debug!("Cleaning up wallet {} (deleted or expired)", checksum);
        let deleted_metadata = deleted_wallets
            .iter()
            .find(|wallet| wallet.checksum == checksum);
        let wallet_path = self.wallet_dir.join(format!("{checksum}.sqlite"));
        let mut removed_from_memory = false;

        // Remove from memory (if it was loaded). Release the map lock before waiting
        // for the wallet: an in-flight sync can hold the wallet lock across Electrum
        // calls, and every other wallet lookup needs the map.
        let removed_entry = self.wallets.lock().await.remove(checksum);
        if let Some(wallet_arc) = removed_entry {
            removed_from_memory = true;
            // Persist final state before removal
            let wallet_data = wallet_arc.lock_owned().await;
            scripts_to_forget.extend(
                wallet_data
                    .0
                    .spk_index()
                    .revealed_spks(..)
                    .map(|(_, script)| script),
            );
            if let Err(e) = persist_wallet(wallet_data).await {
                warn!("Failed to persist wallet before removal: {}", e);
            }
            debug!("Removed wallet from memory");
        }

        if let Some(metadata) = deleted_metadata {
            if metadata.wallet_type == WalletType::Address {
                // Multiple users may share one address-watch subscription. Keep it until the
                // last active watcher is gone.
                if !active_watch_descriptors.contains(metadata.descriptor.as_str()) {
                    match crate::sync::WalletSyncService::script_from_watch_descriptor(
                        &metadata.descriptor,
                        self.network,
                    ) {
                        Ok(script) => {
                            scripts_to_forget.insert(script);
                        }
                        Err(error) => warn!(
                            "Could not resolve deleted address watch {} for Electrum cleanup: {error}",
                            metadata.checksum
                        ),
                    }
                }
            } else if !removed_from_memory && wallet_path.exists() {
                match Self::load_wallet_from_disk(&wallet_path, &metadata.descriptor, self.network)
                    .await
                {
                    Ok((wallet, _)) => scripts_to_forget.extend(
                        wallet
                            .spk_index()
                            .revealed_spks(..)
                            .map(|(_, script)| script),
                    ),
                    Err(error) => warn!(
                        "Could not load deleted wallet {} for Electrum cleanup: {error}",
                        metadata.checksum
                    ),
                }
            }
        }

        // Delete wallet file from disk (only for descriptor-type wallets; address watches have no file)
        if wallet_path.exists() {
            if let Err(e) = std::fs::remove_file(&wallet_path) {
                error!(
                    "  Warning: Failed to delete wallet file {}: {}",
                    wallet_path.display(),
                    e
                );
            } else {
                debug!("Deleted wallet file: {}", wallet_path.display());
            }
        }

        // Hard delete from database. The status guard keeps this a no-op for
        // any wallet that is not soft-deleted.
        if let Err(e) = self
            .metadata_db
            .hard_delete_soft_deleted_wallet(checksum)
            .await
        {
            error!(
                "  Warning: Failed to hard delete wallet {} from database: {}",
                checksum, e
            );
        } else {
            debug!("Hard deleted wallet {} from database", checksum);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, NetworkConfig, OperatingMode};
    use crate::wallet::tests::{TWO_PATH_DESCRIPTOR, create_persisted_test_wallet, queue_wallet};
    use bdk_wallet::bitcoin::Network;
    use tempfile::TempDir;
    use tokio::sync::broadcast;

    async fn test_manager(directory: &TempDir) -> WalletManager {
        let config = AppConfig::new_for_test(
            NetworkConfig::Regtest,
            Some("tcp://127.0.0.1:1".to_string()),
            "127.0.0.1:3000".to_string(),
            directory.path().to_string_lossy().to_string(),
            OperatingMode::Cloud,
            None,
            Some("test-jwt-secret".to_string()),
        );
        let (event_tx, _event_rx) = broadcast::channel(8);
        WalletManager::new(
            event_tx,
            directory.path().join("wallets"),
            directory.path().join("metadata.sqlite").to_str().unwrap(),
            Network::Regtest,
            "tcp://127.0.0.1:1",
            &config,
        )
        .await
    }

    /// Inserts a wallet row, writes its BDK file and loads it into memory, the
    /// way creation does once the wallet is set up.
    async fn insert_loaded_wallet(manager: &WalletManager, user_id: &str, name: &str) -> String {
        let checksum = manager
            .metadata_db
            .insert_wallet(name, &format!("descriptor-{name}"), user_id)
            .await
            .unwrap();
        let wallet_path = manager.wallet_dir.join(format!("{checksum}.sqlite"));
        create_persisted_test_wallet(&wallet_path);
        let (wallet, conn) = WalletManager::load_wallet_from_disk(
            &wallet_path,
            TWO_PATH_DESCRIPTOR,
            Network::Regtest,
        )
        .await
        .unwrap();
        manager
            .register_wallet(checksum.clone(), wallet, conn)
            .await;
        checksum
    }

    async fn is_loaded(manager: &WalletManager, checksum: &str) -> bool {
        manager.wallets.lock().await.contains_key(checksum)
    }

    #[tokio::test]
    async fn cleanup_keeps_loaded_wallets_that_are_not_soft_deleted() {
        let directory = tempfile::tempdir().unwrap();
        let manager = test_manager(&directory).await;
        let user_id = manager
            .metadata_db
            .create_user("cleanup@example.invalid", "hash", None, false, None, None)
            .await
            .unwrap();

        // A wallet whose status is still `pending` when cleanup reads the
        // database is what cleanup sees when its read lands just before the
        // creation task marks the wallet ready and registers it.
        let in_creation = insert_loaded_wallet(&manager, &user_id, "in-creation").await;
        let failed = insert_loaded_wallet(&manager, &user_id, "failed").await;
        manager
            .metadata_db
            .update_wallet_status_if_not_deleted(&failed, WalletStatus::Failed)
            .await
            .unwrap();

        manager.cleanup_deleted_wallets().await.unwrap();

        for checksum in [&in_creation, &failed] {
            assert!(is_loaded(&manager, checksum).await);
            assert!(
                manager
                    .wallet_dir
                    .join(format!("{checksum}.sqlite"))
                    .exists()
            );
            assert!(
                manager
                    .metadata_db
                    .get_wallet_by_checksum(checksum)
                    .await
                    .unwrap()
                    .is_some()
            );
        }
    }

    #[tokio::test]
    async fn cleanup_removes_soft_deleted_and_vanished_wallets() {
        let directory = tempfile::tempdir().unwrap();
        let manager = test_manager(&directory).await;
        let user_id = manager
            .metadata_db
            .create_user("cleanup@example.invalid", "hash", None, false, None, None)
            .await
            .unwrap();

        let deleted = insert_loaded_wallet(&manager, &user_id, "deleted").await;
        manager
            .metadata_db
            .mark_wallet_as_deleted(&deleted)
            .await
            .unwrap();
        let vanished = insert_loaded_wallet(&manager, &user_id, "vanished").await;
        manager
            .metadata_db
            .hard_delete_wallet_by_checksum(&vanished)
            .await
            .unwrap();

        manager.cleanup_deleted_wallets().await.unwrap();

        for checksum in [&deleted, &vanished] {
            assert!(!is_loaded(&manager, checksum).await);
            assert!(
                !manager
                    .wallet_dir
                    .join(format!("{checksum}.sqlite"))
                    .exists()
            );
            assert!(
                manager
                    .metadata_db
                    .get_wallet_by_checksum(checksum)
                    .await
                    .unwrap()
                    .is_none()
            );
        }
    }

    #[tokio::test]
    async fn guarded_hard_delete_ignores_wallets_that_are_not_soft_deleted() {
        let directory = tempfile::tempdir().unwrap();
        let manager = test_manager(&directory).await;
        let user_id = manager
            .metadata_db
            .create_user("cleanup@example.invalid", "hash", None, false, None, None)
            .await
            .unwrap();
        let checksum = manager
            .metadata_db
            .insert_wallet("ready", "descriptor-ready", &user_id)
            .await
            .unwrap();
        manager
            .metadata_db
            .update_wallet_status_if_not_deleted(&checksum, WalletStatus::Ready)
            .await
            .unwrap();

        assert!(
            !manager
                .metadata_db
                .hard_delete_soft_deleted_wallet(&checksum)
                .await
                .unwrap()
        );
        manager
            .metadata_db
            .mark_wallet_as_deleted(&checksum)
            .await
            .unwrap();
        assert!(
            manager
                .metadata_db
                .hard_delete_soft_deleted_wallet(&checksum)
                .await
                .unwrap()
        );
    }

    #[test]
    fn removal_set_is_soft_deleted_plus_loaded_wallets_without_a_row() {
        let ready = queue_wallet("ready", None);
        let mut pending = queue_wallet("pending", None);
        pending.status = WalletStatus::Pending;
        let mut deleted = queue_wallet("deleted", None);
        deleted.status = WalletStatus::Deleted;
        let loaded = ["ready", "pending", "deleted", "vanished"].map(str::to_string);

        let mut to_remove = wallets_to_remove(&loaded, &[ready, pending], &[deleted]);
        to_remove.sort();

        assert_eq!(
            to_remove,
            vec!["deleted".to_string(), "vanished".to_string()]
        );
    }

    #[test]
    fn pending_address_watches_keep_shared_subscriptions_active() {
        let mut ready = queue_wallet("ready", None);
        ready.wallet_type = WalletType::Address;
        ready.descriptor = "addr(ready-address)".to_string();

        let mut pending = queue_wallet("pending", None);
        pending.wallet_type = WalletType::Address;
        pending.status = WalletStatus::Pending;
        pending.descriptor = "addr(shared-address)".to_string();

        let mut deleted = queue_wallet("deleted", None);
        deleted.wallet_type = WalletType::Address;
        deleted.status = WalletStatus::Deleted;
        deleted.descriptor = "addr(shared-address)".to_string();

        let mut deleted_only = queue_wallet("deleted-only", None);
        deleted_only.wallet_type = WalletType::Address;
        deleted_only.status = WalletStatus::Deleted;
        deleted_only.descriptor = "addr(deleted-address)".to_string();

        let wallets = [ready, pending, deleted, deleted_only];
        let active = active_address_watch_descriptors(&wallets);

        assert!(active.contains("addr(ready-address)"));
        assert!(active.contains("addr(shared-address)"));
        assert!(!active.contains("addr(deleted-address)"));
    }
}
