use bdk_wallet::rusqlite::{params, Connection};
use canary::MigrationRunner;
use std::fs;
use std::path::{Path, PathBuf};

fn copy_migrations_through(target_dir: &Path, max_version: u32) {
    let source_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations");
    for entry in fs::read_dir(source_dir).expect("read migrations dir") {
        let entry = entry.expect("read migration entry");
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("sql") {
            continue;
        }

        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("migration filename");
        let version = filename
            .split('_')
            .next()
            .expect("migration version")
            .parse::<u32>()
            .expect("numeric migration version");

        if version <= max_version {
            fs::copy(&path, target_dir.join(filename)).expect("copy migration");
        }
    }
}

fn seed_notification_log_with_method(db_path: &Path) {
    let conn = Connection::open(db_path).expect("open db");
    conn.execute_batch("PRAGMA foreign_keys = ON")
        .expect("enable foreign keys");

    conn.execute(
        "INSERT INTO users (id, email, password_hash, name) VALUES (?1, ?2, ?3, ?4)",
        params!["user-1", "user@example.com", "hash", "User"],
    )
    .expect("insert user");
    conn.execute(
        "INSERT INTO wallets (checksum, name, descriptor, hex_color, status, user_id, wallet_type)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            "wallet-1",
            "Wallet",
            "addr(bc1qexample000000000000000000000000000000000)",
            "#000000",
            "ready",
            "user-1",
            "address"
        ],
    )
    .expect("insert wallet");
    conn.execute(
        "INSERT INTO contacts (id, wallet_checksum, name, is_active)
         VALUES (?1, ?2, ?3, ?4)",
        params!["contact-1", "wallet-1", "Alice", 1],
    )
    .expect("insert contact");
    conn.execute(
        "INSERT INTO contact_notification_methods (
            id,
            contact_id,
            provider_type,
            notification_target,
            wallet_checksum,
            is_enabled
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            "method-1",
            "contact-1",
            "email",
            "alice@example.com",
            "wallet-1",
            1
        ],
    )
    .expect("insert notification method");
    conn.execute(
        "INSERT INTO transactions (
            txid,
            wallet_checksum,
            transaction_type,
            amount_sats,
            fee_sats,
            block_height,
            first_seen_at,
            confirmed_at,
            transaction_status
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            "tx-1",
            "wallet-1",
            "receive",
            1000,
            Option::<i64>::None,
            1,
            1_700_000_000_i64,
            1_700_000_600_i64,
            "confirmed"
        ],
    )
    .expect("insert transaction");
    conn.execute(
        "INSERT INTO notification_logs (
            id,
            transaction_txid,
            transaction_wallet_checksum,
            notification_method_id,
            provider_name,
            provider_message_id,
            status,
            message_content,
            notification_type,
            contact_name_snapshot,
            notification_target_snapshot,
            provider_type_snapshot
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            "log-1",
            "tx-1",
            "wallet-1",
            "method-1",
            "email",
            "provider-message-1",
            "sent",
            "message",
            "received",
            "Alice",
            "alice@example.com",
            "email"
        ],
    )
    .expect("insert notification log");
    conn.execute(
        "INSERT INTO balance_alerts (
            id, wallet_checksum, contact_id, threshold_sats, alert_type, is_active, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            "alert-1",
            "wallet-1",
            "contact-1",
            1000,
            "above",
            1,
            "2026-01-01T00:00:00Z"
        ],
    )
    .expect("insert balance alert");
    conn.execute(
        "INSERT INTO balance_alert_notification_logs (
            id,
            balance_alert_id,
            wallet_checksum,
            notification_method_id,
            provider_name,
            status,
            message_content
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            "balance-log-1",
            "alert-1",
            "wallet-1",
            "method-1",
            "email",
            "sent",
            "balance message"
        ],
    )
    .expect("insert balance alert notification log");
}

