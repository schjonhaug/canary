use super::pool::MetadataDb;
use super::types::*;
use anyhow::Result;
use bdk_wallet::rusqlite::{params, params_from_iter, types::Value};
use tokio::task::spawn_blocking;
use tracing::warn;

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

    pub async fn update_transaction_parent(
        &self,
        wallet_checksum: &str,
        txid: &str,
        parent_txid: &str,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();
        let txid = txid.to_string();
        let parent_txid = parent_txid.to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let changes = conn.execute(
                "UPDATE transactions
                 SET parent_txid = ?1
                 WHERE wallet_checksum = ?2 AND txid = ?3
                   AND (parent_txid IS NULL OR parent_txid != ?1)",
                params![&parent_txid, &checksum, &txid],
            )?;
            Ok(changes > 0)
        })
        .await?
    }

    pub async fn get_transactions_by_wallet_checksum(
        &self,
        wallet_checksum: &str,
        limit: Option<usize>,
        include_notifications: bool,
    ) -> Result<Vec<TransactionWithWallet>> {
        let page = self
            .get_transactions_page_by_wallet_checksum(
                wallet_checksum,
                TransactionPageRequest {
                    limit: limit.unwrap_or(10000),
                    cursor: None,
                    since_timestamp: None,
                    include_notifications,
                },
            )
            .await?;

        Ok(page.transactions)
    }

    pub async fn get_transactions_page_by_wallet_checksum(
        &self,
        wallet_checksum: &str,
        request: TransactionPageRequest,
    ) -> Result<TransactionPage> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();
        let limit = request.limit;
        let include_notifications = request.include_notifications;
        let cursor = request.cursor.clone();
        let since_timestamp = request.since_timestamp;

        spawn_blocking(move || -> Result<TransactionPage> {
            let conn = pool.get()?;
            let sort_timestamp_expr = "COALESCE(t.confirmed_at, t.first_seen_at)";
            let change_timestamp_expr = "MAX(COALESCE(t.confirmed_at, t.first_seen_at), COALESCE(t.replaced_at, 0))";
            let mut query =
                "SELECT t.txid, t.wallet_checksum, w.name, t.transaction_type, t.amount_sats, t.fee_sats, t.block_height, t.first_seen_at, t.confirmed_at, t.parent_txid, t.transaction_status, t.replaced_by_txid, t.replaced_at
                 FROM transactions t
                 JOIN wallets w ON t.wallet_checksum = w.checksum
                 WHERE t.wallet_checksum = ?"
                    .to_string();
            let mut query_params = vec![Value::from(checksum.clone())];

            if let Some(since_timestamp) = since_timestamp {
                query.push_str(&format!(" AND {} >= ?", change_timestamp_expr));
                query_params.push(Value::from(since_timestamp as i64));
            }

            if let Some(cursor) = &cursor {
                query.push_str(&format!(
                    " AND ({} < ? OR ({} = ? AND t.txid < ?))",
                    sort_timestamp_expr, sort_timestamp_expr
                ));
                query_params.push(Value::from(cursor.sort_timestamp as i64));
                query_params.push(Value::from(cursor.sort_timestamp as i64));
                query_params.push(Value::from(cursor.txid.clone()));
            }

            query.push_str(&format!(
                " ORDER BY {} DESC, t.txid DESC LIMIT ?",
                sort_timestamp_expr
            ));
            query_params.push(Value::from((limit + 1) as i64));

            let mut stmt = conn.prepare(&query)?;

            let transaction_iter = stmt.query_map(params_from_iter(query_params), |row| {
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
                    notification_status: vec![],
                })
            })?;

            let mut transactions: Vec<TransactionWithWallet> = transaction_iter
                .collect::<std::result::Result<Vec<_>, _>>()?;

            let has_more = transactions.len() > limit;
            if has_more {
                transactions.truncate(limit);
            }
            let next_cursor = if has_more {
                transactions.last().map(|transaction| TransactionCursor {
                    sort_timestamp: transaction.detail_sort_timestamp(),
                    txid: transaction.txid.clone(),
                })
            } else {
                None
            };

            if include_notifications && !transactions.is_empty() {
                // Single query for all notifications for this wallet, filtered in Rust.
                // This avoids the SQLite variable limit (SQLITE_MAX_VARIABLE_NUMBER)
                // that would be hit with a large IN (?, ?, ...) clause.
                // Note: message_content is intentionally excluded (not part of NotificationStatus).
                let mut notification_stmt = conn.prepare(
                    "SELECT nl.transaction_txid, nl.contact_name_snapshot, nl.provider_name, nl.status,
                            nl.error_message, nl.notification_target_snapshot, nl.provider_type_snapshot,
                            nl.created_at, nl.notification_type
                     FROM notification_logs nl
                     WHERE nl.transaction_wallet_checksum = ?1
                     ORDER BY nl.created_at ASC"
                )?;

                // txid_set ensures we only attach notifications to transactions in the
                // current result set, important when `limit` trims the full list.
                let txid_set: std::collections::HashSet<&str> =
                    transactions.iter().map(|tx| tx.txid.as_str()).collect();

                let notification_iter = notification_stmt.query_map([&checksum], |row| {
                    let txid: String = row.get(0)?;
                    let notification_type: String = row.get(8)?;
                    Ok((txid, NotificationStatus {
                        contact_name: row.get::<_, Option<String>>(1)?.unwrap_or("Unknown".to_string()),
                        provider_name: row.get(2)?,
                        status: row.get(3)?,
                        error_message: row.get(4)?,
                        notification_target: row.get(5)?,
                        provider_type: row.get(6)?,
                        created_at: row.get(7)?,
                        notification_type,
                    }))
                })?;

                // Group notifications by txid, filtering to only fetched transactions
                let mut notifications_map: std::collections::HashMap<String, Vec<NotificationStatus>> =
                    std::collections::HashMap::new();
                for notification in notification_iter {
                    let (txid, status) = notification?;
                    if txid_set.contains(txid.as_str()) {
                        notifications_map.entry(txid).or_default().push(status);
                    }
                }

                // Attach notifications to transactions
                for tx in &mut transactions {
                    if let Some(notifications) = notifications_map.remove(&tx.txid) {
                        tx.notification_status = notifications;
                    }
                }
            }

            Ok(TransactionPage {
                transactions,
                next_cursor,
                has_more,
                applied_since_timestamp: since_timestamp,
            })
        }).await?
    }

    pub async fn get_transaction_notifications(
        &self,
        wallet_checksum: &str,
        txid: &str,
    ) -> Result<Vec<NotificationStatus>> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();
        let txid = txid.to_string();

        spawn_blocking(move || -> Result<Vec<NotificationStatus>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT nl.contact_name_snapshot, nl.provider_name, nl.status,
                        nl.error_message, nl.notification_target_snapshot, nl.provider_type_snapshot,
                        nl.created_at, nl.notification_type
                 FROM notification_logs nl
                 WHERE nl.transaction_wallet_checksum = ?1
                   AND nl.transaction_txid = ?2
                 ORDER BY nl.created_at ASC"
            )?;

            let notification_iter = stmt.query_map([&checksum, &txid], |row| {
                Ok(NotificationStatus {
                    contact_name: row
                        .get::<_, Option<String>>(0)?
                        .unwrap_or("Unknown".to_string()),
                    provider_name: row.get(1)?,
                    status: row.get(2)?,
                    error_message: row.get(3)?,
                    notification_target: row.get(4)?,
                    provider_type: row.get(5)?,
                    created_at: row.get(6)?,
                    notification_type: row.get(7)?,
                })
            })?;

            Ok(notification_iter.collect::<std::result::Result<Vec<_>, _>>()?)
        }).await?
    }

    /// Insert notification log for transaction-based notifications (new schema)
    pub async fn insert_notification_log_for_transaction(
        &self,
        transaction_txid: &str,
        transaction_wallet_checksum: &str,
        params: &NotificationLogParams<'_>,
        notification_type: &str,
    ) -> Result<String> {
        let pool = self.pool.clone();
        let transaction_txid = transaction_txid.to_string();
        let transaction_wallet_checksum = transaction_wallet_checksum.to_string();
        let notification_method_id = params.notification_method_id.to_string();
        let provider_name = params.provider_name.to_string();
        let provider_message_id = params.provider_message_id.map(|s| s.to_string());
        let status = params.status.to_string();
        let error_message = params.error_message.map(|s| s.to_string());
        let message_content = params.message_content.to_string();
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
            ).unwrap_or_else(|e| {
                warn!("Failed to get contact info for notification log: {}", e);
                ("Unknown Contact".to_string(), "Unknown Target".to_string(), "unknown".to_string())
            });

            let notification_target = crate::webhook_provider::redact_notification_target(
                &provider_type,
                &notification_target,
            );

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
}
