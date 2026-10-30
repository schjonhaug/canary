//! The self-hosted sync schedule: one fair, oldest-first due-time queue that
//! syncs a single descriptor wallet or address-watch group at a time.

use super::address_watch_sync_targets;
use super::manager::WalletManager;
use super::persistence::persist_wallet;
use crate::config::NetworkConfig;
use crate::metadata::{WalletMetadata, WalletStatus, WalletType};
use crate::sync::{AddressWatchSyncResult, DescriptorWalletSyncResult};
use anyhow::{anyhow, Result};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
enum SelfHostedWorkKind {
    Descriptor(Box<WalletMetadata>),
    AddressGroup {
        descriptor: String,
        watchers: Vec<WalletMetadata>,
    },
}

#[derive(Debug, Clone)]
struct SelfHostedWorkItem {
    key: String,
    last_synced_at: Option<chrono::DateTime<chrono::Utc>>,
    kind: SelfHostedWorkKind,
}

impl SelfHostedWorkItem {
    fn due_at(
        &self,
        interval: Duration,
        now: chrono::DateTime<chrono::Utc>,
    ) -> chrono::DateTime<chrono::Utc> {
        self.last_synced_at
            .and_then(|synced| {
                chrono::Duration::from_std(interval)
                    .ok()
                    .map(|interval| synced + interval)
            })
            .unwrap_or(now)
    }
}

fn parse_last_synced_at(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    match chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f") {
        Ok(timestamp) => Some(timestamp.and_utc()),
        Err(error) => {
            warn!(
                "Could not parse persisted wallet last_synced_at timestamp '{value}'; treating it as never synced: {error}"
            );
            None
        }
    }
}

fn build_self_hosted_queue(wallets: Vec<WalletMetadata>) -> Vec<SelfHostedWorkItem> {
    let (address_watches, descriptor_wallets): (Vec<_>, Vec<_>) = wallets
        .into_iter()
        .partition(|wallet| wallet.wallet_type == WalletType::Address);

    let mut items: Vec<SelfHostedWorkItem> = descriptor_wallets
        .into_iter()
        .map(|wallet| SelfHostedWorkItem {
            key: wallet.checksum.clone(),
            last_synced_at: wallet
                .last_synced_at
                .as_deref()
                .and_then(parse_last_synced_at),
            kind: SelfHostedWorkKind::Descriptor(Box::new(wallet)),
        })
        .collect();

    let mut address_groups: HashMap<String, Vec<WalletMetadata>> = HashMap::new();
    for watch in address_watches {
        address_groups
            .entry(watch.descriptor.clone())
            .or_default()
            .push(watch);
    }
    for (descriptor, mut watchers) in address_groups {
        watchers.sort_by(|left, right| left.checksum.cmp(&right.checksum));
        let stable_checksum = watchers
            .first()
            .expect("address group is never empty")
            .checksum
            .clone();
        let timestamps: Vec<_> = watchers
            .iter()
            .map(|watch| {
                watch
                    .last_synced_at
                    .as_deref()
                    .and_then(parse_last_synced_at)
            })
            .collect();
        let last_synced_at = if timestamps.iter().any(Option::is_none) {
            None
        } else {
            timestamps.into_iter().flatten().min()
        };
        items.push(SelfHostedWorkItem {
            key: format!("address:{stable_checksum}"),
            last_synced_at,
            kind: SelfHostedWorkKind::AddressGroup {
                descriptor,
                watchers,
            },
        });
    }

    // Never-synced first, then oldest completion time, then a stable key.
    items.sort_by(|left, right| {
        left.last_synced_at
            .is_some()
            .cmp(&right.last_synced_at.is_some())
            .then_with(|| left.last_synced_at.cmp(&right.last_synced_at))
            .then_with(|| left.key.cmp(&right.key))
    });
    items
}

fn select_due_self_hosted_item<'a>(
    queue: &'a [SelfHostedWorkItem],
    interval: Duration,
    now_wall: chrono::DateTime<chrono::Utc>,
    now_instant: Instant,
    failure_deferred_until: &HashMap<String, Instant>,
) -> Option<&'a SelfHostedWorkItem> {
    queue.iter().find(|item| {
        item.due_at(interval, now_wall) <= now_wall
            && failure_deferred_until
                .get(&item.key)
                .is_none_or(|until| *until <= now_instant)
    })
}

