use bdk_wallet::rusqlite::{Connection, Result};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use utoipa::ToSchema;
use crate::migrations::MigrationRunner;
use crate::electrum::BlockHeader;
use std::num::Wrapping;

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub enum EventType {
    #[serde(rename = "send")]
    Send,
    #[serde(rename = "receive")]
    Receive,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
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
    pub phone_number: String,
    pub language: Language,
    pub created_at: String,
}


#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TwilioConfig {
    pub id: Option<i64>,
    pub account_sid: String,
    pub auth_token: String,
    pub messaging_service_sid: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SmsLog {
    pub id: Option<i64>,
    pub event_id: i64,
    pub contact_id: i64,
    pub twilio_sid: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
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
    pub sms_recipients: Vec<String>,
    pub transaction_time: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct DashboardUpdate {
    pub timestamp: u64,
    pub wallets: Vec<WalletMetadata>,
    pub events: Vec<TransactionEventWithWallet>,
}

#[derive(Debug, Default, Clone)]
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

pub struct MetadataDb {
    conn: Mutex<Connection>,
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
            // Continue anyway - we'll create tables manually as fallback
        }
        
        // Get the connection back from the migration runner
        let conn = migration_runner.get_connection();


        Ok(MetadataDb {
            conn: Mutex::new(conn),
        })
    }


    pub fn insert_wallet(
        &self,
        name: &str,
        descriptor: &str,
        wallet_filename: &str,
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let hex_color = calculate_wallet_color(descriptor);
        let mut stmt = conn.prepare(
            "INSERT INTO wallets (name, descriptor, wallet_filename, hex_color, balance_total, last_activity) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;

        let current_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        stmt.execute([name, descriptor, wallet_filename, &hex_color, "0", &current_time])?;
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
            "SELECT w.id, w.name, w.descriptor, w.wallet_filename, w.hex_color, w.created_at, w.balance_total, 
                    (SELECT MAX(te.transaction_time) FROM transaction_events te WHERE te.wallet_id = w.id) as last_activity,
                    COUNT(c.id) as contact_count
             FROM wallets w 
             LEFT JOIN contact_persons c ON w.id = c.wallet_id 
             WHERE w.descriptor = ?1 
             GROUP BY w.id, w.name, w.descriptor, w.wallet_filename, w.hex_color, w.created_at, w.balance_total"
        )?;

        match stmt.query_row([descriptor], |row| {
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
        }) {
            Ok(metadata) => Ok(Some(metadata)),
            Err(bdk_wallet::rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn get_wallet_by_id(&self, id: i64) -> Result<Option<WalletMetadata>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT w.id, w.name, w.descriptor, w.wallet_filename, w.hex_color, w.created_at, w.balance_total, 
                    (SELECT MAX(te.transaction_time) FROM transaction_events te WHERE te.wallet_id = w.id) as last_activity,
                    COUNT(c.id) as contact_count
             FROM wallets w 
             LEFT JOIN contact_persons c ON w.id = c.wallet_id 
             WHERE w.id = ?1 
             GROUP BY w.id, w.name, w.descriptor, w.wallet_filename, w.hex_color, w.created_at, w.balance_total",
        )?;

        match stmt.query_row([id], |row| {
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
        }) {
            Ok(metadata) => Ok(Some(metadata)),
            Err(bdk_wallet::rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn get_all_wallets(&self) -> Result<Vec<WalletMetadata>> {
        let conn = self.conn.lock().unwrap();
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
    }

    pub fn delete_wallet_by_id(&self, id: i64) -> Result<Option<(String, String)>> {
        let conn = self.conn.lock().unwrap();

        // First get the descriptor and filename before deleting
        let mut stmt =
            conn.prepare("SELECT descriptor, wallet_filename FROM wallets WHERE id = ?1")?;
        let wallet_info = match stmt.query_row([id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }) {
            Ok((desc, filename)) => Some((desc, filename)),
            Err(bdk_wallet::rusqlite::Error::QueryReturnedNoRows) => None,
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

    pub fn insert_event(&self, event: &EventInsert) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "INSERT INTO transaction_events (wallet_id, event_type, amount_sats, is_confirmed, is_rbf, is_cpfp, balance_total, transaction_time) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
        )?;

        stmt.execute([
            &event.wallet_id.to_string(),
            event.event_type.as_str(),
            &event.amount_sats.to_string(),
            &(event.is_confirmed as i32).to_string(),
            &(event.is_rbf as i32).to_string(),
            &(event.is_cpfp as i32).to_string(),
            &event
                .balance_total
                .map(|v| v.to_string())
                .unwrap_or_default(),
            &event.transaction_time.to_string(),
        ])?;
        Ok(conn.last_insert_rowid())
    }


    pub fn get_all_events_with_wallets(&self) -> Result<Vec<TransactionEventWithWallet>> {
        let conn = self.conn.lock().unwrap();
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
                sms_recipients: Vec::new(), // Will be populated below
                transaction_time: row.get(9)?,
            })
        })?;

        let mut events = Vec::new();
        for event in event_iter {
            let mut event = event?;
            
            // Get SMS recipients for this event
            if let Some(event_id) = event.id {
                let mut sms_stmt = conn.prepare(
                    "SELECT cp.name FROM sms_logs sl 
                     JOIN contact_persons cp ON sl.contact_id = cp.id 
                     WHERE sl.event_id = ?1"
                )?;
                
                let recipient_iter = sms_stmt.query_map([event_id], |row| {
                    Ok(row.get::<_, String>(0)?)
                })?;
                
                for recipient in recipient_iter {
                    event.sms_recipients.push(recipient?);
                }
            }
            
            events.push(event);
        }

        Ok(events)
    }

    // Contact management functions
    pub fn insert_contact(&self, wallet_id: i64, name: &str, phone_number: &str, language: &Language) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("INSERT INTO contact_persons (wallet_id, name, phone_number, language) VALUES (?1, ?2, ?3, ?4)")?;

        stmt.execute([&wallet_id.to_string(), name, phone_number, language.as_str()])?;
        Ok(conn.last_insert_rowid())
    }


    pub fn delete_contact(&self, contact_id: i64) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        // With CASCADE delete, we only need to delete the contact - sms_logs will be automatically deleted
        let mut stmt = conn.prepare("DELETE FROM contact_persons WHERE id = ?1")?;
        let changes = stmt.execute([contact_id])?;
        Ok(changes > 0)
    }

    // These functions are no longer needed since contacts are now directly linked to wallets
    // Keeping them for compatibility during transition, but they should be removed eventually

    pub fn get_contacts_for_wallet(&self, wallet_id: i64) -> Result<Vec<ContactPerson>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, wallet_id, name, phone_number, language, created_at 
             FROM contact_persons 
             WHERE wallet_id = ?1 ORDER BY name",
        )?;

        let contact_iter = stmt.query_map([wallet_id], |row| {
            let language_str: String = row.get(4)?;
            Ok(ContactPerson {
                id: Some(row.get(0)?),
                wallet_id: row.get(1)?,
                name: row.get(2)?,
                phone_number: row.get(3)?,
                language: Language::from(language_str.as_str()),
                created_at: row.get(5)?,
            })
        })?;

        let mut contacts = Vec::new();
        for contact in contact_iter {
            contacts.push(contact?);
        }

        Ok(contacts)
    }

    // Twilio configuration functions
    pub fn upsert_twilio_config(
        &self,
        account_sid: &str,
        auth_token: &str,
        messaging_service_sid: &str,
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap();

        // Delete existing config (we only want one)
        conn.execute("DELETE FROM twilio_config", [])?;

        // Insert new config
        let mut stmt = conn.prepare(
            "INSERT INTO twilio_config (account_sid, auth_token, messaging_service_sid) VALUES (?1, ?2, ?3)"
        )?;

        stmt.execute([account_sid, auth_token, messaging_service_sid])?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get_twilio_config(&self) -> Result<Option<TwilioConfig>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, account_sid, auth_token, messaging_service_sid, created_at FROM twilio_config LIMIT 1"
        )?;

        match stmt.query_row([], |row| {
            Ok(TwilioConfig {
                id: Some(row.get(0)?),
                account_sid: row.get(1)?,
                auth_token: row.get(2)?,
                messaging_service_sid: row.get(3)?,
                created_at: row.get(4)?,
            })
        }) {
            Ok(config) => Ok(Some(config)),
            Err(bdk_wallet::rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    // SMS logging functions
    pub fn insert_sms_log(
        &self,
        event_id: i64,
        contact_id: i64,
        message_content: &str,
        status: &str,
        twilio_sid: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "INSERT INTO sms_logs (event_id, contact_id, message_content, status, twilio_sid, error_message) VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
        )?;

        stmt.execute([
            &event_id.to_string(),
            &contact_id.to_string(),
            message_content,
            status,
            &twilio_sid.unwrap_or(""),
            &error_message.unwrap_or(""),
        ])?;
        Ok(conn.last_insert_rowid())
    }

    pub fn update_wallet_balance(&self, wallet_id: i64, balance_total: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("UPDATE wallets SET balance_total = ?1 WHERE id = ?2")?;
        stmt.execute([balance_total, wallet_id])?;
        Ok(())
    }

    pub fn update_wallet(&self, wallet_id: i64, name: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("UPDATE wallets SET name = ?1 WHERE id = ?2")?;
        let changes = stmt.execute([name, &wallet_id.to_string()])?;
        Ok(changes > 0)
    }




    /// Store the current block header (replaces any existing)
    pub fn upsert_current_block_header(&self, block_header: &BlockHeader) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "UPDATE current_block_header 
             SET height = ?1, hash = ?2, timestamp = ?3, updated_at = datetime('now') 
             WHERE id = 1"
        )?;
        stmt.execute([
            &block_header.height.to_string(),
            &block_header.hash,
            &block_header.timestamp.to_string(),
        ])?;
        Ok(())
    }

    /// Get the stored current block header
    pub fn get_current_block_header(&self) -> Result<Option<BlockHeader>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT height, hash, timestamp FROM current_block_header WHERE id = 1"
        )?;
        
        let mut rows = stmt.query_map([], |row| {
            Ok(BlockHeader {
                height: row.get::<_, i64>(0)? as u32,
                hash: row.get(1)?,
                timestamp: row.get::<_, i64>(2)? as u64,
            })
        })?;

        match rows.next() {
            Some(result) => {
                let block_header = result?;
                // Return None if this is the dummy row (height=0)
                if block_header.height == 0 {
                    Ok(None)
                } else {
                    Ok(Some(block_header))
                }
            }
            None => Ok(None),
        }
    }
}
