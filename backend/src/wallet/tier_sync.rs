//! The cloud sync schedule: every wallet of one subscription tier synced in
//! parallel, with address watches grouped by descriptor.

use super::WalletEntry;
use super::address_watch_sync_targets;
use super::manager::WalletManager;
use super::persistence::persist_wallet;
use crate::config::{AppConfig, NetworkConfig};
use crate::electrum::ElectrumClientManager;
use crate::metadata::{
    MetadataDb, TransactionNotification, WalletMetadata, WalletStatus, WalletType,
};
use crate::subscription::SubscriptionTier;
use crate::sync::{AddressWatchSyncResult, DescriptorWalletSyncResult, WalletSyncService};
use anyhow::{Result, anyhow};
use futures::future::join_all;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, Semaphore, broadcast};
use tracing::{debug, error, info, warn};

/// Maximum number of wallets to sync in parallel
const MAX_PARALLEL_SYNCS: usize = 10;

impl WalletManager {
    /// Sync all wallets for a specific subscription tier in parallel
    pub async fn sync_tier_parallel(&self, tier: SubscriptionTier) -> Result<()> {
        let sync_start = Instant::now();

        // First, perform wallet cleanup (remove deleted wallets from memory and disk)
        self.cleanup_deleted_wallets().await?;

        // Convert Network to NetworkConfig for the query
        let network_config = NetworkConfig::from_network(self.network);

        // Get wallets for this tier from metadata
        let tier_wallets = self
            .metadata_db
            .get_wallets_for_tier_sync(&tier, &network_config)
            .await?;

        self.log_non_syncing_summary().await;

        if tier_wallets.is_empty() {
            debug!("No {:?} tier wallets to sync", tier);
            return Ok(());
        }

        // Partition into BDK wallets and address watches
        let (address_watches, bdk_wallets): (Vec<_>, Vec<_>) = tier_wallets
            .into_iter()
            .partition(|w| w.wallet_type == WalletType::Address);

        if !address_watches.is_empty() {
            self.sync_address_watches(&tier, &address_watches).await;
        }

        let tier_wallets = bdk_wallets;

        if tier_wallets.is_empty() {
            debug!("No {:?} tier BDK wallets to sync", tier);
            return Ok(());
        }

        debug!(
            "🔄 Starting parallel sync for {} {:?} tier wallets (in-memory)",
            tier_wallets.len(),
            tier
        );

        self.load_missing_wallets(&tier_wallets).await;

        // Get wallet references from in-memory storage
        let wallet_refs: Vec<_> = {
            let wallets_map = self.wallets.lock().await;

            tier_wallets
                .iter()
                .filter_map(|metadata| {
                    wallets_map
                        .get(&metadata.checksum)
                        .map(|wallet_arc| (metadata.clone(), wallet_arc.clone()))
                })
                .collect()
        };

        if wallet_refs.is_empty() {
            warn!("No wallets found in memory for sync");
            return Ok(());
        }

        let (success_count, failure_count) = self.sync_descriptor_wallets(wallet_refs).await;

        let total_duration = sync_start.elapsed();
        debug!(
            "🏁 Parallel in-memory sync completed in {:.2}s - Success: {}, Failed: {}",
            total_duration.as_secs_f64(),
            success_count,
            failure_count
        );

        Ok(())
    }

    /// Logs why wallets outside the sync set are not syncing.
    async fn log_non_syncing_summary(&self) {
        if let Ok(non_syncing_summary) = self.metadata_db.get_non_syncing_wallets_summary().await
            && non_syncing_summary.total_non_syncing > 0
        {
            let mut reasons = Vec::new();
            if non_syncing_summary.expired_trials > 0 {
                reasons.push(format!(
                    "{} expired trials",
                    non_syncing_summary.expired_trials
                ));
            }
            if non_syncing_summary.cancelled_subscriptions > 0 {
                reasons.push(format!(
                    "{} cancelled",
                    non_syncing_summary.cancelled_subscriptions
                ));
            }
            if non_syncing_summary.expired_subscriptions > 0 {
                reasons.push(format!(
                    "{} expired",
                    non_syncing_summary.expired_subscriptions
                ));
            }
            if non_syncing_summary.past_due_subscriptions > 0 {
                reasons.push(format!(
                    "{} past_due",
                    non_syncing_summary.past_due_subscriptions
                ));
            }
            if non_syncing_summary.inactive_wallets > 0 {
                reasons.push(format!("{} inactive", non_syncing_summary.inactive_wallets));
            }

            info!(
                "🔒 Subscription status: {} wallets not syncing ({})",
                non_syncing_summary.total_non_syncing,
                reasons.join(", ")
            );
        }
    }

