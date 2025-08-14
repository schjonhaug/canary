use crate::electrum::BlockHeader;
use crate::migrations::MigrationRunner;
use crate::subscription::SubscriptionTier;
use anyhow::{Context, Result};
use bdk_wallet::rusqlite::{params, OptionalExtension, ToSql};
use chrono;
use phonenumber::PhoneNumber;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use serde::{Deserialize, Serialize};
use std::num::Wrapping;
use std::str::FromStr;
use std::sync::Arc;
use tokio::task::spawn_blocking;
use utoipa::ToSchema;
use uuid::Uuid;

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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserRecord {
    pub id: String, // UUIDv4
    pub email: String,
    pub password_hash: String,
    pub name: Option<String>,
    pub is_admin: bool,
    pub email_verified: bool,
    // Subscription fields
    pub subscription_tier: SubscriptionTier,
    pub trial_ends_at: String,
    pub subscription_status: String,
    pub stripe_customer_id: Option<String>,
    pub stripe_subscription_id: Option<String>,
    pub subscription_started_at: Option<String>,
    pub subscription_ends_at: Option<String>,
    pub created_at: String,
}

impl UserRecord {}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct TwilioConfig {
    pub id: Option<i64>,
    pub account_sid: String,
    pub auth_token: String,
    pub messaging_service_sid: String,
    pub verify_service_sid: Option<String>,
    pub created_at: String,
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
    pub checksum: String,
    pub name: String,
    pub descriptor: String,
    pub hex_color: String,
    pub created_at: String,
    pub balance_total: Option<i64>,
    pub last_activity: Option<String>,
    pub contact_count: Option<i64>,
    pub user_id: String,
    pub is_active: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct Contact {
    pub id: Option<String>, // UUIDv4
    pub wallet_checksum: String,
    pub name: String,
    pub language: Language,
    pub notification_methods: Vec<NotificationMethod>,
    pub created_at: String,
    pub is_active: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct NotificationMethod {
    pub id: Option<String>, // UUIDv4
    pub contact_id: String, // UUIDv4
    pub provider_type: ProviderType,
    pub notification_target: String, // phone number or ntfy topic
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_target: Option<String>, // formatted version for display
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
pub enum ProviderType {
    #[serde(rename = "sms")]
    Sms,
    #[serde(rename = "ntfy")]
    Ntfy,
    #[serde(rename = "email")]
    Email,
}

impl ProviderType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderType::Sms => "sms",
            ProviderType::Ntfy => "ntfy",
            ProviderType::Email => "email",
        }
    }
}

impl From<&str> for ProviderType {
    fn from(s: &str) -> Self {
        match s {
            "sms" => ProviderType::Sms,
            "ntfy" => ProviderType::Ntfy,
            "email" => ProviderType::Email,
            _ => ProviderType::Ntfy, // Default fallback
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct TransactionEvent {
    pub id: Option<String>, // UUIDv4
    pub wallet_checksum: String,
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
    pub id: Option<String>, // UUIDv4
    pub wallet_checksum: String,
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
pub struct NotificationStatus {
    pub contact_name: String,
    pub provider_name: String,
    pub status: String,
    pub error_message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct WalletsListResponse {
    pub timestamp: u64,
    pub wallets: Vec<WalletMetadata>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct WalletDetailResponse {
    pub timestamp: u64,
    pub wallet: WalletMetadata,
    pub events: Vec<TransactionEventWithWallet>,
    pub contacts: Vec<Contact>,
}

#[derive(Debug, Default, Clone)]
pub struct EventInsert {
    pub wallet_checksum: String,
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
    let lightness = 50.0; // 50% lightness for good contrast

    // Convert HSL to RGB
    let c =
        (1.0_f64 - (2.0_f64 * (lightness / 100.0_f64) - 1.0_f64).abs()) * (saturation / 100.0_f64);
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
    pub async fn new(db_path: &str) -> Result<Self> {
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

        // Create connection pool
        let manager = SqliteConnectionManager::file(db_path);
        let pool = Pool::builder()
            .max_size(16)
            .build(manager)
            .context("Failed to create database pool")?;

        let db = MetadataDb {
            pool: Arc::new(pool),
        };

        // Initialize admin user based on auth configuration
        db.initialize_admin_user().await?;

        Ok(db)
    }

    async fn initialize_admin_user(&self) -> Result<()> {
        // Check if auth is enabled
        let auth_enabled = std::env::var("CANARY_ENABLE_AUTH")
            .map(|v| v.to_lowercase() == "true" || v == "1")
            .unwrap_or(false);

        if !auth_enabled {
            // AUTH=false: Create default admin user (id=1) for self-hosted mode
            self.ensure_default_admin_user().await?;
        } else if cfg!(debug_assertions) {
            // AUTH=true in dev mode: Create hardcoded dev test users
            self.ensure_dev_test_users().await?;
        }

        Ok(())
    }

    async fn ensure_default_admin_user(&self) -> Result<()> {
        let pool = self.pool.clone();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;

            // Check if admin user already exists
            let admin_exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM users WHERE id = 1)",
                [],
                |row| row.get(0),
            )?;

            if !admin_exists {
                // Admin user will be created dynamically when auth is enabled
                println!("No admin user exists yet - will be created on first registration");
            }

            Ok(())
        })
        .await?
    }

    async fn ensure_dev_test_users(&self) -> Result<()> {
        use crate::auth::{AuthService, DEV_TEST_EMAILS, DEV_TEST_PASSWORD};

        let pool = self.pool.clone();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            
            // Create a temporary auth service to hash passwords
            let auth_service = AuthService::new("temp".to_string(), None);
            let password_hash = auth_service.hash_password(DEV_TEST_PASSWORD)?;
            
            for (index, email) in DEV_TEST_EMAILS.iter().enumerate() {
                // Check if user already exists
                let exists: bool = conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM users WHERE email = ?1)",
                    [email],
                    |row| row.get(0)
                )?;
                
                if !exists {
                    let (name, tier) = match *email {
                        "delivered+admin@resend.dev" => ("Admin", "team"), // Admin flag will give unlimited access
                        "delivered+alice@resend.dev" => ("Alice", "personal"),
                        "delivered+bob@resend.dev" => ("Bob", "team"),
                        _ => ("Test User", "personal"),
                    };
                    
                    // First user becomes admin
                    let is_admin = index == 0;
                    
                    let user_id = uuid::Uuid::new_v4().to_string();
                    conn.execute(
                        "INSERT INTO users (id, email, password_hash, name, is_admin, email_verified, subscription_tier, subscription_status, created_at) 
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, datetime('now'))",
                        params![&user_id, email, &password_hash, name, is_admin, true, tier, "active"], // Dev users are active
                    )?;
                    
                    println!("[DEV MODE] Created test user: {} (admin: {})", email, is_admin);
                }
            }
            
