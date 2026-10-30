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

impl WalletManager {
    /// Clean up deleted wallets - remove from memory, disk, and database
    pub(super) async fn cleanup_deleted_wallets(&self) -> Result<()> {
        // Get ready wallets from database (source of truth)
        let ready_wallets = self.metadata_db.get_ready_wallets().await?;

        // Pending address watches already own the same endpoint subscription that their initial
        // sync will use. Keep shared scripts until every non-deleted watcher is gone.
        let non_deleted_wallets = self.metadata_db.get_all_wallets().await?;

        // Get wallets marked as deleted in database
        let deleted_wallets = self.metadata_db.get_deleted_wallets().await?;

        // Create set of valid checksums from database
        let valid_checksums: HashSet<String> =
            ready_wallets.iter().map(|w| w.checksum.clone()).collect();
        let active_watch_descriptors = active_address_watch_descriptors(&non_deleted_wallets);

        // Collect wallets that need to be removed
        let mut wallets_to_remove = Vec::new();

        // Check wallets in memory against valid checksums
        {
            let wallets_map = self.wallets.lock().await;
            for checksum in wallets_map.keys() {
                if !valid_checksums.contains(checksum) {
                    wallets_to_remove.push(checksum.clone());
                }
            }
        }

        // Also add wallets marked as deleted in database (even if not in memory)
        for deleted_wallet in &deleted_wallets {
            if !wallets_to_remove.contains(&deleted_wallet.checksum) {
                wallets_to_remove.push(deleted_wallet.checksum.clone());
            }
        }

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
            if let Some(manager) = &self.electrum_client_manager {
                if let Err(error) = manager
                    .forget_scripts(scripts_to_forget.into_iter().collect())
                    .await
                {
                    warn!("Failed to clean up deleted Electrum subscriptions: {error}");
                }
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

        // Hard delete from database (if it was marked as deleted)
        if let Err(e) = self
            .metadata_db
            .hard_delete_wallet_by_checksum(checksum)
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
    use crate::wallet::tests::queue_wallet;

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