    /// Syncs address watches in parallel, grouped by descriptor to deduplicate
    /// Electrum queries when multiple users watch the same address.
    async fn sync_address_watches(
        &self,
        tier: &SubscriptionTier,
        address_watches: &[WalletMetadata],
    ) {
        // Group address watches by descriptor
        let groups = MetadataDb::get_address_watches_grouped_by_descriptor(address_watches);

        let total_watchers = address_watches.len();
        let unique_descriptors = groups.len();
        debug!(
            "🔍 Syncing {} {:?} tier address watches ({} unique descriptors)",
            total_watchers, tier, unique_descriptors
        );

        let semaphore = Arc::new(Semaphore::new(MAX_PARALLEL_SYNCS));
        let metadata_db = self.metadata_db.clone();
        let notification_sender = self.notification_sender.clone();
        let electrum_manager = self.electrum_client_manager.clone();
        let config = self.config.clone();

        // Spawn one task per unique descriptor (not per watcher)
        let addr_tasks: Vec<_> = groups
            .into_iter()
            .map(|(descriptor, watchers)| {
                let semaphore = semaphore.clone();
                let metadata_db = metadata_db.clone();
                let notification_sender = notification_sender.clone();
                let electrum_manager = electrum_manager.clone();
                let config = config.clone();

                tokio::spawn(async move {
                    let _permit = semaphore
                        .acquire()
                        .await
                        .map_err(|e| anyhow!("Failed to acquire semaphore: {e}"))?;

                    let sync_service =
                        WalletSyncService::new(metadata_db.clone(), notification_sender, config);

                    sync_address_group(
                        &sync_service,
                        &metadata_db,
                        electrum_manager.as_deref(),
                        &descriptor,
                        &watchers,
                    )
                    .await
                })
            })
            .collect();

        let results = join_all(addr_tasks).await;
        let mut addr_success = 0;
        let mut addr_failure = 0;
        for result in results {
            match result {
                Ok(Ok(count)) => addr_success += count,
                _ => addr_failure += 1,
            }
        }
        debug!(
            "🔍 Address watch sync done - Success: {}, Failed: {}",
            addr_success, addr_failure
        );
    }

    /// Ensures all BDK tier wallets are loaded in memory.
    async fn load_missing_wallets(&self, tier_wallets: &[WalletMetadata]) {
        let wallets_map = self.wallets.lock().await;
        let mut missing_wallets = Vec::new();

        for wallet_metadata in tier_wallets {
            if !wallets_map.contains_key(&wallet_metadata.checksum) {
                missing_wallets.push(wallet_metadata.clone());
            }
        }

        drop(wallets_map); // Release lock before loading

        // Load any missing wallets into memory
        if !missing_wallets.is_empty() {
            info!(
                " Loading {} missing wallets into memory",
                missing_wallets.len()
            );
            for wallet_metadata in missing_wallets {
                let wallet_path = self
                    .wallet_dir
                    .join(format!("{}.sqlite", wallet_metadata.checksum));
                if wallet_path.exists() {
                    match Self::load_wallet_from_disk(
                        &wallet_path,
                        &wallet_metadata.descriptor,
                        self.network,
                    )
                    .await
                    {
                        Ok((wallet, conn)) => {
                            let mut wallets_map = self.wallets.lock().await;
                            wallets_map.insert(
                                wallet_metadata.checksum.clone(),
                                Arc::new(Mutex::new((wallet, conn))),
                            );
                            debug!(
                                " Loaded wallet: {} ({})",
                                wallet_metadata.name, wallet_metadata.checksum
                            );
                        }
                        Err(e) => {
                            error!(
                                "Failed to load wallet {} into memory: {}",
                                wallet_metadata.name, e
                            );
                        }
                    }
                } else {
                    error!("Wallet file not found: {}", wallet_path.display());
                }
            }
        }
    }

    /// Syncs the loaded descriptor wallets in parallel and returns the success
    /// and failure counts.
    async fn sync_descriptor_wallets(
        &self,
        wallet_refs: Vec<(WalletMetadata, WalletEntry)>,
    ) -> (usize, usize) {
        // Create semaphore to limit concurrent syncs
        let semaphore = Arc::new(Semaphore::new(MAX_PARALLEL_SYNCS));

        // Prepare shared resources
        let metadata_db = self.metadata_db.clone();
        let notification_sender = self.notification_sender.clone();
        let electrum_manager = self.electrum_client_manager.clone();
        let config = self.config.clone();

        // Create parallel sync tasks using in-memory wallets
        let sync_tasks: Vec<_> = wallet_refs
            .into_iter()
            .map(|(wallet_metadata, wallet_arc)| {
                let semaphore = semaphore.clone();
                let metadata_db = metadata_db.clone();
                let notification_sender = notification_sender.clone();
                let electrum_manager = electrum_manager.clone();
                let config = config.clone();

                tokio::spawn(async move {
                    // Acquire semaphore permit
                    let _permit = semaphore
                        .acquire()
                        .await
                        .map_err(|e| anyhow!("Failed to acquire semaphore: {e}"))?;

                    sync_loaded_wallet(
                        metadata_db,
                        notification_sender,
                        config,
                        electrum_manager.as_deref(),
                        wallet_metadata,
                        wallet_arc,
                    )
                    .await
                })
            })
            .collect();

        // Wait for all sync tasks to complete
        let results = join_all(sync_tasks).await;

        // Count successes and failures
        let mut success_count = 0;
        let mut failure_count = 0;

        for result in results {
            match result {
                Ok(Ok(_)) => success_count += 1,
                _ => failure_count += 1,
            }
        }

        (success_count, failure_count)
    }
}

