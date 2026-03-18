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
    pub fkid: i64,
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
                .filter_map(|r| r.ok())
                .collect();
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
                        fkid: row.get(3)?,
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();
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
                 LEFT JOIN wallets w ON c.wallet_checksum = w.checksum
                 WHERE w.checksum IS NULL",
            )?;
            let records: Vec<OrphanedRecord> = stmt
                .query_map([], |row| {
                    Ok(OrphanedRecord {
                        id: row.get(0)?,
                        parent_ref: row.get(1)?,
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();
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
                 LEFT JOIN contacts c ON cnm.contact_id = c.id
                 WHERE c.id IS NULL",
            )?;
            let records: Vec<OrphanedRecord> = stmt
                .query_map([], |row| {
                    Ok(OrphanedRecord {
                        id: row.get(0)?,
                        parent_ref: row.get(1)?,
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();
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
                 LEFT JOIN contact_notification_methods cnm ON nl.notification_method_id = cnm.id
                 WHERE nl.notification_method_id IS NOT NULL AND cnm.id IS NULL",
            )?;
            let records: Vec<OrphanedRecord> = stmt
                .query_map([], |row| {
                    Ok(OrphanedRecord {
                        id: row.get(0)?,
                        parent_ref: row.get(1)?,
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();
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
                 LEFT JOIN wallets w ON te.wallet_checksum = w.checksum
                 WHERE w.checksum IS NULL",
            )?;
            let records: Vec<OrphanedRecord> = stmt
                .query_map([], |row| {
                    Ok(OrphanedRecord {
                        id: row.get(0)?,
                        parent_ref: row.get(1)?,
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();
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
                 LEFT JOIN wallets w ON ba.wallet_checksum = w.checksum
                 WHERE w.checksum IS NULL",
            )?;
            let records: Vec<OrphanedRecord> = stmt
                .query_map([], |row| {
                    Ok(OrphanedRecord {
                        id: row.get(0)?,
                        parent_ref: row.get(1)?,
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();
            Ok(records)
        })
        .await?
    }

    // ============================
    // DUPLICATE DETECTION
    // ============================

    pub async fn find_duplicate_contacts(&self) -> Result<Vec<DuplicateRecord>> {
        let pool = self.pool.clone();
        spawn_blocking(move || -> Result<Vec<DuplicateRecord>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT wallet_checksum || ':' || name AS key, COUNT(*) AS cnt
                 FROM contacts
                 GROUP BY wallet_checksum, name
                 HAVING COUNT(*) > 1",
            )?;
            let records: Vec<DuplicateRecord> = stmt
                .query_map([], |row| {
                    Ok(DuplicateRecord {
                        key: row.get(0)?,
                        count: row.get::<_, i64>(1)? as usize,
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();
            Ok(records)
        })
        .await?
    }

    pub async fn find_duplicate_notification_methods(&self) -> Result<Vec<DuplicateRecord>> {
        let pool = self.pool.clone();
        spawn_blocking(move || -> Result<Vec<DuplicateRecord>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT contact_id || ':' || provider_type || ':' || target AS key, COUNT(*) AS cnt
                 FROM contact_notification_methods
                 GROUP BY contact_id, provider_type, target
                 HAVING COUNT(*) > 1",
            )?;
            let records: Vec<DuplicateRecord> = stmt
                .query_map([], |row| {
                    Ok(DuplicateRecord {
                        key: row.get(0)?,
                        count: row.get::<_, i64>(1)? as usize,
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();
            Ok(records)
        })
        .await?
    }

    // ============================
    // CLEANUP OPERATIONS
    // ============================

    pub async fn cleanup_orphaned_contacts(&self) -> Result<usize> {
        let pool = self.pool.clone();
        spawn_blocking(move || -> Result<usize> {
            let conn = pool.get()?;
            let deleted = conn.execute(
                "DELETE FROM contacts WHERE wallet_checksum NOT IN (SELECT checksum FROM wallets)",
                [],
            )?;
            Ok(deleted)
        })
        .await?
    }

    pub async fn cleanup_orphaned_notification_methods(&self) -> Result<usize> {
        let pool = self.pool.clone();
        spawn_blocking(move || -> Result<usize> {
            let conn = pool.get()?;
            let deleted = conn.execute(
                "DELETE FROM contact_notification_methods WHERE contact_id NOT IN (SELECT id FROM contacts)",
                [],
            )?;
            Ok(deleted)
        })
        .await?
    }

    pub async fn cleanup_orphaned_notification_logs(&self) -> Result<usize> {
        let pool = self.pool.clone();
        spawn_blocking(move || -> Result<usize> {
            let conn = pool.get()?;
            let deleted = conn.execute(
                "DELETE FROM notification_logs WHERE notification_method_id IS NOT NULL AND notification_method_id NOT IN (SELECT id FROM contact_notification_methods)",
                [],
            )?;
            Ok(deleted)
        })
        .await?
    }

    pub async fn cleanup_orphaned_transactions(&self) -> Result<usize> {
        let pool = self.pool.clone();
        spawn_blocking(move || -> Result<usize> {
            let conn = pool.get()?;
            let deleted = conn.execute(
                "DELETE FROM transaction_events WHERE wallet_checksum NOT IN (SELECT checksum FROM wallets)",
                [],
            )?;
            Ok(deleted)
        })
        .await?
    }

    pub async fn cleanup_orphaned_balance_alerts(&self) -> Result<usize> {
        let pool = self.pool.clone();
        spawn_blocking(move || -> Result<usize> {
            let conn = pool.get()?;
            let deleted = conn.execute(
                "DELETE FROM balance_alerts WHERE wallet_checksum NOT IN (SELECT checksum FROM wallets)",
                [],
            )?;
            Ok(deleted)
        })
        .await?
    }
}
