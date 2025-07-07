use bdk_wallet::rusqlite::{Connection, Result};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use utoipa::ToSchema;
use crate::migrations::MigrationRunner;
use crate::electrum::BlockHeader;

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub enum EventType {
    #[serde(rename = "send")]
    Send,
    #[serde(rename = "receive")]
    Receive,
}

impl EventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventType::Send => "send",
            EventType::Receive => "receive",
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

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct WalletMetadata {
    pub id: Option<i64>,
    pub name: String,
    pub descriptor: String,
    pub wallet_filename: String,
    pub created_at: String,
    pub balance_total: Option<i64>,
    pub last_activity: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct ContactPerson {
    pub id: Option<i64>,
    pub name: String,
    pub phone_number: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct WalletContact {
    pub id: Option<i64>,
    pub wallet_id: i64,
    pub contact_id: i64,
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
    pub created_at: String,
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
    pub created_at: String,
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
}

impl Default for EventType {
    fn default() -> Self {
        EventType::Send
    }
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
        let mut stmt = conn.prepare(
            "INSERT INTO wallets (name, descriptor, wallet_filename, balance_total, last_activity) VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;

        let current_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        stmt.execute([name, descriptor, wallet_filename, "0", &current_time])?;
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
            "SELECT id, name, descriptor, wallet_filename, created_at, balance_total, last_activity FROM wallets WHERE descriptor = ?1"
        )?;

        match stmt.query_row([descriptor], |row| {
            Ok(WalletMetadata {
                id: Some(row.get(0)?),
                name: row.get(1)?,
                descriptor: row.get(2)?,
                wallet_filename: row.get(3)?,
                created_at: row.get(4)?,
                balance_total: row.get(5).ok(),
                last_activity: row.get(6).ok(),
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
            "SELECT id, name, descriptor, wallet_filename, created_at, balance_total, last_activity FROM wallets WHERE id = ?1",
        )?;

        match stmt.query_row([id], |row| {
            Ok(WalletMetadata {
                id: Some(row.get(0)?),
                name: row.get(1)?,
                descriptor: row.get(2)?,
                wallet_filename: row.get(3)?,
                created_at: row.get(4)?,
                balance_total: row.get(5).ok(),
                last_activity: row.get(6).ok(),
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
            "SELECT id, name, descriptor, wallet_filename, created_at, balance_total, last_activity FROM wallets ORDER BY created_at DESC"
        )?;

        let wallet_iter = stmt.query_map([], |row| {
            Ok(WalletMetadata {
                id: Some(row.get(0)?),
                name: row.get(1)?,
                descriptor: row.get(2)?,
                wallet_filename: row.get(3)?,
                created_at: row.get(4)?,
                balance_total: row.get(5).ok(),
                last_activity: row.get(6).ok(),
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
            "INSERT INTO transaction_events (wallet_id, event_type, amount_sats, is_confirmed, is_rbf, is_cpfp, balance_total) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
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
        ])?;
        Ok(conn.last_insert_rowid())
    }


    pub fn get_all_events_with_wallets(&self) -> Result<Vec<TransactionEventWithWallet>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT te.id, te.wallet_id, w.name, te.event_type, te.amount_sats, te.is_confirmed, te.is_rbf, te.is_cpfp, te.balance_total, te.created_at 
             FROM transaction_events te 
             JOIN wallets w ON te.wallet_id = w.id 
             ORDER BY te.created_at DESC"
        )?;

        let event_iter = stmt.query_map([], |row| {
            let sqlite_timestamp: String = row.get(9)?;
            // Convert SQLite timestamp to RFC3339 (ISO 8601 with timezone)
            let created_at = if sqlite_timestamp.ends_with('Z') {
                sqlite_timestamp
            } else {
                format!("{}Z", sqlite_timestamp.replace(' ', "T"))
            };
            
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
                created_at,
            })
        })?;

        let mut events = Vec::new();
        for event in event_iter {
            events.push(event?);
        }

        Ok(events)
    }

    // Contact management functions
    pub fn insert_contact(&self, name: &str, phone_number: &str) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("INSERT INTO contact_persons (name, phone_number) VALUES (?1, ?2)")?;

        stmt.execute([name, phone_number])?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get_all_contacts(&self) -> Result<Vec<ContactPerson>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, phone_number, created_at FROM contact_persons ORDER BY name",
        )?;

        let contact_iter = stmt.query_map([], |row| {
            Ok(ContactPerson {
                id: Some(row.get(0)?),
                name: row.get(1)?,
                phone_number: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;

        let mut contacts = Vec::new();
        for contact in contact_iter {
            contacts.push(contact?);
        }

        Ok(contacts)
    }

    pub fn delete_contact(&self, contact_id: i64) -> Result<bool> {
        let conn = self.conn.lock().unwrap();

        // Start a transaction to ensure atomicity
        let tx = conn.unchecked_transaction()?;

        // First, delete from sms_logs that reference this contact
        tx.execute("DELETE FROM sms_logs WHERE contact_id = ?1", [contact_id])?;

        // Then, delete from wallet_contacts that reference this contact
        tx.execute(
            "DELETE FROM wallet_contacts WHERE contact_id = ?1",
            [contact_id],
        )?;

        // Finally, delete the contact itself
        let changes = tx.execute("DELETE FROM contact_persons WHERE id = ?1", [contact_id])?;

        tx.commit()?;
        Ok(changes > 0)
    }

    // Wallet-contact relationship functions
    pub fn add_contact_to_wallet(&self, wallet_id: i64, contact_id: i64) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("INSERT INTO wallet_contacts (wallet_id, contact_id) VALUES (?1, ?2)")?;

        stmt.execute([wallet_id, contact_id])?;
        Ok(conn.last_insert_rowid())
    }

    pub fn remove_contact_from_wallet(&self, wallet_id: i64, contact_id: i64) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("DELETE FROM wallet_contacts WHERE wallet_id = ?1 AND contact_id = ?2")?;
        let changes = stmt.execute([wallet_id, contact_id])?;
        Ok(changes > 0)
    }

    pub fn get_contacts_for_wallet(&self, wallet_id: i64) -> Result<Vec<ContactPerson>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT cp.id, cp.name, cp.phone_number, cp.created_at 
             FROM contact_persons cp 
             JOIN wallet_contacts wc ON cp.id = wc.contact_id 
             WHERE wc.wallet_id = ?1 ORDER BY cp.name",
        )?;

        let contact_iter = stmt.query_map([wallet_id], |row| {
            Ok(ContactPerson {
                id: Some(row.get(0)?),
                name: row.get(1)?,
                phone_number: row.get(2)?,
                created_at: row.get(3)?,
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
        status: &str,
        twilio_sid: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "INSERT INTO sms_logs (event_id, contact_id, status, twilio_sid, error_message) VALUES (?1, ?2, ?3, ?4, ?5)"
        )?;

        stmt.execute([
            &event_id.to_string(),
            &contact_id.to_string(),
            status,
            &twilio_sid.unwrap_or(""),
            &error_message.unwrap_or(""),
        ])?;
        Ok(conn.last_insert_rowid())
    }

    pub fn update_wallet_balance(&self, wallet_id: i64, balance_total: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("UPDATE wallets SET balance_total = ?1, last_activity = CURRENT_TIMESTAMP WHERE id = ?2")?;
        stmt.execute([balance_total, wallet_id])?;
        Ok(())
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
