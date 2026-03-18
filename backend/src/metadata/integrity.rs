use super::pool::MetadataDb;
use anyhow::Result;
use tokio::task::spawn_blocking;

/// Pool health statistics
pub struct PoolHealthReport {
    pub total_connections: u32,
    pub idle_connections: u32,
    pub max_connections: u32,
}

/// A foreign key violation found by PRAGMA foreign_key_check
pub struct ForeignKeyViolation {
    pub table: String,
    pub rowid: i64,
    pub parent: String,
}

/// An orphaned record referencing a non-existent parent
pub struct OrphanedRecord {
    pub id: String,
    pub parent_ref: String,
}

/// A set of duplicate records
pub struct DuplicateRecord {
    pub key: String,
    pub count: usize,
}

/// Results of a transactional cleanup operation
pub struct CleanupCounts {
    pub contacts_deleted: usize,
    pub methods_deleted: usize,
    pub logs_deleted: usize,
    pub alert_logs_deleted: usize,
    pub alerts_deleted: usize,
    pub transactions_deleted: usize,
}

impl MetadataDb {
    // ============================
    // POOL & SCHEMA CHECKS
    // ============================

    pub fn check_pool_health(&self) -> PoolHealthReport {
        let state = self.pool.state();
        PoolHealthReport {
            total_connections: state.connections,
            idle_connections: state.idle_connections,
            max_connections: self.pool.max_size(),
        }
    }

    pub async fn get_schema_version(&self) -> Result<String> {
        let pool = self.pool.clone();
        spawn_blocking(move || -> Result<String> {
            let conn = pool.get()?;
            let version: String = conn.query_row(
                "SELECT COALESCE(MAX(version), 'unknown') FROM schema_migrations",
                [],
                |row| row.get(0),
            )?;
            Ok(version)
        })
        .await?
    }

    pub async fn check_sqlite_integrity(&self) -> Result<Vec<String>> {
        let pool = self.pool.clone();
        spawn_blocking(move || -> Result<Vec<String>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare("PRAGMA quick_check")?;
            let results: Vec<String> = stmt
                .query_map([], |row| row.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(results)
        })
        .await?
    }