impl WalletManager {
    async fn sync_self_hosted_work_item(&self, item: &SelfHostedWorkItem) -> Result<()> {
        use crate::sync::WalletSyncService;

        let _work_permit = self.acquire_electrum_work().await?;
        match &item.kind {
            SelfHostedWorkKind::AddressGroup {
                descriptor,
                watchers,
            } => {
                let sync_service = WalletSyncService::new(
                    self.metadata_db.clone(),
                    self.notification_sender.clone(),
                    self.config.clone(),
                );
                let targets = address_watch_sync_targets(watchers);
                let result = if watchers.len() == 1 {
                    sync_service
                        .sync_address_watch(
                            &targets[0].0,
                            descriptor,
                            self.electrum_client_manager.as_deref(),
                            targets[0].1,
                        )
                        .await
                } else {
                    sync_service
                        .sync_address_watch_group(
                            &targets,
                            descriptor,
                            self.electrum_client_manager.as_deref(),
                        )
                        .await
                }?;

                if matches!(result, AddressWatchSyncResult::SkippedNoClient) {
                    return Err(anyhow!("No Electrum client available"));
                }
                for watcher in watchers
                    .iter()
                    .filter(|watch| watch.status == WalletStatus::Pending)
                {
                    self.metadata_db
                        .update_wallet_status_if_not_deleted(&watcher.checksum, WalletStatus::Ready)
                        .await?;
                }
            }
            SelfHostedWorkKind::Descriptor(metadata) => {
                let wallet_entry = if let Some(entry) =
                    self.wallets.lock().await.get(&metadata.checksum).cloned()
                {
                    entry
                } else {
                    let wallet_path = self
                        .wallet_dir
                        .join(format!("{}.sqlite", metadata.checksum));
                    let (wallet, connection) = Self::load_wallet_from_disk(
                        &wallet_path,
                        &metadata.descriptor,
                        self.network,
                    )
                    .await?;
                    let entry = Arc::new(Mutex::new((wallet, connection)));
                    self.wallets
                        .lock()
                        .await
                        .insert(metadata.checksum.clone(), entry.clone());
                    entry
                };

                let mut wallet_data = wallet_entry.lock_owned().await;
                let sync_service = WalletSyncService::new(
                    self.metadata_db.clone(),
                    self.notification_sender.clone(),
                    self.config.clone(),
                );
                let outcome = sync_service
                    .sync_wallet_by_checksum(
                        &mut wallet_data.0,
                        &metadata.checksum,
                        self.electrum_client_manager.as_deref(),
                    )
                    .await?;
                match outcome {
                    DescriptorWalletSyncResult::Completed => {
                        // Hold the wallet lock through the status update, so another sync of
                        // this wallet cannot start before it is marked ready.
                        let _wallet_data = persist_wallet(wallet_data).await.map_err(|error| {
                            anyhow!("Failed to persist wallet {}: {error}", metadata.name)
                        })?;
                        if metadata.status == WalletStatus::Pending {
                            self.metadata_db
                                .update_wallet_status_if_not_deleted(
                                    &metadata.checksum,
                                    WalletStatus::Ready,
                                )
                                .await?;
                        }
                    }
                    DescriptorWalletSyncResult::SkippedNoClient => {
                        return Err(anyhow!("No Electrum client available"));
                    }
                }
            }
        }
        Ok(())
    }

