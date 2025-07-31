use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use bdk_wallet::rusqlite::params;
use std::sync::Arc;
use tokio::task::spawn_blocking;
use crate::migrations::MigrationRunner;
use crate::electrum::BlockHeader;
use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use std::num::Wrapping;

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, Copy, PartialEq)]
pub enum EventType {
    #[serde(rename = "send")]
    Send,
    #[serde(rename = "receive")]
    Receive,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, Copy, PartialEq)]
pub enum Language {
    #[serde(rename = "en")]
    English,
    #[serde(rename = "no")]
    Norwegian,
}


impl EventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventType::Send => "send",
            EventType::Receive => "receive",
        }
    }
}

impl Language {
    pub fn as_str(&self) -> &'static str {
        match self {
            Language::English => "en",
            Language::Norwegian => "no",
        }
    }
}

impl From<&str> for EventType {
    fn from(s: &str) -> Self {
        match s {
            "send" => EventType::Send,
            "receive" => EventType::Receive,
            _ => panic!("Invalid event type: {}", s),
        }
    }
}

impl From<&str> for Language {
    fn from(s: &str) -> Self {
        match s {
            "en" => Language::English,
            "no" => Language::Norwegian,
            _ => panic!("Invalid language: {}", s),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct WalletMetadata {
    pub id: Option<i64>,
    pub name: String,
    pub descriptor: String,
    pub wallet_filename: String,
    pub hex_color: String,
    pub created_at: String,
    pub balance_total: Option<i64>,
    pub last_activity: Option<String>,
    pub contact_count: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct ContactPerson {
    pub id: Option<i64>,
    pub wallet_id: i64,
    pub name: String,
    pub contact_address: String,
    pub language: Language,
    pub created_at: String,
}


#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct TransactionEvent {
    pub id: Option<i64>,
    pub wallet_id: i64,
    pub event_type: EventType,
    pub amount_sats: i64,
    pub is_confirmed: bool,
    pub is_rbf: bool,
    pub is_cpfp: bool,
    pub balance_total: Option<i64>,
    pub transaction_time: u64,
    pub notification_status: Vec<NotificationStatus>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct TransactionEventWithWallet {
    pub id: Option<i64>,
    pub wallet_id: i64,
    pub wallet_name: String,
    pub event_type: EventType,
    pub amount_sats: i64,
    pub is_confirmed: bool,
    pub is_rbf: bool,
    pub is_cpfp: bool,
    pub balance_total: Option<i64>,
    pub transaction_time: u64,
    pub notification_status: Vec<NotificationStatus>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct NotificationLog {
    pub id: Option<i64>,
    pub event_id: i64,
    pub contact_id: i64,
    pub provider_name: String,
    pub provider_message_id: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
    pub message_content: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct NotificationStatus {
    pub contact_name: String,
    pub provider_name: String,
    pub status: String,
    pub error_message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct DashboardUpdate {
    pub timestamp: u64,
    pub wallets: Vec<WalletMetadata>,
    pub events: Vec<TransactionEventWithWallet>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct EventInsert {
    pub wallet_id: i64,
    pub event_type: EventType,
    pub amount_sats: i64,
    pub is_confirmed: bool,
    pub is_rbf: bool,
    pub is_cpfp: bool,
    pub balance_total: Option<i64>,
    pub transaction_time: u64,
}

impl Default for EventType {
    fn default() -> Self {
        EventType::Send
    }
}

/// Extract checksum from a Bitcoin descriptor
fn extract_checksum(descriptor: &str) -> String {
    if let Some(start) = descriptor.rfind('#') {
        descriptor[start + 1..].to_string()
    } else {
        "Unknown".to_string()
    }
}

/// Convert checksum to hex color using DJB2 hash algorithm
fn checksum_to_hex_color(checksum: &str) -> String {
    // DJB2 hash algorithm with position weighting for better distribution
    let mut hash = Wrapping(5381u32);
    for (i, ch) in checksum.chars().enumerate() {
        let char_code = ch as u32;
        // DJB2: hash = ((hash << 5) + hash) + char
        // Add position weighting to further improve distribution
        hash = ((hash << 5) + hash) + Wrapping(char_code * (i as u32 + 1));
    }
    
    // Get hue (0-360 degrees)
    let hue = (hash.0 % 360) as f64;
    
    // Fixed saturation and lightness for consistent appearance
    let saturation = 70.0; // 70% saturation for vibrant colors
    let lightness = 50.0;  // 50% lightness for good contrast
    
    // Convert HSL to RGB
    let c = (1.0_f64 - (2.0_f64 * (lightness / 100.0_f64) - 1.0_f64).abs()) * (saturation / 100.0_f64);
    let x = c * (1.0_f64 - ((hue / 60.0_f64) % 2.0_f64 - 1.0_f64).abs());
    let m = (lightness / 100.0_f64) - c / 2.0_f64;
    
    let (r, g, b) = if hue < 60.0_f64 {
        (c, x, 0.0_f64)
    } else if hue < 120.0_f64 {
        (x, c, 0.0_f64)
    } else if hue < 180.0_f64 {
        (0.0_f64, c, x)
    } else if hue < 240.0_f64 {
        (0.0_f64, x, c)
    } else if hue < 300.0_f64 {
        (x, 0.0_f64, c)
    } else {
        (c, 0.0_f64, x)
    };
    
    let r = ((r + m) * 255.0_f64).round() as u8;
    let g = ((g + m) * 255.0_f64).round() as u8;
    let b = ((b + m) * 255.0_f64).round() as u8;
    
    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

/// Calculate hex color from descriptor
pub fn calculate_wallet_color(descriptor: &str) -> String {
    let checksum = extract_checksum(descriptor);
    checksum_to_hex_color(&checksum)
}

type DbPool = Pool<SqliteConnectionManager>;

#[derive(Clone)]
pub struct MetadataDb {
    pool: Arc<DbPool>,
}

impl MetadataDb {
    pub fn new(db_path: &str) -> Result<Self> {
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
            eprintln!("Warning: No migrations directory found in any of: {:?}", migration_paths);
        }
        
        // Get the connection back from the migration runner and close it
        let conn = migration_runner.get_connection();
        drop(conn);
        
        // Create connection pool
        let manager = SqliteConnectionManager::file(db_path);
        let pool = Pool::builder()
            .max_size(16)
            .build(manager)
            .context("Failed to create database pool")?;

        Ok(MetadataDb {
            pool: Arc::new(pool),
        })
    }

    pub async fn insert_wallet(
        &self,
        name: &str,
        descriptor: &str,
        wallet_filename: &str,
    ) -> Result<i64> {
        let pool = self.pool.clone();
        let name = name.to_string();
        let descriptor = descriptor.to_string();
        let wallet_filename = wallet_filename.to_string();
        
        spawn_blocking(move || -> Result<i64> {
            let conn = pool.get()?;
            let hex_color = calculate_wallet_color(&descriptor);
            let current_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            
            conn.execute(
                "INSERT INTO wallets (name, descriptor, wallet_filename, hex_color, balance_total, last_activity) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![&name, &descriptor, &wallet_filename, &hex_color, "0", &current_time],
            )?;
            Ok(conn.last_insert_rowid())
        }).await?
    }

    pub async fn descriptor_exists(&self, descriptor: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let descriptor = descriptor.to_string();
        
        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM wallets WHERE descriptor = ?1",
                params![descriptor],
                |row| row.get(0),
            )?;
            Ok(count > 0)
        }).await?
    }

    pub async fn get_wallet_name_by_filename(&self, wallet_filename: &str) -> Result<String> {
        let pool = self.pool.clone();
        let wallet_filename = wallet_filename.to_string();
        
        spawn_blocking(move || -> Result<String> {
            let conn = pool.get()?;
            let name: String = conn.query_row(
                "SELECT name FROM wallets WHERE wallet_filename = ?1",
                params![wallet_filename],
                |row| row.get(0),
            )?;
            Ok(name)
        }).await?
    }

    pub async fn get_wallet_by_descriptor(&self, descriptor: &str) -> Result<Option<WalletMetadata>> {
        let pool = self.pool.clone();
        let descriptor = descriptor.to_string();
        
        spawn_blocking(move || -> Result<Option<WalletMetadata>> {
            let conn = pool.get()?;
            match conn.query_row(
                "SELECT w.id, w.name, w.descriptor, w.wallet_filename, w.hex_color, w.created_at, w.balance_total, 
                        (SELECT MAX(te.transaction_time) FROM transaction_events te WHERE te.wallet_id = w.id) as last_activity,
                        COUNT(c.id) as contact_count
                 FROM wallets w 
                 LEFT JOIN contact_persons c ON w.id = c.wallet_id 
                 WHERE w.descriptor = ?1 
                 GROUP BY w.id, w.name, w.descriptor, w.wallet_filename, w.hex_color, w.created_at, w.balance_total",
                params![descriptor],
                |row| {
                    Ok(WalletMetadata {
                        id: Some(row.get(0)?),
                        name: row.get(1)?,
                        descriptor: row.get(2)?,
                        wallet_filename: row.get(3)?,
                        hex_color: row.get(4)?,
                        created_at: row.get(5)?,
                        balance_total: row.get(6).ok(),
                        last_activity: row.get::<_, Option<i64>>(7).ok().flatten().map(|t| t.to_string()),
                        contact_count: Some(row.get(8)?),
                    })
                },
            ) {
                Ok(metadata) => Ok(Some(metadata)),
                Err(bdk_wallet::rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        }).await?
    }

    pub async fn get_wallet_by_id(&self, id: i64) -> Result<Option<WalletMetadata>> {
        let pool = self.pool.clone();
        
        spawn_blocking(move || -> Result<Option<WalletMetadata>> {
            let conn = pool.get()?;
            match conn.query_row(
                "SELECT w.id, w.name, w.descriptor, w.wallet_filename, w.hex_color, w.created_at, w.balance_total, 
                        (SELECT MAX(te.transaction_time) FROM transaction_events te WHERE te.wallet_id = w.id) as last_activity,
                        COUNT(c.id) as contact_count
                 FROM wallets w 
                 LEFT JOIN contact_persons c ON w.id = c.wallet_id 
                 WHERE w.id = ?1 
                 GROUP BY w.id, w.name, w.descriptor, w.wallet_filename, w.hex_color, w.created_at, w.balance_total",
                params![id],
                |row| {
                    Ok(WalletMetadata {
                        id: Some(row.get(0)?),
                        name: row.get(1)?,
                        descriptor: row.get(2)?,
                        wallet_filename: row.get(3)?,
                        hex_color: row.get(4)?,
                        created_at: row.get(5)?,
                        balance_total: row.get(6).ok(),
                        last_activity: row.get::<_, Option<i64>>(7).ok().flatten().map(|t| t.to_string()),
                        contact_count: Some(row.get(8)?),
                    })
                },
            ) {
                Ok(metadata) => Ok(Some(metadata)),
                Err(bdk_wallet::rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        }).await?
    }

    pub async fn get_wallet_by_filename(&self, wallet_filename: &str) -> Result<Option<WalletMetadata>> {
        let pool = self.pool.clone();
        let wallet_filename = wallet_filename.to_string();
        
        spawn_blocking(move || -> Result<Option<WalletMetadata>> {
            let conn = pool.get()?;
            match conn.query_row(
                "SELECT w.id, w.name, w.descriptor, w.wallet_filename, w.hex_color, w.created_at, w.balance_total, 
                        (SELECT MAX(te.transaction_time) FROM transaction_events te WHERE te.wallet_id = w.id) as last_activity,
                        COUNT(c.id) as contact_count
                 FROM wallets w 
                 LEFT JOIN contact_persons c ON w.id = c.wallet_id 
                 WHERE w.wallet_filename = ?1 
                 GROUP BY w.id, w.name, w.descriptor, w.wallet_filename, w.hex_color, w.created_at, w.balance_total",
                params![wallet_filename],
                |row| {
                    Ok(WalletMetadata {
                        id: Some(row.get(0)?),
                        name: row.get(1)?,
                        descriptor: row.get(2)?,
                        wallet_filename: row.get(3)?,
                        hex_color: row.get(4)?,
                        created_at: row.get(5)?,
                        balance_total: Some(row.get(6).unwrap_or(0)),
                        last_activity: row.get::<_, Option<i64>>(7).ok().flatten().map(|t| t.to_string()),
                        contact_count: Some(row.get(8)?),
                    })
                },
            ) {
                Ok(wallet) => Ok(Some(wallet)),
                Err(bdk_wallet::rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        }).await?
    }

    pub async fn get_all_wallets(&self) -> Result<Vec<WalletMetadata>> {
        let pool = self.pool.clone();
        
        spawn_blocking(move || -> Result<Vec<WalletMetadata>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT w.id, w.name, w.descriptor, w.wallet_filename, w.hex_color, w.created_at, w.balance_total, 
                        (SELECT MAX(te.transaction_time) FROM transaction_events te WHERE te.wallet_id = w.id) as last_activity,
                        COUNT(c.id) as contact_count
                 FROM wallets w 
                 LEFT JOIN contact_persons c ON w.id = c.wallet_id 
                 GROUP BY w.id, w.name, w.descriptor, w.wallet_filename, w.hex_color, w.created_at, w.balance_total 
                 ORDER BY w.created_at DESC"
            )?;

            let wallet_iter = stmt.query_map([], |row| {
                Ok(WalletMetadata {
                    id: Some(row.get(0)?),
                    name: row.get(1)?,
                    descriptor: row.get(2)?,
                    wallet_filename: row.get(3)?,
                    hex_color: row.get(4)?,
                    created_at: row.get(5)?,
                    balance_total: Some(row.get(6).unwrap_or(0)),
                    last_activity: row.get::<_, Option<i64>>(7).ok().flatten().map(|t| t.to_string()),
                    contact_count: Some(row.get(8)?),
                })
            })?;

            let mut wallets = Vec::new();
            for wallet in wallet_iter {
                wallets.push(wallet?);
            }

            Ok(wallets)
        }).await?
    }

    pub async fn delete_wallet_by_id(&self, id: i64) -> Result<Option<(String, String)>> {
        let pool = self.pool.clone();
        
        spawn_blocking(move || -> Result<Option<(String, String)>> {
            let conn = pool.get()?;

            // First get the descriptor and filename before deleting
            let wallet_info = match conn.query_row(
                "SELECT descriptor, wallet_filename FROM wallets WHERE id = ?1",
                params![id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            ) {
                Ok((desc, filename)) => Some((desc, filename)),
                Err(bdk_wallet::rusqlite::Error::QueryReturnedNoRows) => None,
                Err(e) => return Err(e.into()),
            };

            if let Some((descriptor, filename)) = wallet_info {
                // Delete the wallet
                let changes = conn.execute("DELETE FROM wallets WHERE id = ?1", params![id])?;

                if changes > 0 {
                    Ok(Some((descriptor, filename)))
                } else {
                    Ok(None)
                }
            } else {
                Ok(None)
            }
        }).await?
    }

    pub async fn insert_event(&self, event: &EventInsert) -> Result<i64> {
        let pool = self.pool.clone();
        let event = *event;
        
        spawn_blocking(move || -> Result<i64> {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO transaction_events (wallet_id, event_type, amount_sats, is_confirmed, is_rbf, is_cpfp, balance_total, transaction_time) 
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    event.wallet_id,
                    event.event_type.as_str(),
                    event.amount_sats,
                    event.is_confirmed as i32,
                    event.is_rbf as i32,
                    event.is_cpfp as i32,
                    event.balance_total,
                    event.transaction_time,
                ],
            )?;
            Ok(conn.last_insert_rowid())
        }).await?
    }

    pub async fn get_all_events_with_wallets(&self) -> Result<Vec<TransactionEventWithWallet>> {
        let pool = self.pool.clone();
        
        spawn_blocking(move || -> Result<Vec<TransactionEventWithWallet>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT te.id, te.wallet_id, w.name, te.event_type, te.amount_sats, te.is_confirmed, te.is_rbf, te.is_cpfp, te.balance_total, te.transaction_time 
                 FROM transaction_events te 
                 JOIN wallets w ON te.wallet_id = w.id 
                 ORDER BY te.transaction_time DESC, te.id DESC"
            )?;

            let event_iter = stmt.query_map([], |row| {
                Ok(TransactionEventWithWallet {
                    id: Some(row.get(0)?),
                    wallet_id: row.get(1)?,
                    wallet_name: row.get(2)?,
                    event_type: EventType::from(row.get::<_, String>(3)?.as_str()),
                    amount_sats: row.get(4)?,
                    is_confirmed: row.get(5)?,
                    is_rbf: row.get(6)?,
                    is_cpfp: row.get(7)?,
                    balance_total: row.get(8).ok(),
                    transaction_time: row.get(9)?,
                    notification_status: Vec::new(), // Will be populated later
                })
            })?;

            let mut events = Vec::new();
            for event in event_iter {
                let mut event = event?;
                
                // Get notification logs for this event
                if let Some(event_id) = event.id {
                    let mut notification_status = Vec::new();
                    
                    let mut log_stmt = conn.prepare(
                        "SELECT nl.provider_name, nl.status, nl.error_message, cp.name as contact_name
                         FROM notification_logs nl
                         JOIN contact_persons cp ON nl.contact_id = cp.id
                         WHERE nl.event_id = ?1
                         ORDER BY nl.created_at DESC"
                    )?;
                    
                    let log_iter = log_stmt.query_map([event_id], |row| {
                        Ok(NotificationStatus {
                            contact_name: row.get(3)?,
                            provider_name: row.get(0)?,
                            status: row.get(1)?,
                            error_message: row.get(2)?,
                        })
                    })?;
                    
                    for log in log_iter {
                        notification_status.push(log?);
                    }
                    
                    event.notification_status = notification_status;
                }
                
                events.push(event);
            }

            Ok(events)
        }).await?
    }

    pub async fn insert_contact(&self, wallet_id: i64, name: &str, contact_address: &str, language: &Language) -> Result<i64> {
        let pool = self.pool.clone();
        let name = name.to_string();
        let contact_address = contact_address.to_string();
        let language = *language;
        
        spawn_blocking(move || -> Result<i64> {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO contact_persons (wallet_id, name, contact_address, language) VALUES (?1, ?2, ?3, ?4)",
                params![wallet_id, &name, &contact_address, language.as_str()],
            )?;
            Ok(conn.last_insert_rowid())
        }).await?
    }

    pub async fn delete_contact(&self, contact_id: i64) -> Result<bool> {
        let pool = self.pool.clone();
        
        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let changes = conn.execute("DELETE FROM contact_persons WHERE id = ?1", params![contact_id])?;
            Ok(changes > 0)
        }).await?
    }

    pub async fn get_contacts_for_wallet(&self, wallet_id: i64) -> Result<Vec<ContactPerson>> {
        let pool = self.pool.clone();
        
        spawn_blocking(move || -> Result<Vec<ContactPerson>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, wallet_id, name, contact_address, language, created_at 
                 FROM contact_persons 
                 WHERE wallet_id = ?1 ORDER BY name",
            )?;

            let contact_iter = stmt.query_map(params![wallet_id], |row| {
                let language_str: String = row.get(4)?;
                Ok(ContactPerson {
                    id: Some(row.get(0)?),
                    wallet_id: row.get(1)?,
                    name: row.get(2)?,
                    contact_address: row.get(3)?,
                    language: Language::from(language_str.as_str()),
                    created_at: row.get(5)?,
                })
            })?;

            let mut contacts = Vec::new();
            for contact in contact_iter {
                contacts.push(contact?);
            }

            Ok(contacts)
        }).await?
    }

    pub async fn update_wallet_balance(&self, wallet_id: i64, balance_total: i64) -> Result<()> {
        let pool = self.pool.clone();
        
        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE wallets SET balance_total = ?1 WHERE id = ?2",
                params![balance_total, wallet_id],
            )?;
            Ok(())
        }).await?
    }

    pub async fn update_wallet(&self, wallet_id: i64, name: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let name = name.to_string();
        
        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let changes = conn.execute(
                "UPDATE wallets SET name = ?1 WHERE id = ?2",
                params![&name, wallet_id],
            )?;
            Ok(changes > 0)
        }).await?
    }

    pub async fn upsert_current_block_header(&self, block_header: &BlockHeader) -> Result<()> {
        let pool = self.pool.clone();
        let block_header = block_header.clone();
        
        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE current_block_header 
                 SET height = ?1, timestamp = ?2, updated_at = datetime('now') 
                 WHERE id = 1",
                params![
                    block_header.height,
                    block_header.timestamp,
                ],
            )?;
            Ok(())
        }).await?
    }

    pub async fn get_current_block_header(&self) -> Result<Option<BlockHeader>> {
        let pool = self.pool.clone();
        
        spawn_blocking(move || -> Result<Option<BlockHeader>> {
            let conn = pool.get()?;
            match conn.query_row(
                "SELECT height, timestamp FROM current_block_header WHERE id = 1",
                [],
                |row| {
                    Ok(BlockHeader {
                        height: row.get::<_, i64>(0)? as u32,
                        timestamp: row.get::<_, i64>(1)? as u64,
                    })
                },
            ) {
                Ok(block_header) => {
                    // Return None if this is the dummy row (height=0)
                    if block_header.height == 0 {
                        Ok(None)
                    } else {
                        Ok(Some(block_header))
                    }
                }
                Err(bdk_wallet::rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        }).await?
    }

    pub async fn insert_notification_log(
        &self,
        event_id: i64,
        contact_id: i64,
        provider_name: &str,
        provider_message_id: Option<&str>,
        status: &str,
        error_message: Option<&str>,
        message_content: &str,
    ) -> Result<i64> {
        let pool = self.pool.clone();
        let provider_name = provider_name.to_string();
        let provider_message_id = provider_message_id.map(|s| s.to_string());
        let status = status.to_string();
        let error_message = error_message.map(|s| s.to_string());
        let message_content = message_content.to_string();
        
        spawn_blocking(move || -> Result<i64> {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO notification_logs (event_id, contact_id, provider_name, provider_message_id, status, error_message, message_content) 
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    event_id,
                    contact_id,
                    provider_name,
                    provider_message_id,
                    status,
                    error_message,
                    message_content,
                ],
            )?;
            Ok(conn.last_insert_rowid())
        }).await?
    }

    pub async fn get_notification_logs_for_event(&self, event_id: i64) -> Result<Vec<NotificationLog>> {
        let pool = self.pool.clone();
        
        spawn_blocking(move || -> Result<Vec<NotificationLog>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, event_id, contact_id, provider_name, provider_message_id, status, error_message, message_content, created_at 
                 FROM notification_logs 
                 WHERE event_id = ?1 
                 ORDER BY created_at DESC"
            )?;
            
            let log_iter = stmt.query_map([event_id], |row| {
                Ok(NotificationLog {
                    id: Some(row.get(0)?),
                    event_id: row.get(1)?,
                    contact_id: row.get(2)?,
                    provider_name: row.get(3)?,
                    provider_message_id: row.get(4)?,
                    status: row.get(5)?,
                    error_message: row.get(6)?,
                    message_content: row.get(7)?,
                    created_at: row.get(8)?,
                })
            })?;
            
            let mut logs = Vec::new();
            for log in log_iter {
                logs.push(log?);
            }
            
            Ok(logs)
        }).await?
    }
}