    pub async fn check_foreign_keys(&self) -> Result<Vec<ForeignKeyViolation>> {
        let pool = self.pool.clone();
        spawn_blocking(move || -> Result<Vec<ForeignKeyViolation>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare("PRAGMA foreign_key_check")?;
            let violations: Vec<ForeignKeyViolation> = stmt
                .query_map([], |row| {
                    Ok(ForeignKeyViolation {
                        table: row.get(0)?,
                        rowid: row.get(1)?,
                        parent: row.get(2)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(violations)
        })
        .await?
    }

    // ============================
    // ORPHANED RECORD DETECTION
    // ============================

    pub async fn find_orphaned_contacts(&self) -> Result<Vec<OrphanedRecord>> {
        let pool = self.pool.clone();
        spawn_blocking(move || -> Result<Vec<OrphanedRecord>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT c.id, c.wallet_checksum FROM contacts c
                 WHERE NOT EXISTS (SELECT 1 FROM wallets w WHERE w.checksum = c.wallet_checksum AND w.status != 'deleted')",
            )?;
            let records: Vec<OrphanedRecord> = stmt
                .query_map([], |row| {
                    Ok(OrphanedRecord {
                        id: row.get(0)?,
                        parent_ref: row.get(1)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(records)
        })
        .await?
    }

    pub async fn find_orphaned_notification_methods(&self) -> Result<Vec<OrphanedRecord>> {
        let pool = self.pool.clone();
        spawn_blocking(move || -> Result<Vec<OrphanedRecord>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT cnm.id, cnm.contact_id FROM contact_notification_methods cnm
                 WHERE NOT EXISTS (SELECT 1 FROM contacts c WHERE c.id = cnm.contact_id)",
            )?;
            let records: Vec<OrphanedRecord> = stmt
                .query_map([], |row| {
                    Ok(OrphanedRecord {
                        id: row.get(0)?,
                        parent_ref: row.get(1)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(records)
        })
        .await?
    }

    pub async fn find_orphaned_notification_logs(&self) -> Result<Vec<OrphanedRecord>> {
        let pool = self.pool.clone();
        spawn_blocking(move || -> Result<Vec<OrphanedRecord>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT nl.id, nl.notification_method_id FROM notification_logs nl
                 WHERE nl.notification_method_id IS NOT NULL
                 AND NOT EXISTS (SELECT 1 FROM contact_notification_methods cnm WHERE cnm.id = nl.notification_method_id)",
            )?;
            let records: Vec<OrphanedRecord> = stmt
                .query_map([], |row| {
                    Ok(OrphanedRecord {
                        id: row.get(0)?,
                        parent_ref: row.get(1)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(records)
        })
        .await?
    }

    pub async fn find_orphaned_transactions(&self) -> Result<Vec<OrphanedRecord>> {
        let pool = self.pool.clone();
        spawn_blocking(move || -> Result<Vec<OrphanedRecord>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT te.id, te.wallet_checksum FROM transaction_events te
                 WHERE NOT EXISTS (SELECT 1 FROM wallets w WHERE w.checksum = te.wallet_checksum AND w.status != 'deleted')",
            )?;
            let records: Vec<OrphanedRecord> = stmt
                .query_map([], |row| {
                    Ok(OrphanedRecord {
                        id: row.get(0)?,
                        parent_ref: row.get(1)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(records)
        })
        .await?
    }

    pub async fn find_orphaned_balance_alerts(&self) -> Result<Vec<OrphanedRecord>> {
        let pool = self.pool.clone();
        spawn_blocking(move || -> Result<Vec<OrphanedRecord>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT ba.id, ba.wallet_checksum FROM balance_alerts ba
                 WHERE NOT EXISTS (SELECT 1 FROM wallets w WHERE w.checksum = ba.wallet_checksum AND w.status != 'deleted')",
            )?;
            let records: Vec<OrphanedRecord> = stmt
                .query_map([], |row| {
                    Ok(OrphanedRecord {
                        id: row.get(0)?,
                        parent_ref: row.get(1)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(records)
        })
        .await?
    }

    pub async fn find_orphaned_balance_alert_notification_logs(&self) -> Result<Vec<OrphanedRecord>> {
        let pool = self.pool.clone();
        spawn_blocking(move || -> Result<Vec<OrphanedRecord>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT banl.id, banl.balance_alert_id FROM balance_alert_notification_logs banl
                 WHERE NOT EXISTS (SELECT 1 FROM balance_alerts ba WHERE ba.id = banl.balance_alert_id)",
            )?;
            let records: Vec<OrphanedRecord> = stmt
                .query_map([], |row| {
                    Ok(OrphanedRecord {
                        id: row.get(0)?,
                        parent_ref: row.get(1)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(records)
        })
        .await?
    }

    // ============================
    // DUPLICATE DETECTION
    // ============================

    pub async fn find_duplicate_notification_methods(&self) -> Result<Vec<DuplicateRecord>> {
        let pool = self.pool.clone();
        spawn_blocking(move || -> Result<Vec<DuplicateRecord>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT contact_id || ':' || provider_type || ':' || notification_target AS key, COUNT(*) AS cnt
                 FROM contact_notification_methods
                 GROUP BY contact_id, provider_type, notification_target
                 HAVING COUNT(*) > 1",
            )?;
            let records: Vec<DuplicateRecord> = stmt
                .query_map([], |row| {
                    Ok(DuplicateRecord {
                        key: row.get(0)?,
                        count: row.get::<_, i64>(1)? as usize,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(records)
        })
        .await?
    }

    // ============================
    // CLEANUP OPERATIONS
    // ============================

    /// Run all cleanup operations in a single database transaction.
    /// Deletes in correct dependency order (children before parents).
    pub async fn run_cleanup(&self) -> Result<CleanupCounts> {
        let pool = self.pool.clone();
        spawn_blocking(move || -> Result<CleanupCounts> {
            let mut conn = pool.get()?;
            let tx = conn.transaction()?;

            // Delete in correct dependency order (children before parents)
            let logs_deleted = tx.execute(
                "DELETE FROM notification_logs WHERE notification_method_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM contact_notification_methods cnm WHERE cnm.id = notification_logs.notification_method_id)",
                [],
            )?;
            let methods_deleted = tx.execute(
                "DELETE FROM contact_notification_methods WHERE NOT EXISTS (SELECT 1 FROM contacts c WHERE c.id = contact_notification_methods.contact_id)",
                [],
            )?;
            let contacts_deleted = tx.execute(
                "DELETE FROM contacts WHERE NOT EXISTS (SELECT 1 FROM wallets w WHERE w.checksum = contacts.wallet_checksum AND w.status != 'deleted')",
                [],
            )?;
            let alert_logs_deleted = tx.execute(
                "DELETE FROM balance_alert_notification_logs WHERE NOT EXISTS (SELECT 1 FROM balance_alerts ba WHERE ba.id = balance_alert_notification_logs.balance_alert_id)",
                [],
            )?;
            let alerts_deleted = tx.execute(
                "DELETE FROM balance_alerts WHERE NOT EXISTS (SELECT 1 FROM wallets w WHERE w.checksum = balance_alerts.wallet_checksum AND w.status != 'deleted')",
                [],
            )?;
            let txs_deleted = tx.execute(
                "DELETE FROM transaction_events WHERE NOT EXISTS (SELECT 1 FROM wallets w WHERE w.checksum = transaction_events.wallet_checksum AND w.status != 'deleted')",
                [],
            )?;

            tx.commit()?;

            Ok(CleanupCounts {
                contacts_deleted,
                methods_deleted,
                logs_deleted,
                alert_logs_deleted,
                alerts_deleted,
                transactions_deleted: txs_deleted,
            })
        })
        .await?
    }
}
