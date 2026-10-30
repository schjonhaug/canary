//! Long-running background tasks started by [`super::run`].

use crate::config::AppConfig;
use crate::electrum::ElectrumClientManager;
use crate::metadata::{MetadataDb, NonSyncingWalletsSummary};
use crate::subscription::SubscriptionTier;
use crate::wallet::WalletManager;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{interval, sleep, timeout};
use tracing::{error, info, warn};

const BLOCK_HEIGHT_TIMEOUT: Duration = Duration::from_secs(5);
const SESSION_CLEANUP_INTERVAL: Duration = Duration::from_hours(1);

/// Starts wallet synchronization.
///
/// Self-hosted installs run a single queue that spreads syncs over the
/// configured interval. Cloud installs sync each subscription tier on its own
/// schedule.
pub fn spawn_wallet_sync(config: &AppConfig, wallet_manager: &Arc<WalletManager>) {
    if config.is_self_hosted_mode() {
        let sync_interval = config.get_sync_interval();
        info!("Self-hosted per-wallet sync target: {sync_interval}s");
        let wallet_manager = Arc::clone(wallet_manager);
        tokio::spawn(async move {
            if let Err(e) = wallet_manager
                .run_self_hosted_sync_queue(sync_interval)
                .await
            {
                error!("Self-hosted sync queue stopped: {e}");
            }
        });
        return;
    }

    let (personal_interval, team_interval) = SubscriptionTier::get_sync_intervals(&config.network);
    for (tier, interval_secs) in [
        (SubscriptionTier::Team, team_interval),
        (SubscriptionTier::Personal, personal_interval),
    ] {
        info!("{tier:?} tier sync interval: {interval_secs}s");
        let wallet_manager = Arc::clone(wallet_manager);
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(interval_secs));
            loop {
                ticker.tick().await;
                if let Err(e) = wallet_manager.sync_tier_parallel(tier).await {
                    error!("{tier:?} tier sync failed: {e}");
                }
            }
        });
    }

    let metadata_db = wallet_manager.metadata_db.clone();
    tokio::spawn(async move {
        // Give the sync tasks a moment to start before reporting.
        sleep(Duration::from_secs(2)).await;
        if let Ok(summary) = metadata_db.get_non_syncing_wallets_summary().await {
            log_non_syncing_wallets(&summary);
        }
    });
}

fn log_non_syncing_wallets(summary: &NonSyncingWalletsSummary) {
    if summary.total_non_syncing == 0 {
        info!("All wallets have active subscriptions");
        return;
    }

    let reasons: Vec<String> = [
        (summary.expired_trials, "expired trials"),
        (summary.cancelled_subscriptions, "cancelled"),
        (summary.expired_subscriptions, "expired"),
        (summary.past_due_subscriptions, "past_due"),
        (summary.inactive_wallets, "inactive (tier limits)"),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, reason)| format!("{count} {reason}"))
    .collect();

    info!(
        "Startup subscription status: {} wallets not syncing ({})",
        summary.total_non_syncing,
        reasons.join(", ")
    );
}

/// Keeps the stored chain tip up to date.
///
/// Runs on the same cadence as wallet sync (the Team interval in cloud mode).
/// The first check happens immediately and always stores the server's tip.
pub async fn spawn_block_header_sync(config: &AppConfig, wallet_manager: &Arc<WalletManager>) {
    let Some(electrum) = wallet_manager.get_electrum_manager() else {
        warn!("No Electrum client manager available; block headers will not be tracked");
        return;
    };
    let interval_secs = if config.is_self_hosted_mode() {
        config.get_sync_interval()
    } else {
        SubscriptionTier::get_sync_intervals(&config.network).1
    };
    let metadata_db = wallet_manager.metadata_db.clone();
    let mut stored_height = metadata_db
        .get_current_block_header()
        .await
        .ok()
        .flatten()
        .map(|header| header.height);

    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(interval_secs));
        let mut force = true;
        loop {
            ticker.tick().await;
            if let Some(height) =
                refresh_block_header(&electrum, &metadata_db, stored_height, force).await
            {
                stored_height = Some(height);
                force = false;
            }
        }
    });
}

/// Fetches the chain tip and stores it when it is newer than `stored_height`
/// (or unconditionally when `force` is set). Returns the stored height.
async fn refresh_block_header(
    electrum: &ElectrumClientManager,
    metadata_db: &MetadataDb,
    stored_height: Option<u32>,
    force: bool,
) -> Option<u32> {
    let client = electrum.client_or_reconnect().await?;

    let height = match timeout(BLOCK_HEIGHT_TIMEOUT, client.get_current_block_height()).await {
        Ok(Ok(height)) => height,
        Ok(Err(e)) => {
            warn!("Electrum error checking block height: {e}");
            electrum.reconnect_after_transport_error(&e).await;
            return None;
        }
        Err(_) => {
            warn!("Timed out checking block height; reconnecting to Electrum");
            electrum
                .mark_disconnected("Block height check timed out")
                .await;
            let _ = electrum.reconnect().await;
            return None;
        }
    };

    if !force && stored_height.is_some_and(|stored| height <= stored) {
        return None;
    }

    match client.get_block_header(height).await {
        Ok(header) => {
            info!("New block header: height={}", header.height);
            if let Err(e) = metadata_db.upsert_current_block_header(&header).await {
                error!("Failed to store block header: {e}");
            }
            Some(header.height)
        }
        Err(e) => {
            warn!("Failed to get block header for height {height}: {e}");
            electrum.reconnect_after_transport_error(&e).await;
            None
        }
    }
}

/// Deletes expired login sessions every hour.
pub fn spawn_session_cleanup(wallet_manager: &Arc<WalletManager>) {
    let metadata_db = wallet_manager.metadata_db.clone();
    tokio::spawn(async move {
        let mut ticker = interval(SESSION_CLEANUP_INTERVAL);
        loop {
            ticker.tick().await;
            match metadata_db.cleanup_expired_sessions().await {
                Ok(0) => {}
                Ok(deleted) => info!("Cleaned up {deleted} expired sessions"),
                Err(e) => error!("Failed to clean up expired sessions: {e}"),
            }
        }
    });
}

/// Re-applies subscription limits for every account (cloud mode), so wallets
/// of expired or downgraded subscriptions stop syncing after a restart.
pub fn spawn_startup_subscription_limits(wallet_manager: &Arc<WalletManager>) {
    let wallet_manager = Arc::clone(wallet_manager);
    tokio::spawn(async move {
        info!("Applying subscription limits for all accounts");
        let users = match wallet_manager.metadata_db.get_all_users().await {
            Ok(users) => users,
            Err(e) => {
                error!("Failed to apply startup subscription limits: {e}");
                return;
            }
        };

        for user in users {
            if wallet_manager
                .apply_subscription_limits(
                    &user.id,
                    user.subscription_tier.as_str(),
                    &user.subscription_status,
                    user.is_admin,
                    user.trial_ends_at.clone(),
                    user.subscription_ends_at.clone(),
                )
                .await
                .is_err()
            {
                // The error can contain account details, so it is not logged.
                error!("Failed to apply subscription limits for an account");
            }
        }
        info!("Finished applying subscription limits");
    });
}