fn apply_partial_031(db_path: &Path) {
    let conn = Connection::open(db_path).expect("open db");
    // Keep this helper aligned with the early side effects of migration 031.
    // Last synced with 031_per_contact_notifications.sql on 2026-06-10.
    // It intentionally stops before notification_logs is dropped; the test
    // covers the recoverable partial states expected after the hardened
    // transactional migration. The historical notification_logs-absent state
    // documented in 031 still needs manual repair and is not auto-recoverable.
    conn.execute_batch(
        "
        ALTER TABLE contacts ADD COLUMN notify_sending BOOLEAN NOT NULL DEFAULT 1;
        ALTER TABLE contacts ADD COLUMN notify_sent BOOLEAN NOT NULL DEFAULT 1;
        ALTER TABLE contacts ADD COLUMN notify_receiving BOOLEAN NOT NULL DEFAULT 1;
        ALTER TABLE contacts ADD COLUMN notify_received BOOLEAN NOT NULL DEFAULT 1;
        ALTER TABLE contacts ADD COLUMN notify_cpfp BOOLEAN NOT NULL DEFAULT 1;
        ALTER TABLE contacts ADD COLUMN notify_rbf BOOLEAN NOT NULL DEFAULT 1;
        ALTER TABLE contacts ADD COLUMN include_wallet_balance_in_tx_notifications BOOLEAN NOT NULL DEFAULT 0;
        ALTER TABLE contact_notification_methods ADD COLUMN is_enabled BOOLEAN NOT NULL DEFAULT 1;
        ALTER TABLE balance_alerts ADD COLUMN contact_id TEXT REFERENCES contacts(id) ON DELETE CASCADE;
        ALTER TABLE balance_alert_notifications ADD COLUMN contact_id TEXT REFERENCES contacts(id) ON DELETE SET NULL;
        -- Simulate an interrupted 031 after fan-out has inserted the contact
        -- copy, but before the original wallet-level alert is deactivated and
        -- before schema_migrations records 031 as applied.
        INSERT INTO balance_alerts (
            id,
            wallet_checksum,
            threshold_sats,
            alert_type,
            is_active,
            created_at,
            threshold_currency,
            threshold_fiat_amount,
            last_checked_balance_sats,
            contact_id
        )
        SELECT
            'partial-' || ba.id || '-' || c.id,
            ba.wallet_checksum,
            ba.threshold_sats,
            ba.alert_type,
            ba.is_active,
            ba.created_at,
            ba.threshold_currency,
            ba.threshold_fiat_amount,
            ba.last_checked_balance_sats,
            c.id
        FROM balance_alerts ba
        JOIN contacts c ON c.wallet_checksum = ba.wallet_checksum
        WHERE ba.id IN ('alert-wallet-1', 'alert-wallet-2')
          AND c.id IN ('contact-1', 'contact-2');
        CREATE INDEX idx_balance_alerts_contact_id ON balance_alerts(contact_id);
        CREATE INDEX idx_balance_alerts_wallet_contact_active ON balance_alerts(wallet_checksum, contact_id, is_active);
        CREATE TABLE notification_logs_new (
            id TEXT PRIMARY KEY,
            transaction_txid TEXT NOT NULL,
            transaction_wallet_checksum TEXT NOT NULL,
            notification_method_id TEXT,
            provider_name TEXT NOT NULL,
            provider_message_id TEXT,
            status TEXT NOT NULL CHECK (status IN ('sent', 'failed', 'delivered')),
            error_message TEXT,
            message_content TEXT NOT NULL,
            notification_type TEXT NOT NULL DEFAULT 'pending' CHECK (notification_type IN ('pending', 'confirmed', 'balance_alert', 'sending', 'sent', 'receiving', 'received', 'cpfp', 'rbf')),
            contact_name_snapshot TEXT,
            notification_target_snapshot TEXT,
            provider_type_snapshot TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (transaction_txid, transaction_wallet_checksum) REFERENCES transactions (txid, wallet_checksum) ON DELETE CASCADE,
            FOREIGN KEY (notification_method_id) REFERENCES contact_notification_methods (id) ON DELETE SET NULL
        );
        ",
    )
    .expect("apply partial 031");
}

#[test]
fn migration_035_preserves_notification_log_method_references() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let db_path = temp_dir.path().join("metadata.sqlite");
    let migrations_dir = temp_dir.path().join("migrations");
    fs::create_dir(&migrations_dir).expect("create migrations dir");

    copy_migrations_through(&migrations_dir, 34);
    MigrationRunner::new(db_path.to_str().expect("db path"))
        .expect("create migration runner")
        .run_migrations(migrations_dir.to_str().expect("migrations path"))
        .expect("run migrations through 034");

    seed_notification_log_with_method(&db_path);

    copy_migrations_through(&migrations_dir, 35);
    MigrationRunner::new(db_path.to_str().expect("db path"))
        .expect("create migration runner")
        .run_migrations(migrations_dir.to_str().expect("migrations path"))
        .expect("run migrations through 035");

    let conn = Connection::open(&db_path).expect("open db");
    conn.execute_batch("PRAGMA foreign_keys = ON")
        .expect("enable foreign keys");

    let notification_method_id: Option<String> = conn
        .query_row(
            "SELECT notification_method_id FROM notification_logs WHERE id = 'log-1'",
            [],
            |row| row.get(0),
        )
        .expect("notification method id");
    assert_eq!(notification_method_id.as_deref(), Some("method-1"));

    let preserved_method_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM contact_notification_methods WHERE id = 'method-1'",
            [],
            |row| row.get(0),
        )
        .expect("preserved method count");
    assert_eq!(preserved_method_count, 1);

    let fk_violation_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .expect("foreign key violation count");
    assert_eq!(fk_violation_count, 0);
}

