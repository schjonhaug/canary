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
        
        // Check if initial schema has already been applied
        let mut stmt = self.conn.prepare(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = ?1"
        ).map_err(|e| anyhow::Error::from(e))?;
        let count: i32 = stmt.query_row(["001"], |row| row.get(0)).map_err(|e| anyhow::Error::from(e))?;
        
        if count > 0 {
            println!("Initial schema already applied, skipping");
            
            // Still check for dev users migration in debug mode
            if cfg!(debug_assertions) {
                self.apply_dev_users_migration(migrations_path)?;
            }
            
            return Ok(());
        }
        
        // Apply the single initial schema
        let schema_file = migrations_path.join("001_initial_schema.sql");
        
        if !schema_file.exists() {
            return Err(anyhow::Error::msg("Initial schema file not found: 001_initial_schema.sql"));
        }
        
        println!("Applying initial schema: 001_initial_schema.sql");
        let sql = fs::read_to_string(&schema_file)?;
        
        // Execute each statement in the schema
        // SQLite doesn't support multiple statements in execute(), so we split them
        let statements: Vec<&str> = sql.split(';').collect();
        
        for statement in statements.iter() {
            let trimmed = statement.trim();
            // Skip empty statements and comments
            if trimmed.is_empty() || trimmed.lines().all(|line| line.trim().starts_with("--") || line.trim().is_empty()) {
                continue;
            }
            
            if let Err(e) = self.conn.execute(trimmed, []) {
                eprintln!("Error executing initial schema: {}", e);
                eprintln!("Statement: {}", trimmed);
                return Err(e.into());
            }
        }
        
        // Record that initial schema has been applied
        self.conn.execute(
            "INSERT INTO schema_migrations (version) VALUES (?1)",
            ["001"],
        ).map_err(|e| anyhow::Error::from(e))?;
        
        println!("Successfully applied initial schema");
        
        // In debug mode, also apply dev users migration
        if cfg!(debug_assertions) {
            self.apply_dev_users_migration(migrations_path)?;
        }
        
        Ok(())
    }
    
    fn apply_dev_users_migration(&self, migrations_path: &Path) -> Result<()> {
        // Check if dev users migration has already been applied
        let mut stmt = self.conn.prepare(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = ?1"
        ).map_err(|e| anyhow::Error::from(e))?;
        let count: i32 = stmt.query_row(["002_dev"], |row| row.get(0)).map_err(|e| anyhow::Error::from(e))?;
        
        if count == 0 {
            let dev_users_file = migrations_path.join("002_dev_users.sql");
            if dev_users_file.exists() {
                println!("Applying dev users migration: 002_dev_users.sql");
                let sql = fs::read_to_string(&dev_users_file)?;
                
                let statements: Vec<&str> = sql.split(';').collect();
                for statement in statements.iter() {
                    let trimmed = statement.trim();
                    if trimmed.is_empty() || trimmed.lines().all(|line| line.trim().starts_with("--") || line.trim().is_empty()) {
                        continue;
                    }
                    
                    if let Err(e) = self.conn.execute(trimmed, []) {
                        eprintln!("Error executing dev users migration: {}", e);
                        eprintln!("Statement: {}", trimmed);
                        // Don't fail on dev user creation errors
                    }
                }
                
                // Record that dev users migration has been applied
                self.conn.execute(
                    "INSERT INTO schema_migrations (version) VALUES (?1)",
                    ["002_dev"],
                ).map_err(|e| anyhow::Error::from(e))?;
                
                println!("Successfully applied dev users migration");
            }
        } else {
            println!("Dev users migration already applied");
        }
        Ok(())
    }
    
    pub fn get_connection(self) -> Connection {
        self.conn
    }
}