            Ok(())
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
        })
        .await?
    }

    /// Extract checksum from a Bitcoin descriptor
    pub fn extract_checksum(&self, descriptor: &str) -> String {
        extract_checksum(descriptor)
    }

    pub async fn insert_wallet(
        &self,
        name: &str,
        descriptor: &str,
        user_id: &str,
    ) -> Result<String> {
        let pool = self.pool.clone();
        let name = name.to_string();
        let descriptor = descriptor.to_string();
        let user_id = user_id.to_string();

        spawn_blocking(move || -> Result<String> {
            let conn = pool.get()?;
            let hex_color = calculate_wallet_color(&descriptor);
            let current_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            
            // Extract checksum from descriptor (part after #)
            let checksum = descriptor.split('#').last()
                .ok_or_else(|| anyhow::anyhow!("Invalid descriptor format: missing checksum"))?
                .to_string();
            
            conn.execute(
                "INSERT INTO wallets (checksum, name, descriptor, hex_color, balance_total, last_activity, user_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![&checksum, &name, &descriptor, &hex_color, "0", &current_time, user_id],
            )?;
            Ok(checksum)
        }).await?
    }


    pub async fn get_wallet_by_descriptor(
        &self,
        descriptor: &str,
    ) -> Result<Option<WalletMetadata>> {
        let pool = self.pool.clone();
        let descriptor = descriptor.to_string();

        spawn_blocking(move || -> Result<Option<WalletMetadata>> {
            let conn = pool.get()?;
            match conn.query_row(
                "SELECT w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total, 
                        (SELECT MAX(te.transaction_time) FROM transaction_events te WHERE te.wallet_checksum = w.checksum) as last_activity,
                        COUNT(c.id) as contact_count, w.user_id, w.is_active
                 FROM wallets w 
                 LEFT JOIN contacts c ON w.checksum = c.wallet_checksum 
                 WHERE w.descriptor = ?1 
                 GROUP BY w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total, w.user_id, w.is_active",
                params![descriptor],
                |row| {
                    Ok(WalletMetadata {
                        checksum: row.get(0)?,
                        name: row.get(1)?,
                        descriptor: row.get(2)?,
                        hex_color: row.get(3)?,
                        created_at: row.get(4)?,
                        balance_total: row.get(5).ok(),
                        last_activity: row.get::<_, Option<i64>>(6).ok().flatten().map(|t| t.to_string()),
                        contact_count: Some(row.get(7)?),
                        user_id: row.get(8)?,
                        is_active: row.get::<_, i64>(9).unwrap_or(1) != 0,
                    })
                },
            ) {
                Ok(metadata) => Ok(Some(metadata)),
                Err(bdk_wallet::rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        }).await?
    }

    pub async fn get_wallet_by_checksum(&self, checksum: &str) -> Result<Option<WalletMetadata>> {
        let pool = self.pool.clone();
        let checksum = checksum.to_string();

        spawn_blocking(move || -> Result<Option<WalletMetadata>> {
            let conn = pool.get()?;
            match conn.query_row(
                "SELECT w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total, 
                        (SELECT MAX(te.transaction_time) FROM transaction_events te WHERE te.wallet_checksum = w.checksum) as last_activity,
                        COUNT(c.id) as contact_count, w.user_id, w.is_active
                 FROM wallets w 
                 LEFT JOIN contacts c ON w.checksum = c.wallet_checksum 
                 WHERE w.checksum = ?1 
                 GROUP BY w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total, w.user_id, w.is_active",
                params![checksum],
                |row| {
                    Ok(WalletMetadata {
                        checksum: row.get(0)?,
                        name: row.get(1)?,
                        descriptor: row.get(2)?,
                        hex_color: row.get(3)?,
                        created_at: row.get(4)?,
                        balance_total: row.get(5).ok(),
                        last_activity: row.get::<_, Option<i64>>(6).ok().flatten().map(|t| t.to_string()),
                        contact_count: Some(row.get(7)?),
                        user_id: row.get(8)?,
                        is_active: row.get::<_, i64>(9).unwrap_or(1) != 0,
                    })
                },
            ) {
                Ok(metadata) => Ok(Some(metadata)),
                Err(bdk_wallet::rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        }).await?
    }

    pub async fn get_all_wallets(&self) -> Result<Vec<WalletMetadata>> {
        self.get_wallets_for_user(None).await
    }

    pub async fn get_wallets_for_user(&self, user_id: Option<&str>) -> Result<Vec<WalletMetadata>> {
        let pool = self.pool.clone();
        let user_id = user_id.map(|id| id.to_string());

        spawn_blocking(move || -> Result<Vec<WalletMetadata>> {
            let conn = pool.get()?;
            
            let query = match user_id {
                Some(_) => {
                    "SELECT w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total, 
                            (SELECT MAX(te.transaction_time) FROM transaction_events te WHERE te.wallet_checksum = w.checksum) as last_activity,
                            COUNT(c.id) as contact_count, w.user_id, w.is_active
                     FROM wallets w 
                     LEFT JOIN contacts c ON w.checksum = c.wallet_checksum 
                     WHERE w.user_id = ?1
                     GROUP BY w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total, w.user_id, w.is_active
                     ORDER BY w.created_at DESC"
                }
                None => {
                    "SELECT w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total, 
                            (SELECT MAX(te.transaction_time) FROM transaction_events te WHERE te.wallet_checksum = w.checksum) as last_activity,
                            COUNT(c.id) as contact_count, w.user_id, w.is_active
                     FROM wallets w 
                     LEFT JOIN contacts c ON w.checksum = c.wallet_checksum 
                     GROUP BY w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total, w.user_id, w.is_active
                     ORDER BY w.created_at DESC"
                }
            };
            
            let mut stmt = conn.prepare(query)?;

            let params: Vec<&dyn ToSql> = match user_id.as_ref() {
                Some(uid) => vec![uid],
                None => vec![],
            };
            
            let wallet_iter = stmt.query_map(&params[..], |row| {
                Ok(WalletMetadata {
                    checksum: row.get(0)?,
                    name: row.get(1)?,
                    descriptor: row.get(2)?,
                    hex_color: row.get(3)?,
                    created_at: row.get(4)?,
                    balance_total: Some(row.get(5).unwrap_or(0)),
                    last_activity: row.get::<_, Option<i64>>(6).ok().flatten().map(|t| t.to_string()),
                    contact_count: Some(row.get(7)?),
                    user_id: row.get(8)?,
                    is_active: row.get::<_, i64>(9).unwrap_or(1) != 0, // SQLite stores bool as int
                })
            })?;

            let mut wallets = Vec::new();
            for wallet in wallet_iter {
                wallets.push(wallet?);
            }

            Ok(wallets)
        }).await?
    }

    /// Get wallets for a user ordered by creation time (oldest first) for subscription limits enforcement
    pub async fn get_wallets_for_user_oldest_first(&self, user_id: &str) -> Result<Vec<WalletMetadata>> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();

        spawn_blocking(move || -> Result<Vec<WalletMetadata>> {
            let conn = pool.get()?;
            
            let query = "SELECT w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total, 
                               (SELECT MAX(te.transaction_time) FROM transaction_events te WHERE te.wallet_checksum = w.checksum) as last_activity,
                               COUNT(c.id) as contact_count, w.user_id, w.is_active
                        FROM wallets w 
                        LEFT JOIN contacts c ON w.checksum = c.wallet_checksum 
                        WHERE w.user_id = ?1
                        GROUP BY w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total, w.user_id, w.is_active
                        ORDER BY w.created_at ASC"; // Oldest first for subscription limits
            
            let mut stmt = conn.prepare(query)?;
            
            let wallet_iter = stmt.query_map(&[&user_id], |row| {
                Ok(WalletMetadata {
                    checksum: row.get(0)?,
                    name: row.get(1)?,
                    descriptor: row.get(2)?,
                    hex_color: row.get(3)?,
                    created_at: row.get(4)?,
                    balance_total: Some(row.get(5).unwrap_or(0)),
                    last_activity: row.get::<_, Option<i64>>(6).ok().flatten().map(|t| t.to_string()),
                    contact_count: Some(row.get(7)?),
                    user_id: row.get(8)?,
                    is_active: row.get::<_, i64>(9).unwrap_or(1) != 0, // SQLite stores bool as int
                })
            })?;

            let mut wallets = Vec::new();
            for wallet in wallet_iter {
                wallets.push(wallet?);
            }

            Ok(wallets)
        }).await?
    }

    pub async fn count_wallets_for_user(&self, user_id: &str) -> Result<usize> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();

        spawn_blocking(move || -> Result<usize> {
            let conn = pool.get()?;
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM wallets WHERE user_id = ?1",
                    params![user_id],
                    |row| row.get(0),
                )?;
            Ok(count as usize)
        })
        .await?
    }

    pub async fn count_contacts_for_wallet(&self, wallet_checksum: &str) -> Result<usize> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();

        spawn_blocking(move || -> Result<usize> {
            let conn = pool.get()?;
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM contacts WHERE wallet_checksum = ?1",
                    params![checksum],
                    |row| row.get(0),
                )?;
            Ok(count as usize)
        })
        .await?
    }

    pub async fn is_wallet_owned_by_user(
        &self,
        wallet_checksum: &str,
        user_id: &str,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();
        let user_id = user_id.to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let exists: bool = conn
                .prepare("SELECT 1 FROM wallets WHERE checksum = ?1 AND user_id = ?2")?
                .exists(params![checksum, user_id])?;
            Ok(exists)
        })
        .await?
    }

    pub async fn delete_wallet_by_checksum(
        &self,
        checksum: &str,
    ) -> Result<Option<(String, String)>> {
        let pool = self.pool.clone();
        let checksum = checksum.to_string();

        spawn_blocking(move || -> Result<Option<(String, String)>> {
            let conn = pool.get()?;

            // First get the descriptor before deleting (filename is generated from checksum)
            let descriptor = match conn.query_row(
                "SELECT descriptor FROM wallets WHERE checksum = ?1",
                params![checksum],
                |row| row.get::<_, String>(0),
            ) {
                Ok(desc) => Some(desc),
                Err(bdk_wallet::rusqlite::Error::QueryReturnedNoRows) => None,
                Err(e) => return Err(e.into()),
            };

            if let Some(descriptor) = descriptor {
                // Delete the wallet
                let changes =
                    conn.execute("DELETE FROM wallets WHERE checksum = ?1", params![checksum])?;

                if changes > 0 {
                    // Generate filename from checksum
                    let filename = format!("{}.sqlite", checksum);
                    Ok(Some((descriptor, filename)))
                } else {
                    Ok(None)
                }
            } else {
                Ok(None)
            }
        })
        .await?
    }

    pub async fn insert_event(&self, event: &EventInsert) -> Result<String> {
        let pool = self.pool.clone();
        let event = event.clone();

        spawn_blocking(move || -> Result<String> {
            let conn = pool.get()?;
            let event_id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO transaction_events (id, wallet_checksum, event_type, amount_sats, is_confirmed, is_rbf, is_cpfp, balance_total, transaction_time) 
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    &event_id,
                    event.wallet_checksum,
                    event.event_type.as_str(),
                    event.amount_sats,
                    event.is_confirmed as i32,
                    event.is_rbf as i32,
                    event.is_cpfp as i32,
                    event.balance_total,
                    event.transaction_time,
                ],
            )?;
            Ok(event_id)
        }).await?
    }

    pub async fn get_all_events_with_wallets(&self) -> Result<Vec<TransactionEventWithWallet>> {
        let pool = self.pool.clone();

        spawn_blocking(move || -> Result<Vec<TransactionEventWithWallet>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT te.id, te.wallet_checksum, w.name, te.event_type, te.amount_sats, te.is_confirmed, te.is_rbf, te.is_cpfp, te.balance_total, te.transaction_time 
                 FROM transaction_events te 
                 JOIN wallets w ON te.wallet_checksum = w.checksum 
                 ORDER BY te.transaction_time DESC, te.id DESC"
            )?;

            let event_iter = stmt.query_map([], |row| {
                Ok(TransactionEventWithWallet {
                    id: Some(row.get(0)?),
                    wallet_checksum: row.get(1)?,
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
                if let Some(ref event_id) = event.id {
                    let mut notification_status = Vec::new();
                    
                    let mut log_stmt = conn.prepare(
                        "SELECT nl.provider_name, nl.status, nl.error_message, cp.name as contact_name
                         FROM notification_logs nl
                         JOIN contact_notification_methods cnm ON nl.notification_method_id = cnm.id
                         JOIN contacts cp ON cnm.contact_id = cp.id
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

    // Normalized contact methods
    pub async fn insert_contact_with_notification_methods(
        &self,
        wallet_checksum: &str,
        name: &str,
        language: &Language,
        notification_methods: Vec<(ProviderType, String)>,
    ) -> Result<String> {
        let pool = self.pool.clone();
        let name = name.to_string();
        let language = *language;
        let checksum = wallet_checksum.to_string();

        spawn_blocking(move || -> Result<String> {
            let conn = pool.get()?;
            let tx = conn.unchecked_transaction()?;
            
            // Insert contact with UUID
            let contact_id = Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO contacts (id, wallet_checksum, name, language) VALUES (?1, ?2, ?3, ?4)",
                params![&contact_id, checksum, &name, language.as_str()],
            )?;
            
            // Insert notification methods
            for (provider_type, notification_target) in notification_methods {
                let method_id = Uuid::new_v4().to_string();
                tx.execute(
                    "INSERT INTO contact_notification_methods (id, contact_id, provider_type, notification_target) VALUES (?1, ?2, ?3, ?4)",
                    params![&method_id, &contact_id, provider_type.as_str(), &notification_target],
                )?;
            }
            
            tx.commit()?;
            Ok(contact_id)
        }).await?
    }

    pub async fn get_contacts_with_notification_methods(
        &self,
        wallet_checksum: &str,
    ) -> Result<Vec<Contact>> {
        self.get_contacts_with_notification_methods_filtered(wallet_checksum, false).await
    }

    /// Get contacts for subscription limits ordered by creation time (oldest first)
    pub async fn get_contacts_oldest_first_for_limits(&self, wallet_checksum: &str) -> Result<Vec<Contact>> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();

        spawn_blocking(move || -> Result<Vec<Contact>> {
            let conn = pool.get()?;

            // Get ALL contacts ordered by created_at ASC (oldest first) for limits enforcement
            let query = "SELECT id, wallet_checksum, name, language, created_at, is_active 
                         FROM contacts 
                         WHERE wallet_checksum = ?1 ORDER BY created_at ASC";
            let mut stmt = conn.prepare(query)?;

            let contact_iter = stmt.query_map(params![checksum], |row| {
                let language_str: String = row.get(3)?;
                Ok((
                    row.get::<_, String>(0)?, // id as UUIDv4
                    Contact {
                        id: Some(row.get(0)?),
                        wallet_checksum: row.get(1)?,
                        name: row.get(2)?,
                        language: Language::from(language_str.as_str()),
                        notification_methods: Vec::new(), // Will be populated below
                        created_at: row.get(4)?,
                        is_active: row.get::<_, i64>(5).unwrap_or(1) != 0, // SQLite stores bool as int
                    },
                ))
            })?;

            let mut contacts: std::collections::HashMap<String, Contact> =
                std::collections::HashMap::new();
            for result in contact_iter {
                let (id, contact) = result?;
                contacts.insert(id, contact);
            }

            // Get notification methods for each contact
            for (contact_id, contact) in contacts.iter_mut() {
                let methods_query = "SELECT id, provider_type, notification_target, created_at
                                   FROM contact_notification_methods 
                                   WHERE contact_id = ?1";
                let mut methods_stmt = conn.prepare(methods_query)?;

                let methods_iter = methods_stmt.query_map(params![contact_id], |row| {
                    let provider_str: String = row.get(1)?;
                    Ok(NotificationMethod {
                        id: Some(row.get(0)?),
                        contact_id: contact_id.clone(),
                        provider_type: ProviderType::from(provider_str.as_str()),
                        notification_target: row.get(2)?,
                        display_target: None, // TODO: Add display_target logic if needed
                        created_at: row.get(3)?,
                    })
                })?;

                for method_result in methods_iter {
                    contact.notification_methods.push(method_result?);
                }
            }

            Ok(contacts.into_values().collect())
        }).await?
    }

    pub async fn get_contacts_with_notification_methods_filtered(
        &self,
        wallet_checksum: &str,
        include_inactive: bool,
    ) -> Result<Vec<Contact>> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();

        spawn_blocking(move || -> Result<Vec<Contact>> {
            let conn = pool.get()?;

            // Get contacts for the wallet (active only or all based on parameter)
            let query = if include_inactive {
                "SELECT id, wallet_checksum, name, language, created_at, is_active 
                 FROM contacts 
                 WHERE wallet_checksum = ?1 ORDER BY name"
            } else {
                "SELECT id, wallet_checksum, name, language, created_at, 1 as is_active
                 FROM contacts 
                 WHERE wallet_checksum = ?1 AND is_active = 1 ORDER BY name"
            };
            let mut stmt = conn.prepare(query)?;

            let contact_iter = stmt.query_map(params![checksum], |row| {
                let language_str: String = row.get(3)?;
                Ok((
                    row.get::<_, String>(0)?, // id as UUIDv4
                    Contact {
                        id: Some(row.get(0)?),
                        wallet_checksum: row.get(1)?,
                        name: row.get(2)?,
                        language: Language::from(language_str.as_str()),
                        notification_methods: Vec::new(), // Will be populated below
                        created_at: row.get(4)?,
                        is_active: row.get::<_, i64>(5).unwrap_or(1) != 0, // SQLite stores bool as int
                    },
                ))
            })?;

            let mut contacts: std::collections::HashMap<String, Contact> =
                std::collections::HashMap::new();
            for contact_result in contact_iter {
                let (contact_id, contact) = contact_result?;
                contacts.insert(contact_id, contact);
            }

            // Now get all notification methods for these contacts
            let contact_ids: Vec<String> = contacts.keys().cloned().collect();
            if !contact_ids.is_empty() {
                let placeholders = contact_ids
                    .iter()
                    .map(|_| "?")
                    .collect::<Vec<_>>()
                    .join(",");
                let query = format!(
                    "SELECT id, contact_id, provider_type, notification_target, created_at 
                     FROM contact_notification_methods 
                     WHERE contact_id IN ({}) ORDER BY contact_id, provider_type",
                    placeholders
                );

                let mut method_stmt = conn.prepare(&query)?;
                let method_params: Vec<&dyn ToSql> =
                    contact_ids.iter().map(|id| id as &dyn ToSql).collect();

                let method_iter = method_stmt.query_map(method_params.as_slice(), |row| {
                    let provider_type_str: String = row.get(2)?;
                    let provider_type = ProviderType::from(provider_type_str.as_str());
                    let notification_target: String = row.get(3)?;

                    // Format phone numbers for display
                    let display_target = if provider_type == ProviderType::Sms {
                        PhoneNumber::from_str(&notification_target)
                            .ok()
                            .map(|phone| {
                                phone
                                    .format()
                                    .mode(phonenumber::Mode::International)
                                    .to_string()
                            })
                    } else {
                        None
                    };

                    Ok(NotificationMethod {
                        id: Some(row.get(0)?),
                        contact_id: row.get(1)?,
                        provider_type,
                        notification_target,
                        display_target,
                        created_at: row.get(4)?,
                    })
                })?;

                // Add notification methods to their corresponding contacts
                for method_result in method_iter {
                    let method = method_result?;
                    if let Some(contact) = contacts.get_mut(&method.contact_id) {
                        contact.notification_methods.push(method);
                    }
                }
            }

            Ok(contacts.into_values().collect())
        })
        .await?
    }

    pub async fn delete_wallet_contact(
        &self,
        wallet_checksum: &str,
        contact_id: &str,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();
        let contact_id = contact_id.to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let rows_affected = conn.execute(
                "DELETE FROM contacts WHERE id = ?1 AND wallet_checksum = ?2",
                params![contact_id, checksum],
            )?;
            Ok(rows_affected > 0)
        })
        .await?
    }

    pub async fn update_wallet_balance_by_checksum(
        &self,
        wallet_checksum: &str,
        balance_total: i64,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE wallets SET balance_total = ?1 WHERE checksum = ?2",
                params![balance_total, checksum],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn update_wallet_by_checksum(
        &self,
        wallet_checksum: &str,
        name: &str,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let name = name.to_string();
        let checksum = wallet_checksum.to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let changes = conn.execute(
                "UPDATE wallets SET name = ?1 WHERE checksum = ?2",
                params![&name, checksum],
            )?;
            Ok(changes > 0)
        })
        .await?
    }

    pub async fn update_wallet_last_synced(&self, checksum: &str) -> Result<()> {
        let pool = self.pool.clone();
        let checksum = checksum.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            let current_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();
            conn.execute(
                "UPDATE wallets SET last_synced_at = ?1 WHERE checksum = ?2",
                params![current_time, checksum],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn get_wallets_due_for_sync(&self) -> Result<Vec<(WalletMetadata, SubscriptionTier)>> {
        let pool = self.pool.clone();

        spawn_blocking(move || -> Result<Vec<(WalletMetadata, SubscriptionTier)>> {
            let conn = pool.get()?;
            let current_time = chrono::Utc::now();
            
            let mut stmt = conn.prepare(
                "SELECT w.checksum, w.name, w.descriptor, w.hex_color, w.balance_total, 
                        w.last_activity, w.last_synced_at, w.user_id, w.created_at,
                        u.subscription_tier
                 FROM wallets w 
                 JOIN users u ON w.user_id = u.id
                 WHERE w.is_active = 1 AND (
                    -- Active subscriptions
                    u.subscription_status = 'active'
                    OR 
                    -- Trial users within trial period  
                    (u.subscription_status = 'trial' AND datetime(u.trial_ends_at) > datetime('now'))
                    OR
                    -- Cancelled users still within their paid period
                    (u.subscription_status = 'canceled' AND u.subscription_ends_at IS NOT NULL AND datetime(u.subscription_ends_at) > datetime('now'))
                 )
                 ORDER BY w.checksum"
            )?;

            let wallet_rows = stmt.query_map([], |row| {
                Ok((
                    WalletMetadata {
                        checksum: row.get(0)?,
                        name: row.get(1)?,
                        descriptor: row.get(2)?,
                        hex_color: row.get(3)?,
                        balance_total: row.get(4)?,
                        last_activity: row.get(5)?,
                        contact_count: None, // Not counting contacts in this query
                        user_id: row.get(7)?,
                        created_at: row.get(8)?,
                        is_active: true, // Query already filters for active wallets
                    },
                    SubscriptionTier::from(row.get::<_, String>(9)?),
                    row.get::<_, Option<String>>(6)?, // last_synced_at
                ))
            })?;

            let mut due_wallets = Vec::new();
            
            for row in wallet_rows {
                let (wallet, tier, last_synced_at) = row?;
                let tier_limits = tier.limits();
                
                // Check if this wallet is due for sync based on its owner's tier
                let should_sync = match last_synced_at {
                    Some(last_sync_str) => {
                        // Parse the last sync time
                        match chrono::DateTime::parse_from_str(&format!("{} +00:00", last_sync_str), "%Y-%m-%d %H:%M:%S%.3f %z") {
                            Ok(last_sync) => {
                                let elapsed = current_time.signed_duration_since(last_sync.with_timezone(&chrono::Utc));
                                elapsed.num_seconds() >= tier_limits.sync_interval_secs as i64
                            },
                            Err(_) => true, // If we can't parse, sync anyway
                        }
                    },
                    None => true, // Never synced before
                };
                
                if should_sync {
                    due_wallets.push((wallet, tier));
                }
            }

            Ok(due_wallets)
        })
        .await?
    }

    pub async fn upsert_current_block_header(&self, block_header: &BlockHeader) -> Result<()> {
        let pool = self.pool.clone();
        let block_header = block_header.clone();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "INSERT OR REPLACE INTO current_block_header (id, height, timestamp, updated_at) 
                 VALUES (1, ?1, ?2, datetime('now'))",
                params![block_header.height, block_header.timestamp,],
            )?;
            Ok(())
        })
        .await?
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
        })
        .await?
    }

    pub async fn insert_notification_log_for_method(
        &self,
        event_id: &str,
        notification_method_id: &str,
        provider_name: &str,
        provider_message_id: Option<&str>,
        status: &str,
        error_message: Option<&str>,
        message_content: &str,
    ) -> Result<String> {
        let pool = self.pool.clone();
        let event_id = event_id.to_string();
        let notification_method_id = notification_method_id.to_string();
        let provider_name = provider_name.to_string();
        let provider_message_id = provider_message_id.map(|s| s.to_string());
        let status = status.to_string();
        let error_message = error_message.map(|s| s.to_string());
        let message_content = message_content.to_string();

        spawn_blocking(move || -> Result<String> {
            let conn = pool.get()?;
            let log_id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO notification_logs (id, event_id, notification_method_id, provider_name, provider_message_id, status, error_message, message_content) 
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    &log_id,
                    &event_id,
                    &notification_method_id,
                    &provider_name,
                    &provider_message_id,
                    &status,
                    &error_message,
                    &message_content,
                ],
            )?;
            Ok(log_id)
        }).await?
    }

    // User management methods
    pub async fn create_user(
        &self,
        email: &str,
        password_hash: &str,
        name: Option<&str>,
        email_verified: bool,
    ) -> Result<String> {
        let pool = self.pool.clone();
        let email = email.to_string();
        let password_hash = password_hash.to_string();
        let name = name.map(|n| n.to_string());

        // Check if auth is enabled to determine admin logic
        let auth_enabled = std::env::var("CANARY_ENABLE_AUTH")
            .map(|v| v.to_lowercase() == "true" || v == "1")
            .unwrap_or(false);

        let result = spawn_blocking(move || -> Result<(String, bool)> {
            let conn = pool.get()?;
            let tx = conn.unchecked_transaction()?;
            
            // Check if user already exists
            let existing: Option<String> = tx
                .prepare("SELECT id FROM users WHERE email = ?1")?
                .query_row(params![&email], |row| row.get(0))
                .ok();
            
            if let Some(_id) = existing {
                tx.rollback()?;
                return Err(anyhow::anyhow!("User with this email already exists"));
            }
            
            // Determine if this user should be admin
            let mut final_is_admin = false;
            
            if auth_enabled {
                // AUTH=true: Check if any admin users already exist  
                let admin_count: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM users WHERE is_admin = 1",
                    [],
                    |row| row.get(0)
                )?;
                
                println!("DEBUG: User {} - current admin_count={}, auth_enabled={}", email, admin_count, auth_enabled);
                
                if admin_count == 0 {
                    final_is_admin = true;
                    println!("No admin users exist, creating first admin user: {}", email);
                } else {
                    println!("Admin users already exist (count={}), creating regular user: {}", admin_count, email);
                }
            }
            // Note: When AUTH=false, we don't create users through this function
            
            let user_name = name;
            
            // Generate UUID for new user
            let user_id = Uuid::new_v4().to_string();
            
            println!("DEBUG: Creating user {} with name {:?}, is_admin={}", email, user_name, final_is_admin);
            
            // Create new user
            tx.execute(
                "INSERT INTO users (id, email, password_hash, name, is_admin, email_verified, subscription_tier, subscription_status) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![&user_id, &email, &password_hash, user_name, final_is_admin, email_verified, "team", "pending"],
            )?;
            
            tx.commit()?;
            Ok((user_id, final_is_admin))
        }).await??; // First ? for JoinError, second ? for inner Result

        let (user_id, _was_admin) = result;

        Ok(user_id)
    }

    pub async fn get_user_by_email(&self, email: &str) -> Result<Option<UserRecord>> {
        let pool = self.pool.clone();
        let email = email.to_string();

        spawn_blocking(move || -> Result<Option<UserRecord>> {
            let conn = pool.get()?;
            let result = conn
                .prepare("SELECT id, email, password_hash, name, is_admin, email_verified, subscription_tier, trial_ends_at, subscription_status, stripe_customer_id, stripe_subscription_id, subscription_started_at, subscription_ends_at, created_at FROM users WHERE email = ?1")?
                .query_row(params![&email], |row| {
                    Ok(UserRecord {
                        id: row.get(0)?,
                        email: row.get(1)?,
                        password_hash: row.get(2)?,
                        name: row.get(3)?,
                        is_admin: row.get(4)?,
                        email_verified: row.get(5)?,
                        subscription_tier: SubscriptionTier::from(row.get::<_, String>(6)?),
                        trial_ends_at: row.get(7)?,
                        subscription_status: row.get(8)?,
                        stripe_customer_id: row.get(9)?,
                        stripe_subscription_id: row.get(10)?,
                        subscription_started_at: row.get(11)?,
                        subscription_ends_at: row.get(12)?,
                        created_at: row.get(13)?,
                    })
                })
                .ok();
            Ok(result)
        }).await?
    }

    pub async fn get_user_by_id(&self, user_id: &str) -> Result<Option<UserRecord>> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();

        spawn_blocking(move || -> Result<Option<UserRecord>> {
            let conn = pool.get()?;
            let result = conn
                .prepare("SELECT id, email, password_hash, name, is_admin, email_verified, subscription_tier, trial_ends_at, subscription_status, stripe_customer_id, stripe_subscription_id, subscription_started_at, subscription_ends_at, created_at FROM users WHERE id = ?1")?
                .query_row(params![user_id], |row| {
                    Ok(UserRecord {
                        id: row.get(0)?,
                        email: row.get(1)?,
                        password_hash: row.get(2)?,
                        name: row.get(3)?,
                        is_admin: row.get(4)?,
                        email_verified: row.get(5)?,
                        subscription_tier: SubscriptionTier::from(row.get::<_, String>(6)?),
                        trial_ends_at: row.get(7)?,
                        subscription_status: row.get(8)?,
                        stripe_customer_id: row.get(9)?,
                        stripe_subscription_id: row.get(10)?,
                        subscription_started_at: row.get(11)?,
                        subscription_ends_at: row.get(12)?,
                        created_at: row.get(13)?,
                    })
                })
                .ok();
            Ok(result)
        }).await?
    }

    pub async fn get_user_by_stripe_customer_id(&self, stripe_customer_id: &str) -> Result<Option<UserRecord>> {
        let pool = self.pool.clone();
        let stripe_customer_id = stripe_customer_id.to_string();

        spawn_blocking(move || -> Result<Option<UserRecord>> {
            let conn = pool.get()?;
            let result = conn
                .prepare("SELECT id, email, password_hash, name, is_admin, email_verified, subscription_tier, trial_ends_at, subscription_status, stripe_customer_id, stripe_subscription_id, subscription_started_at, subscription_ends_at, created_at FROM users WHERE stripe_customer_id = ?1")?
                .query_row(params![stripe_customer_id], |row| {
                    Ok(UserRecord {
                        id: row.get(0)?,
                        email: row.get(1)?,
                        password_hash: row.get(2)?,
                        name: row.get(3)?,
                        is_admin: row.get(4)?,
                        email_verified: row.get(5)?,
                        subscription_tier: SubscriptionTier::from(row.get::<_, String>(6)?),
                        trial_ends_at: row.get(7)?,
                        subscription_status: row.get(8)?,
                        stripe_customer_id: row.get(9)?,
                        stripe_subscription_id: row.get(10)?,
                        subscription_started_at: row.get(11)?,
                        subscription_ends_at: row.get(12)?,
                        created_at: row.get(13)?,
                    })
                })
                .ok();
            Ok(result)
        }).await?
    }

    pub async fn update_last_login(&self, user_id: &str) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            let current_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            conn.execute(
                "UPDATE users SET last_login = ?1 WHERE id = ?2",
                params![&current_time, user_id],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn get_all_users(&self) -> Result<Vec<UserRecord>> {
        let pool = self.pool.clone();
        
        spawn_blocking(move || -> Result<Vec<UserRecord>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, email, password_hash, name, is_admin, email_verified, subscription_tier, 
                        subscription_status, trial_ends_at, subscription_started_at,
                        stripe_customer_id, stripe_subscription_id, subscription_ends_at,
                        created_at
                 FROM users"
            )?;
            
            let user_iter = stmt.query_map([], |row| {
                Ok(UserRecord {
                    id: row.get(0)?,
                    email: row.get(1)?,
                    password_hash: row.get(2)?,
                    name: row.get(3)?,
                    is_admin: row.get(4)?,
                    email_verified: row.get(5)?,
                    subscription_tier: SubscriptionTier::from(row.get::<_, String>(6)?),
                    subscription_status: row.get(7)?,
                    trial_ends_at: row.get(8)?,
                    subscription_started_at: row.get(9)?,
                    stripe_customer_id: row.get(10)?,
                    stripe_subscription_id: row.get(11)?,
                    subscription_ends_at: row.get(12)?,
                    created_at: row.get(13)?,
                })
            })?;
            
            let mut users = Vec::new();
            for user in user_iter {
                users.push(user?);
            }
            
            Ok(users)
        })
        .await?
    }

    pub async fn update_user_name(&self, user_id: &str, name: &str) -> Result<()> {
        let pool = self.pool.clone();
        let name = name.to_string();
        let user_id = user_id.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE users SET name = ?1 WHERE id = ?2",
                params![&name, user_id],
            )?;
            Ok(())
        })
        .await?
    }

    // Stripe subscription management
    pub async fn update_user_stripe_customer(
        &self,
        user_id: &str,
        stripe_customer_id: &str,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let stripe_customer_id = stripe_customer_id.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE users SET stripe_customer_id = ?1 WHERE id = ?2",
                params![stripe_customer_id, user_id],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn update_user_subscription_status(
        &self,
        user_id: &str,
        subscription_status: &str,
        stripe_subscription_id: Option<&str>,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let subscription_status = subscription_status.to_string();
        let stripe_subscription_id = stripe_subscription_id.map(|s| s.to_string());

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE users SET 
                    subscription_status = ?1,
                    stripe_subscription_id = COALESCE(?2, stripe_subscription_id)
                WHERE id = ?3",
                params![
                    subscription_status,
                    stripe_subscription_id,
                    user_id,
                ],
            )?;
            Ok(())
        }).await?
    }

    pub async fn update_user_subscription(
        &self,
        user_id: &str,
        subscription_tier: &str,
        subscription_status: &str,
        stripe_subscription_id: Option<&str>,
        subscription_started_at: Option<&str>,
        trial_ends_at: Option<&str>,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let subscription_tier = subscription_tier.to_string();
        let subscription_status = subscription_status.to_string();
        let stripe_subscription_id = stripe_subscription_id.map(|s| s.to_string());
        let subscription_started_at = subscription_started_at.map(|s| s.to_string());
        let trial_ends_at = trial_ends_at.map(|s| s.to_string());

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE users SET 
                    subscription_tier = ?1, 
                    subscription_status = ?2, 
                    stripe_subscription_id = ?3, 
                    subscription_started_at = ?4,
                    trial_ends_at = COALESCE(?5, trial_ends_at)
                WHERE id = ?6",
                params![
                    subscription_tier,
                    subscription_status,
                    stripe_subscription_id,
                    subscription_started_at,
                    trial_ends_at,
                    user_id
                ],
            )?;
            Ok(())
        })
        .await?
    }


    // Session management
    pub async fn create_session(
        &self,
        user_id: &str,
        token_hash: &str,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<String> {
        let pool = self.pool.clone();
        let token_hash = token_hash.to_string();
        let expires_at = expires_at.format("%Y-%m-%d %H:%M:%S").to_string();

        let user_id = user_id.to_string();
        spawn_blocking(move || -> Result<String> {
            let conn = pool.get()?;
            let session_id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO sessions (id, user_id, token_hash, expires_at) VALUES (?1, ?2, ?3, ?4)",
                params![&session_id, &user_id, &token_hash, &expires_at],
            )?;
            Ok(session_id)
        })
        .await?
    }

    pub async fn delete_session(&self, token_hash: &str) -> Result<()> {
        let pool = self.pool.clone();
        let token_hash = token_hash.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "DELETE FROM sessions WHERE token_hash = ?1",
                params![&token_hash],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn cleanup_expired_sessions(&self) -> Result<u64> {
        let pool = self.pool.clone();

        spawn_blocking(move || -> Result<u64> {
            let conn = pool.get()?;
            let current_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            let rows_deleted = conn.execute(
                "DELETE FROM sessions WHERE expires_at < ?1",
                params![&current_time],
            )?;
            Ok(rows_deleted as u64)
        })
        .await?
    }

    // Email verification token management
    pub async fn create_email_verification_token(&self, user_id: &str, token: &str) -> Result<()> {
        let pool = self.pool.clone();
        let token = token.to_string();
        let expires_at = (chrono::Utc::now() + chrono::Duration::hours(24))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        let user_id = user_id.to_string();
        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO email_verification_tokens (user_id, token, expires_at) VALUES (?1, ?2, ?3)",
                params![&user_id, &token, &expires_at],
            )?;
            Ok(())
        }).await?
    }

    pub async fn verify_email_token(&self, token: &str) -> Result<Option<String>> {
        let pool = self.pool.clone();
        let token = token.to_string();
        let current_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        
        tracing::debug!("Verifying email token: {} at time: {}", token, current_time);

        spawn_blocking(move || -> Result<Option<String>> {
            let conn = pool.get()?;
            let tx = conn.unchecked_transaction()?;
            
            // Get user_id for valid, non-expired token
            let user_id: Option<String> = tx
                .prepare("SELECT user_id FROM email_verification_tokens WHERE token = ?1 AND expires_at > ?2")?
                .query_row(params![&token, &current_time], |row| row.get(0))
                .ok();
            
            tracing::debug!("Token query result: {:?}", user_id);
            
            if let Some(user_id) = user_id {
                // Mark user as verified
                tx.execute(
                    "UPDATE users SET email_verified = TRUE WHERE id = ?1",
                    params![user_id],
                )?;
                
                // Delete the token
                tx.execute(
                    "DELETE FROM email_verification_tokens WHERE token = ?1",
                    params![&token],
                )?;
                
                tx.commit()?;
                Ok(Some(user_id))
            } else {
                tx.rollback()?;
                Ok(None)
            }
        }).await?
    }

    // Password reset token management
    pub async fn create_password_reset_token(&self, user_id: &str, token: &str) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let token = token.to_string();
        let expires_at = (chrono::Utc::now() + chrono::Duration::hours(1))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            
            // Delete any existing tokens for this user
            conn.execute(
                "DELETE FROM password_reset_tokens WHERE user_id = ?1",
                params![&user_id],
            )?;
            
            // Create new token
            conn.execute(
                "INSERT INTO password_reset_tokens (user_id, token, expires_at) VALUES (?1, ?2, ?3)",
                params![&user_id, &token, &expires_at],
            )?;
            Ok(())
        }).await?
    }

    pub async fn verify_password_reset_token(&self, token: &str) -> Result<Option<i64>> {
        let pool = self.pool.clone();
        let token = token.to_string();
        let current_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        spawn_blocking(move || -> Result<Option<i64>> {
            let conn = pool.get()?;
            let user_id: Option<i64> = conn
                .prepare("SELECT user_id FROM password_reset_tokens WHERE token = ?1 AND expires_at > ?2")?
                .query_row(params![&token, &current_time], |row| row.get(0))
                .ok();
            Ok(user_id)
        }).await?
    }

    pub async fn update_user_password(&self, user_id: i64, password_hash: &str) -> Result<()> {
        let pool = self.pool.clone();
        let password_hash = password_hash.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            let tx = conn.unchecked_transaction()?;

            // Update password
            tx.execute(
                "UPDATE users SET password_hash = ?1 WHERE id = ?2",
                params![&password_hash, user_id],
            )?;

            // Delete all password reset tokens for this user
            tx.execute(
                "DELETE FROM password_reset_tokens WHERE user_id = ?1",
                params![user_id],
            )?;

            tx.commit()?;
            Ok(())
        })
        .await?
    }

    // Rate limiting for OTP (SMS contact verification only)
    pub async fn check_rate_limit(&self, phone_number: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let phone_number = phone_number.to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let current_time = chrono::Utc::now();
            let current_time_str = current_time.format("%Y-%m-%d %H:%M:%S").to_string();
            
            // Check if blocked
            let blocked: Option<String> = conn
                .prepare("SELECT blocked_until FROM otp_attempts WHERE phone_number = ?1")?
                .query_row(params![&phone_number], |row| row.get(0))
                .ok();
            
            if let Some(blocked_until) = blocked {
                if blocked_until > current_time_str {
                    return Ok(false); // Still blocked
                }
            }
            
            // Check recent attempts (last 15 minutes)
            let fifteen_minutes_ago = (current_time - chrono::Duration::minutes(15))
                .format("%Y-%m-%d %H:%M:%S").to_string();
            
            let recent_attempts: i32 = conn
                .prepare("SELECT attempt_count FROM otp_attempts WHERE phone_number = ?1 AND last_attempt > ?2")?
                .query_row(params![&phone_number, &fifteen_minutes_ago], |row| row.get(0))
                .unwrap_or(0);
            
            if recent_attempts >= 5 {
                // Block for 30 minutes
                let blocked_until = (current_time + chrono::Duration::minutes(30))
                    .format("%Y-%m-%d %H:%M:%S").to_string();
                
                conn.execute(
                    "UPDATE otp_attempts SET blocked_until = ?1 WHERE phone_number = ?2",
                    params![&blocked_until, &phone_number],
                )?;
                
                return Ok(false);
            }
            
            // Update attempt count
            let exists: bool = conn
                .prepare("SELECT 1 FROM otp_attempts WHERE phone_number = ?1")?
                .exists(params![&phone_number])?;
            
            if exists {
                conn.execute(
                    "UPDATE otp_attempts SET attempt_count = attempt_count + 1, last_attempt = ?1 WHERE phone_number = ?2",
                    params![&current_time_str, &phone_number],
                )?;
            } else {
                conn.execute(
                    "INSERT INTO otp_attempts (phone_number, attempt_count, last_attempt) VALUES (?1, 1, ?2)",
                    params![&phone_number, &current_time_str],
                )?;
            }
            
            Ok(true)
        }).await?
    }

    pub async fn clear_rate_limit(&self, phone_number: &str) -> Result<()> {
        let pool = self.pool.clone();
        let phone_number = phone_number.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "DELETE FROM otp_attempts WHERE phone_number = ?1",
                params![&phone_number],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn create_pending_contact_verification(
        &self,
        wallet_checksum: &str,
        provider_type: &str,
        notification_target: &str,
        contact_name: &str,
        language: &str,
        verification_code: Option<&str>,
    ) -> Result<i64> {
        let pool = self.pool.clone();
        let wallet_checksum = wallet_checksum.to_string();
        let provider_type = provider_type.to_string();
        let notification_target = notification_target.to_string();
        let contact_name = contact_name.to_string();
        let language = language.to_string();
        let verification_code = verification_code.map(|s| s.to_string());

        spawn_blocking(move || {
            let conn = pool.get()?;
            let expires_at = chrono::Utc::now() + chrono::Duration::minutes(10);
            
            conn.execute(
                "INSERT INTO pending_contact_verifications 
                 (wallet_checksum, provider_type, notification_target, contact_name, language, verification_code, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    &wallet_checksum,
                    &provider_type,
                    &notification_target,
                    &contact_name,
                    &language,
                    &verification_code,
                    expires_at.to_rfc3339()
                ],
            )?;
            Ok(conn.last_insert_rowid())
        }).await?
    }

    pub async fn get_pending_verification(
        &self,
        wallet_checksum: &str,
        notification_target: &str,
    ) -> Result<Option<(i64, String, String, Option<String>)>> {
        let pool = self.pool.clone();
        let wallet_checksum = wallet_checksum.to_string();
        let notification_target = notification_target.to_string();

        spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, contact_name, language, verification_code 
                 FROM pending_contact_verifications 
                 WHERE wallet_checksum = ?1 
                 AND notification_target = ?2 
                 AND expires_at > datetime('now')
                 ORDER BY created_at DESC
                 LIMIT 1",
            )?;

            let result = stmt
                .query_row(params![&wallet_checksum, &notification_target], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                })
                .optional()?;

            Ok(result)
        })
        .await?
    }

    pub async fn delete_pending_verification(&self, verification_id: i64) -> Result<()> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            let conn = pool.get()?;
            conn.execute(
                "DELETE FROM pending_contact_verifications WHERE id = ?1",
                params![verification_id],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn cleanup_expired_verifications(&self) -> Result<u64> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            let conn = pool.get()?;
            let deleted = conn.execute(
                "DELETE FROM pending_contact_verifications WHERE expires_at <= datetime('now')",
                [],
            )?;
            Ok(deleted as u64)
        })
        .await?
    }

    /// Check for expired subscriptions and trials, mark users as expired (keep tier but stop syncing)
    pub async fn process_expired_subscriptions(&self) -> Result<usize> {
        let pool = self.pool.clone();

        spawn_blocking(move || -> Result<usize> {
            let conn = pool.get()?;

            // Find users whose subscriptions have expired
            let mut stmt = conn.prepare(
                "SELECT id, email, subscription_tier, subscription_status, trial_ends_at, subscription_ends_at 
                 FROM users 
                 WHERE (
                    -- Expired trials (users who didn't subscribe after trial ended)
                    (subscription_status = 'trial' AND datetime(trial_ends_at) <= datetime('now'))
                    OR 
                    -- Expired cancelled subscriptions (users who cancelled and period ended)
                    (subscription_status = 'canceled' AND subscription_ends_at IS NOT NULL AND datetime(subscription_ends_at) <= datetime('now'))
                 )
                 AND subscription_status != 'expired'"
            )?;

            let expired_users: Vec<(String, String, String, String)> = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?, // id
                        row.get::<_, String>(1)?, // email
                        row.get::<_, String>(2)?, // subscription_tier
                        row.get::<_, String>(3)?, // subscription_status
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;

            let count = expired_users.len();

            // Mark users as expired but keep their tier (they just stop getting wallet syncing)
            for (user_id, email, tier, status) in expired_users {
                match conn.execute(
                    "UPDATE users SET 
                        subscription_status = 'expired'
                     WHERE id = ?1",
                    params![user_id],
                ) {
                    Ok(_) => {
                        tracing::info!("📊 Marked user {} ({}) as expired - keeping {} tier but stopping sync", email, status, tier);
                    }
                    Err(e) => {
                        tracing::error!("Failed to update user {} status: {}", email, e);
                    }
                }
            }

            Ok(count)
        }).await?
    }

    /// Update wallet active status for subscription tier limits
    pub async fn update_wallet_active_status(&self, checksum: &str, is_active: bool) -> Result<()> {
        let pool = self.pool.clone();
        let checksum = checksum.to_string();
        
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            
            conn.execute(
                "UPDATE wallets SET is_active = ? WHERE checksum = ?",
                params![is_active, checksum],
            )?;
            
            Ok::<(), anyhow::Error>(())
        }).await??;
        
        Ok(())
    }

    /// Update contact active status for subscription tier limits
    pub async fn update_contact_active_status(&self, contact_id: &str, is_active: bool) -> Result<()> {
        let pool = self.pool.clone();
        let contact_id = contact_id.to_string();
        
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            
            conn.execute(
                "UPDATE contacts SET is_active = ? WHERE id = ?",
                params![is_active, contact_id],
            )?;
            
            Ok::<(), anyhow::Error>(())
        }).await??;
        
        Ok(())
    }
}
