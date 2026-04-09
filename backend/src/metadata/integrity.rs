use super::pool::MetadataDb;
use anyhow::Result;
use bdk_wallet::rusqlite::params;
use serde_json::json;
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

impl CleanupCounts {
    pub fn total_deleted(&self) -> usize {
        self.contacts_deleted
            + self.methods_deleted
            + self.logs_deleted
            + self.alert_logs_deleted
            + self.alerts_deleted
            + self.transactions_deleted
    }
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
    pub async fn find_orphaned_balance_alert_notification_logs(
        &self,
    ) -> Result<Vec<OrphanedRecord>> {
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

        let (contacts_r, methods_r, logs_r, txs_r, alerts_r, alert_logs_r) = tokio::join!(
            self.find_orphaned_contacts(),
            self.find_orphaned_notification_methods(),
            self.find_orphaned_notification_logs(),
            self.find_orphaned_transactions(),
            self.find_orphaned_balance_alerts(),
            self.find_orphaned_balance_alert_notification_logs(),
        );

        let orphan_checks: [(&str, Result<Vec<_>, _>); 6] = [
            ("contacts", contacts_r),
            ("notification methods", methods_r),
            ("notification logs", logs_r),
            ("transactions", txs_r),
            ("balance alerts", alerts_r),
            ("balance alert notification logs", alert_logs_r),
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
    /// Deletes in correct dependency order (children before parents) and records
    /// the admin operation in the persistent audit log.
    pub async fn run_cleanup(&self, actor_user_id: &str) -> Result<CleanupCounts> {
        let pool = self.pool.clone();
        let actor_user_id = actor_user_id.to_string();
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
            // Notification logs with NULL method_id (set by ON DELETE SET NULL
            // from phase 2) are intentionally preserved as audit records.
            let logs_deleted = logs_deleted_phase1;
            // Phase 4: Delete orphaned balance alert notification logs
            let alert_logs_deleted = tx.execute(
                "DELETE FROM balance_alert_notification_logs WHERE NOT EXISTS (SELECT 1 FROM balance_alerts ba WHERE ba.id = balance_alert_notification_logs.balance_alert_id)",
                [],
            )?;
            // Phase 5: Delete orphaned balance alerts
            let alerts_deleted = tx.execute(
                "DELETE FROM balance_alerts WHERE NOT EXISTS (SELECT 1 FROM wallets w WHERE w.checksum = balance_alerts.wallet_checksum AND w.status != 'deleted')",
                [],
            )?;
            // Phase 6: Delete orphaned transactions (composite PK: txid, wallet_checksum)
            let txs_deleted = tx.execute(
                "DELETE FROM transactions WHERE NOT EXISTS (SELECT 1 FROM wallets w WHERE w.checksum = transactions.wallet_checksum AND w.status != 'deleted')",
                [],
            )?;

            // Record every auto-fix invocation, including no-op cleanups, so
            // admins can see who ran the operation and when.
            let counts = CleanupCounts {
                contacts_deleted,
                methods_deleted,
                logs_deleted,
                alert_logs_deleted,
                alerts_deleted,
                transactions_deleted: txs_deleted,
            };
            let audit_id = uuid::Uuid::new_v4().to_string();
            let details = json!({
                "contacts_deleted": counts.contacts_deleted,
                "methods_deleted": counts.methods_deleted,
                "logs_deleted": counts.logs_deleted,
                "balance_alert_notification_logs_deleted": counts.alert_logs_deleted,
                "balance_alerts_deleted": counts.alerts_deleted,
                "transactions_deleted": counts.transactions_deleted,
                "total_deleted": counts.total_deleted(),
            })
            .to_string();
            tx.execute(
                "INSERT INTO admin_audit_log (id, actor_user_id, operation, target, details_json)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    audit_id,
                    actor_user_id,
                    "database_integrity_auto_fix",
                    "metadata_database",
                    details
                ],
            )?;

            tx.commit()?;

            Ok(counts)
        })
        .await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::MigrationRunner;
    use r2d2::Pool;
    use r2d2_sqlite::SqliteConnectionManager;
    use tempfile::{tempdir, TempDir};

