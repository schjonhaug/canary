use super::pool::MetadataDb;
use super::types::*;
use crate::electrum::BlockHeader;
use crate::exchange_rates;
use anyhow::Result;
use bdk_wallet::rusqlite::{params, OptionalExtension};
use tokio::task::spawn_blocking;
use uuid::Uuid;

impl MetadataDb {
    // ============================
    // BLOCKCHAIN OPERATIONS
    // ============================

    pub async fn upsert_current_block_header(&self, block_header: &BlockHeader) -> Result<()> {
        let pool = self.pool.clone();
        let block_header = block_header.clone();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "INSERT OR REPLACE INTO current_block_header (id, height, timestamp, updated_at)
                 VALUES (1, ?1, ?2, datetime('now'))",
                params![block_header.height, block_header.timestamp,],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn get_current_block_header(&self) -> Result<Option<BlockHeader>> {
        let pool = self.pool.clone();

        spawn_blocking(move || -> Result<Option<BlockHeader>> {
            let conn = pool.get()?;
            match conn.query_row(
                "SELECT height, timestamp FROM current_block_header WHERE id = 1",
                [],
                |row| {
                    Ok(BlockHeader {
                        height: row.get::<_, i64>(0)? as u32,
                        timestamp: row.get::<_, i64>(1)? as u64,
                    })
                },
            ) {
                Ok(block_header) => {
                    // Return None if this is the dummy row (height=0)
                    if block_header.height == 0 {
                        Ok(None)
                    } else {
                        Ok(Some(block_header))
                    }
                }
                Err(bdk_wallet::rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
        .await?
    }

    /// Insert notification log for balance alert notifications (separate from transactions)
    pub async fn insert_notification_log_for_balance_alert(
        &self,
        balance_alert_id: &str,
        wallet_checksum: &str,
        notification_method_id: &str,
        provider_name: &str,
        provider_message_id: Option<&str>,
        status: &str,
        error_message: Option<&str>,
        message_content: &str,
    ) -> Result<String> {
        let pool = self.pool.clone();
        let balance_alert_id = balance_alert_id.to_string();
        let wallet_checksum = wallet_checksum.to_string();
        let notification_method_id = notification_method_id.to_string();
        let provider_name = provider_name.to_string();
        let provider_message_id = provider_message_id.map(|s| s.to_string());
        let status = status.to_string();
        let error_message = error_message.map(|s| s.to_string());
        let message_content = message_content.to_string();

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

    // Exchange rate methods
    pub async fn get_exchange_rates(
        &self,
    ) -> Result<std::collections::HashMap<String, exchange_rates::ExchangeRate>> {
        let pool = self.pool.clone();
        spawn_blocking(
            move || -> Result<std::collections::HashMap<String, exchange_rates::ExchangeRate>> {
                let conn = pool.get()?;
                let mut stmt = conn
                    .prepare("SELECT currency, rate_per_btc, last_updated FROM exchange_rates")?;
                let rate_iter = stmt.query_map(params![], |row| {
                    Ok(exchange_rates::ExchangeRate {
                        currency: row.get(0)?,
                        rate_per_btc: row.get(1)?,
                        last_updated: chrono::DateTime::parse_from_rfc3339(
                            &row.get::<_, String>(2)?,
                        )
                        .map_err(|e| {
                            bdk_wallet::rusqlite::Error::FromSqlConversionFailure(
                                2,
                                bdk_wallet::rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?
                        .with_timezone(&chrono::Utc),
                    })
                })?;

                let mut rates = std::collections::HashMap::new();
                for rate in rate_iter {
                    let rate = rate?;
                    rates.insert(rate.currency.clone(), rate);
                }
                Ok(rates)
            },
        )
        .await?
    }

    pub async fn store_exchange_rates(
        &self,
        rates: &std::collections::HashMap<String, exchange_rates::ExchangeRate>,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let rates = rates.clone();
        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            let tx = conn.unchecked_transaction()?;

            // Clear old rates
            tx.execute("DELETE FROM exchange_rates", params![])?;

            // Insert new rates
            for (currency, rate) in rates {
                tx.execute(
                    "INSERT INTO exchange_rates (currency, rate_per_btc, last_updated) VALUES (?1, ?2, ?3)",
                    params![currency, rate.rate_per_btc, rate.last_updated.to_rfc3339()],
                )?;
            }

            tx.commit()?;
            Ok(())
        })
        .await?
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
        let pool = self.pool.clone();
        let wallet_checksum = wallet_checksum.to_string();
        let alert_id = Uuid::new_v4().to_string();
        let alert_type_str = alert_type.as_str().to_string();

        spawn_blocking(move || -> Result<BalanceAlert> {
            let conn = pool.get()?;
            let current_time = chrono::Utc::now().to_rfc3339();

            conn.execute(
                "INSERT INTO balance_alerts (id, wallet_checksum, threshold_sats, alert_type, is_active, created_at, threshold_currency, threshold_fiat_amount, last_checked_balance_sats)
                 VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7, ?8)",
                params![alert_id, wallet_checksum, threshold_sats, alert_type_str, current_time, threshold_currency, threshold_fiat_amount, current_balance_sats],
            )?;

            Ok(BalanceAlert {
                id: alert_id,
                wallet_checksum,
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
                "SELECT id, wallet_checksum, threshold_sats, alert_type, is_active, last_triggered_at, created_at,
                        threshold_currency, threshold_fiat_amount, last_checked_balance_sats
                 FROM balance_alerts
                 WHERE wallet_checksum = ?1 AND is_active = 1"
            )?;

            let alert_iter = stmt.query_map(params![wallet_checksum], |row| {
                Ok(BalanceAlert {
                    id: row.get(0)?,
                    wallet_checksum: row.get(1)?,
                    threshold_sats: row.get(2)?,
                    alert_type: BalanceAlertType::try_from(row.get::<_, String>(3)?.as_str())
                        .map_err(|e| bdk_wallet::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                    is_active: row.get::<_, i64>(4)? != 0,
                    last_triggered_at: row.get::<_, Option<i64>>(5)?.map(|t| t as u64),
                    created_at: row.get(6)?,
                    threshold_currency: row.get(7)?,
                    threshold_fiat_amount: row.get(8)?,
                    last_checked_balance_sats: row.get(9)?,
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
                "SELECT id, wallet_checksum, threshold_sats, alert_type, is_active, last_triggered_at, created_at,
                        threshold_currency, threshold_fiat_amount, last_checked_balance_sats
                 FROM balance_alerts
                 WHERE wallet_checksum = ?1
                 ORDER BY created_at ASC"
            )?;

            let alert_iter = stmt.query_map(params![wallet_checksum], |row| {
                Ok(BalanceAlert {
                    id: row.get(0)?,
                    wallet_checksum: row.get(1)?,
                    threshold_sats: row.get(2)?,
                    alert_type: BalanceAlertType::try_from(row.get::<_, String>(3)?.as_str())
                        .map_err(|e| bdk_wallet::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                    is_active: row.get::<_, i64>(4)? != 0,
                    last_triggered_at: row.get::<_, Option<i64>>(5)?.map(|t| t as u64),
                    created_at: row.get(6)?,
                    threshold_currency: row.get(7)?,
                    threshold_fiat_amount: row.get(8)?,
                    last_checked_balance_sats: row.get(9)?,
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
                "SELECT id, wallet_checksum, threshold_sats, alert_type, is_active, last_triggered_at, created_at,
                        threshold_currency, threshold_fiat_amount, last_checked_balance_sats
                 FROM balance_alerts
                 WHERE id = ?1",
            )?;

            let alert = stmt
                .query_row(params![alert_id], |row| {
                    Ok(BalanceAlert {
                        id: row.get(0)?,
                        wallet_checksum: row.get(1)?,
                        threshold_sats: row.get(2)?,
                        alert_type: BalanceAlertType::try_from(row.get::<_, String>(3)?.as_str())
                            .map_err(|e| bdk_wallet::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                        is_active: row.get::<_, i64>(4)? != 0,
                        last_triggered_at: row.get::<_, Option<i64>>(5)?.map(|t| t as u64),
                        created_at: row.get(6)?,
                        threshold_currency: row.get(7)?,
                        threshold_fiat_amount: row.get(8)?,
                        last_checked_balance_sats: row.get(9)?,
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
        let pool = self.pool.clone();
        let wallet_checksum = wallet_checksum.to_string();
        let alert_type_str = alert_type.as_str().to_string();

        spawn_blocking(move || -> Result<Option<BalanceAlert>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, wallet_checksum, threshold_sats, alert_type, is_active, last_triggered_at, created_at, threshold_currency, threshold_fiat_amount, last_checked_balance_sats
                 FROM balance_alerts
                 WHERE wallet_checksum = ?1 AND alert_type = ?2 AND threshold_sats = ?3
                 LIMIT 1"
            )?;

            let row = stmt.query_row(params![wallet_checksum, alert_type_str, threshold_sats], |row| {
                Ok(BalanceAlert {
                    id: row.get(0)?,
                    wallet_checksum: row.get(1)?,
                    threshold_sats: row.get(2)?,
                    alert_type: BalanceAlertType::try_from(row.get::<_, String>(3)?.as_str())
                        .map_err(|e| bdk_wallet::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                    is_active: row.get::<_, i64>(4)? != 0,
                    last_triggered_at: row.get::<_, Option<i64>>(5)?.map(|t| t as u64),
                    created_at: row.get(6)?,
                    threshold_currency: row.get(7)?,
                    threshold_fiat_amount: row.get(8)?,
                    last_checked_balance_sats: row.get(9)?,
                })
            });

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
        threshold_sats: i64,
        current_balance_sats: i64,
        alert_type: BalanceAlertType,
        threshold_currency: Option<String>,
        threshold_fiat_amount: Option<f64>,
        exchange_rate_snapshot: Option<f64>,
    ) -> Result<BalanceAlertNotification> {
        let pool = self.pool.clone();
        let notification_id = Uuid::new_v4().to_string();
        let balance_alert_id = balance_alert_id.to_string();
        let wallet_checksum = wallet_checksum.to_string();
        let alert_type_str = alert_type.as_str().to_string();
        let notification_sent_at = chrono::Utc::now().timestamp() as u64;

        spawn_blocking(move || -> Result<BalanceAlertNotification> {
            let conn = pool.get()?;
            let current_time = chrono::Utc::now().to_rfc3339();

            conn.execute(
                "INSERT INTO balance_alert_notifications
                 (id, balance_alert_id, wallet_checksum, threshold_sats, current_balance_sats, alert_type, notification_sent_at, created_at, threshold_currency, threshold_fiat_amount, exchange_rate_snapshot)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    notification_id,
                    balance_alert_id,
                    wallet_checksum,
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
