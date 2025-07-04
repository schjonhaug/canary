use bdk_wallet::rusqlite::{Connection, Result as SqliteResult};
use anyhow::Result;
use std::fs;
use std::path::Path;

pub struct MigrationRunner {
    conn: Connection,
}

impl MigrationRunner {
    pub fn new(db_path: &str) -> SqliteResult<Self> {
        let conn = Connection::open(db_path)?;
        
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
        
        
        // Get list of migration files
        let mut migration_files: Vec<_> = fs::read_dir(migrations_path)?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.extension()? == "sql" {
                    Some(path)
                } else {
                    None
                }
            })
            .collect();
        
        // Sort migrations by filename (which should start with version number)
        migration_files.sort();
        
        for migration_file in migration_files {
            let filename = migration_file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");
            
            // Extract version from filename (everything before first underscore)
            let version = filename
                .split('_')
                .next()
                .unwrap_or(filename)
                .split('.')
                .next()
                .unwrap_or(filename);
            
            // Check if migration has already been applied
            let mut stmt = self.conn.prepare(
                "SELECT COUNT(*) FROM schema_migrations WHERE version = ?1"
            ).map_err(|e| anyhow::Error::from(e))?;
            let count: i32 = stmt.query_row([version], |row| row.get(0)).map_err(|e| anyhow::Error::from(e))?;
            
            if count > 0 {
                println!("Migration {} already applied, skipping", filename);
                continue;
            }
            
            // Read and execute migration
            println!("Applying migration: {}", filename);
            let sql = fs::read_to_string(&migration_file)?;
            
            // Execute each statement in the migration
            // SQLite doesn't support multiple statements in execute(), so we split them
            let statements: Vec<&str> = sql.split(';').collect();
            
            for statement in statements.iter() {
                let trimmed = statement.trim();
                // Skip empty statements and comments
                if trimmed.is_empty() || trimmed.lines().all(|line| line.trim().starts_with("--") || line.trim().is_empty()) {
                    continue;
                }
                
                if let Err(e) = self.conn.execute(trimmed, []) {
                    eprintln!("Error executing migration {}: {}", filename, e);
                    eprintln!("Statement: {}", trimmed);
                    return Err(e.into());
                }
            }
            
            // Record that migration has been applied
            self.conn.execute(
                "INSERT INTO schema_migrations (version) VALUES (?1)",
                [version],
            ).map_err(|e| anyhow::Error::from(e))?;
            
            println!("Successfully applied migration: {}", filename);
        }
        
        Ok(())
    }
    
    pub fn get_connection(self) -> Connection {
        self.conn
    }
}