use super::pool::MetadataDb;
use super::types::*;
use anyhow::Result;
use bdk_wallet::rusqlite::params;
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

    pub async fn get_transactions_by_wallet_checksum(
        &self,
        wallet_checksum: &str,
        limit: Option<usize>,
        include_notifications: bool,
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
                 ORDER BY COALESCE(t.confirmed_at, t.first_seen_at) DESC, t.txid DESC
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
                    notification_status: vec![],
                })
            })?;

            let mut transactions: Vec<TransactionWithWallet> = transaction_iter
                .collect::<std::result::Result<Vec<_>, _>>()?;

            if include_notifications && !transactions.is_empty() {
                // Batch query: fetch all notification logs for this wallet's transactions at once
                let placeholders = transactions.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let sql = format!(
                    "SELECT nl.transaction_txid, nl.contact_name_snapshot, nl.provider_name, nl.status,
                            nl.error_message, nl.notification_target_snapshot, nl.provider_type_snapshot,
                            nl.created_at, nl.message_content, nl.notification_type
                     FROM notification_logs nl
                     WHERE nl.transaction_wallet_checksum = ?1
                       AND nl.transaction_txid IN ({})
                     ORDER BY nl.created_at ASC",
                    placeholders
                );

                let mut notification_stmt = conn.prepare(&sql)?;

                // Build params: first is wallet checksum, then all txids
                let mut param_values: Vec<Box<dyn bdk_wallet::rusqlite::types::ToSql>> = Vec::with_capacity(1 + transactions.len());
                param_values.push(Box::new(checksum.clone()));
                for tx in &transactions {
                    param_values.push(Box::new(tx.txid.clone()));
                }
                let params: Vec<&dyn bdk_wallet::rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();

                let notification_iter = notification_stmt.query_map(params.as_slice(), |row| {
                    let txid: String = row.get(0)?;
                    let notification_type: String = row.get(9)?;
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

                // Group notifications by txid
                let mut notifications_map: std::collections::HashMap<String, Vec<NotificationStatus>> =
                    std::collections::HashMap::new();
                for notification in notification_iter {
                    let (txid, status) = notification?;
                    notifications_map.entry(txid).or_default().push(status);
                }

                // Attach notifications to transactions
                for tx in &mut transactions {
                    if let Some(notifications) = notifications_map.remove(&tx.txid) {
                        tx.notification_status = notifications;
                    }
                }
            }

            Ok(transactions)
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
