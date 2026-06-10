use anyhow::Result;
use bdk_wallet::rusqlite::{Connection, Result as SqliteResult};
use std::fs;
use std::path::Path;
use std::time::Duration;

pub struct MigrationRunner {
    conn: Connection,
}

impl MigrationRunner {
    pub fn new(db_path: &str) -> SqliteResult<Self> {
        let conn = Connection::open(db_path)?;

        // Wait briefly for another startup process to finish its SQLite write.
        conn.busy_timeout(Duration::from_secs(5))?;

        // Enable foreign key constraints
        conn.execute("PRAGMA foreign_keys = ON", [])?;

        // Create migrations table to track applied migrations
        conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version TEXT PRIMARY KEY,
                applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        Ok(Self { conn })
    }

    pub fn run_migrations(&self, migrations_dir: &str) -> Result<()> {
        let migrations_path = Path::new(migrations_dir);

        if !migrations_path.exists() {
            println!("Migrations directory not found: {}", migrations_dir);
            return Ok(());
        }

        // Get all applied migrations
        let applied_migrations = self.get_applied_migrations()?;

        // Get all migration files
        let mut migration_files = Vec::new();
        for entry in fs::read_dir(migrations_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "sql") {
                if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                    migration_files.push((filename.to_string(), path));
                }
            }
        }

        // Sort migration files by name (which includes version number)
        migration_files.sort_by(|a, b| a.0.cmp(&b.0));

        // Apply each migration that hasn't been applied yet
        for (filename, path) in migration_files {
            // Extract version from filename (e.g., "001" from "001_initial_schema.sql")
            let version = filename.split('_').next().unwrap_or(&filename);

            if applied_migrations.contains(version) {
                println!("Migration {} already applied, skipping", filename);
                continue;
            }

            println!("Applying migration: {}", filename);
            self.apply_migration(&path, version)?;
        }

        Ok(())
    }

    fn get_applied_migrations(&self) -> Result<std::collections::HashSet<String>> {
        let mut applied = std::collections::HashSet::new();

        let mut stmt = self.conn.prepare("SELECT version FROM schema_migrations")?;
        let rows = stmt.query_map([], |row| {
            let version: String = row.get(0)?;
            Ok(version)
        })?;

        for row in rows {
            applied.insert(row?);
        }

        Ok(applied)
    }

    fn apply_migration(&self, migration_path: &Path, version: &str) -> Result<()> {
        let sql = fs::read_to_string(migration_path)?;

        // Execute each statement in the migration. Migration SQL is repository
        // controlled and must not contain semicolons inside comments or string
        // literals. Keeping statement-level execution lets us tolerate a
        // previously interrupted ALTER TABLE ADD COLUMN migration.
        let statements: Vec<&str> = sql.split(';').collect();

        for statement in statements.iter() {
            let trimmed = statement.trim();
            // Skip empty statements and comments
            if trimmed.is_empty()
                || trimmed
                    .lines()
                    .all(|line| line.trim().starts_with("--") || line.trim().is_empty())
            {
                continue;
            }

            if let Err(e) = self.conn.execute(trimmed, []) {
                if is_duplicate_add_column_error(trimmed, &e) {
                    tracing::info!(
                        "Column already exists while applying migration {}; continuing",
                        version
                    );
                    continue;
                }

                tracing::error!("Error executing migration {}: {}", version, e);
                tracing::error!("Statement: {}", trimmed);
                // Some migrations open their own explicit transaction. If one
                // fails, close it so a restarted process can retry cleanly. If
                // no transaction is active, this intentionally becomes a no-op.
                let _ = self.conn.execute("ROLLBACK", []);
                return Err(e.into());
            }
        }

        // Record that this migration has been applied. If a migration commits
        // before this insert and the process exits here, the migration must be
        // safe to re-run on next startup.
        self.conn.execute(
            "INSERT INTO schema_migrations (version) VALUES (?1)",
            [version],
        )?;

        println!("Successfully applied migration: {}", version);

        Ok(())
    }

    pub fn get_connection(self) -> Connection {
        self.conn
    }
}

fn is_duplicate_add_column_error(statement: &str, error: &bdk_wallet::rusqlite::Error) -> bool {
    // rusqlite exposes SQLite's duplicate-column failure as an error string.
    // Limit the recovery path to ALTER TABLE ADD COLUMN statements so other
    // migration errors still fail fast.
    let uncommented_statement = statement
        .lines()
        .filter(|line| !line.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join(" ");
    let tokens: Vec<String> = uncommented_statement
        .split_whitespace()
        .map(|token| token.to_ascii_uppercase())
        .collect();
    tokens
        .windows(2)
        .any(|window| window[0] == "ALTER" && window[1] == "TABLE")
        && tokens
            .windows(2)
            .any(|window| window[0] == "ADD" && window[1] == "COLUMN")
        && error.to_string().contains("duplicate column name:")
}