#[test]
fn migration_037_preserves_methods_indexes_and_foreign_keys_and_allows_reused_urls() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let db_path = temp_dir.path().join("metadata.sqlite");
    let migrations_dir = temp_dir.path().join("migrations");
    fs::create_dir(&migrations_dir).expect("create migrations dir");

    copy_migrations_through(&migrations_dir, 36);
    MigrationRunner::new(db_path.to_str().expect("db path"))
        .expect("create migration runner")
        .run_migrations(migrations_dir.to_str().expect("migrations path"))
        .expect("run migrations through 036");
    seed_notification_log_with_method(&db_path);

    copy_migrations_through(&migrations_dir, 37);
    MigrationRunner::new(db_path.to_str().expect("db path"))
        .expect("create migration runner")
        .run_migrations(migrations_dir.to_str().expect("migrations path"))
        .expect("run migration 037");

    let conn = Connection::open(&db_path).expect("open db");
    conn.execute_batch("PRAGMA foreign_keys = ON")
        .expect("enable foreign keys");

    let preserved_method_id: Option<String> = conn
        .query_row(
            "SELECT notification_method_id FROM notification_logs WHERE id = 'log-1'",
            [],
            |row| row.get(0),
        )
        .expect("preserved notification log reference");
    assert_eq!(preserved_method_id.as_deref(), Some("method-1"));
    let preserved_balance_method_id: Option<String> = conn
        .query_row(
            "SELECT notification_method_id FROM balance_alert_notification_logs
             WHERE id = 'balance-log-1'",
            [],
            |row| row.get(0),
        )
        .expect("preserved balance alert log reference");
    assert_eq!(preserved_balance_method_id.as_deref(), Some("method-1"));

    conn.execute(
        "INSERT INTO contacts (id, wallet_checksum, name, is_active)
         VALUES ('contact-2', 'wallet-1', 'Bob', 1)",
        [],
    )
    .expect("insert second contact");
    for (id, contact_id) in [("webhook-1", "contact-1"), ("webhook-2", "contact-2")] {
        conn.execute(
            "INSERT INTO contact_notification_methods
             (id, contact_id, provider_type, notification_target, wallet_checksum, is_enabled)
             VALUES (?1, ?2, 'webhook', 'http://receiver.local/hooks/canary?token=secret', 'wallet-1', 1)",
            params![id, contact_id],
        )
        .expect("reuse webhook URL across contacts");
    }

    for index_name in [
        "idx_contact_notification_methods_contact_id",
        "idx_contact_notification_methods_provider_type",
        "idx_contact_notification_methods_wallet_provider_target",
        "idx_unique_wallet_notification_target",
    ] {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
                [index_name],
                |row| row.get(0),
            )
            .expect("index lookup");
        assert_eq!(count, 1, "missing index {index_name}");
    }

    let fk_violation_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .expect("foreign key violation count");
    assert_eq!(fk_violation_count, 0);
}

