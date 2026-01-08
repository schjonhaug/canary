use crate::config::AppConfig;
use crate::migrations::MigrationRunner;
use anyhow::{Context, Result};
use bdk_wallet::rusqlite::Connection;
use r2d2::{CustomizeConnection, Pool};
use r2d2_sqlite::SqliteConnectionManager;
use std::sync::Arc;

#[derive(Debug)]
struct ForeignKeyEnabler;

impl CustomizeConnection<Connection, bdk_wallet::rusqlite::Error> for ForeignKeyEnabler {
    fn on_acquire(&self, conn: &mut Connection) -> Result<(), bdk_wallet::rusqlite::Error> {
        conn.execute_batch("PRAGMA foreign_keys = ON")
    }
}

pub(crate) type DbPool = Pool<SqliteConnectionManager>;

#[derive(Clone)]
pub struct MetadataDb {
    pub(crate) pool: Arc<DbPool>,
}

impl MetadataDb {
    pub async fn new(db_path: &str, config: &AppConfig) -> Result<Self> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = std::path::Path::new(db_path).parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!("Warning: Failed to create database directory: {}", e);
            }
        }

        // Run migrations first
        let migration_runner = MigrationRunner::new(db_path)?;
        // Try multiple migration paths (for development and production)
        let migration_paths = ["./migrations", "../migrations", "migrations"];
        let mut migrations_run = false;
        for path in &migration_paths {
            if std::path::Path::new(path).exists() {
                if let Err(e) = migration_runner.run_migrations(path) {
                    eprintln!("Migration error with path {}: {}", path, e);
                } else {
                    migrations_run = true;
                    break;
                }
            }
        }
        if !migrations_run {
            eprintln!(
                "Warning: No migrations directory found in any of: {:?}",
                migration_paths
            );
        }

        // Get the connection back from the migration runner and close it
        let conn = migration_runner.get_connection();
        drop(conn);

        // Create connection pool with foreign key enforcement
        let manager = SqliteConnectionManager::file(db_path);
        let pool = Pool::builder()
            .max_size(16)
            .connection_customizer(Box::new(ForeignKeyEnabler))
            .build(manager)
            .context("Failed to create database pool")?;

        let db = MetadataDb {
            pool: Arc::new(pool),
        };

        // Initialize user based on operating mode
        db.initialize_user_for_mode(config).await?;

        Ok(db)
    }
}
