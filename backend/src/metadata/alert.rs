use super::pool::MetadataDb;
use super::types::*;
use anyhow::Result;
use bdk_wallet::rusqlite::{params, OptionalExtension};
use tokio::task::spawn_blocking;
use uuid::Uuid;

pub struct CreateBalanceAlertInput<'a> {
    pub wallet_checksum: &'a str,
    pub contact_id: Option<&'a str>,
    pub threshold_sats: i64,
    pub alert_type: BalanceAlertType,
    pub threshold_currency: Option<String>,
    pub threshold_fiat_amount: Option<f64>,
    pub current_balance_sats: Option<i64>,
}

impl MetadataDb {
    // ============================
    // BALANCE ALERT OPERATIONS
    // ============================

    /// Insert notification log for balance alert notifications (separate from transactions)
    pub async fn insert_notification_log_for_balance_alert(
        &self,
        balance_alert_id: &str,
        wallet_checksum: &str,
        params: &NotificationLogParams<'_>,
    ) -> Result<String> {
        let pool = self.pool.clone();
        let balance_alert_id = balance_alert_id.to_string();
        let wallet_checksum = wallet_checksum.to_string();
        let notification_method_id = params.notification_method_id.to_string();
        let provider_name = params.provider_name.to_string();
        let provider_message_id = params.provider_message_id.map(|s| s.to_string());
        let status = params.status.to_string();
        let error_message = params.error_message.map(|s| s.to_string());
        let message_content = params.message_content.to_string();

        spawn_blocking(move || -> Result<String> {
            let conn = pool.get()?;
            let log_id = uuid::Uuid::new_v4().to_string();

            // Get contact info for snapshot
            let (contact_name, notification_target, provider_type) = conn
                .prepare(
                    "SELECT c.name, cnm.notification_target, cnm.provider_type
                     FROM contact_notification_methods cnm
                     JOIN contacts c ON cnm.contact_id = c.id
                     WHERE cnm.id = ?1"
                )?
                .query_row(params![&notification_method_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                }).unwrap_or_else(|_| ("Unknown Contact".to_string(), "Unknown Target".to_string(), "unknown".to_string()));

            let notification_target = crate::webhook_provider::redact_notification_target(
                &provider_type,
                &notification_target,
            );

            conn.execute(
                "INSERT INTO balance_alert_notification_logs (id, balance_alert_id, wallet_checksum, notification_method_id, provider_name, provider_message_id, status, error_message, message_content, contact_name_snapshot, notification_target_snapshot, provider_type_snapshot)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    &log_id,
                    &balance_alert_id,
                    &wallet_checksum,
                    &notification_method_id,
                    &provider_name,
                    &provider_message_id,
                    &status,
                    &error_message,
                    &message_content,
                    &contact_name,
                    &notification_target,
                    &provider_type,
                ],
            )?;
            Ok(log_id)
        }).await?
    }

    // ============================
    // BALANCE ALERTS CRUD METHODS
    // ============================

    pub async fn create_balance_alert(
        &self,
        wallet_checksum: &str,
        threshold_sats: i64,
        alert_type: BalanceAlertType,
        threshold_currency: Option<String>,
        threshold_fiat_amount: Option<f64>,
        current_balance_sats: Option<i64>,
    ) -> Result<BalanceAlert> {
        self.create_balance_alert_with_contact(CreateBalanceAlertInput {
            wallet_checksum,
            contact_id: None,
            threshold_sats,
            alert_type,
            threshold_currency,
            threshold_fiat_amount,
            current_balance_sats,
        })
        .await
    }

    pub async fn create_balance_alert_with_contact(
        &self,
        input: CreateBalanceAlertInput<'_>,
    ) -> Result<BalanceAlert> {
        let pool = self.pool.clone();
        let wallet_checksum = input.wallet_checksum.to_string();
        let contact_id = input.contact_id.map(|value| value.to_string());
        let alert_id = Uuid::new_v4().to_string();
        let alert_type = input.alert_type;
        let alert_type_str = input.alert_type.as_str().to_string();
        let threshold_sats = input.threshold_sats;
        let threshold_currency = input.threshold_currency;
        let threshold_fiat_amount = input.threshold_fiat_amount;
        let current_balance_sats = input.current_balance_sats;

        spawn_blocking(move || -> Result<BalanceAlert> {
            let conn = pool.get()?;
            let current_time = chrono::Utc::now().to_rfc3339();

            conn.execute(
                "INSERT INTO balance_alerts (id, wallet_checksum, contact_id, threshold_sats, alert_type, is_active, created_at, threshold_currency, threshold_fiat_amount, last_checked_balance_sats)
                 VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7, ?8, ?9)",
                params![alert_id, wallet_checksum, contact_id, threshold_sats, alert_type_str, current_time, threshold_currency, threshold_fiat_amount, current_balance_sats],
            )?;

            Ok(BalanceAlert {
                id: alert_id,
                wallet_checksum,
                contact_id,
                threshold_sats,
                alert_type,
                is_active: true,
                last_triggered_at: None,
                created_at: current_time,
                threshold_currency,
                threshold_fiat_amount,
                last_checked_balance_sats: current_balance_sats,
            })
        })
        .await?
    }

    pub async fn get_active_balance_alerts_for_wallet(
        &self,
        wallet_checksum: &str,
    ) -> Result<Vec<BalanceAlert>> {
        let pool = self.pool.clone();
        let wallet_checksum = wallet_checksum.to_string();

        spawn_blocking(move || -> Result<Vec<BalanceAlert>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, wallet_checksum, contact_id, threshold_sats, alert_type, is_active, last_triggered_at, created_at,
                        threshold_currency, threshold_fiat_amount, last_checked_balance_sats
                 FROM balance_alerts
                 WHERE wallet_checksum = ?1 AND is_active = 1"
            )?;

            let alert_iter = stmt.query_map(params![wallet_checksum], |row| {
                Ok(BalanceAlert {
                    id: row.get(0)?,
                    wallet_checksum: row.get(1)?,
                    contact_id: row.get(2)?,
                    threshold_sats: row.get(3)?,
                    alert_type: BalanceAlertType::try_from(row.get::<_, String>(4)?.as_str())
                        .map_err(|e| bdk_wallet::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                    is_active: row.get::<_, i64>(5)? != 0,
                    last_triggered_at: row.get::<_, Option<i64>>(6)?.map(|t| t as u64),
                    created_at: row.get(7)?,
                    threshold_currency: row.get(8)?,
                    threshold_fiat_amount: row.get(9)?,
                    last_checked_balance_sats: row.get(10)?,
                })
            })?;

            let mut alerts = Vec::new();
            for alert in alert_iter {
                alerts.push(alert?);
            }
            Ok(alerts)
        })
        .await?
    }

    pub async fn get_all_balance_alerts_for_wallet(
        &self,
        wallet_checksum: &str,
    ) -> Result<Vec<BalanceAlert>> {
        let pool = self.pool.clone();
        let wallet_checksum = wallet_checksum.to_string();

        spawn_blocking(move || -> Result<Vec<BalanceAlert>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, wallet_checksum, contact_id, threshold_sats, alert_type, is_active, last_triggered_at, created_at,
                        threshold_currency, threshold_fiat_amount, last_checked_balance_sats
                 FROM balance_alerts
                 WHERE wallet_checksum = ?1
                 ORDER BY created_at ASC"
            )?;

            let alert_iter = stmt.query_map(params![wallet_checksum], |row| {
                Ok(BalanceAlert {
                    id: row.get(0)?,
                    wallet_checksum: row.get(1)?,
                    contact_id: row.get(2)?,
                    threshold_sats: row.get(3)?,
                    alert_type: BalanceAlertType::try_from(row.get::<_, String>(4)?.as_str())
                        .map_err(|e| bdk_wallet::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                    is_active: row.get::<_, i64>(5)? != 0,
                    last_triggered_at: row.get::<_, Option<i64>>(6)?.map(|t| t as u64),
                    created_at: row.get(7)?,
                    threshold_currency: row.get(8)?,
                    threshold_fiat_amount: row.get(9)?,
                    last_checked_balance_sats: row.get(10)?,
                })
            })?;

            let mut alerts = Vec::new();
            for alert in alert_iter {
                alerts.push(alert?);
            }
            Ok(alerts)
        })
        .await?
    }

    pub async fn get_balance_alert_by_id(&self, alert_id: &str) -> Result<Option<BalanceAlert>> {
        let pool = self.pool.clone();
        let alert_id = alert_id.to_string();

        spawn_blocking(move || -> Result<Option<BalanceAlert>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, wallet_checksum, contact_id, threshold_sats, alert_type, is_active, last_triggered_at, created_at,
                        threshold_currency, threshold_fiat_amount, last_checked_balance_sats
                 FROM balance_alerts
                 WHERE id = ?1",
            )?;

            let alert = stmt
                .query_row(params![alert_id], |row| {
                    Ok(BalanceAlert {
                        id: row.get(0)?,
                        wallet_checksum: row.get(1)?,
                        contact_id: row.get(2)?,
                        threshold_sats: row.get(3)?,
                        alert_type: BalanceAlertType::try_from(row.get::<_, String>(4)?.as_str())
                            .map_err(|e| bdk_wallet::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                        is_active: row.get::<_, i64>(5)? != 0,
                        last_triggered_at: row.get::<_, Option<i64>>(6)?.map(|t| t as u64),
                        created_at: row.get(7)?,
                        threshold_currency: row.get(8)?,
                        threshold_fiat_amount: row.get(9)?,
                        last_checked_balance_sats: row.get(10)?,
                    })
                })
                .optional()?;

            Ok(alert)
        })
        .await?
    }

    #[allow(dead_code)] // Used in system tests
    pub async fn deactivate_balance_alert(&self, alert_id: &str) -> Result<()> {
        let pool = self.pool.clone();
        let alert_id = alert_id.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE balance_alerts
                 SET is_active = 0
                 WHERE id = ?1",
                params![alert_id],
            )?;
            Ok(())
        })
        .await?
    }

    /// Update the last checked balance for an alert (for threshold crossing detection)
    pub async fn update_alert_last_checked_balance(
        &self,
        alert_id: &str,
        balance_sats: i64,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let alert_id = alert_id.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE balance_alerts SET last_checked_balance_sats = ?1 WHERE id = ?2",
                params![balance_sats, alert_id],
            )?;
            Ok(())
        })
        .await?
    }

    /// Update the last triggered timestamp when an alert fires
    pub async fn update_balance_alert_triggered_timestamp(&self, alert_id: &str) -> Result<()> {
        let pool = self.pool.clone();
        let alert_id = alert_id.to_string();
        let triggered_at = chrono::Utc::now().timestamp() as u64;

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE balance_alerts SET last_triggered_at = ?1 WHERE id = ?2",
                params![triggered_at as i64, alert_id],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn delete_balance_alert(&self, alert_id: &str) -> Result<()> {
        let pool = self.pool.clone();
        let alert_id = alert_id.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "DELETE FROM balance_alerts WHERE id = ?1",
                params![alert_id],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn check_duplicate_balance_alert(
        &self,
        wallet_checksum: &str,
        threshold_sats: i64,
        alert_type: BalanceAlertType,
    ) -> Result<Option<BalanceAlert>> {
        self.check_duplicate_balance_alert_for_contact(
            wallet_checksum,
            None,
            threshold_sats,
            alert_type,
        )
        .await
    }

    pub async fn check_duplicate_balance_alert_for_contact(
        &self,
        wallet_checksum: &str,
        contact_id: Option<&str>,
        threshold_sats: i64,
        alert_type: BalanceAlertType,
    ) -> Result<Option<BalanceAlert>> {
        let pool = self.pool.clone();
        let wallet_checksum = wallet_checksum.to_string();
        let contact_id = contact_id.map(|value| value.to_string());
        let alert_type_str = alert_type.as_str().to_string();

        spawn_blocking(move || -> Result<Option<BalanceAlert>> {
            let conn = pool.get()?;
            let contact_clause = if contact_id.is_some() {
                "contact_id = ?4"
            } else {
                "contact_id IS NULL"
            };
            let query = format!(
                "SELECT id, wallet_checksum, contact_id, threshold_sats, alert_type, is_active, last_triggered_at, created_at, threshold_currency, threshold_fiat_amount, last_checked_balance_sats
                 FROM balance_alerts
                 WHERE wallet_checksum = ?1 AND alert_type = ?2 AND threshold_sats = ?3 AND {}
                 LIMIT 1",
                contact_clause
            );
            let mut stmt = conn.prepare(&query)?;

            let row = if let Some(contact_id) = contact_id {
                stmt.query_row(
                    params![wallet_checksum, alert_type_str, threshold_sats, contact_id],
                    |row| {
                        Ok(BalanceAlert {
                            id: row.get(0)?,
                            wallet_checksum: row.get(1)?,
                            contact_id: row.get(2)?,
                            threshold_sats: row.get(3)?,
                            alert_type: BalanceAlertType::try_from(row.get::<_, String>(4)?.as_str())
                                .map_err(|e| bdk_wallet::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                            is_active: row.get::<_, i64>(5)? != 0,
                            last_triggered_at: row.get::<_, Option<i64>>(6)?.map(|t| t as u64),
                            created_at: row.get(7)?,
                            threshold_currency: row.get(8)?,
                            threshold_fiat_amount: row.get(9)?,
                            last_checked_balance_sats: row.get(10)?,
                        })
                    },
                )
            } else {
                stmt.query_row(params![wallet_checksum, alert_type_str, threshold_sats], |row| {
                Ok(BalanceAlert {
                    id: row.get(0)?,
                    wallet_checksum: row.get(1)?,
                    contact_id: row.get(2)?,
                    threshold_sats: row.get(3)?,
                    alert_type: BalanceAlertType::try_from(row.get::<_, String>(4)?.as_str())
                        .map_err(|e| bdk_wallet::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                    is_active: row.get::<_, i64>(5)? != 0,
                    last_triggered_at: row.get::<_, Option<i64>>(6)?.map(|t| t as u64),
                    created_at: row.get(7)?,
                    threshold_currency: row.get(8)?,
                    threshold_fiat_amount: row.get(9)?,
                    last_checked_balance_sats: row.get(10)?,
                })
                })
            };

            match row {
                Ok(alert) => Ok(Some(alert)),
                Err(bdk_wallet::rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
        .await?
    }

    pub async fn create_balance_alert_notification(
        &self,
        balance_alert_id: &str,
        wallet_checksum: &str,
        params: &BalanceAlertTriggerParams,
    ) -> Result<BalanceAlertNotification> {
        let pool = self.pool.clone();
        let notification_id = Uuid::new_v4().to_string();
        let balance_alert_id = balance_alert_id.to_string();
        let wallet_checksum = wallet_checksum.to_string();
        let alert_type = params.alert_type;
        let alert_type_str = alert_type.as_str().to_string();
        let threshold_sats = params.threshold_sats;
        let current_balance_sats = params.current_balance_sats;
        let threshold_currency = params.threshold_currency.clone();
        let contact_id = params.contact_id.clone();
        let threshold_fiat_amount = params.threshold_fiat_amount;
        let exchange_rate_snapshot = params.exchange_rate_snapshot;
        let notification_sent_at = chrono::Utc::now().timestamp() as u64;

        spawn_blocking(move || -> Result<BalanceAlertNotification> {
            let conn = pool.get()?;
            let current_time = chrono::Utc::now().to_rfc3339();

            conn.execute(
                "INSERT INTO balance_alert_notifications
                 (id, balance_alert_id, wallet_checksum, contact_id, threshold_sats, current_balance_sats, alert_type, notification_sent_at, created_at, threshold_currency, threshold_fiat_amount, exchange_rate_snapshot)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    notification_id,
                    balance_alert_id,
                    wallet_checksum,
                    contact_id,
                    threshold_sats,
                    current_balance_sats,
                    alert_type_str,
                    notification_sent_at as i64,
                    current_time,
                    threshold_currency,
                    threshold_fiat_amount,
                    exchange_rate_snapshot
                ],
            )?;

            Ok(BalanceAlertNotification {
                id: notification_id,
                balance_alert_id,
                wallet_checksum,
                contact_id,
                threshold_sats,
                current_balance_sats,
                alert_type,
                notification_sent_at,
                created_at: current_time,
                threshold_currency,
                threshold_fiat_amount,
                exchange_rate_snapshot,
            })
        })
        .await?
    }
}