fn seed_wallet_with_legacy_balance_alert(db_path: &Path) {
    let conn = Connection::open(db_path).expect("open db");
    conn.execute(
        "INSERT INTO users (id, email, password_hash, name) VALUES (?1, ?2, ?3, ?4)",
        params!["user-1", "user@example.com", "hash", "User"],
    )
    .expect("insert user");
    conn.execute(
        "INSERT INTO wallets (checksum, name, descriptor, hex_color, status, user_id, wallet_type)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            "wallet-1",
            "Wallet",
            "addr(bc1qexample000000000000000000000000000000000)",
            "#000000",
            "ready",
            "user-1",
            "address"
        ],
    )
    .expect("insert wallet");
    conn.execute(
        "INSERT INTO wallets (checksum, name, descriptor, hex_color, status, user_id, wallet_type)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            "wallet-no-contacts",
            "Wallet Without Contacts",
            "addr(bc1qexample111111111111111111111111111111111)",
            "#111111",
            "ready",
            "user-1",
            "address"
        ],
    )
    .expect("insert wallet without contacts");
    conn.execute(
        "INSERT INTO contacts (id, wallet_checksum, name, is_active)
         VALUES (?1, ?2, ?3, ?4)",
        params!["contact-1", "wallet-1", "Contact", 1],
    )
    .expect("insert contact");
    conn.execute(
        "INSERT INTO contacts (id, wallet_checksum, name, is_active)
         VALUES (?1, ?2, ?3, ?4)",
        params!["contact-2", "wallet-1", "Second Contact", 1],
    )
    .expect("insert second contact");
    conn.execute(
        "INSERT INTO balance_alerts (
            id,
            wallet_checksum,
            threshold_sats,
            alert_type,
            is_active,
            created_at,
            threshold_currency,
            threshold_fiat_amount,
            last_checked_balance_sats
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            "alert-wallet-1",
            "wallet-1",
            0,
            "equals",
            1,
            "2026-01-01 00:00:00",
            Option::<String>::None,
            Option::<f64>::None,
            0
        ],
    )
    .expect("insert balance alert");
    conn.execute(
        "INSERT INTO balance_alerts (
            id,
            wallet_checksum,
            threshold_sats,
            alert_type,
            is_active,
            created_at,
            threshold_currency,
            threshold_fiat_amount,
            last_checked_balance_sats
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            "alert-wallet-2",
            "wallet-1",
            100,
            "below",
            1,
            "2026-01-01 00:00:01",
            Option::<String>::None,
            Option::<f64>::None,
            200
        ],
    )
    .expect("insert second balance alert");
    conn.execute(
        "INSERT INTO balance_alerts (
            id,
            wallet_checksum,
            threshold_sats,
            alert_type,
            is_active,
            created_at,
            threshold_currency,
            threshold_fiat_amount,
            last_checked_balance_sats
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            "alert-wallet-no-contacts",
            "wallet-no-contacts",
            50,
            "below",
            1,
            "2026-01-01 00:00:02",
            Option::<String>::None,
            Option::<f64>::None,
            75
        ],
    )
    .expect("insert no-contact balance alert");
    conn.execute(
        "INSERT INTO balance_alerts (
            id,
            wallet_checksum,
            threshold_sats,
            alert_type,
            is_active,
            created_at,
            threshold_currency,
            threshold_fiat_amount,
            last_checked_balance_sats
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            "alert-wallet-inactive",
            "wallet-1",
            999,
            "above",
            0,
            "2026-01-01 00:00:03",
            Option::<String>::None,
            Option::<f64>::None,
            500
        ],
    )
    .expect("insert inactive balance alert");
    conn.execute(
        "INSERT INTO transactions (
            txid,
            wallet_checksum,
            transaction_type,
            amount_sats,
            first_seen_at
         )
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params!["txid-1", "wallet-1", "receive", 5000, 1_789_000_000_i64],
    )
    .expect("insert transaction");
    conn.execute(
        "INSERT INTO notification_logs (
            id,
            transaction_txid,
            transaction_wallet_checksum,
            notification_method_id,
            provider_name,
            status,
            message_content,
            notification_type
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            "log-1",
            "txid-1",
            "wallet-1",
            Option::<String>::None,
            "email",
            "sent",
            "original notification",
            "pending"
        ],
    )
    .expect("insert notification log");
}

fn assert_migration_031_032_state(conn: &Connection) {
    let latest_version: String = conn
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("latest migration version");
    assert_eq!(latest_version, "032");

    let notification_logs_new_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'notification_logs_new'",
            [],
            |row| row.get(0),
        )
        .expect("notification_logs_new count");
    assert_eq!(notification_logs_new_count, 0);

    let notification_logs_sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'notification_logs'",
            [],
            |row| row.get(0),
        )
        .expect("notification_logs schema");
    assert!(notification_logs_sql.contains("'rbf'"));

    conn.query_row(
        "SELECT notify_sending FROM contacts LIMIT 1",
        [],
        |_| Ok(()),
    )
    .expect("notify_sending column should exist");

    let notification_log_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM notification_logs", [], |row| {
            row.get(0)
        })
        .expect("notification log count");
    assert_eq!(notification_log_count, 1);

    let notification_log_content: String = conn
        .query_row(
            "SELECT message_content FROM notification_logs WHERE id = 'log-1'",
            [],
            |row| row.get(0),
        )
        .expect("notification log content");
    assert_eq!(notification_log_content, "original notification");

    let contact_alert_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM balance_alerts
             WHERE wallet_checksum = 'wallet-1'
               AND contact_id IN ('contact-1', 'contact-2')
               AND (
                   (threshold_sats = 0 AND alert_type = 'equals')
                   OR (threshold_sats = 100 AND alert_type = 'below')
               )",
            [],
            |row| row.get(0),
        )
        .expect("contact-level alert count");
    assert_eq!(contact_alert_count, 4);

    let migrated_wallet_alert_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM balance_alerts
             WHERE id IN ('alert-wallet-1', 'alert-wallet-2')",
            [],
            |row| row.get(0),
        )
        .expect("migrated wallet-level alert count");
    assert_eq!(migrated_wallet_alert_count, 0);

    let no_contact_alert_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM balance_alerts
             WHERE id = 'alert-wallet-no-contacts'
               AND wallet_checksum = 'wallet-no-contacts'
               AND contact_id IS NULL
               AND is_active = 1",
            [],
            |row| row.get(0),
        )
        .expect("no-contact alert count");
    assert_eq!(no_contact_alert_count, 1);

    let inactive_contact_alert_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM balance_alerts
             WHERE wallet_checksum = 'wallet-1'
               AND contact_id IS NOT NULL
               AND threshold_sats = 999
               AND alert_type = 'above'",
            [],
            |row| row.get(0),
        )
        .expect("inactive contact alert count");
    assert_eq!(inactive_contact_alert_count, 0);

    let inactive_wallet_alert_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM balance_alerts
             WHERE id = 'alert-wallet-inactive'
               AND contact_id IS NULL
               AND is_active = 0",
            [],
            |row| row.get(0),
        )
        .expect("inactive wallet alert count");
    assert_eq!(inactive_wallet_alert_count, 1);
}