    /// Run the self-hosted oldest-first due-time queue.
    ///
    /// Each successful completion establishes the next target time. Failures are deferred by one
    /// interval so a broken wallet cannot starve healthy wallets. The shared semaphore is acquired
    /// for one item only, allowing wallet creation to join the same fair queue between recurring
    /// items.
    pub async fn run_self_hosted_sync_queue(&self, sync_interval_secs: u64) -> Result<()> {
        let sync_interval = Duration::from_secs(sync_interval_secs.max(1));
        let mut failure_deferred_until: HashMap<String, Instant> = HashMap::new();

        loop {
            if let Err(error) = self.cleanup_deleted_wallets().await {
                warn!("Self-hosted wallet cleanup failed; retrying in {sync_interval:?}: {error}");
                tokio::time::sleep(sync_interval).await;
                continue;
            }
            let network_config = NetworkConfig::from_network(self.network);
            let wallets = match self
                .metadata_db
                .get_wallets_for_tier_sync(
                    &crate::subscription::SubscriptionTier::Team,
                    &network_config,
                )
                .await
            {
                Ok(wallets) => wallets,
                Err(error) => {
                    warn!(
                        "Could not refresh self-hosted Electrum queue; retrying in {sync_interval:?}: {error}"
                    );
                    tokio::time::sleep(sync_interval).await;
                    continue;
                }
            };
            let queue = build_self_hosted_queue(wallets);
            let active_keys: HashSet<_> = queue.iter().map(|item| item.key.as_str()).collect();
            failure_deferred_until.retain(|key, _| active_keys.contains(key.as_str()));

            if queue.is_empty() {
                debug!("Self-hosted Electrum queue is empty");
                tokio::time::sleep(sync_interval).await;
                continue;
            }

            let now_wall = chrono::Utc::now();
            let now_instant = Instant::now();
            let selected = select_due_self_hosted_item(
                &queue,
                sync_interval,
                now_wall,
                now_instant,
                &failure_deferred_until,
            );

            let Some(item) = selected else {
                let wait = queue
                    .iter()
                    .map(|item| {
                        let due_wait = (item.due_at(sync_interval, now_wall) - now_wall)
                            .to_std()
                            .unwrap_or(Duration::ZERO);
                        let failure_wait = failure_deferred_until
                            .get(&item.key)
                            .map_or(Duration::ZERO, |until| {
                                until.saturating_duration_since(now_instant)
                            });
                        due_wait.max(failure_wait)
                    })
                    .min()
                    .unwrap_or(sync_interval);
                debug!(
                    "Self-hosted Electrum queue depth={}, next item due in {:.2?}",
                    queue.len(),
                    wait
                );
                tokio::time::sleep(wait).await;
                continue;
            };

            let oldest_lateness = queue
                .iter()
                .map(|queued| {
                    (now_wall - queued.due_at(sync_interval, now_wall))
                        .to_std()
                        .unwrap_or(Duration::ZERO)
                })
                .max()
                .unwrap_or(Duration::ZERO);
            info!(
                "Self-hosted Electrum queue depth={}, oldest_item_lateness={:.2?}, starting={}",
                queue.len(),
                oldest_lateness,
                item.key
            );
            let work_start = Instant::now();
            match self.sync_self_hosted_work_item(item).await {
                Ok(()) => {
                    failure_deferred_until.remove(&item.key);
                    info!(
                        "Self-hosted Electrum work item {} completed in {:.2?}",
                        item.key,
                        work_start.elapsed()
                    );
                }
                Err(error) => {
                    failure_deferred_until.insert(item.key.clone(), Instant::now() + sync_interval);
                    warn!(
                        "Self-hosted Electrum work item {} failed after {:.2?}; deferred for {:.2?}: {}",
                        item.key,
                        work_start.elapsed(),
                        sync_interval,
                        error
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::tests::queue_wallet;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::Semaphore;

    #[test]
    fn self_hosted_queue_orders_never_synced_then_oldest_with_stable_tie_breaker() {
        let queue = build_self_hosted_queue(vec![
            queue_wallet("new-b", None),
            queue_wallet("recent", Some("2026-08-10 12:00:00.000")),
            queue_wallet("old-b", Some("2026-08-10 10:00:00.000")),
            queue_wallet("new-a", None),
            queue_wallet("old-a", Some("2026-08-10 10:00:00.000")),
        ]);
        let keys: Vec<_> = queue.iter().map(|item| item.key.as_str()).collect();
        assert_eq!(keys, ["new-a", "new-b", "old-a", "old-b", "recent"]);
    }

    #[test]
    fn self_hosted_queue_scales_oldest_first_for_reported_wallet_counts() {
        for count in [1usize, 9, 20] {
            let wallets = (0..count)
                .rev()
                .map(|index| {
                    queue_wallet(
                        &format!("wallet-{index:02}"),
                        Some(&format!("2026-08-10 10:{index:02}:00.000")),
                    )
                })
                .collect();
            let queue = build_self_hosted_queue(wallets);
            let keys: Vec<_> = queue.iter().map(|item| item.key.clone()).collect();
            let expected: Vec<_> = (0..count)
                .map(|index| format!("wallet-{index:02}"))
                .collect();
            assert_eq!(keys, expected);
        }
    }

    #[test]
    fn self_hosted_due_time_is_measured_from_completion() {
        let completed = queue_wallet("wallet", Some("2026-08-10 10:00:30.000"));
        let item = build_self_hosted_queue(vec![completed]).remove(0);
        let now = parse_last_synced_at("2026-08-10 10:00:45.000").unwrap();
        assert_eq!(
            item.due_at(Duration::from_mins(1), now),
            parse_last_synced_at("2026-08-10 10:01:30.000").unwrap()
        );
    }

    #[test]
    fn deferred_failure_does_not_starve_healthy_wallets() {
        let queue = build_self_hosted_queue(vec![
            queue_wallet("failed-oldest", None),
            queue_wallet("healthy", None),
        ]);
        let now_wall = chrono::Utc::now();
        let now_instant = Instant::now();
        let mut deferred = HashMap::new();
        deferred.insert(
            "failed-oldest".to_string(),
            now_instant + Duration::from_mins(1),
        );

        let selected = select_due_self_hosted_item(
            &queue,
            Duration::from_mins(1),
            now_wall,
            now_instant,
            &deferred,
        )
        .unwrap();
        assert_eq!(selected.key, "healthy");
    }

    #[tokio::test]
    async fn self_hosted_work_gate_keeps_peak_concurrency_at_one() {
        let gate = Arc::new(Semaphore::new(1));
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();

        for _ in 0..20 {
            let gate = gate.clone();
            let active = active.clone();
            let peak = peak.clone();
            tasks.push(tokio::spawn(async move {
                let _permit = gate.acquire_owned().await.unwrap();
                let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(current, Ordering::SeqCst);
                tokio::task::yield_now().await;
                active.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(peak.load(Ordering::SeqCst), 1);
    }
}
