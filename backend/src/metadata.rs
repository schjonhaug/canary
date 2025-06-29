use rusqlite::{Connection, Result};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

#[derive(Debug, Serialize, Deserialize)]
pub struct WalletMetadata {
    pub id: Option<i64>,
    pub name: String,
    pub descriptor: String,
    pub wallet_filename: String,
    pub created_at: String,
}

pub struct MetadataDb {
    conn: Mutex<Connection>,
}

impl MetadataDb {
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS wallets (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                descriptor TEXT NOT NULL UNIQUE,
                wallet_filename TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        Ok(MetadataDb { conn: Mutex::new(conn) })
    }

    pub fn insert_wallet(&self, name: &str, descriptor: &str, wallet_filename: &str) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "INSERT INTO wallets (name, descriptor, wallet_filename) VALUES (?1, ?2, ?3)"
        )?;
        
        stmt.execute([name, descriptor, wallet_filename])?;
        Ok(conn.last_insert_rowid())
    }


    pub fn descriptor_exists(&self, descriptor: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM wallets WHERE descriptor = ?1")?;
        let count: i64 = stmt.query_row([descriptor], |row| row.get(0))?;
        Ok(count > 0)
    }

    pub fn get_wallet_name_by_filename(&self, wallet_filename: &str) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT name FROM wallets WHERE wallet_filename = ?1")?;
        let name: String = stmt.query_row([wallet_filename], |row| row.get(0))?;
        Ok(name)
    }
}