#[test]
fn migration_031_recovers_from_partially_applied_schema() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let migrations_dir = temp_dir.path().join("migrations");
    fs::create_dir(&migrations_dir).expect("create migrations dir");
    let db_path = temp_dir.path().join("metadata.sqlite");

    copy_migrations_through(&migrations_dir, 30);
    MigrationRunner::new(db_path.to_str().expect("db path"))
        .expect("create migration runner")
        .run_migrations(migrations_dir.to_str().expect("migrations path"))
        .expect("run migrations through 030");

    seed_wallet_with_legacy_balance_alert(&db_path);
    apply_partial_031(&db_path);

    // Run through 032 because 032 removes the inactive wallet-level originals
    // created by a successful 031 fan-out.
    copy_migrations_through(&migrations_dir, 32);
    MigrationRunner::new(db_path.to_str().expect("db path"))
        .expect("create migration runner")
        .run_migrations(migrations_dir.to_str().expect("migrations path"))
        .expect("resume migrations through 032");

    let conn = bdk_wallet::rusqlite::Connection::open(&db_path).expect("open db");
    assert_migration_031_032_state(&conn);
}

#[test]
fn migration_031_handles_clean_first_run_and_wallets_without_contacts() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let migrations_dir = temp_dir.path().join("migrations");
    fs::create_dir(&migrations_dir).expect("create migrations dir");
    let db_path = temp_dir.path().join("metadata.sqlite");

    copy_migrations_through(&migrations_dir, 30);
    MigrationRunner::new(db_path.to_str().expect("db path"))
        .expect("create migration runner")
        .run_migrations(migrations_dir.to_str().expect("migrations path"))
        .expect("run migrations through 030");

    seed_wallet_with_legacy_balance_alert(&db_path);

    copy_migrations_through(&migrations_dir, 32);
    MigrationRunner::new(db_path.to_str().expect("db path"))
        .expect("create migration runner")
        .run_migrations(migrations_dir.to_str().expect("migrations path"))
        .expect("run migrations through 032");

    let conn = bdk_wallet::rusqlite::Connection::open(&db_path).expect("open db");
    assert_migration_031_032_state(&conn);
}

