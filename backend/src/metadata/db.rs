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
    // TRANSACTION OPERATIONS
    // ============================
    pub async fn insert_transaction(&self, transaction: &TransactionInsert) -> Result<String> {
        let pool = self.pool.clone();
        let transaction = transaction.clone();

        spawn_blocking(move || -> Result<String> {
            let conn = pool.get()?;
            conn.execute(
                "INSERT OR IGNORE INTO transactions (txid, wallet_checksum, transaction_type, amount_sats, fee_sats, block_height, first_seen_at, confirmed_at, parent_txid, transaction_status, replaced_by_txid, replaced_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    &transaction.txid,
                    &transaction.wallet_checksum,
                    transaction.transaction_type.as_str(),
                    transaction.amount_sats,
                    transaction.fee_sats,
                    transaction.block_height,
                    transaction.first_seen_at,
                    transaction.confirmed_at,
                    transaction.parent_txid.as_ref(),
                    &transaction.transaction_status,
                    transaction.replaced_by_txid.as_ref(),
                    transaction.replaced_at,
                ],
            )?;
            Ok(transaction.txid.clone())
        }).await?
    }

    pub async fn get_transaction_by_txid(
        &self,
        wallet_checksum: &str,
        txid: &str,
    ) -> Result<Option<Transaction>> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();
        let txid = txid.to_string();

        spawn_blocking(move || -> Result<Option<Transaction>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT txid, wallet_checksum, transaction_type, amount_sats, fee_sats, block_height, first_seen_at, confirmed_at, parent_txid, transaction_status, replaced_by_txid, replaced_at
                 FROM transactions
                 WHERE wallet_checksum = ?1 AND txid = ?2"
            )?;

            let mut rows = stmt.query_map([&checksum, &txid], |row| {
                Ok(Transaction {
                    txid: row.get(0)?,
                    wallet_checksum: row.get(1)?,
                    transaction_type: EventType::try_from(row.get::<_, String>(2)?.as_str())
                        .map_err(|e| bdk_wallet::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                    amount_sats: row.get(3)?,
                    fee_sats: row.get(4)?,
                    block_height: row.get(5)?,
                    first_seen_at: row.get(6)?,
                    confirmed_at: row.get(7)?,
                    parent_txid: row.get(8)?,
                    transaction_status: row.get(9)?,
                    replaced_by_txid: row.get(10)?,
                    replaced_at: row.get(11)?,
                    notification_status: vec![], // Will be populated by calling code if needed
                })
            })?;

            match rows.next() {
                Some(row) => Ok(Some(row?)),
                None => Ok(None),
            }
        }).await?
    }

    pub async fn update_transaction_confirmation(
        &self,
        wallet_checksum: &str,
        txid: &str,
        block_height: u32,
        confirmed_at: u64,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();
        let txid = txid.to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let changes = conn.execute(
                "UPDATE transactions SET block_height = ?1, confirmed_at = ?2, transaction_status = 'confirmed' WHERE wallet_checksum = ?3 AND txid = ?4",
                params![block_height, confirmed_at, &checksum, &txid],
            )?;
            Ok(changes > 0)
        }).await?
    }

    pub async fn get_transactions_by_wallet_checksum(
        &self,
        wallet_checksum: &str,
        limit: Option<usize>,
    ) -> Result<Vec<TransactionWithWallet>> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();
        let limit = limit.unwrap_or(10000); // Large default for sync operations

        spawn_blocking(move || -> Result<Vec<TransactionWithWallet>> {
            let conn = pool.get()?;

            // First get transactions
            let mut stmt = conn.prepare(
                "SELECT t.txid, t.wallet_checksum, w.name, t.transaction_type, t.amount_sats, t.fee_sats, t.block_height, t.first_seen_at, t.confirmed_at, t.parent_txid, t.transaction_status, t.replaced_by_txid, t.replaced_at
                 FROM transactions t
                 JOIN wallets w ON t.wallet_checksum = w.checksum
                 WHERE t.wallet_checksum = ?1
                 ORDER BY t.first_seen_at DESC, t.txid DESC
                 LIMIT ?2"
            )?;

            let transaction_iter = stmt.query_map([&checksum, &limit.to_string()], |row| {
                Ok(TransactionWithWallet {
                    txid: row.get(0)?,
                    wallet_checksum: row.get(1)?,
                    wallet_name: row.get(2)?,
                    transaction_type: EventType::try_from(row.get::<_, String>(3)?.as_str())
                        .map_err(|e| bdk_wallet::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                    amount_sats: row.get(4)?,
                    fee_sats: row.get(5)?,
                    block_height: row.get(6)?,
                    first_seen_at: row.get(7)?,
                    confirmed_at: row.get(8)?,
                    parent_txid: row.get(9)?,
                    transaction_status: row.get(10)?,
                    replaced_by_txid: row.get(11)?,
                    replaced_at: row.get(12)?,
                    notification_status: vec![], // Will be populated below
                })
            })?;

            let mut transactions = Vec::new();
            for transaction in transaction_iter {
                let mut tx = transaction?;

                // Get notification status for this transaction
                let mut notification_stmt = conn.prepare(
                    "SELECT nl.contact_name_snapshot, nl.provider_name, nl.status, nl.error_message,
                            nl.notification_target_snapshot, nl.provider_type_snapshot, nl.created_at, nl.message_content, nl.notification_type
                     FROM notification_logs nl
                     WHERE nl.transaction_txid = ?1 AND nl.transaction_wallet_checksum = ?2
                     ORDER BY nl.created_at ASC"
                )?;

                let notification_iter = notification_stmt.query_map([&tx.txid, &tx.wallet_checksum], |row| {
                    let notification_type: String = row.get(8)?;

                    Ok(NotificationStatus {
                        contact_name: row.get::<_, Option<String>>(0)?.unwrap_or("Unknown".to_string()),
                        provider_name: row.get(1)?,
                        status: row.get(2)?,
                        error_message: row.get(3)?,
                        notification_target: row.get(4)?,
                        provider_type: row.get(5)?,
                        created_at: row.get(6)?,
                        notification_type,
                    })
                })?;

                for notification in notification_iter {
                    tx.notification_status.push(notification?);
                }

                transactions.push(tx);
            }

            Ok(transactions)
        }).await?
    }

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

    /// Insert notification log for transaction-based notifications (new schema)
    pub async fn insert_notification_log_for_transaction(
        &self,
        transaction_txid: &str,
        transaction_wallet_checksum: &str,
        notification_method_id: &str,
        provider_name: &str,
        provider_message_id: Option<&str>,
        status: &str,
        error_message: Option<&str>,
        message_content: &str,
        notification_type: &str,
    ) -> Result<String> {
        let pool = self.pool.clone();
        let transaction_txid = transaction_txid.to_string();
        let transaction_wallet_checksum = transaction_wallet_checksum.to_string();
        let notification_method_id = notification_method_id.to_string();
        let provider_name = provider_name.to_string();
        let provider_message_id = provider_message_id.map(|s| s.to_string());
        let status = status.to_string();
        let error_message = error_message.map(|s| s.to_string());
        let message_content = message_content.to_string();
        let notification_type = notification_type.to_string();

        spawn_blocking(move || -> Result<String> {
            let conn = pool.get()?;
            let log_id = uuid::Uuid::new_v4().to_string();

            // Get the contact info at the time of notification to preserve it
            let (contact_name, notification_target, provider_type): (String, String, String) = conn.query_row(
                "SELECT c.name, cnm.notification_target, cnm.provider_type
                 FROM contact_notification_methods cnm
                 JOIN contacts c ON cnm.contact_id = c.id
                 WHERE cnm.id = ?1",
                params![&notification_method_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            ).unwrap_or_else(|_| ("Unknown Contact".to_string(), "Unknown Target".to_string(), "unknown".to_string()));

            conn.execute(
                "INSERT INTO notification_logs (id, transaction_txid, transaction_wallet_checksum, notification_method_id, provider_name, provider_message_id, status, error_message, message_content, notification_type, contact_name_snapshot, notification_target_snapshot, provider_type_snapshot)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    &log_id,
                    &transaction_txid,
                    &transaction_wallet_checksum,
                    &notification_method_id,
                    &provider_name,
                    &provider_message_id,
                    &status,
                    &error_message,
                    &message_content,
                    &notification_type,
                    &contact_name,
                    &notification_target,
                    &provider_type,
                ],
            )?;
            Ok(log_id)
        }).await?
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

    /// Mark a transaction as replaced by another transaction (RBF)
    pub async fn mark_transaction_replaced(
        &self,
        wallet_checksum: &str,
        original_txid: &str,
        replaced_by_txid: &str,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();
        let original_txid = original_txid.to_string();
        let replaced_by_txid = replaced_by_txid.to_string();
        let replaced_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let affected = conn.execute(
                "UPDATE transactions
                 SET transaction_status = 'replaced', replaced_by_txid = ?1, replaced_at = ?2
                 WHERE wallet_checksum = ?3 AND txid = ?4 AND transaction_status = 'pending'",
                params![&replaced_by_txid, replaced_at, &checksum, &original_txid],
            )?;
            Ok(affected > 0)
        })
        .await?
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
