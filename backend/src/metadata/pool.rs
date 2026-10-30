//! [`MetadataDb`] itself: opening and migrating the database, the r2d2 connection
//! pool, and `with_conn`, which runs blocking queries off the async runtime.

use crate::config::{AppConfig, SyncIntervalOverrides};
use crate::migrations::MigrationRunner;
use anyhow::{Context, Result};
use bdk_wallet::rusqlite::Connection;
use r2d2::{CustomizeConnection, Pool};
use r2d2_sqlite::SqliteConnectionManager;
use std::sync::Arc;
use tokio::task::spawn_blocking;
use tracing::{error, warn};

#[derive(Debug)]
struct SqliteConnectionInitializer;

impl CustomizeConnection<Connection, bdk_wallet::rusqlite::Error> for SqliteConnectionInitializer {
    fn on_acquire(&self, conn: &mut Connection) -> Result<(), bdk_wallet::rusqlite::Error> {
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;",
        )
    }
}

pub(crate) type DbPool = Pool<SqliteConnectionManager>;

#[derive(Clone)]
pub struct MetadataDb {
    pub(crate) pool: Arc<DbPool>,
    /// Cloud tier sync interval overrides, loaded for the configured network.
    pub(crate) sync_interval_overrides: SyncIntervalOverrides,
}

impl MetadataDb {
    pub async fn new(db_path: &str, config: &AppConfig) -> Result<Self> {
        // Migrations, file permissions and pool creation are synchronous SQLite and file I/O.
        let path = db_path.to_string();
        let pool = spawn_blocking(move || open_pool(&path))
            .await
            .context("Database setup task failed")??;

        let db = MetadataDb {
            pool: Arc::new(pool),
            sync_interval_overrides: config.sync_interval_overrides(),
        };

        // Initialize user based on operating mode
        db.initialize_user_for_mode(config).await?;

        Ok(db)
    }

    /// Runs `f` with a pooled connection on the blocking thread pool.
    ///
    /// `SQLite` calls block, so they must not run on an async task. `f` gets
    /// `&mut Connection` so it can open its own transaction; the helper does not
    /// start or commit one.
    pub(super) async fn with_conn<T, F>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut Connection) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let pool = Arc::clone(&self.pool);
        spawn_blocking(move || {
            let mut conn = pool.get()?;
            f(&mut conn)
        })
        .await?
    }
}

/// Migrates the database at `db_path`, restricts its file permissions and opens the pool.
fn open_pool(db_path: &str) -> Result<DbPool> {
    // Create parent directory if it doesn't exist
    if let Some(parent) = std::path::Path::new(db_path).parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        warn!("Failed to create database directory: {}", e);
    }

    // Run migrations first
    let migration_runner = MigrationRunner::new(db_path)?;
    // Try multiple migration paths (for development and production)
    let migration_paths = ["./migrations", "../migrations", "migrations"];
    let mut migrations_run = false;
    for path in &migration_paths {
        if std::path::Path::new(path).exists() {
            if let Err(e) = migration_runner.run_migrations(path) {
                error!("Migration error with path {}: {}", path, e);
            } else {
                migrations_run = true;
                break;
            }
        }
    }
    if !migrations_run {
        warn!(
            "No migrations directory found in any of: {:?}",
            migration_paths
        );
    }

    let conn = migration_runner.get_connection();
    conn.execute_batch("PRAGMA journal_mode = WAL;")
        .context("Failed to initialize SQLite journal mode")?;
    drop(conn);

    #[cfg(unix)]
    for path in [
        db_path.to_string(),
        format!("{db_path}-wal"),
        format!("{db_path}-shm"),
    ] {
        if std::path::Path::new(&path).exists()
            && let Err(e) =
                std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o600))
        {
            warn!(
                "Failed to restrict SQLite file permissions for {}: {}",
                path, e
            );
        }
    }

    // Create connection pool with foreign key enforcement
    let manager = SqliteConnectionManager::file(db_path);
    Pool::builder()
        .max_size(16)
        .connection_customizer(Box::new(SqliteConnectionInitializer))
        .build(manager)
        .context("Failed to create database pool")
}