#[test]
fn migration_033_cleans_active_wallet_level_alerts_with_contacts() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let migrations_dir = temp_dir.path().join("migrations");
    fs::create_dir(&migrations_dir).expect("create migrations dir");
    let db_path = temp_dir.path().join("metadata.sqlite");

    copy_migrations_through(&migrations_dir, 32);
    MigrationRunner::new(db_path.to_str().expect("db path"))
        .expect("create migration runner")
        .run_migrations(migrations_dir.to_str().expect("migrations path"))
        .expect("run migrations through 032");

    let conn = Connection::open(&db_path).expect("open db");
    conn.execute(
        "INSERT INTO users (id, email, password_hash, name) VALUES (?1, ?2, ?3, ?4)",
        params!["user-1", "user@example.com", "hash", "User"],
    )
    .expect("insert user");
    conn.execute(
        "INSERT INTO wallets (checksum, name, descriptor, hex_color, status, user_id, wallet_type)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            "wallet-1",
            "Wallet",
            "addr(bc1qexample000000000000000000000000000000000)",
            "#000000",
            "ready",
            "user-1",
            "address"
        ],
    )
    .expect("insert wallet");
    conn.execute(
        "INSERT INTO wallets (checksum, name, descriptor, hex_color, status, user_id, wallet_type)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            "wallet-no-contacts",
            "Wallet Without Contacts",
            "addr(bc1qexample111111111111111111111111111111111)",
            "#111111",
            "ready",
            "user-1",
            "address"
        ],
    )
    .expect("insert wallet without contacts");
    conn.execute(
        "INSERT INTO contacts (id, wallet_checksum, name, is_active)
         VALUES (?1, ?2, ?3, ?4)",
        params!["contact-1", "wallet-1", "Contact", 1],
    )
    .expect("insert contact");
    conn.execute(
        "INSERT INTO contacts (id, wallet_checksum, name, is_active)
         VALUES (?1, ?2, ?3, ?4)",
        params!["contact-2", "wallet-1", "Second Contact", 1],
    )
    .expect("insert second contact");
    conn.execute(
        "INSERT INTO contacts (id, wallet_checksum, name, is_active)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            "inactive-contact-1",
            "wallet-no-contacts",
            "Inactive Contact",
            0
        ],
    )
    .expect("insert inactive contact");
    conn.execute(
        "INSERT INTO balance_alerts (
            id,
            wallet_checksum,
            threshold_sats,
            alert_type,
            is_active,
            created_at,
            last_checked_balance_sats
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            "post-032-wallet-alert",
            "wallet-1",
            21_000_000,
            "above",
            1,
            "2026-06-11T06:41:33.907501+00:00",
            16_999_436
        ],
    )
    .expect("insert post-032 wallet-level alert");
    conn.execute(
        "INSERT INTO balance_alerts (
            id,
            wallet_checksum,
            threshold_sats,
            alert_type,
            is_active,
            created_at,
            last_checked_balance_sats,
            contact_id
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            "manually-recreated-contact-alert",
            "wallet-1",
            21_000_000,
            "above",
            1,
            "2026-06-11T07:00:00.000000+00:00",
            16_999_436,
            "contact-2"
        ],
    )
    .expect("insert manually recreated contact-level alert");
    conn.execute(
        "INSERT INTO balance_alerts (
            id,
            wallet_checksum,
            threshold_sats,
            alert_type,
            is_active,
            created_at,
            last_checked_balance_sats
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            "post-032-wallet-alert-with-history",
            "wallet-1",
            42_000_000,
            "below",
            1,
            "2026-06-11T07:01:00.000000+00:00",
            50_000_000
        ],
    )
    .expect("insert post-032 wallet-level alert with history");
    conn.execute(
        "INSERT INTO balance_alert_notifications (
            id,
            balance_alert_id,
            wallet_checksum,
            threshold_sats,
            current_balance_sats,
            alert_type,
            notification_sent_at
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            "history-notification-1",
            "post-032-wallet-alert-with-history",
            "wallet-1",
            42_000_000,
            41_000_000,
            "below",
            1_782_000_000_i64
        ],
    )
    .expect("insert balance alert notification history");
    conn.execute(
        "INSERT INTO balance_alerts (
            id,
            wallet_checksum,
            threshold_sats,
            alert_type,
            is_active,
            created_at,
            last_checked_balance_sats
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            "post-032-wallet-alert-with-log-history",
            "wallet-1",
            84_000_000,
            "above",
            1,
            "2026-06-11T07:02:00.000000+00:00",
            85_000_000
        ],
    )
    .expect("insert post-032 wallet-level alert with log history");
    conn.execute(
        "INSERT INTO balance_alert_notification_logs (
            id,
            balance_alert_id,
            wallet_checksum,
            provider_name,
            status,
            message_content
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            "history-log-1",
            "post-032-wallet-alert-with-log-history",
            "wallet-1",
            "email",
            "sent",
            "Balance alert sent"
        ],
    )
    .expect("insert balance alert notification log history");
    conn.execute(
        "INSERT INTO balance_alerts (
            id,
            wallet_checksum,
            threshold_sats,
            alert_type,
            is_active,
            created_at,
            last_checked_balance_sats
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            "wallet-alert-with-inactive-contact-match",
            "wallet-no-contacts",
            1_234_567,
            "equals",
            1,
            "2026-06-11T07:03:00.000000+00:00",
            1_234_567
        ],
    )
    .expect("insert wallet alert with inactive contact match");
    conn.execute(
        "INSERT INTO balance_alerts (
            id,
            wallet_checksum,
            threshold_sats,
            alert_type,
            is_active,
            created_at,
            last_checked_balance_sats,
            contact_id
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            "inactive-contact-matching-alert",
            "wallet-no-contacts",
            1_234_567,
            "equals",
            1,
            "2026-06-11T07:03:30.000000+00:00",
            1_234_567,
            "inactive-contact-1"
        ],
    )
    .expect("insert inactive contact matching alert");
    conn.execute(
        "INSERT INTO balance_alerts (
            id,
            wallet_checksum,
            threshold_sats,
            alert_type,
            is_active,
            created_at,
            last_checked_balance_sats
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            "post-032-no-contact-alert",
            "wallet-no-contacts",
            0,
            "equals",
            1,
            "2026-06-11T06:41:33.907104+00:00",
            16_999_436
        ],
    )
    .expect("insert no-contact wallet-level alert");
    drop(conn);

    copy_migrations_through(&migrations_dir, 33);
    MigrationRunner::new(db_path.to_str().expect("db path"))
        .expect("create migration runner")
        .run_migrations(migrations_dir.to_str().expect("migrations path"))
        .expect("run migrations through 033");

    let conn = Connection::open(&db_path).expect("open db");
    let latest_version: String = conn
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("latest migration version");
    assert_eq!(latest_version, "033");

    let original_alert_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM balance_alerts WHERE id = 'post-032-wallet-alert'",
            [],
            |row| row.get(0),
        )
        .expect("original alert count");
    assert_eq!(original_alert_count, 0);

    let contact_alert_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM balance_alerts
             WHERE wallet_checksum = 'wallet-1'
               AND contact_id IN ('contact-1', 'contact-2')
               AND threshold_sats = 21000000
               AND alert_type = 'above'
               AND is_active = 1",
            [],
            |row| row.get(0),
        )
        .expect("contact alert count");
    assert_eq!(contact_alert_count, 2);

    let existing_contact_alert_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM balance_alerts
             WHERE id = 'manually-recreated-contact-alert'
               AND wallet_checksum = 'wallet-1'
               AND contact_id = 'contact-2'
               AND threshold_sats = 21000000
               AND alert_type = 'above'
               AND is_active = 1",
            [],
            |row| row.get(0),
        )
        .expect("existing contact alert count");
    assert_eq!(existing_contact_alert_count, 1);

    let historical_original_is_active: i64 = conn
        .query_row(
            "SELECT is_active FROM balance_alerts
             WHERE id = 'post-032-wallet-alert-with-history'
               AND contact_id IS NULL",
            [],
            |row| row.get(0),
        )
        .expect("historical original active state");
    assert_eq!(historical_original_is_active, 0);

    let historical_contact_alert_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM balance_alerts
             WHERE wallet_checksum = 'wallet-1'
               AND contact_id IN ('contact-1', 'contact-2')
               AND threshold_sats = 42000000
               AND alert_type = 'below'
               AND is_active = 1",
            [],
            |row| row.get(0),
        )
        .expect("historical contact alert count");
    assert_eq!(historical_contact_alert_count, 2);

    let preserved_notification_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM balance_alert_notifications
             WHERE balance_alert_id = 'post-032-wallet-alert-with-history'",
            [],
            |row| row.get(0),
        )
        .expect("preserved notification count");
    assert_eq!(preserved_notification_count, 1);

    let log_history_original_is_active: i64 = conn
        .query_row(
            "SELECT is_active FROM balance_alerts
             WHERE id = 'post-032-wallet-alert-with-log-history'
               AND contact_id IS NULL",
            [],
            |row| row.get(0),
        )
        .expect("log history original active state");
    assert_eq!(log_history_original_is_active, 0);

    let log_history_contact_alert_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM balance_alerts
             WHERE wallet_checksum = 'wallet-1'
               AND contact_id IN ('contact-1', 'contact-2')
               AND threshold_sats = 84000000
               AND alert_type = 'above'
               AND is_active = 1",
            [],
            |row| row.get(0),
        )
        .expect("log history contact alert count");
    assert_eq!(log_history_contact_alert_count, 2);

    let preserved_log_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM balance_alert_notification_logs
             WHERE balance_alert_id = 'post-032-wallet-alert-with-log-history'",
            [],
            |row| row.get(0),
        )
        .expect("preserved log count");
    assert_eq!(preserved_log_count, 1);

    let inactive_contact_wallet_alert_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM balance_alerts
             WHERE id = 'wallet-alert-with-inactive-contact-match'
               AND wallet_checksum = 'wallet-no-contacts'
               AND contact_id IS NULL
               AND is_active = 1",
            [],
            |row| row.get(0),
        )
        .expect("inactive contact wallet alert count");
    assert_eq!(inactive_contact_wallet_alert_count, 1);

    let no_contact_alert_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM balance_alerts
             WHERE id = 'post-032-no-contact-alert'
               AND wallet_checksum = 'wallet-no-contacts'
               AND contact_id IS NULL
               AND is_active = 1",
            [],
            |row| row.get(0),
        )
        .expect("no-contact alert count");
    assert_eq!(no_contact_alert_count, 1);
    drop(conn);

    copy_migrations_through(&migrations_dir, 34);
    MigrationRunner::new(db_path.to_str().expect("db path"))
        .expect("create migration runner")
        .run_migrations(migrations_dir.to_str().expect("migrations path"))
        .expect("run migrations through 034");

    let conn = Connection::open(&db_path).expect("open db");
    let latest_version: String = conn
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("latest migration version");
    assert_eq!(latest_version, "034");

    let remaining_wallet_level_alert_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM balance_alerts WHERE contact_id IS NULL",
            [],
            |row| row.get(0),
        )
        .expect("remaining wallet-level alert count");
    assert_eq!(remaining_wallet_level_alert_count, 0);

    let remaining_wallet_level_notification_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM balance_alert_notifications
             WHERE balance_alert_id IN (
                 'post-032-wallet-alert-with-history',
                 'post-032-wallet-alert-with-log-history'
             )",
            [],
            |row| row.get(0),
        )
        .expect("remaining wallet-level notification count");
    assert_eq!(remaining_wallet_level_notification_count, 0);

    let remaining_wallet_level_log_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM balance_alert_notification_logs
             WHERE balance_alert_id IN (
                 'post-032-wallet-alert-with-history',
                 'post-032-wallet-alert-with-log-history'
             )",
            [],
            |row| row.get(0),
        )
        .expect("remaining wallet-level log count");
    assert_eq!(remaining_wallet_level_log_count, 0);

    let remaining_contact_alert_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM balance_alerts WHERE contact_id IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .expect("remaining contact alert count");
    assert_eq!(remaining_contact_alert_count, 7);
}

