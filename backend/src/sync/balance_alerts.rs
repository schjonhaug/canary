//! Balance alerts: detects threshold crossings, in sats or fiat, on each sync and
//! publishes balance-alert notifications.

use super::WalletSyncService;
use crate::metadata::{BalanceAlert, BalanceAlertTriggerParams};
use crate::utils::amount::{fiat_to_cents, sats_to_btc};
use anyhow::Result;
use tracing::{debug, info, warn};

impl WalletSyncService {
    /// Check balance alerts and send notifications for triggered thresholds
    pub async fn check_balance_alerts(
        &self,
        wallet_checksum: &str,
        current_balance_sats: i64,
    ) -> Result<Vec<BalanceAlert>> {
        let mut triggered_alerts = Vec::new();
        debug!(
            "[{}] Checking balance alerts for balance: {} sats",
            wallet_checksum, current_balance_sats
        );

        // Get all active balance alerts for this wallet
        let active_alerts = self
            .metadata_db
            .get_active_balance_alerts_for_wallet(wallet_checksum)
            .await?;

        if active_alerts.is_empty() {
            debug!("[{}] No active balance alerts to check", wallet_checksum);
            return Ok(Vec::new());
        }

        debug!(
            "[{}] Found {} active balance alerts to check",
            wallet_checksum,
            active_alerts.len()
        );

        for alert in active_alerts {
            // Determine threshold to compare against
            // For fiat thresholds, convert current balance to fiat using current exchange rate
            let Some((comparison_threshold, exchange_rate_snapshot)) = self
                .alert_comparison_threshold(&alert, wallet_checksum, current_balance_sats)
                .await
            else {
                continue;
            };

            // Determine current value for comparison
            let current_value = if alert.threshold_currency.is_some() {
                // For fiat alerts, convert current balance to fiat cents
                if let Some(rate) = exchange_rate_snapshot {
                    let balance_btc = sats_to_btc(current_balance_sats);
                    let balance_fiat = balance_btc * rate;
                    fiat_to_cents(balance_fiat)
                } else {
                    current_balance_sats // Fallback (should not reach here)
                }
            } else {
                current_balance_sats
            };

            // Implement threshold crossing detection
            // Only fire when balance crosses the threshold in the relevant direction
            let should_trigger = Self::crossed_threshold(
                &alert,
                wallet_checksum,
                current_balance_sats,
                current_value,
                comparison_threshold,
            );

            // Update last_checked_balance_sats for next sync cycle
            // For fiat alerts, store the fiat value (in cents) to detect rate changes
            // For BTC alerts, store the satoshi value
            let value_to_store = if alert.threshold_currency.is_some() {
                // Store fiat value in cents for fiat alerts
                current_value
            } else {
                // Store satoshi value for BTC alerts
                current_balance_sats
            };

            if let Err(e) = self
                .metadata_db
                .update_alert_last_checked_balance(&alert.id, value_to_store)
                .await
            {
                warn!(
                    "[{}] Failed to update last_checked_balance for alert {}: {}",
                    wallet_checksum, alert.id, e
                );
            }

            if should_trigger {
                // Add to triggered alerts list for testing
                triggered_alerts.push(alert.clone());
                self.fire_balance_alert(
                    &alert,
                    wallet_checksum,
                    current_balance_sats,
                    exchange_rate_snapshot,
                )
                .await;
            }
        }

        Ok(triggered_alerts)
    }

    /// The value an alert's current balance is compared against: sats for BTC
    /// thresholds, fiat cents for fiat thresholds, together with the exchange
    /// rate used. `None` skips the alert when no rate is available.
    async fn alert_comparison_threshold(
        &self,
        alert: &BalanceAlert,
        wallet_checksum: &str,
        current_balance_sats: i64,
    ) -> Option<(i64, Option<f64>)> {
        if let (Some(ref currency), Some(fiat_amount)) =
            (&alert.threshold_currency, alert.threshold_fiat_amount)
        {
            // Fiat threshold - need to convert current balance to fiat
            match self.metadata_db.get_exchange_rates().await {
                Ok(rates) => {
                    if let Some(rate) = rates.get(currency) {
                        let rate_per_btc = rate.rate_per_btc;
                        // Convert current balance from sats to BTC to fiat
                        let balance_btc = sats_to_btc(current_balance_sats);
                        let balance_fiat = balance_btc * rate_per_btc;

                        debug!(
                            "[{}] Checking fiat alert: balance {} {} (from {} sats at rate {} {}/BTC), threshold {} {}",
                            wallet_checksum, balance_fiat, currency, current_balance_sats, rate_per_btc, currency, fiat_amount, currency
                        );

                        // For fiat alerts, we compare fiat amounts directly
                        // We'll use a threshold in "fiat cents" for comparison to avoid floating point issues
                        let threshold_cents = fiat_to_cents(fiat_amount);

                        Some((threshold_cents, Some(rate_per_btc)))
                    } else {
                        warn!(
                            "[{}] Exchange rate for {} not available, skipping fiat alert check",
                            wallet_checksum, currency
                        );
                        None
                    }
                }
                Err(e) => {
                    warn!(
                        "[{}] Failed to fetch exchange rates for fiat alert: {}, skipping",
                        wallet_checksum, e
                    );
                    None
                }
            }
        } else {
            // BTC threshold - compare satoshis directly
            Some((alert.threshold_sats, None))
        }
    }

