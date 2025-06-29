use rusqlite::{Connection, Result};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
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


    pub fn get_wallet_by_descriptor(&self, descriptor: &str) -> Result<Option<WalletMetadata>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, descriptor, wallet_filename, created_at FROM wallets WHERE descriptor = ?1"
        )?;
        
        match stmt.query_row([descriptor], |row| {
            Ok(WalletMetadata {
                id: Some(row.get(0)?),
                name: row.get(1)?,
                descriptor: row.get(2)?,
                wallet_filename: row.get(3)?,
                created_at: row.get(4)?,
            })
        }) {
            Ok(metadata) => Ok(Some(metadata)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn get_wallet_by_id(&self, id: i64) -> Result<Option<WalletMetadata>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, descriptor, wallet_filename, created_at FROM wallets WHERE id = ?1"
        )?;
        
        match stmt.query_row([id], |row| {
            Ok(WalletMetadata {
                id: Some(row.get(0)?),
                name: row.get(1)?,
                descriptor: row.get(2)?,
                wallet_filename: row.get(3)?,
                created_at: row.get(4)?,
            })
        }) {
            Ok(metadata) => Ok(Some(metadata)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn get_all_wallets(&self) -> Result<Vec<WalletMetadata>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, descriptor, wallet_filename, created_at FROM wallets ORDER BY created_at DESC"
        )?;
        
        let wallet_iter = stmt.query_map([], |row| {
            Ok(WalletMetadata {
                id: Some(row.get(0)?),
                name: row.get(1)?,
                descriptor: row.get(2)?,
                wallet_filename: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        
        let mut wallets = Vec::new();
        for wallet in wallet_iter {
            wallets.push(wallet?);
        }
        
        Ok(wallets)
    }

    pub fn delete_wallet_by_id(&self, id: i64) -> Result<Option<(String, String)>> {
        let conn = self.conn.lock().unwrap();
        
        // First get the descriptor and filename before deleting
        let mut stmt = conn.prepare("SELECT descriptor, wallet_filename FROM wallets WHERE id = ?1")?;
        let wallet_info = match stmt.query_row([id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }) {
            Ok((desc, filename)) => Some((desc, filename)),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(e),
        };
        
        if let Some((descriptor, filename)) = wallet_info {
            // Delete the wallet
            let mut delete_stmt = conn.prepare("DELETE FROM wallets WHERE id = ?1")?;
            let changes = delete_stmt.execute([id])?;
            
            if changes > 0 {
                Ok(Some((descriptor, filename)))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }
}