#[test]
fn migration_043_preserves_existing_content_and_constrains_privacy_levels() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let db_path = temp_dir.path().join("metadata.sqlite");
    let migrations_dir = temp_dir.path().join("migrations");
    fs::create_dir(&migrations_dir).expect("create migrations dir");

    copy_migrations_through(&migrations_dir, 42);
    MigrationRunner::new(db_path.to_str().expect("db path"))
        .expect("create migration runner")
        .run_migrations(migrations_dir.to_str().expect("migrations path"))
        .expect("run migrations through 042");
    seed_notification_log_with_method(&db_path);

    copy_migrations_through(&migrations_dir, 43);
    MigrationRunner::new(db_path.to_str().expect("db path"))
        .expect("create migration runner")
        .run_migrations(migrations_dir.to_str().expect("migrations path"))
        .expect("run migration 043");

    let conn = Connection::open(&db_path).expect("open db");
    let migrated_level: String = conn
        .query_row(
            "SELECT content_privacy_level FROM contact_notification_methods WHERE id = 'method-1'",
            [],
            |row| row.get(0),
        )
        .expect("migrated privacy level");
    assert_eq!(migrated_level, "detailed");

    for (id, topic, level) in [
        ("method-standard", "topic-standard", "standard"),
        ("method-minimal", "topic-minimal", "minimal"),
    ] {
        conn.execute(
            "INSERT INTO contact_notification_methods
             (id, contact_id, provider_type, notification_target, wallet_checksum, is_enabled, content_privacy_level)
             VALUES (?1, 'contact-1', 'ntfy', ?2, 'wallet-1', 1, ?3)",
            params![id, topic, level],
        )
        .expect("insert supported privacy level");
    }
    assert!(conn
        .execute(
            "INSERT INTO contact_notification_methods
             (id, contact_id, provider_type, notification_target, wallet_checksum, is_enabled, content_privacy_level)
             VALUES ('method-invalid', 'contact-1', 'ntfy', 'topic-invalid', 'wallet-1', 1, 'verbose')",
            [],
        )
        .is_err());
}