    /// Whether the balance crossed the alert's threshold since the last sync,
    /// in the alert's direction. The first check only records a baseline.
    fn crossed_threshold(
        alert: &BalanceAlert,
        wallet_checksum: &str,
        current_balance_sats: i64,
        current_value: i64,
        comparison_threshold: i64,
    ) -> bool {
        let last_checked = alert.last_checked_balance_sats;

        if let Some(last_checked_value) = last_checked {
            // For fiat alerts, last_checked_value is already in fiat cents
            // For BTC alerts, last_checked_value is in satoshis
            let previous_value = last_checked_value;

            match alert.alert_type {
                crate::metadata::BalanceAlertType::Above => {
                    // Fire when crossing from at-or-below to above
                    previous_value <= comparison_threshold && current_value > comparison_threshold
                }
                crate::metadata::BalanceAlertType::Below => {
                    // Fire when crossing from at-or-above to below
                    previous_value >= comparison_threshold && current_value < comparison_threshold
                }
                crate::metadata::BalanceAlertType::Equals => {
                    // Fire when crossing to equals (from any non-equals value)
                    previous_value != comparison_threshold && current_value == comparison_threshold
                }
            }
        } else {
            // First check - initialize last_checked_balance_sats without firing
            debug!(
                "[{}] Initializing balance alert {} with current balance: {} sats",
                wallet_checksum, alert.id, current_balance_sats
            );
            false
        }
    }

    /// Records a triggered alert and publishes its notification.
    async fn fire_balance_alert(
        &self,
        alert: &BalanceAlert,
        wallet_checksum: &str,
        current_balance_sats: i64,
        exchange_rate_snapshot: Option<f64>,
    ) {
        let threshold_desc = if let (Some(ref currency), Some(fiat_amount)) =
            (&alert.threshold_currency, alert.threshold_fiat_amount)
        {
            format!(
                "{} {} (≈ {} sats)",
                fiat_amount, currency, alert.threshold_sats
            )
        } else {
            format!("{} sats", alert.threshold_sats)
        };

        info!(
            "[{}] Balance alert triggered: {} {} (current: {} sats)",
            wallet_checksum,
            alert.alert_type.as_str(),
            threshold_desc,
            current_balance_sats
        );

        // Update last_triggered_at timestamp
        if let Err(e) = self
            .metadata_db
            .update_balance_alert_triggered_timestamp(&alert.id)
            .await
        {
            warn!(
                "[{}] Failed to update balance alert triggered timestamp: {}",
                wallet_checksum, e
            );
        }

        // Create notification record in balance_alert_notifications table
        let trigger_params = BalanceAlertTriggerParams {
            contact_id: alert.contact_id.clone(),
            threshold_sats: alert.threshold_sats,
            current_balance_sats,
            alert_type: alert.alert_type,
            threshold_currency: alert.threshold_currency.clone(),
            threshold_fiat_amount: alert.threshold_fiat_amount,
            exchange_rate_snapshot,
        };
        if let Err(e) = self
            .metadata_db
            .create_balance_alert_notification(&alert.id, wallet_checksum, &trigger_params)
            .await
        {
            warn!(
                "[{}] Failed to create balance alert notification record: {}",
                wallet_checksum, e
            );
        }

        // Send balance alert notification via existing notification system
        self.send_balance_alert_notification(
            alert,
            wallet_checksum,
            current_balance_sats,
            exchange_rate_snapshot,
        );

        // Alert remains active and will check for next crossing
        debug!(
            "[{}] Balance alert {} triggered, will check for next crossing in future syncs",
            wallet_checksum, alert.id
        );
    }

    /// Send balance alert notification using existing notification system
    fn send_balance_alert_notification(
        &self,
        alert: &BalanceAlert,
        wallet_checksum: &str,
        current_balance_sats: i64,
        exchange_rate_snapshot: Option<f64>,
    ) {
        // Create a balance alert notification that mimics transaction notifications
        let balance_alert_notification = crate::metadata::BalanceAlertNotification {
            id: uuid::Uuid::new_v4().to_string(),
            balance_alert_id: alert.id.clone(),
            wallet_checksum: wallet_checksum.to_string(),
            contact_id: alert.contact_id.clone(),
            threshold_sats: alert.threshold_sats,
            current_balance_sats,
            alert_type: alert.alert_type,
            notification_sent_at: u64::try_from(chrono::Utc::now().timestamp()).unwrap_or(0),
            created_at: chrono::Utc::now().to_rfc3339(),
            threshold_currency: alert.threshold_currency.clone(),
            threshold_fiat_amount: alert.threshold_fiat_amount,
            exchange_rate_snapshot,
        };

        // Send notification via broadcast channel (same as transaction notifications)
        if let Err(e) =
            self.notification_sender
                .send(crate::metadata::TransactionNotification::BalanceAlert(
                    balance_alert_notification,
                ))
        {
            warn!(
                "[{}] Failed to send balance alert notification via broadcast: {}",
                wallet_checksum, e
            );
        } else {
            debug!(
                "[{}] Sent balance alert notification via broadcast channel",
                wallet_checksum
            );
        }
    }
}