/// Syncs one address-watch descriptor for every watcher of it and returns the
/// number of watchers covered.
async fn sync_address_group(
    sync_service: &WalletSyncService,
    metadata_db: &MetadataDb,
    electrum_manager: Option<&ElectrumClientManager>,
    descriptor: &str,
    watchers: &[WalletMetadata],
) -> Result<usize> {
    let watcher_count = watchers.len();
    let targets = address_watch_sync_targets(watchers);

    if watcher_count == 1 {
        let (checksum, is_pending) = &targets[0];
        // Single watcher — suppress notifications on first sync to avoid
        // alerting on historical transactions
        match sync_service
            .sync_address_watch(
                checksum,
                descriptor,
                electrum_manager,
                *is_pending, // suppress on first sync for pending wallets
            )
            .await
        {
            Ok(AddressWatchSyncResult::Completed { .. }) => {
                // Promote pending watcher to ready
                if *is_pending
                    && let Err(e) = metadata_db
                        .update_wallet_status_if_not_deleted(checksum, WalletStatus::Ready)
                        .await
                {
                    warn!(
                        "Failed to promote address watch {} to ready: {}",
                        checksum, e
                    );
                }
                Ok(watcher_count)
            }
            Ok(AddressWatchSyncResult::SkippedNoClient) => Ok(watcher_count),
            Err(e) => {
                warn!("Failed to sync address watch {}: {}", checksum, e);
                Err(e)
            }
        }
    } else {
        // Multiple watchers share Electrum queries, but notification
        // suppression remains specific to each pending watcher.
        match sync_service
            .sync_address_watch_group(&targets, descriptor, electrum_manager)
            .await
        {
            Ok(AddressWatchSyncResult::Completed { .. }) => {
                // Promote any pending watchers to ready
                for w in watchers {
                    if w.status == WalletStatus::Pending
                        && let Err(e) = metadata_db
                            .update_wallet_status_if_not_deleted(&w.checksum, WalletStatus::Ready)
                            .await
                    {
                        warn!(
                            "Failed to promote address watch {} to ready: {}",
                            w.checksum, e
                        );
                    }
                }
                Ok(watcher_count)
            }
            Ok(AddressWatchSyncResult::SkippedNoClient) => Ok(watcher_count),
            Err(e) => {
                warn!("Failed to sync address watch group ({}): {}", descriptor, e);
                Err(e)
            }
        }
    }
}

/// Syncs one loaded descriptor wallet, persists it and promotes it to ready
/// after its first sync. Returns the wallet name.
async fn sync_loaded_wallet(
    metadata_db: MetadataDb,
    notification_sender: broadcast::Sender<TransactionNotification>,
    config: AppConfig,
    electrum_manager: Option<&ElectrumClientManager>,
    wallet_metadata: WalletMetadata,
    wallet_arc: WalletEntry,
) -> Result<String> {
    let wallet_start = Instant::now();

    // Lock the specific wallet for sync
    let mut wallet_data = wallet_arc.lock_owned().await;

    // Create sync service with config for mode-based retry logic
    let metadata_db_ref = metadata_db.clone();
    let sync_service = WalletSyncService::new(metadata_db, notification_sender, config);

    // Perform sync
    let outcome = sync_service
        .sync_wallet_by_checksum(
            &mut wallet_data.0,
            &wallet_metadata.checksum,
            electrum_manager,
        )
        .await;
    match outcome {
        Ok(DescriptorWalletSyncResult::Completed) => {
            // A successful Electrum sync can stage BDK state even when Canary's
            // transaction reconciliation found no app-level changes. Hold the
            // wallet lock through the status update, so another sync of this
            // wallet cannot start before it is marked ready.
            let _wallet_data = persist_wallet(wallet_data).await.map_err(|e| {
                anyhow!(
                    "Failed to persist wallet {} after sync: {}",
                    wallet_metadata.name,
                    e
                )
            })?;

            // Promote pending wallet to ready after first successful sync
            if wallet_metadata.status == WalletStatus::Pending
                && let Err(e) = metadata_db_ref
                    .update_wallet_status_if_not_deleted(
                        &wallet_metadata.checksum,
                        WalletStatus::Ready,
                    )
                    .await
            {
                warn!(
                    "Failed to promote wallet {} to ready: {}",
                    wallet_metadata.name, e
                );
            }

            let sync_duration = wallet_start.elapsed();
            debug!(
                "✅ Synced wallet {} in {:.2}s (from memory)",
                wallet_metadata.name,
                sync_duration.as_secs_f64()
            );
            Ok(wallet_metadata.name)
        }
        Ok(DescriptorWalletSyncResult::SkippedNoClient) => {
            // No Electrum client available, skip
            Ok(wallet_metadata.name)
        }
        Err(e) => {
            warn!("Failed to sync wallet {}: {}", wallet_metadata.name, e);
            Err(e)
        }
    }
}
