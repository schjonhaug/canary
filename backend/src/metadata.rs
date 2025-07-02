use bdk_wallet::rusqlite::{Connection, Result};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use utoipa::ToSchema;

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
    pub message: String,
    pub created_at: String,
}

#[derive(Debug, Default, Clone)]
pub struct EventInsert<'a> {
    pub wallet_id: i64,
    pub event_type: EventType,
    pub amount_sats: i64,
    pub is_confirmed: bool,
    pub is_rbf: bool,
    pub is_cpfp: bool,
    pub message: &'a str,
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

        conn.execute(
            "CREATE TABLE IF NOT EXISTS transaction_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                wallet_id INTEGER NOT NULL,
                event_type TEXT NOT NULL CHECK (event_type IN ('send', 'receive')),
                amount_sats INTEGER NOT NULL,
                is_confirmed BOOLEAN DEFAULT FALSE,
                is_rbf BOOLEAN DEFAULT FALSE,
                is_cpfp BOOLEAN DEFAULT FALSE,
                message TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (wallet_id) REFERENCES wallets (id)
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS contact_persons (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                phone_number TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS wallet_contacts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                wallet_id INTEGER NOT NULL,
                contact_id INTEGER NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (wallet_id) REFERENCES wallets (id),
                FOREIGN KEY (contact_id) REFERENCES contact_persons (id),
                UNIQUE(wallet_id, contact_id)
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS twilio_config (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                account_sid TEXT NOT NULL,
                auth_token TEXT NOT NULL,
                messaging_service_sid TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS sms_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id INTEGER NOT NULL,
                contact_id INTEGER NOT NULL,
                twilio_sid TEXT,
                status TEXT NOT NULL CHECK (status IN ('sent', 'failed', 'pending')),
                error_message TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (event_id) REFERENCES transaction_events (id),
                FOREIGN KEY (contact_id) REFERENCES contact_persons (id)
            )",
            [],
        )?;

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
            "INSERT INTO wallets (name, descriptor, wallet_filename) VALUES (?1, ?2, ?3)",
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
            Err(bdk_wallet::rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn get_wallet_by_id(&self, id: i64) -> Result<Option<WalletMetadata>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, descriptor, wallet_filename, created_at FROM wallets WHERE id = ?1",
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
            Err(bdk_wallet::rusqlite::Error::QueryReturnedNoRows) => Ok(None),
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
            "INSERT INTO transaction_events (wallet_id, event_type, amount_sats, is_confirmed, is_rbf, is_cpfp, message) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
        )?;

        stmt.execute([
            &event.wallet_id.to_string(),
            event.event_type.as_str(),
            &event.amount_sats.to_string(),
            &(event.is_confirmed as i32).to_string(),
            &(event.is_rbf as i32).to_string(),
            &(event.is_cpfp as i32).to_string(),
            event.message,
        ])?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get_events_by_wallet(&self, wallet_id: i64) -> Result<Vec<TransactionEvent>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, wallet_id, event_type, amount_sats, is_confirmed, is_rbf, is_cpfp, message, created_at 
             FROM transaction_events WHERE wallet_id = ?1 ORDER BY created_at DESC"
        )?;

        let event_iter = stmt.query_map([wallet_id], |row| {
            Ok(TransactionEvent {
                id: Some(row.get(0)?),
                wallet_id: row.get(1)?,
                event_type: EventType::from(row.get::<_, String>(2)?.as_str()),
                amount_sats: row.get(3)?,
                is_confirmed: row.get(4)?,
                is_rbf: row.get(5)?,
                is_cpfp: row.get(6)?,
                message: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?;

        let mut events = Vec::new();
        for event in event_iter {
            events.push(event?);
        }

        Ok(events)
    }

    pub fn get_all_events(&self) -> Result<Vec<TransactionEvent>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, wallet_id, event_type, amount_sats, is_confirmed, is_rbf, is_cpfp, message, created_at 
             FROM transaction_events ORDER BY created_at DESC"
        )?;

        let event_iter = stmt.query_map([], |row| {
            Ok(TransactionEvent {
                id: Some(row.get(0)?),
                wallet_id: row.get(1)?,
                event_type: EventType::from(row.get::<_, String>(2)?.as_str()),
                amount_sats: row.get(3)?,
                is_confirmed: row.get(4)?,
                is_rbf: row.get(5)?,
                is_cpfp: row.get(6)?,
                message: row.get(7)?,
                created_at: row.get(8)?,
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
        tx.execute("DELETE FROM wallet_contacts WHERE contact_id = ?1", [contact_id])?;
        
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

    pub fn get_sms_logs_by_event(&self, event_id: i64) -> Result<Vec<SmsLog>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, event_id, contact_id, twilio_sid, status, error_message, created_at 
             FROM sms_logs WHERE event_id = ?1 ORDER BY created_at DESC",
        )?;

        let log_iter = stmt.query_map([event_id], |row| {
            let twilio_sid: String = row.get(3)?;
            let error_message: String = row.get(5)?;

            Ok(SmsLog {
                id: Some(row.get(0)?),
                event_id: row.get(1)?,
                contact_id: row.get(2)?,
                twilio_sid: if twilio_sid.is_empty() {
                    None
                } else {
                    Some(twilio_sid)
                },
                status: row.get(4)?,
                error_message: if error_message.is_empty() {
                    None
                } else {
                    Some(error_message)
                },
                created_at: row.get(6)?,
            })
        })?;

        let mut logs = Vec::new();
        for log in log_iter {
            logs.push(log?);
        }

        Ok(logs)
    }
}