#[test]
fn migration_045_translates_v152_and_migration_044_databases() {
    for applied_through in [43, 44] {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let db_path = temp_dir.path().join("metadata.sqlite");
        let migrations_dir = temp_dir.path().join("migrations");
        fs::create_dir(&migrations_dir).expect("create migrations dir");

        copy_migrations_through(&migrations_dir, applied_through);
        MigrationRunner::new(db_path.to_str().expect("db path"))
            .expect("create migration runner")
            .run_migrations(migrations_dir.to_str().expect("migrations path"))
            .expect("run pre-045 migrations");
        seed_notification_log_with_method(&db_path);

        let conn = Connection::open(&db_path).expect("open pre-045 db");
        conn.execute(
            "UPDATE contacts
             SET include_wallet_balance_in_tx_notifications = 1
             WHERE id = 'contact-1'",
            [],
        )
        .expect("enable legacy transaction balance");
        for (id, target, level) in [
            ("method-minimal", "minimal@example.com", "minimal"),
            ("method-standard", "standard@example.com", "standard"),
            ("method-detailed", "detailed@example.com", "detailed"),
        ] {
            conn.execute(
                "INSERT INTO contact_notification_methods
                 (id, contact_id, provider_type, notification_target, wallet_checksum,
                  is_enabled, content_privacy_level)
                 VALUES (?1, 'contact-1', 'email', ?2, 'wallet-1', 1, ?3)",
                params![id, target, level],
            )
            .expect("insert legacy method");
        }
        drop(conn);

        copy_migrations_through(&migrations_dir, 45);
        MigrationRunner::new(db_path.to_str().expect("db path"))
            .expect("create migration runner")
            .run_migrations(migrations_dir.to_str().expect("migrations path"))
            .expect("run migration 045");

        let conn = Connection::open(&db_path).expect("open migrated db");
        let read_fields = |method_id: &str| -> (i64, i64, i64, i64, i64, i64, i64) {
            conn.query_row(
                "SELECT content_wallet_name, content_event_type,
                        content_transaction_amount, content_transaction_balance,
                        content_balance_alert_condition, content_balance_alert_threshold,
                        content_balance_alert_balance
                 FROM contact_notification_methods WHERE id = ?1",
                [method_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .expect("read migrated content fields")
        };

        assert_eq!(read_fields("method-minimal"), (0, 0, 0, 0, 0, 0, 0));
        assert_eq!(read_fields("method-standard"), (1, 1, 0, 0, 0, 0, 0));
        assert_eq!(read_fields("method-detailed"), (1, 1, 1, 1, 1, 1, 1));
        assert_eq!(read_fields("method-1"), (1, 1, 1, 1, 1, 1, 1));
    }
}
