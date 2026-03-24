use super::pool::MetadataDb;
use anyhow::Result;
use tokio::task::spawn_blocking;

/// Pool health statistics
#[derive(Debug)]
pub struct PoolHealthReport {
    pub total_connections: u32,
    pub idle_connections: u32,
    pub max_connections: u32,
}

/// A foreign key violation found by PRAGMA foreign_key_check.
/// `rowid` is Option because WITHOUT ROWID tables return NULL.
#[derive(Debug)]
pub struct ForeignKeyViolation {
    pub table: String,
    pub rowid: Option<i64>,
    pub parent: String,
}

/// An orphaned record referencing a non-existent parent
#[derive(Debug)]
pub struct OrphanedRecord {
    pub id: String,
    pub parent_ref: String,
}

/// A set of duplicate records
#[derive(Debug)]
pub struct DuplicateRecord {
    pub key: String,
    pub count: usize,
}

/// Results of a transactional cleanup operation
#[derive(Debug)]
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

    /// Returns the latest migration filename prefix (e.g. "025_add_something")
    /// via lexicographic MAX. Works correctly as long as prefixes are zero-padded.
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

    /// Uses PRAGMA quick_check (faster than integrity_check, skips some B-tree
    /// page content verification). Suitable for routine health checks.
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
            // transactions table uses composite PK (txid, wallet_checksum)
            let mut stmt = conn.prepare(
                "SELECT t.txid || ':' || t.wallet_checksum, t.wallet_checksum FROM transactions t
                 WHERE NOT EXISTS (SELECT 1 FROM wallets w WHERE w.checksum = t.wallet_checksum AND w.status != 'deleted')",
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

    /// Safety net: this FK has ON DELETE CASCADE so orphans shouldn't exist under
    /// normal operation, but checks for any that slipped through (e.g. if FK
    /// enforcement was temporarily disabled).
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
    // STARTUP DIAGNOSTICS
    // ============================

    /// Run lightweight integrity checks suitable for application startup.
    /// Logs warnings for any issues found but never fails the startup.
    pub async fn run_startup_checks(&self) {
        tracing::info!("Running startup database integrity checks...");

        match self.get_schema_version().await {
            Ok(version) => tracing::info!("Database schema version: {}", version),
            Err(e) => tracing::warn!("Failed to check schema version: {}", e),
        }

        match self.check_foreign_keys().await {
            Ok(violations) if violations.is_empty() => {
                tracing::debug!("Foreign key check: OK");
            }
            Ok(violations) => {
                tracing::warn!("Foreign key violations found: {}", violations.len());
                let display_count = violations.len().min(10);
                for v in &violations[..display_count] {
                    tracing::warn!(
                        "  FK violation: table={}, rowid={:?}, parent={}",
                        v.table,
                        v.rowid,
                        v.parent
                    );
                }
                if violations.len() > 10 {
                    tracing::warn!("  ... and {} more FK violations", violations.len() - 10);
                }
            }
            Err(e) => tracing::warn!("Failed to check foreign keys: {}", e),
        }

        let orphan_checks: Vec<(&str, _)> = vec![
            ("contacts", self.find_orphaned_contacts().await),
            ("notification methods", self.find_orphaned_notification_methods().await),
            ("notification logs", self.find_orphaned_notification_logs().await),
            ("transactions", self.find_orphaned_transactions().await),
            ("balance alerts", self.find_orphaned_balance_alerts().await),
            ("balance alert notification logs", self.find_orphaned_balance_alert_notification_logs().await),
        ];

        let mut total_orphans = 0usize;
        let mut check_failed = false;

        for (name, result) in orphan_checks {
            match result {
                Ok(records) if !records.is_empty() => {
                    tracing::warn!("Found {} orphaned {}", records.len(), name);
                    total_orphans += records.len();
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!("Failed to check orphaned {}: {}", name, e);
                    check_failed = true;
                }
            }
        }

        if check_failed {
            tracing::warn!(
                "Database integrity check completed with errors: {} orphaned records found (some checks failed)",
                total_orphans
            );
        } else if total_orphans == 0 {
            tracing::info!("Database integrity check completed: no issues found");
        } else {
            tracing::warn!(
                "Database integrity check completed: {} orphaned records found. Use POST /api/admin/database/integrity with auto_fix:true to clean up.",
                total_orphans
            );
        }
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

            // Phase 1: Delete orphaned leaf records (notification_logs with missing methods)
            let logs_deleted_phase1 = tx.execute(
                "DELETE FROM notification_logs WHERE notification_method_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM contact_notification_methods cnm WHERE cnm.id = notification_logs.notification_method_id)",
                [],
            )?;
            // Phase 2: Delete orphaned methods (missing contacts)
            // This may NULL out notification_logs.notification_method_id via ON DELETE SET NULL
            let methods_deleted = tx.execute(
                "DELETE FROM contact_notification_methods WHERE NOT EXISTS (SELECT 1 FROM contacts c WHERE c.id = contact_notification_methods.contact_id)",
                [],
            )?;
            // Phase 3: Delete orphaned contacts (missing or soft-deleted wallets)
            let contacts_deleted = tx.execute(
                "DELETE FROM contacts WHERE NOT EXISTS (SELECT 1 FROM wallets w WHERE w.checksum = contacts.wallet_checksum AND w.status != 'deleted')",
                [],
            )?;
            // Note: No Phase 4 needed. Phase 2 deletes orphaned methods, and
            // notification_logs.notification_method_id has ON DELETE SET NULL,
            // so those logs are preserved as audit records with NULL method_id.
            let logs_deleted = logs_deleted_phase1;
            // Phase 5: Delete orphaned balance alert notification logs
            let alert_logs_deleted = tx.execute(
                "DELETE FROM balance_alert_notification_logs WHERE NOT EXISTS (SELECT 1 FROM balance_alerts ba WHERE ba.id = balance_alert_notification_logs.balance_alert_id)",
                [],
            )?;
            // Phase 6: Delete orphaned balance alerts
            let alerts_deleted = tx.execute(
                "DELETE FROM balance_alerts WHERE NOT EXISTS (SELECT 1 FROM wallets w WHERE w.checksum = balance_alerts.wallet_checksum AND w.status != 'deleted')",
                [],
            )?;
            // Phase 7: Delete orphaned transactions (composite PK: txid, wallet_checksum)
            let txs_deleted = tx.execute(
                "DELETE FROM transactions WHERE NOT EXISTS (SELECT 1 FROM wallets w WHERE w.checksum = transactions.wallet_checksum AND w.status != 'deleted')",
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