    async fn create_test_db() -> (MetadataDb, TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("metadata.sqlite");
        let db_path = db_path.to_str().unwrap();
        let migration_runner = MigrationRunner::new(db_path).unwrap();
        migration_runner
            .run_migrations(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations"))
            .unwrap();
        // Close the migration connection before opening the pooled connection.
        drop(migration_runner.get_connection());

        let manager = SqliteConnectionManager::file(db_path);
        let pool = Pool::builder().max_size(4).build(manager).unwrap();
        (
            MetadataDb {
                pool: std::sync::Arc::new(pool),
            },
            temp_dir,
        )
    }

    fn insert_orphaned_contact(db: &MetadataDb) {
        let conn = db.pool.get().unwrap();
        conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
        conn.execute(
            "INSERT INTO contacts (id, wallet_checksum, name)
             VALUES ('orphan-contact', 'missing-wallet', 'Orphan Contact')",
            [],
        )
        .unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();
    }

    #[tokio::test]
    async fn cleanup_writes_admin_audit_log_with_counts() {
        let (db, _temp_dir) = create_test_db().await;

        insert_orphaned_contact(&db);

        let counts = db.run_cleanup("admin-user-id").await.unwrap();

        assert_eq!(counts.contacts_deleted, 1);

        let conn = db.pool.get().unwrap();
        let (actor_user_id, operation, target, details_json): (String, String, String, String) =
            conn.query_row(
                "SELECT actor_user_id, operation, target, details_json FROM admin_audit_log",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        let details: serde_json::Value = serde_json::from_str(&details_json).unwrap();

        assert_eq!(actor_user_id, "admin-user-id");
        assert_eq!(operation, "database_integrity_auto_fix");
        assert_eq!(target, "metadata_database");
        assert_eq!(details["contacts_deleted"], 1);
        assert_eq!(details["methods_deleted"], 0);
        assert_eq!(details["logs_deleted"], 0);
        assert_eq!(details["balance_alert_notification_logs_deleted"], 0);
        assert_eq!(details["balance_alerts_deleted"], 0);
        assert_eq!(details["transactions_deleted"], 0);
        assert_eq!(details["total_deleted"], 1);
    }

    #[tokio::test]
    async fn cleanup_writes_admin_audit_log_for_noop() {
        let (db, _temp_dir) = create_test_db().await;

        let counts = db.run_cleanup("admin-user-id").await.unwrap();

        assert_eq!(counts.contacts_deleted, 0);

        let conn = db.pool.get().unwrap();
        let details_json: String = conn
            .query_row("SELECT details_json FROM admin_audit_log", [], |row| {
                row.get(0)
            })
            .unwrap();
        let details: serde_json::Value = serde_json::from_str(&details_json).unwrap();

        assert_eq!(details["contacts_deleted"], 0);
        assert_eq!(details["methods_deleted"], 0);
        assert_eq!(details["logs_deleted"], 0);
        assert_eq!(details["balance_alert_notification_logs_deleted"], 0);
        assert_eq!(details["balance_alerts_deleted"], 0);
        assert_eq!(details["transactions_deleted"], 0);
        assert_eq!(details["total_deleted"], 0);
    }

    #[tokio::test]
    async fn cleanup_rolls_back_when_audit_insert_fails() {
        let (db, _temp_dir) = create_test_db().await;
        insert_orphaned_contact(&db);

        {
            let conn = db.pool.get().unwrap();
            conn.execute("DROP TABLE admin_audit_log", []).unwrap();
        }

        let err = db.run_cleanup("admin-user-id").await.unwrap_err();

        assert!(err.to_string().contains("no such table: admin_audit_log"));

        let conn = db.pool.get().unwrap();
        let orphan_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM contacts WHERE id = 'orphan-contact'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(orphan_count, 1);
    }
}
