//! Descriptor-wallet sync: the Electrum retry and reconnection loop around BDK's
//! sync, followed by the balance, transaction and balance-alert updates.

use super::{DescriptorWalletSyncResult, WalletSyncService};
use crate::admin_notifications::AdminNotifications;
use crate::electrum::ElectrumClientManager;
use crate::utils::amount::sats_to_i64;
use anyhow::{anyhow, Result};
use bdk_wallet::{rusqlite::Connection, PersistedWallet};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Number of consecutive reconnection failures before sending an alert
const ALERT_FAILURE_THRESHOLD: u32 = 3;

impl WalletSyncService {
    /// Sync a single wallet using transaction-based approach
    pub(crate) async fn sync_wallet_by_checksum(
        &self,
        wallet: &mut PersistedWallet<Connection>,
        wallet_checksum: &str,
        electrum_manager: Option<&ElectrumClientManager>,
    ) -> Result<DescriptorWalletSyncResult> {
        let sync_start = Instant::now();
        debug!("[{}] Starting descriptor-based sync", wallet_checksum);
        let mut electrum_duration = Duration::ZERO;
        let mut electrum_attempts: u32 = 0;

        // Perform the actual sync with Electrum with mode-based retry logic. When no client
        // manager is configured, preserve the local transaction reconciliation that follows.
        let sync_result = if let Some(manager) = electrum_manager {
            let max_retries: u32 = if self.config.is_cloud_mode() { 3 } else { 1 };
            let use_exponential_backoff = self.config.is_cloud_mode();

            let mut last_error: Option<anyhow::Error> = None;
            let mut sync_completed = false;

            for attempt in 1..=max_retries {
                // Get fresh client from manager each attempt (may have reconnected)
                let client = if let Some(c) = manager.get_client().await {
                    c
                } else {
                    warn!(
                        "[{}] No Electrum client available (attempt {}), triggering reconnection",
                        wallet_checksum, attempt
                    );
                    match manager.reconnect().await {
                        Ok(true) => {
                            info!("[{}] Reconnection successful", wallet_checksum);
                            // Send reconnected notification if we had previous failures (atomic check)
                            if manager.should_send_reconnected_notification() {
                                let admin_notifications =
                                    AdminNotifications::from_config(&self.config);
                                if admin_notifications.is_enabled() {
                                    admin_notifications
                                        .notify_electrum_reconnected(manager.url())
                                        .await;
                                }
                            }
                            if let Some(c) = manager.get_client().await {
                                c
                            } else {
                                error!(
                                    "[{}] Client still unavailable after reconnection",
                                    wallet_checksum
                                );
                                last_error =
                                    Some(anyhow!("Electrum client unavailable after reconnection"));
                                continue;
                            }
                        }
                        Ok(false) => {
                            debug!(
                                "[{}] Reconnection in progress by another task, waiting...",
                                wallet_checksum
                            );
                            last_error = Some(anyhow!(
                                "Electrum client unavailable while reconnection is in progress"
                            ));
                            sleep(Duration::from_secs(2)).await;
                            continue;
                        }
                        Err(e) => {
                            error!("[{}] Reconnection failed: {}", wallet_checksum, e);
                            last_error = Some(anyhow!("Electrum reconnection failed: {e}"));
                            // Check if we should send an alert
                            let failures = manager.get_consecutive_failures();
                            if failures >= ALERT_FAILURE_THRESHOLD && !manager.has_alert_been_sent()
                            {
                                let admin_notifications =
                                    AdminNotifications::from_config(&self.config);
                                if admin_notifications.is_enabled() {
                                    admin_notifications
                                        .notify_electrum_disconnect(
                                            manager.url(),
                                            failures,
                                            Some(&e.to_string()),
                                        )
                                        .await;
                                    manager.mark_alert_sent();
                                    warn!(
                                        "[{}] Sent admin alert for {} consecutive Electrum failures",
                                        wallet_checksum, failures
                                    );
                                }
                            }
                            continue;
                        }
                    }
                };

                let attempt_start = Instant::now();
                let result = client.sync_wallet(wallet).await;
                let attempt_elapsed = attempt_start.elapsed();
                electrum_duration += attempt_elapsed;
                electrum_attempts = attempt;

                match result {
                    Ok(()) => {
                        debug!(
                            "[{}] Electrum sync attempt {} succeeded in {:.2?}",
                            wallet_checksum, attempt, attempt_elapsed
                        );
                        if attempt > 1 {
                            debug!(
                                "[{}] Sync succeeded on attempt {}/{}",
                                wallet_checksum, attempt, max_retries
                            );
                        }
                        last_error = None;
                        sync_completed = true;
                        break;
                    }
                    Err(e) => {
                        let error_message = e.to_string();
                        let error_type = e.kind();

                        warn!(
                            "[{}] Electrum sync attempt {} failed in {:.2?} ({}): {}",
                            wallet_checksum, attempt, attempt_elapsed, error_type, error_message
                        );
                        last_error = Some(e.into());

                        if error_type.requires_reconnect() {
                            warn!(
                                "[{}] {} error detected, triggering reconnection",
                                wallet_checksum, error_type
                            );
                            manager.mark_disconnected(&error_message).await;

                            match manager.reconnect().await {
                                Ok(true) => {
                                    info!(
                                        "[{}] Reconnection successful, will retry sync",
                                        wallet_checksum
                                    );
                                    // Send reconnected notification if we had previous failures (atomic check)
                                    if manager.should_send_reconnected_notification() {
                                        let admin_notifications =
                                            AdminNotifications::from_config(&self.config);
                                        if admin_notifications.is_enabled() {
                                            admin_notifications
                                                .notify_electrum_reconnected(manager.url())
                                                .await;
                                        }
                                    }
                                }
                                Ok(false) => {
                                    debug!(
                                        "[{}] Reconnection already in progress by another task",
                                        wallet_checksum
                                    );
                                }
                                Err(reconnect_err) => {
                                    error!(
                                        "[{}] Reconnection failed: {}",
                                        wallet_checksum, reconnect_err
                                    );
                                    // Check if we should send an alert
                                    let failures = manager.get_consecutive_failures();
                                    if failures >= ALERT_FAILURE_THRESHOLD
                                        && !manager.has_alert_been_sent()
                                    {
                                        let admin_notifications =
                                            AdminNotifications::from_config(&self.config);
                                        if admin_notifications.is_enabled() {
                                            admin_notifications
                                                .notify_electrum_disconnect(
                                                    manager.url(),
                                                    failures,
                                                    Some(&reconnect_err.to_string()),
                                                )
                                                .await;
                                            manager.mark_alert_sent();
                                            warn!(
                                                "[{}] Sent admin alert for {} consecutive Electrum failures",
                                                wallet_checksum, failures
                                            );
                                        }
                                    }
                                }
                            }
                        }

                        // Apply backoff delay before retry (cloud mode only)
                        if self.config.is_cloud_mode() && attempt < max_retries {
                            let delay_secs = if use_exponential_backoff {
                                // Exponential backoff: 5s, 10s, 20s
                                5 * (2u64.pow(attempt - 1))
                            } else {
                                0
                            };

                            warn!(
                                "[{}] Sync attempt {}/{} failed ({}), retrying in {}s: {}",
                                wallet_checksum,
                                attempt,
                                max_retries,
                                error_type,
                                delay_secs,
                                error_message
                            );

                            if delay_secs > 0 {
                                sleep(Duration::from_secs(delay_secs)).await;
                            }
                        } else if attempt < max_retries {
                            // Self-hosted mode - simple retry without categorization
                            warn!(
                                "[{}] Sync attempt {}/{} failed: {}",
                                wallet_checksum, attempt, max_retries, error_message
                            );
                        }
                    }
                }
            }

            // A configured client that could not complete synchronization is a real failure,
            // distinct from an installation with no client manager at all.
            if !sync_completed {
                let error = last_error.unwrap_or_else(|| anyhow!("Electrum sync did not complete"));
                if self.config.is_cloud_mode() {
                    error!(
                        "[{}] Failed to sync with Electrum after {} attempts: {}",
                        wallet_checksum, max_retries, error
                    );
                } else {
                    error!(
                        "[{}] Failed to sync with Electrum: {}",
                        wallet_checksum, error
                    );
                }
                return Err(error);
            }

            DescriptorWalletSyncResult::Completed
        } else {
            debug!(
                "[{}] Electrum sync skipped because no client manager is configured; reconciling local wallet state",
                wallet_checksum
            );
            DescriptorWalletSyncResult::SkippedNoClient
        };

        // Update last_synced_at timestamp
        let last_synced_start = Instant::now();
        let _ = self
            .metadata_db
            .update_wallet_last_synced(wallet_checksum)
            .await;
        debug!(
            "[{}] Metadata last_synced_at update took {:.2?}",
            wallet_checksum,
            last_synced_start.elapsed()
        );

        // Update wallet balance before transaction processing so notification
        // handlers reload the current balance when formatting messages.
        let balance_update_start = Instant::now();
        let current_balance = wallet.balance().total();
        self.metadata_db
            .update_wallet_balance_by_checksum(
                wallet_checksum,
                sats_to_i64(current_balance.to_sat()),
            )
            .await?;
        debug!(
            "[{}] Wallet balance metadata update took {:.2?}",
            wallet_checksum,
            balance_update_start.elapsed()
        );

        // Process all transactions and detect changes
        let tx_process_start = Instant::now();
        // Get the current client for transaction processing (may be None if disconnected)
        let electrum_client = match electrum_manager {
            Some(manager) => manager.get_client().await,
            None => None,
        };
        let summary = self
            .process_wallet_transactions(wallet, wallet_checksum, electrum_client.as_ref())
            .await?;
        debug!(
            "[{}] Transaction processing took {:.2?}",
            wallet_checksum,
            tx_process_start.elapsed()
        );

        // Check balance alerts on every sync (for both BTC and fiat alerts)
        let balance_alert_start = Instant::now();
        if let Err(e) = self
            .check_balance_alerts(wallet_checksum, sats_to_i64(current_balance.to_sat()))
            .await
            .map(|_| ())
        {
            warn!("[{}] Balance alert checking failed: {}", wallet_checksum, e);
        }
        debug!(
            "[{}] Balance alert checking took {:.2?}",
            wallet_checksum,
            balance_alert_start.elapsed()
        );

        let sync_duration = sync_start.elapsed();
        let has_changes = summary.has_changes();
        if has_changes {
            info!(
                "[{}] Descriptor-based sync complete in {:.2}s (electrum {:.2}s across {} attempt(s)); changes={}, new_transactions={}, confirmations={}, conflicts_marked={}",
                wallet_checksum,
                sync_duration.as_secs_f64(),
                electrum_duration.as_secs_f64(),
                electrum_attempts,
                has_changes,
                summary.new_transactions,
                summary.confirmation_updates,
                summary.conflicts_marked
            );
        } else {
            debug!(
                "[{}] Descriptor-based sync complete in {:.2}s (electrum {:.2}s across {} attempt(s)); changes=false",
                wallet_checksum,
                sync_duration.as_secs_f64(),
                electrum_duration.as_secs_f64(),
                electrum_attempts,
            );
        }

        // Log warning for unusually long syncs (cloud mode only)
        if self.config.is_cloud_mode() && sync_duration.as_secs() > 120 {
            warn!(
                "[{}] WARNING: Sync took {:.1}s (>120s), potential performance issue",
                wallet_checksum,
                sync_duration.as_secs_f64()
            );
        }
        Ok(sync_result)
    }
}
