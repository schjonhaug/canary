use crate::config::AppConfig;
use crate::electrum::BlockHeader;
use crate::exchange_rates;
use crate::migrations::MigrationRunner;
use crate::subscription::SubscriptionTier;
use anyhow::{anyhow, Context, Result};
use bdk_wallet::rusqlite::Connection;
use bdk_wallet::rusqlite::{params, OptionalExtension, ToSql};
use chrono;
use phonenumber::PhoneNumber;
use r2d2::{CustomizeConnection, Pool};
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
    pub trial_ends_at: Option<String>,
    pub subscription_status: String,
    pub stripe_customer_id: Option<String>,
    pub stripe_subscription_id: Option<String>,
    pub subscription_started_at: Option<String>,
    pub subscription_ends_at: Option<String>,
    pub created_at: String,
    // User preferences
    pub preferred_fiat_currency: Option<String>,
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
    pub status: String,
    pub contact_count: Option<i64>,
    pub user_id: String,
    pub is_active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance_fiat: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fiat_currency: Option<String>,
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

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, Copy, PartialEq)]
pub enum BalanceAlertType {
    #[serde(rename = "above")]
    Above,
    #[serde(rename = "below")]
    Below,
    #[serde(rename = "equals")]
    Equals,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct BalanceAlert {
    pub id: String, // UUIDv4
    pub wallet_checksum: String,
    pub threshold_sats: i64,
    pub alert_type: BalanceAlertType,
    pub is_active: bool,
    pub last_triggered_at: Option<u64>, // Unix timestamp
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct BalanceAlertNotification {
    pub id: String, // UUIDv4
    pub balance_alert_id: String,
    pub wallet_checksum: String,
    pub threshold_sats: i64,
    pub current_balance_sats: i64,
    pub alert_type: BalanceAlertType,
    pub notification_sent_at: u64, // Unix timestamp
    pub created_at: String,
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

impl BalanceAlertType {
    pub fn as_str(&self) -> &'static str {
        match self {
            BalanceAlertType::Above => "above",
            BalanceAlertType::Below => "below",
            BalanceAlertType::Equals => "equals",
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

impl From<&str> for BalanceAlertType {
    fn from(s: &str) -> Self {
        match s {
            "above" => BalanceAlertType::Above,
            "below" => BalanceAlertType::Below,
            "equals" => BalanceAlertType::Equals,
            _ => panic!("Invalid balance alert type: {}", s),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct Transaction {
    pub txid: String, // Bitcoin transaction ID (hash) - primary key
    pub wallet_checksum: String,
    pub transaction_type: EventType,
    pub amount_sats: i64,
    pub fee_sats: Option<i64>, // Transaction fee (for send transactions)
    pub block_height: Option<u32>, // NULL = mempool, >0 = confirmed at this height
    pub first_seen_at: u64,    // Unix timestamp when we first detected this transaction
    pub confirmed_at: Option<u64>, // Unix timestamp when transaction was confirmed
    pub parent_txid: Option<String>,
    // RBF replacement tracking
    pub transaction_status: String, // 'pending', 'confirmed', 'replaced'
    pub replaced_by_txid: Option<String>, // Transaction ID that replaced this one (if any)
    pub replaced_at: Option<u64>,   // Unix timestamp when this transaction was replaced
    pub notification_status: Vec<NotificationStatus>,
}

/// Notification wrapper for transactions
/// Used to indicate why a notification is being sent for a transaction
#[derive(Debug, Clone)]
pub enum TransactionNotification {
    /// New transaction detected in mempool (first notification round)
    Pending(Transaction),
    /// Transaction confirmed in block (second notification round)
    Confirmed(Transaction),
    /// Balance alert triggered (balance threshold crossed)
    BalanceAlert(BalanceAlertNotification),
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct TransactionWithWallet {
    pub txid: String, // Bitcoin transaction ID (hash) - primary key
    pub wallet_checksum: String,
    pub wallet_name: String,
    pub transaction_type: EventType,
    pub amount_sats: i64,
    pub fee_sats: Option<i64>, // Transaction fee (for send transactions)
    pub block_height: Option<u32>, // NULL = mempool, >0 = confirmed at this height
    pub first_seen_at: u64,    // Unix timestamp when we first detected this transaction
    pub confirmed_at: Option<u64>, // Unix timestamp when transaction was confirmed
    pub parent_txid: Option<String>,
    // RBF replacement tracking
    pub transaction_status: String, // 'pending', 'confirmed', 'replaced'
    pub replaced_by_txid: Option<String>, // Transaction ID that replaced this one (if any)
    pub replaced_at: Option<u64>,   // Unix timestamp when this transaction was replaced
    pub notification_status: Vec<NotificationStatus>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct NotificationStatus {
    pub contact_name: String,
    pub provider_name: String,
    pub status: String,
    pub error_message: Option<String>,
    pub notification_target: Option<String>, // Phone number, email, or ntfy topic
    pub provider_type: Option<String>,       // 'sms', 'email', 'ntfy'
    pub created_at: String,                  // When the notification was sent
    pub notification_type: String,           // "pending" or "confirmed"
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
    pub transactions: Vec<TransactionWithWallet>,
    pub contacts: Vec<Contact>,
}

#[derive(Debug, Clone)]
pub struct TransactionInsert {
    pub txid: String, // Bitcoin transaction ID (hash)
    pub wallet_checksum: String,
    pub transaction_type: EventType,
    pub amount_sats: i64,
    pub fee_sats: Option<i64>, // Transaction fee (for send transactions)
    pub block_height: Option<u32>, // NULL = mempool, >0 = confirmed at this height
    pub first_seen_at: u64,    // Unix timestamp when we first detected this transaction
    pub confirmed_at: Option<u64>, // Unix timestamp when transaction was confirmed
    pub parent_txid: Option<String>,
    // RBF replacement tracking
    pub transaction_status: String, // 'pending', 'confirmed', 'replaced'
    pub replaced_by_txid: Option<String>, // Transaction ID that replaced this one (if any)
    pub replaced_at: Option<u64>,   // Unix timestamp when this transaction was replaced
}

impl Default for TransactionInsert {
    fn default() -> Self {
        Self {
            txid: String::new(),
            wallet_checksum: String::new(),
            transaction_type: EventType::Send,
            amount_sats: 0,
            fee_sats: None,
            block_height: None,
            first_seen_at: 0,
            confirmed_at: None,
            parent_txid: None,
            transaction_status: "pending".to_string(),
            replaced_by_txid: None,
            replaced_at: None,
        }
    }
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

#[derive(Debug)]
struct ForeignKeyEnabler;

impl CustomizeConnection<Connection, bdk_wallet::rusqlite::Error> for ForeignKeyEnabler {
    fn on_acquire(&self, conn: &mut Connection) -> Result<(), bdk_wallet::rusqlite::Error> {
        conn.execute_batch("PRAGMA foreign_keys = ON")
    }
}

type DbPool = Pool<SqliteConnectionManager>;

#[derive(Clone)]
pub struct MetadataDb {
    pool: Arc<DbPool>,
}

impl MetadataDb {
    pub async fn new(db_path: &str, config: &AppConfig) -> Result<Self> {
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

        // Create connection pool with foreign key enforcement
        let manager = SqliteConnectionManager::file(db_path);
        let pool = Pool::builder()
            .max_size(16)
            .connection_customizer(Box::new(ForeignKeyEnabler))
            .build(manager)
            .context("Failed to create database pool")?;

        let db = MetadataDb {
            pool: Arc::new(pool),
        };

        // Initialize user based on operating mode
        db.initialize_user_for_mode(config).await?;

        Ok(db)
    }

    async fn initialize_user_for_mode(&self, config: &AppConfig) -> Result<()> {
        if config.is_foss_mode() {
            // FOSS mode: Create hardcoded "foss-user" admin
            self.ensure_foss_user().await?;
        } else if cfg!(debug_assertions) {
            // SAAS mode in dev: Create hardcoded dev test users
            self.ensure_dev_test_users().await?;
        }
        // SAAS mode in production: Users created via registration

        Ok(())
    }

    async fn ensure_foss_user(&self) -> Result<()> {
        let pool = self.pool.clone();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;

            // Check if foss-user already exists
            let foss_user_exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM users WHERE id = 'foss-user')",
                [],
                |row| row.get(0),
            )?;

            if !foss_user_exists {
                // Detect system locale for currency, fallback to USD
                let default_currency = std::env::var("LANG")
                    .or_else(|_| std::env::var("LC_ALL"))
                    .map(|locale| crate::exchange_rates::ExchangeRateService::locale_to_currency(&locale))
                    .unwrap_or("USD");

                // Create the hardcoded FOSS user with locale-based currency
                conn.execute(
                    "INSERT INTO users (id, email, password_hash, name, is_admin, email_verified, subscription_tier, subscription_status, created_at, preferred_fiat_currency)
                     VALUES ('foss-user', 'admin@local', '', 'Admin', 1, 1, 'team', 'active', datetime('now'), ?1)",
                    [default_currency],
                )?;

                println!("✅ Created FOSS user: admin@local (foss-user) with currency: {}", default_currency);
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
                        "delivered+charlie@resend.dev" => ("Charlie", "team"),
                        _ => ("Test User", "personal"),
                    };

                    // First user becomes admin
                    let is_admin = index == 0;

                    let user_id = uuid::Uuid::new_v4().to_string();
                    conn.execute(
                        "INSERT INTO users (id, email, password_hash, name, is_admin, email_verified, subscription_tier, subscription_status, created_at, preferred_fiat_currency)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, datetime('now'), ?9)",
                        params![&user_id, email, &password_hash, name, is_admin, true, tier, "pending", "USD"], // Dev users follow same flow as real users
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
                "INSERT INTO wallets (checksum, name, descriptor, hex_color, balance_total, last_activity, status, user_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![&checksum, &name, &descriptor, &hex_color, "0", &current_time, "pending", user_id],
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
                        (SELECT MAX(t.first_seen_at) FROM transactions t WHERE t.wallet_checksum = w.checksum) as last_activity,
                        w.status, COUNT(c.id) as contact_count, w.user_id, w.is_active
                 FROM wallets w
                 LEFT JOIN contacts c ON w.checksum = c.wallet_checksum
                 WHERE w.descriptor = ?1
                 GROUP BY w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total, w.status, w.user_id, w.is_active",
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
                        status: row.get(7)?,
                        contact_count: Some(row.get(8)?),
                        user_id: row.get(9)?,
                        is_active: row.get::<_, i64>(10).unwrap_or(1) != 0,
                        balance_fiat: None,
                        fiat_currency: None,
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
                        (SELECT MAX(t.first_seen_at) FROM transactions t WHERE t.wallet_checksum = w.checksum) as last_activity,
                        w.status, COUNT(c.id) as contact_count, w.user_id, w.is_active
                 FROM wallets w
                 LEFT JOIN contacts c ON w.checksum = c.wallet_checksum
                 WHERE w.checksum = ?1
                 GROUP BY w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total, w.status, w.user_id, w.is_active",
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
                        status: row.get(7)?,
                        contact_count: Some(row.get(8)?),
                        user_id: row.get(9)?,
                        is_active: row.get::<_, i64>(10).unwrap_or(1) != 0,
                        balance_fiat: None,
                        fiat_currency: None,
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
                            (SELECT MAX(t.first_seen_at) FROM transactions t WHERE t.wallet_checksum = w.checksum) as last_activity,
                            w.status, COUNT(c.id) as contact_count, w.user_id, w.is_active
                     FROM wallets w
                     LEFT JOIN contacts c ON w.checksum = c.wallet_checksum
                     WHERE w.user_id = ?1 AND w.status != 'deleted'
                     GROUP BY w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total, w.status, w.user_id, w.is_active
                     ORDER BY w.created_at DESC"
                }
                None => {
                    "SELECT w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total,
                            (SELECT MAX(t.first_seen_at) FROM transactions t WHERE t.wallet_checksum = w.checksum) as last_activity,
                            w.status, COUNT(c.id) as contact_count, w.user_id, w.is_active
                     FROM wallets w
                     LEFT JOIN contacts c ON w.checksum = c.wallet_checksum
                     WHERE w.status != 'deleted'
                     GROUP BY w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total, w.status, w.user_id, w.is_active
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
                    status: row.get(7)?,
                    contact_count: Some(row.get(8)?),
                    user_id: row.get(9)?,
                    is_active: row.get::<_, i64>(10).unwrap_or(1) != 0, // SQLite stores bool as int
                    balance_fiat: None,
                    fiat_currency: None,
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
    pub async fn get_wallets_for_user_oldest_first(
        &self,
        user_id: &str,
    ) -> Result<Vec<WalletMetadata>> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();

        spawn_blocking(move || -> Result<Vec<WalletMetadata>> {
            let conn = pool.get()?;

            let query = "SELECT w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total,
                               (SELECT MAX(t.first_seen_at) FROM transactions t WHERE t.wallet_checksum = w.checksum) as last_activity,
                               w.status, COUNT(c.id) as contact_count, w.user_id, w.is_active
                        FROM wallets w
                        LEFT JOIN contacts c ON w.checksum = c.wallet_checksum
                        WHERE w.user_id = ?1
                        GROUP BY w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total, w.status, w.user_id, w.is_active
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
                    status: row.get(7)?,
                    contact_count: Some(row.get(8)?),
                    user_id: row.get(9)?,
                    is_active: row.get::<_, i64>(10).unwrap_or(1) != 0, // SQLite stores bool as int
                    balance_fiat: None,
                    fiat_currency: None,
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
            let count: i64 = conn.query_row(
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
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM contacts WHERE wallet_checksum = ?1",
                params![checksum],
                |row| row.get(0),
            )?;
            Ok(count as usize)
        })
        .await?
    }

    /// Check if a notification target (email or phone) is already used by another contact in the same wallet
    pub async fn check_duplicate_notification_target(
        &self,
        wallet_checksum: &str,
        provider_type: &str,
        notification_target: &str,
        exclude_contact_id: Option<&str>,
    ) -> Result<Option<String>> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();
        let provider = provider_type.to_string();
        let target = notification_target.to_string();
        let exclude_id = exclude_contact_id.map(|s| s.to_string());

        spawn_blocking(move || -> Result<Option<String>> {
            let conn = pool.get()?;

            // For emails, do case-insensitive comparison
            let existing_contact_name: Option<String> = if provider == "email" {
                if let Some(contact_id) = exclude_id {
                    conn.query_row(
                        "SELECT c.name FROM contacts c
                         JOIN contact_notification_methods cnm ON c.id = cnm.contact_id
                         WHERE cnm.wallet_checksum = ?1
                         AND cnm.provider_type = ?2
                         AND LOWER(cnm.notification_target) = LOWER(?3)
                         AND c.id != ?4
                         LIMIT 1",
                        params![&checksum, &provider, &target, &contact_id],
                        |row| row.get(0),
                    )
                    .optional()?
                } else {
                    conn.query_row(
                        "SELECT c.name FROM contacts c
                         JOIN contact_notification_methods cnm ON c.id = cnm.contact_id
                         WHERE cnm.wallet_checksum = ?1
                         AND cnm.provider_type = ?2
                         AND LOWER(cnm.notification_target) = LOWER(?3)
                         LIMIT 1",
                        params![&checksum, &provider, &target],
                        |row| row.get(0),
                    )
                    .optional()?
                }
            } else if let Some(contact_id) = exclude_id {
                // For SMS and other types, exact match
                conn.query_row(
                    "SELECT c.name FROM contacts c
                     JOIN contact_notification_methods cnm ON c.id = cnm.contact_id
                     WHERE cnm.wallet_checksum = ?1
                     AND cnm.provider_type = ?2
                     AND cnm.notification_target = ?3
                     AND c.id != ?4
                     LIMIT 1",
                    params![&checksum, &provider, &target, &contact_id],
                    |row| row.get(0),
                )
                .optional()?
            } else {
                conn.query_row(
                    "SELECT c.name FROM contacts c
                     JOIN contact_notification_methods cnm ON c.id = cnm.contact_id
                     WHERE cnm.wallet_checksum = ?1
                     AND cnm.provider_type = ?2
                     AND cnm.notification_target = ?3
                     LIMIT 1",
                    params![&checksum, &provider, &target],
                    |row| row.get(0),
                )
                .optional()?
            };

            Ok(existing_contact_name)
        })
        .await?
    }

    /// Check for duplicate notification targets across multiple targets for batch validation
    pub async fn check_duplicate_notification_targets(
        &self,
        wallet_checksum: &str,
        notification_methods: &[(String, String)], // (provider_type, notification_target)
        exclude_contact_id: Option<&str>,
    ) -> Result<Vec<String>> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();
        let methods = notification_methods.to_vec();
        let exclude_id = exclude_contact_id.map(|s| s.to_string());

        spawn_blocking(move || -> Result<Vec<String>> {
            let conn = pool.get()?;
            let mut duplicates = Vec::new();

            for (provider_type, notification_target) in &methods {
                // Skip ntfy as it's auto-generated and unique
                if provider_type == "ntfy" {
                    continue;
                }

                // For emails, do case-insensitive comparison
                let existing_contact_name: Option<String> = if provider_type == "email" {
                    if let Some(contact_id) = &exclude_id {
                        conn.query_row(
                            "SELECT c.name FROM contacts c
                             JOIN contact_notification_methods cnm ON c.id = cnm.contact_id
                             WHERE cnm.wallet_checksum = ?1
                             AND cnm.provider_type = ?2
                             AND LOWER(cnm.notification_target) = LOWER(?3)
                             AND c.id != ?4
                             LIMIT 1",
                            params![&checksum, provider_type, notification_target, contact_id],
                            |row| row.get::<_, String>(0),
                        )
                        .optional()?
                    } else {
                        conn.query_row(
                            "SELECT c.name FROM contacts c
                             JOIN contact_notification_methods cnm ON c.id = cnm.contact_id
                             WHERE cnm.wallet_checksum = ?1
                             AND cnm.provider_type = ?2
                             AND LOWER(cnm.notification_target) = LOWER(?3)
                             LIMIT 1",
                            params![&checksum, provider_type, notification_target],
                            |row| row.get::<_, String>(0),
                        )
                        .optional()?
                    }
                } else if let Some(contact_id) = &exclude_id {
                    // For SMS and other types, exact match
                    conn.query_row(
                        "SELECT c.name FROM contacts c
                         JOIN contact_notification_methods cnm ON c.id = cnm.contact_id
                         WHERE cnm.wallet_checksum = ?1
                         AND cnm.provider_type = ?2
                         AND cnm.notification_target = ?3
                         AND c.id != ?4
                         LIMIT 1",
                        params![&checksum, provider_type, notification_target, contact_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?
                } else {
                    conn.query_row(
                        "SELECT c.name FROM contacts c
                         JOIN contact_notification_methods cnm ON c.id = cnm.contact_id
                         WHERE cnm.wallet_checksum = ?1
                         AND cnm.provider_type = ?2
                         AND cnm.notification_target = ?3
                         LIMIT 1",
                        params![&checksum, provider_type, notification_target],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?
                };

                if let Some(existing_contact_name) = existing_contact_name {
                    let provider_label = if provider_type == "email" {
                        "Email"
                    } else {
                        "Phone number"
                    };
                    duplicates.push(format!(
                        "{} '{}' is already used by contact '{}'",
                        provider_label, notification_target, existing_contact_name
                    ));
                }
            }

            Ok(duplicates)
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

    /// Mark a wallet as deleted (soft delete) - used for non-blocking deletion
    pub async fn mark_wallet_as_deleted(&self, checksum: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let checksum = checksum.to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let changes = conn.execute(
                "UPDATE wallets SET status = 'deleted' WHERE checksum = ?1",
                params![checksum],
            )?;
            Ok(changes > 0)
        })
        .await?
    }

    /// Hard delete wallet from database (used after soft delete cleanup)
    pub async fn hard_delete_wallet_by_checksum(&self, checksum: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let checksum = checksum.to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;

            // First delete notification_logs that reference this wallet's transactions
            // This prevents foreign key constraint failures when transactions are cascade deleted
            conn.execute(
                "DELETE FROM notification_logs WHERE transaction_wallet_checksum = ?1",
                params![checksum],
            )?;

            // Now delete the wallet - this will cascade delete transactions, contacts, etc.
            let changes =
                conn.execute("DELETE FROM wallets WHERE checksum = ?1", params![checksum])?;
            Ok(changes > 0)
        })
        .await?
    }

    // New transaction-based methods
    pub async fn insert_transaction(&self, transaction: &TransactionInsert) -> Result<String> {
        let pool = self.pool.clone();
        let transaction = transaction.clone();

        spawn_blocking(move || -> Result<String> {
            let conn = pool.get()?;
            conn.execute(
                "INSERT OR IGNORE INTO transactions (txid, wallet_checksum, transaction_type, amount_sats, fee_sats, block_height, first_seen_at, confirmed_at, parent_txid, transaction_status, replaced_by_txid, replaced_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    &transaction.txid,
                    &transaction.wallet_checksum,
                    transaction.transaction_type.as_str(),
                    transaction.amount_sats,
                    transaction.fee_sats,
                    transaction.block_height,
                    transaction.first_seen_at,
                    transaction.confirmed_at,
                    transaction.parent_txid.as_ref(),
                    &transaction.transaction_status,
                    transaction.replaced_by_txid.as_ref(),
                    transaction.replaced_at,
                ],
            )?;
            Ok(transaction.txid.clone())
        }).await?
    }

    pub async fn get_transaction_by_txid(
        &self,
        wallet_checksum: &str,
        txid: &str,
    ) -> Result<Option<Transaction>> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();
        let txid = txid.to_string();

        spawn_blocking(move || -> Result<Option<Transaction>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT txid, wallet_checksum, transaction_type, amount_sats, fee_sats, block_height, first_seen_at, confirmed_at, parent_txid, transaction_status, replaced_by_txid, replaced_at
                 FROM transactions
                 WHERE wallet_checksum = ?1 AND txid = ?2"
            )?;

            let mut rows = stmt.query_map([&checksum, &txid], |row| {
                Ok(Transaction {
                    txid: row.get(0)?,
                    wallet_checksum: row.get(1)?,
                    transaction_type: EventType::from(row.get::<_, String>(2)?.as_str()),
                    amount_sats: row.get(3)?,
                    fee_sats: row.get(4)?,
                    block_height: row.get(5)?,
                    first_seen_at: row.get(6)?,
                    confirmed_at: row.get(7)?,
                    parent_txid: row.get(8)?,
                    transaction_status: row.get(9)?,
                    replaced_by_txid: row.get(10)?,
                    replaced_at: row.get(11)?,
                    notification_status: vec![], // Will be populated by calling code if needed
                })
            })?;

            match rows.next() {
                Some(row) => Ok(Some(row?)),
                None => Ok(None),
            }
        }).await?
    }

    pub async fn update_transaction_confirmation(
        &self,
        wallet_checksum: &str,
        txid: &str,
        block_height: u32,
        confirmed_at: u64,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();
        let txid = txid.to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let changes = conn.execute(
                "UPDATE transactions SET block_height = ?1, confirmed_at = ?2, transaction_status = 'confirmed' WHERE wallet_checksum = ?3 AND txid = ?4",
                params![block_height, confirmed_at, &checksum, &txid],
            )?;
            Ok(changes > 0)
        }).await?
    }

    pub async fn get_transactions_by_wallet_checksum(
        &self,
        wallet_checksum: &str,
        limit: Option<usize>,
    ) -> Result<Vec<TransactionWithWallet>> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();
        let limit = limit.unwrap_or(10000); // Large default for sync operations

        spawn_blocking(move || -> Result<Vec<TransactionWithWallet>> {
            let conn = pool.get()?;

            // First get transactions
            let mut stmt = conn.prepare(
                "SELECT t.txid, t.wallet_checksum, w.name, t.transaction_type, t.amount_sats, t.fee_sats, t.block_height, t.first_seen_at, t.confirmed_at, t.parent_txid, t.transaction_status, t.replaced_by_txid, t.replaced_at
                 FROM transactions t
                 JOIN wallets w ON t.wallet_checksum = w.checksum
                 WHERE t.wallet_checksum = ?1
                 ORDER BY t.first_seen_at DESC, t.txid DESC
                 LIMIT ?2"
            )?;

            let transaction_iter = stmt.query_map([&checksum, &limit.to_string()], |row| {
                Ok(TransactionWithWallet {
                    txid: row.get(0)?,
                    wallet_checksum: row.get(1)?,
                    wallet_name: row.get(2)?,
                    transaction_type: EventType::from(row.get::<_, String>(3)?.as_str()),
                    amount_sats: row.get(4)?,
                    fee_sats: row.get(5)?,
                    block_height: row.get(6)?,
                    first_seen_at: row.get(7)?,
                    confirmed_at: row.get(8)?,
                    parent_txid: row.get(9)?,
                    transaction_status: row.get(10)?,
                    replaced_by_txid: row.get(11)?,
                    replaced_at: row.get(12)?,
                    notification_status: vec![], // Will be populated below
                })
            })?;

            let mut transactions = Vec::new();
            for transaction in transaction_iter {
                let mut tx = transaction?;

                // Get notification status for this transaction
                let mut notification_stmt = conn.prepare(
                    "SELECT nl.contact_name_snapshot, nl.provider_name, nl.status, nl.error_message,
                            nl.notification_target_snapshot, nl.provider_type_snapshot, nl.created_at, nl.message_content, nl.notification_type
                     FROM notification_logs nl
                     WHERE nl.transaction_txid = ?1 AND nl.transaction_wallet_checksum = ?2
                     ORDER BY nl.created_at ASC"
                )?;

                let notification_iter = notification_stmt.query_map([&tx.txid, &tx.wallet_checksum], |row| {
                    let notification_type: String = row.get(8)?;

                    Ok(NotificationStatus {
                        contact_name: row.get::<_, Option<String>>(0)?.unwrap_or("Unknown".to_string()),
                        provider_name: row.get(1)?,
                        status: row.get(2)?,
                        error_message: row.get(3)?,
                        notification_target: row.get(4)?,
                        provider_type: row.get(5)?,
                        created_at: row.get(6)?,
                        notification_type,
                    })
                })?;

                for notification in notification_iter {
                    tx.notification_status.push(notification?);
                }

                transactions.push(tx);
            }

            Ok(transactions)
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
                    "INSERT INTO contact_notification_methods (id, contact_id, provider_type, notification_target, wallet_checksum) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![&method_id, &contact_id, provider_type.as_str(), &notification_target, &checksum],
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
        self.get_contacts_with_notification_methods_filtered(wallet_checksum, false)
            .await
    }

    /// Get contacts for subscription limits ordered by creation time (oldest first)
    pub async fn get_contacts_oldest_first_for_limits(
        &self,
        wallet_checksum: &str,
    ) -> Result<Vec<Contact>> {
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
                        display_target: None,
                        created_at: row.get(3)?,
                    })
                })?;

                for method_result in methods_iter {
                    contact.notification_methods.push(method_result?);
                }
            }

            Ok(contacts.into_values().collect())
        })
        .await?
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
                 WHERE wallet_checksum = ?1 ORDER BY name, created_at"
            } else {
                "SELECT id, wallet_checksum, name, language, created_at, 1 as is_active
                 FROM contacts
                 WHERE wallet_checksum = ?1 AND is_active = 1 ORDER BY name, created_at"
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

    /// Get a single contact with its notification methods by ID and wallet checksum
    pub async fn get_single_contact_with_methods(
        &self,
        contact_id: &str,
        wallet_checksum: &str,
    ) -> Result<Option<Contact>> {
        let pool = self.pool.clone();
        let contact_id = contact_id.to_string();
        let checksum = wallet_checksum.to_string();

        spawn_blocking(move || -> Result<Option<Contact>> {
            let conn = pool.get()?;

            // Get the contact
            let query = "SELECT id, wallet_checksum, name, language, created_at, is_active
                         FROM contacts
                         WHERE id = ?1 AND wallet_checksum = ?2";
            let mut stmt = conn.prepare(query)?;
            let contact_result = stmt.query_row(params![contact_id, checksum], |row| {
                let language_str: String = row.get(3)?;
                Ok(Contact {
                    id: Some(row.get(0)?),
                    wallet_checksum: row.get(1)?,
                    name: row.get(2)?,
                    language: Language::from(language_str.as_str()),
                    notification_methods: Vec::new(), // Will be populated below
                    created_at: row.get(4)?,
                    is_active: row.get::<_, i64>(5).unwrap_or(1) != 0, // SQLite stores bool as int
                })
            });

            let mut contact = match contact_result {
                Ok(contact) => contact,
                Err(bdk_wallet::rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
                Err(e) => return Err(e.into()),
            };

            // Get notification methods for this contact
            let methods_query = "SELECT id, provider_type, notification_target, created_at
                               FROM contact_notification_methods
                               WHERE contact_id = ?1";
            let mut methods_stmt = conn.prepare(methods_query)?;
            let methods_iter = methods_stmt.query_map(params![contact_id], |row| {
                let provider_type_str: String = row.get(1)?;
                let provider_type = ProviderType::from(provider_type_str.as_str());
                let notification_target: String = row.get(2)?;

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
                    contact_id: contact_id.clone(),
                    provider_type,
                    notification_target,
                    display_target,
                    created_at: row.get(3)?,
                })
            })?;

            for method_result in methods_iter {
                contact.notification_methods.push(method_result?);
            }

            Ok(Some(contact))
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
            let current_time = chrono::Utc::now()
                .format("%Y-%m-%d %H:%M:%S%.3f")
                .to_string();
            conn.execute(
                "UPDATE wallets SET last_synced_at = ?1 WHERE checksum = ?2",
                params![current_time, checksum],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn update_wallet_status(&self, checksum: &str, status: &str) -> Result<()> {
        let pool = self.pool.clone();
        let checksum = checksum.to_string();
        let status = status.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE wallets SET status = ?1 WHERE checksum = ?2",
                params![status, checksum],
            )?;
            Ok(())
        })
        .await?
    }

    /// Get all wallets for a specific tier that are due for sync
    pub async fn get_wallets_for_tier_sync(
        &self,
        tier: &crate::subscription::SubscriptionTier,
        network: &crate::config::NetworkConfig,
    ) -> Result<Vec<WalletMetadata>> {
        let pool = self.pool.clone();
        let network = network.clone();
        let tier_str = match tier {
            crate::subscription::SubscriptionTier::Personal => "personal",
            crate::subscription::SubscriptionTier::Team => "team",
        }
        .to_string();

        spawn_blocking(move || -> Result<Vec<WalletMetadata>> {
            let conn = pool.get()?;
            let tier_limits = crate::subscription::SubscriptionTier::from(tier_str.clone()).limits(&network);

            let mut stmt = conn.prepare(
                "SELECT w.checksum, w.name, w.descriptor, w.hex_color, w.balance_total,
                        w.last_activity, w.last_synced_at, w.status, w.user_id, w.created_at
                 FROM wallets w
                 JOIN users u ON w.user_id = u.id
                 WHERE w.is_active = 1 AND w.status = 'ready'
                   AND u.subscription_tier = ?1
                   AND (
                    -- Admin users bypass all subscription checks
                    u.is_admin = 1
                    OR
                    -- Regular users need valid subscription
                    (
                      -- Active subscriptions
                      u.subscription_status = 'active'
                      OR
                      -- Trial users within trial period
                      (u.subscription_status = 'trialing' AND datetime(u.trial_ends_at) > datetime('now'))
                      OR
                      -- Cancelled users still within their paid period
                      (u.subscription_status = 'canceled' AND u.subscription_ends_at IS NOT NULL AND datetime(u.subscription_ends_at) > datetime('now'))
                    )
                 )
                 AND (
                    -- Admin users bypass timing restrictions
                    u.is_admin = 1
                    OR
                    -- Regular users follow timing rules
                    (
                      -- Never synced before
                      w.last_synced_at IS NULL
                      OR
                      -- Or due for sync based on tier interval
                      datetime(w.last_synced_at) <= datetime('now', '-' || ?2 || ' seconds')
                    )
                 )
                 ORDER BY w.checksum"
            )?;

            let wallet_rows = stmt.query_map(
                params![tier_str, tier_limits.sync_interval_secs],
                |row| {
                    Ok(WalletMetadata {
                        checksum: row.get(0)?,
                        name: row.get(1)?,
                        descriptor: row.get(2)?,
                        hex_color: row.get(3)?,
                        balance_total: row.get(4)?,
                        last_activity: row.get(5)?,
                        status: row.get(7)?,
                        contact_count: None, // Not counting contacts in this query
                        user_id: row.get(8)?,
                        created_at: row.get(9)?,
                        is_active: true, // Query already filters for active wallets
                        balance_fiat: None,
                        fiat_currency: None,
                    })
                }
            )?;

            let mut due_wallets = Vec::new();
            for row in wallet_rows {
                due_wallets.push(row?);
            }

            Ok(due_wallets)
        })
        .await?
    }

    pub async fn get_ready_wallets(&self) -> Result<Vec<WalletMetadata>> {
        let pool = self.pool.clone();

        spawn_blocking(move || -> Result<Vec<WalletMetadata>> {
            let conn = pool.get()?;

            let query = "SELECT w.checksum, w.name, w.descriptor, w.hex_color,
                                w.created_at, w.balance_total,
                                (SELECT MAX(t.first_seen_at) FROM transactions t
                                 WHERE t.wallet_checksum = w.checksum) as last_activity,
                                w.status, COUNT(c.id) as contact_count,
                                w.user_id, w.is_active
                         FROM wallets w
                         LEFT JOIN contacts c ON w.checksum = c.wallet_checksum
                         WHERE w.is_active = 1 AND w.status = 'ready'
                         GROUP BY w.checksum, w.name, w.descriptor, w.hex_color,
                                  w.created_at, w.balance_total, w.status,
                                  w.user_id, w.is_active
                         ORDER BY w.created_at DESC";

            let mut stmt = conn.prepare(query)?;

            let wallet_iter = stmt.query_map([], |row| {
                Ok(WalletMetadata {
                    checksum: row.get(0)?,
                    name: row.get(1)?,
                    descriptor: row.get(2)?,
                    hex_color: row.get(3)?,
                    created_at: row.get(4)?,
                    balance_total: Some(row.get(5).unwrap_or(0)),
                    last_activity: row
                        .get::<_, Option<i64>>(6)
                        .ok()
                        .flatten()
                        .map(|t| t.to_string()),
                    status: row.get(7)?,
                    contact_count: row.get(8).unwrap_or(Some(0)),
                    user_id: row.get(9)?,
                    is_active: row.get(10)?,
                    balance_fiat: None,
                    fiat_currency: None,
                })
            })?;

            wallet_iter
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| anyhow!("Failed to query ready wallets: {}", e))
        })
        .await?
    }

    /// Get all wallets marked as deleted
    pub async fn get_deleted_wallets(&self) -> Result<Vec<WalletMetadata>> {
        let pool = self.pool.clone();

        spawn_blocking(move || -> Result<Vec<WalletMetadata>> {
            let conn = pool.get()?;

            let query = "SELECT w.checksum, w.name, w.descriptor, w.hex_color,
                                w.created_at, w.balance_total,
                                (SELECT MAX(t.first_seen_at) FROM transactions t
                                 WHERE t.wallet_checksum = w.checksum) as last_activity,
                                w.status, COUNT(c.id) as contact_count,
                                w.user_id, w.is_active
                         FROM wallets w
                         LEFT JOIN contacts c ON w.checksum = c.wallet_checksum
                         WHERE w.status = 'deleted'
                         GROUP BY w.checksum, w.name, w.descriptor, w.hex_color,
                                  w.created_at, w.balance_total, w.status,
                                  w.user_id, w.is_active
                         ORDER BY w.created_at DESC";

            let mut stmt = conn.prepare(query)?;

            let wallet_iter = stmt.query_map([], |row| {
                Ok(WalletMetadata {
                    checksum: row.get(0)?,
                    name: row.get(1)?,
                    descriptor: row.get(2)?,
                    hex_color: row.get(3)?,
                    created_at: row.get(4)?,
                    balance_total: Some(row.get(5).unwrap_or(0)),
                    last_activity: row
                        .get::<_, Option<i64>>(6)
                        .ok()
                        .flatten()
                        .map(|t| t.to_string()),
                    status: row.get(7)?,
                    contact_count: row.get(8).unwrap_or(Some(0)),
                    user_id: row.get(9)?,
                    is_active: row.get(10)?,
                    balance_fiat: None,
                    fiat_currency: None,
                })
            })?;

            wallet_iter
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| anyhow!("Failed to query deleted wallets: {}", e))
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

    /// Insert notification log for transaction-based notifications (new schema)
    pub async fn insert_notification_log_for_transaction(
        &self,
        transaction_txid: &str,
        transaction_wallet_checksum: &str,
        notification_method_id: &str,
        provider_name: &str,
        provider_message_id: Option<&str>,
        status: &str,
        error_message: Option<&str>,
        message_content: &str,
        notification_type: &str,
    ) -> Result<String> {
        let pool = self.pool.clone();
        let transaction_txid = transaction_txid.to_string();
        let transaction_wallet_checksum = transaction_wallet_checksum.to_string();
        let notification_method_id = notification_method_id.to_string();
        let provider_name = provider_name.to_string();
        let provider_message_id = provider_message_id.map(|s| s.to_string());
        let status = status.to_string();
        let error_message = error_message.map(|s| s.to_string());
        let message_content = message_content.to_string();
        let notification_type = notification_type.to_string();

        spawn_blocking(move || -> Result<String> {
            let conn = pool.get()?;
            let log_id = uuid::Uuid::new_v4().to_string();

            // Get the contact info at the time of notification to preserve it
            let (contact_name, notification_target, provider_type): (String, String, String) = conn.query_row(
                "SELECT c.name, cnm.notification_target, cnm.provider_type
                 FROM contact_notification_methods cnm
                 JOIN contacts c ON cnm.contact_id = c.id
                 WHERE cnm.id = ?1",
                params![&notification_method_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            ).unwrap_or_else(|_| ("Unknown Contact".to_string(), "Unknown Target".to_string(), "unknown".to_string()));

            conn.execute(
                "INSERT INTO notification_logs (id, transaction_txid, transaction_wallet_checksum, notification_method_id, provider_name, provider_message_id, status, error_message, message_content, notification_type, contact_name_snapshot, notification_target_snapshot, provider_type_snapshot)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    &log_id,
                    &transaction_txid,
                    &transaction_wallet_checksum,
                    &notification_method_id,
                    &provider_name,
                    &provider_message_id,
                    &status,
                    &error_message,
                    &message_content,
                    &notification_type,
                    &contact_name,
                    &notification_target,
                    &provider_type,
                ],
            )?;
            Ok(log_id)
        }).await?
    }

    /// Insert notification log for balance alert notifications (separate from transactions)
    pub async fn insert_notification_log_for_balance_alert(
        &self,
        balance_alert_id: &str,
        wallet_checksum: &str,
        notification_method_id: &str,
        provider_name: &str,
        provider_message_id: Option<&str>,
        status: &str,
        error_message: Option<&str>,
        message_content: &str,
    ) -> Result<String> {
        let pool = self.pool.clone();
        let balance_alert_id = balance_alert_id.to_string();
        let wallet_checksum = wallet_checksum.to_string();
        let notification_method_id = notification_method_id.to_string();
        let provider_name = provider_name.to_string();
        let provider_message_id = provider_message_id.map(|s| s.to_string());
        let status = status.to_string();
        let error_message = error_message.map(|s| s.to_string());
        let message_content = message_content.to_string();

        spawn_blocking(move || -> Result<String> {
            let conn = pool.get()?;
            let log_id = uuid::Uuid::new_v4().to_string();

            // Get contact info for snapshot
            let (contact_name, notification_target, provider_type) = conn
                .prepare(
                    "SELECT c.name, cnm.notification_target, cnm.provider_type
                     FROM contact_notification_methods cnm
                     JOIN contacts c ON cnm.contact_id = c.id
                     WHERE cnm.id = ?1"
                )?
                .query_row(params![&notification_method_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                }).unwrap_or_else(|_| ("Unknown Contact".to_string(), "Unknown Target".to_string(), "unknown".to_string()));

            conn.execute(
                "INSERT INTO balance_alert_notification_logs (id, balance_alert_id, wallet_checksum, notification_method_id, provider_name, provider_message_id, status, error_message, message_content, contact_name_snapshot, notification_target_snapshot, provider_type_snapshot)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    &log_id,
                    &balance_alert_id,
                    &wallet_checksum,
                    &notification_method_id,
                    &provider_name,
                    &provider_message_id,
                    &status,
                    &error_message,
                    &message_content,
                    &contact_name,
                    &notification_target,
                    &provider_type,
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
        preferred_currency: Option<&str>,
    ) -> Result<String> {
        let pool = self.pool.clone();
        let email = email.to_string();
        let password_hash = password_hash.to_string();
        let name = name.map(|n| n.to_string());
        let preferred_currency = preferred_currency.map(|c| c.to_string());

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

            // Determine if this user should be admin (first user becomes admin)
            let admin_count: i64 = tx.query_row(
                "SELECT COUNT(*) FROM users WHERE is_admin = 1",
                [],
                |row| row.get(0)
            )?;

            let final_is_admin = admin_count == 0;

            if final_is_admin {
                println!("Creating first admin user: {}", email);
            } else {
                println!("Creating regular user: {} (existing admins: {})", email, admin_count);
            }

            let user_name = name;

            // Generate UUID for new user
            let user_id = Uuid::new_v4().to_string();

            println!("DEBUG: Creating user {} with name {:?}, is_admin={}", email, user_name, final_is_admin);

            // Create new user
            tx.execute(
                "INSERT INTO users (id, email, password_hash, name, is_admin, email_verified, subscription_tier, subscription_status, preferred_fiat_currency) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![&user_id, &email, &password_hash, user_name, final_is_admin, email_verified, "team", "pending", preferred_currency.as_deref().unwrap_or("USD")],
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
                .prepare("SELECT id, email, password_hash, name, is_admin, email_verified, subscription_tier, trial_ends_at, subscription_status, stripe_customer_id, stripe_subscription_id, subscription_started_at, subscription_ends_at, created_at, preferred_fiat_currency FROM users WHERE email = ?1")?
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
                        preferred_fiat_currency: row.get(14)?,
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
                .prepare("SELECT id, email, password_hash, name, is_admin, email_verified, subscription_tier, trial_ends_at, subscription_status, stripe_customer_id, stripe_subscription_id, subscription_started_at, subscription_ends_at, created_at, preferred_fiat_currency FROM users WHERE id = ?1")?
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
                        preferred_fiat_currency: row.get(14)?,
                    })
                })
                .ok();
            Ok(result)
        }).await?
    }

    pub async fn get_user_by_stripe_customer_id(
        &self,
        stripe_customer_id: &str,
    ) -> Result<Option<UserRecord>> {
        let pool = self.pool.clone();
        let stripe_customer_id = stripe_customer_id.to_string();

        spawn_blocking(move || -> Result<Option<UserRecord>> {
            let conn = pool.get()?;
            let result = conn
                .prepare("SELECT id, email, password_hash, name, is_admin, email_verified, subscription_tier, trial_ends_at, subscription_status, stripe_customer_id, stripe_subscription_id, subscription_started_at, subscription_ends_at, created_at, preferred_fiat_currency FROM users WHERE stripe_customer_id = ?1")?
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
                        preferred_fiat_currency: row.get(14)?,
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
                        created_at, preferred_fiat_currency
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
                    preferred_fiat_currency: row.get(14)?,
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

    pub async fn update_user_trial_status(
        &self,
        user_id: &str,
        subscription_status: &str,
        trial_ends_at: Option<String>,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let subscription_status = subscription_status.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE users SET
                    subscription_status = ?1,
                    trial_ends_at = ?2
                WHERE id = ?3",
                params![subscription_status, trial_ends_at, user_id],
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
                params![subscription_status, stripe_subscription_id, user_id,],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn update_user_subscription(
        &self,
        user_id: &str,
        subscription_tier: &str,
        subscription_status: &str,
        stripe_subscription_id: Option<&str>,
        subscription_started_at: Option<&str>,
        subscription_ends_at: Option<&str>,
        trial_ends_at: Option<&str>,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let subscription_tier = subscription_tier.to_string();
        let subscription_status = subscription_status.to_string();
        let stripe_subscription_id = stripe_subscription_id.map(|s| s.to_string());
        let subscription_started_at = subscription_started_at.map(|s| s.to_string());
        let subscription_ends_at = subscription_ends_at.map(|s| s.to_string());
        let trial_ends_at = trial_ends_at.map(|s| s.to_string());

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE users SET
                    subscription_tier = ?1,
                    subscription_status = ?2,
                    stripe_subscription_id = ?3,
                    subscription_started_at = ?4,
                    subscription_ends_at = ?5,
                    trial_ends_at = COALESCE(?6, trial_ends_at)
                WHERE id = ?7",
                params![
                    subscription_tier,
                    subscription_status,
                    stripe_subscription_id,
                    subscription_started_at,
                    subscription_ends_at,
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

    /// Mark a pending verification as completed (used for contact updates)
    /// This keeps the verification record for the PUT endpoint to find
    pub async fn mark_verification_completed(&self, verification_id: i64) -> Result<()> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE pending_contact_verifications
                 SET verified_at = CURRENT_TIMESTAMP
                 WHERE id = ?1",
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
            let deleted_expired = conn.execute(
                "DELETE FROM pending_contact_verifications WHERE expires_at <= datetime('now')",
                [],
            )?;

            // Also clean up old completed verifications (older than 24 hours)
            let deleted_completed = conn.execute(
                "DELETE FROM pending_contact_verifications
                 WHERE verified_at IS NOT NULL
                 AND verified_at <= datetime('now', '-24 hours')",
                [],
            )?;

            Ok((deleted_expired + deleted_completed) as u64)
        })
        .await?
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
        })
        .await??;

        Ok(())
    }

    /// Update contact active status for subscription tier limits
    pub async fn update_contact_active_status(
        &self,
        contact_id: &str,
        is_active: bool,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let contact_id = contact_id.to_string();

        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;

            conn.execute(
                "UPDATE contacts SET is_active = ? WHERE id = ?",
                params![is_active, contact_id],
            )?;

            Ok::<(), anyhow::Error>(())
        })
        .await??;

        Ok(())
    }

    /// Check if a phone number or email was recently verified for this wallet
    /// Used to ensure security when creating contacts with SMS/email methods
    pub async fn was_recently_verified(
        &self,
        wallet_checksum: &str,
        notification_target: &str,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();
        let target = notification_target.to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM pending_contact_verifications
                 WHERE wallet_checksum = ?1
                 AND notification_target = ?2
                 AND verified_at IS NOT NULL
                 AND verified_at > datetime('now', '-30 minutes')",
                params![checksum, target],
                |row| row.get(0),
            )?;
            Ok(count > 0)
        })
        .await?
    }

    /// Update contact with new methods using a transaction
    /// This ensures atomic updates - if any part fails, the old contact remains unchanged
    pub async fn update_contact_with_methods(
        &self,
        contact_id: &str,
        wallet_checksum: &str,
        name: &str,
        language: &Language,
        new_methods: Vec<(ProviderType, String)>,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let contact_id = contact_id.to_string();
        let checksum = wallet_checksum.to_string();
        let contact_name = name.to_string();
        let lang = language.clone();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;

            // Start transaction
            conn.execute("BEGIN TRANSACTION", [])?;

            match (|| -> Result<()> {
                // Update contact basics
                conn.execute(
                    "UPDATE contacts SET name = ?1, language = ?2 WHERE id = ?3 AND wallet_checksum = ?4",
                    params![contact_name, lang.as_str(), contact_id, checksum],
                )?;

                // Check if contact was updated (exists and belongs to wallet)
                let affected: i64 = conn.query_row(
                    "SELECT changes()",
                    [],
                    |row| row.get(0),
                )?;

                if affected == 0 {
                    return Err(anyhow::anyhow!("Contact not found or access denied"));
                }

                // Delete all old notification methods
                conn.execute(
                    "DELETE FROM contact_notification_methods WHERE contact_id = ?1",
                    params![contact_id],
                )?;

                // Insert new methods
                for (provider_type, target) in new_methods {
                    let method_id = uuid::Uuid::new_v4().to_string();
                    conn.execute(
                        "INSERT INTO contact_notification_methods
                         (id, contact_id, provider_type, notification_target, wallet_checksum)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![method_id, contact_id, provider_type.as_str(), target, checksum],
                    )?;
                }

                Ok(())
            })() {
                Ok(()) => {
                    conn.execute("COMMIT", [])?;
                    Ok(())
                }
                Err(e) => {
                    conn.execute("ROLLBACK", [])?;
                    Err(e)
                }
            }
        })
        .await?
    }

    /// Mark a transaction as replaced by another transaction (RBF)
    pub async fn mark_transaction_replaced(
        &self,
        wallet_checksum: &str,
        original_txid: &str,
        replaced_by_txid: &str,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();
        let original_txid = original_txid.to_string();
        let replaced_by_txid = replaced_by_txid.to_string();
        let replaced_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let affected = conn.execute(
                "UPDATE transactions
                 SET transaction_status = 'replaced', replaced_by_txid = ?1, replaced_at = ?2
                 WHERE wallet_checksum = ?3 AND txid = ?4 AND transaction_status = 'pending'",
                params![&replaced_by_txid, replaced_at, &checksum, &original_txid],
            )?;
            Ok(affected > 0)
        })
        .await?
    }

    // Exchange rate methods
    pub async fn get_exchange_rates(
        &self,
    ) -> Result<std::collections::HashMap<String, exchange_rates::ExchangeRate>> {
        let pool = self.pool.clone();
        spawn_blocking(
            move || -> Result<std::collections::HashMap<String, exchange_rates::ExchangeRate>> {
                let conn = pool.get()?;
                let mut stmt = conn
                    .prepare("SELECT currency, rate_per_btc, last_updated FROM exchange_rates")?;
                let rate_iter = stmt.query_map(params![], |row| {
                    Ok(exchange_rates::ExchangeRate {
                        currency: row.get(0)?,
                        rate_per_btc: row.get(1)?,
                        last_updated: chrono::DateTime::parse_from_rfc3339(
                            &row.get::<_, String>(2)?,
                        )
                        .map_err(|e| {
                            bdk_wallet::rusqlite::Error::FromSqlConversionFailure(
                                2,
                                bdk_wallet::rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?
                        .with_timezone(&chrono::Utc),
                    })
                })?;

                let mut rates = std::collections::HashMap::new();
                for rate in rate_iter {
                    let rate = rate?;
                    rates.insert(rate.currency.clone(), rate);
                }
                Ok(rates)
            },
        )
        .await?
    }

    pub async fn store_exchange_rates(
        &self,
        rates: &std::collections::HashMap<String, exchange_rates::ExchangeRate>,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let rates = rates.clone();
        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            let tx = conn.unchecked_transaction()?;

            // Clear old rates
            tx.execute("DELETE FROM exchange_rates", params![])?;

            // Insert new rates
            for (currency, rate) in rates {
                tx.execute(
                    "INSERT INTO exchange_rates (currency, rate_per_btc, last_updated) VALUES (?1, ?2, ?3)",
                    params![currency, rate.rate_per_btc, rate.last_updated.to_rfc3339()],
                )?;
            }

            tx.commit()?;
            Ok(())
        })
        .await?
    }

    pub async fn get_user_preferred_currency(&self, user_id: &str) -> Result<String> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        spawn_blocking(move || -> Result<String> {
            let conn = pool.get()?;
            let currency: Option<String> = conn
                .prepare("SELECT preferred_fiat_currency FROM users WHERE id = ?1")?
                .query_row(params![user_id], |row| row.get(0))
                .optional()?;
            Ok(currency.unwrap_or_else(|| "USD".to_string()))
        })
        .await?
    }

    pub async fn update_user_preferred_currency(
        &self,
        user_id: &str,
        currency: &str,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let currency = currency.to_string();
        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE users SET preferred_fiat_currency = ?1 WHERE id = ?2",
                params![currency, user_id],
            )?;
            Ok(())
        })
        .await?
    }

    // ============================
    // BALANCE ALERTS CRUD METHODS
    // ============================

    pub async fn create_balance_alert(
        &self,
        wallet_checksum: &str,
        threshold_sats: i64,
        alert_type: BalanceAlertType,
    ) -> Result<BalanceAlert> {
        let pool = self.pool.clone();
        let wallet_checksum = wallet_checksum.to_string();
        let alert_id = Uuid::new_v4().to_string();
        let alert_type_str = alert_type.as_str().to_string();

        spawn_blocking(move || -> Result<BalanceAlert> {
            let conn = pool.get()?;
            let current_time = chrono::Utc::now().to_rfc3339();

            conn.execute(
                "INSERT INTO balance_alerts (id, wallet_checksum, threshold_sats, alert_type, is_active, created_at)
                 VALUES (?1, ?2, ?3, ?4, 1, ?5)",
                params![alert_id, wallet_checksum, threshold_sats, alert_type_str, current_time],
            )?;

            Ok(BalanceAlert {
                id: alert_id,
                wallet_checksum,
                threshold_sats,
                alert_type,
                is_active: true,
                last_triggered_at: None,
                created_at: current_time,
            })
        })
        .await?
    }

    pub async fn get_active_balance_alerts_for_wallet(
        &self,
        wallet_checksum: &str,
    ) -> Result<Vec<BalanceAlert>> {
        let pool = self.pool.clone();
        let wallet_checksum = wallet_checksum.to_string();

        spawn_blocking(move || -> Result<Vec<BalanceAlert>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, wallet_checksum, threshold_sats, alert_type, is_active, last_triggered_at, created_at
                 FROM balance_alerts
                 WHERE wallet_checksum = ?1 AND is_active = 1"
            )?;

            let alert_iter = stmt.query_map(params![wallet_checksum], |row| {
                Ok(BalanceAlert {
                    id: row.get(0)?,
                    wallet_checksum: row.get(1)?,
                    threshold_sats: row.get(2)?,
                    alert_type: BalanceAlertType::from(row.get::<_, String>(3)?.as_str()),
                    is_active: row.get::<_, i64>(4)? != 0,
                    last_triggered_at: row.get::<_, Option<i64>>(5)?.map(|t| t as u64),
                    created_at: row.get(6)?,
                })
            })?;

            let mut alerts = Vec::new();
            for alert in alert_iter {
                alerts.push(alert?);
            }
            Ok(alerts)
        })
        .await?
    }

    pub async fn get_all_balance_alerts_for_wallet(
        &self,
        wallet_checksum: &str,
    ) -> Result<Vec<BalanceAlert>> {
        let pool = self.pool.clone();
        let wallet_checksum = wallet_checksum.to_string();

        spawn_blocking(move || -> Result<Vec<BalanceAlert>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, wallet_checksum, threshold_sats, alert_type, is_active, last_triggered_at, created_at
                 FROM balance_alerts
                 WHERE wallet_checksum = ?1
                 ORDER BY created_at DESC"
            )?;

            let alert_iter = stmt.query_map(params![wallet_checksum], |row| {
                Ok(BalanceAlert {
                    id: row.get(0)?,
                    wallet_checksum: row.get(1)?,
                    threshold_sats: row.get(2)?,
                    alert_type: BalanceAlertType::from(row.get::<_, String>(3)?.as_str()),
                    is_active: row.get::<_, i64>(4)? != 0,
                    last_triggered_at: row.get::<_, Option<i64>>(5)?.map(|t| t as u64),
                    created_at: row.get(6)?,
                })
            })?;

            let mut alerts = Vec::new();
            for alert in alert_iter {
                alerts.push(alert?);
            }
            Ok(alerts)
        })
        .await?
    }

    pub async fn disable_balance_alert_after_trigger(&self, alert_id: &str) -> Result<()> {
        let pool = self.pool.clone();
        let alert_id = alert_id.to_string();
        let triggered_at = chrono::Utc::now().timestamp() as u64;

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE balance_alerts
                 SET is_active = 0, last_triggered_at = ?1
                 WHERE id = ?2",
                params![triggered_at as i64, alert_id],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn reactivate_balance_alert(&self, alert_id: &str) -> Result<BalanceAlert> {
        let pool = self.pool.clone();
        let alert_id = alert_id.to_string();

        spawn_blocking(move || -> Result<BalanceAlert> {
            let conn = pool.get()?;

            // Update the alert
            conn.execute(
                "UPDATE balance_alerts SET is_active = 1 WHERE id = ?1",
                params![alert_id],
            )?;

            // Fetch and return the updated alert
            let mut stmt = conn.prepare(
                "SELECT id, wallet_checksum, threshold_sats, alert_type, is_active, last_triggered_at, created_at
                 FROM balance_alerts WHERE id = ?1"
            )?;

            let alert = stmt.query_row(params![alert_id], |row| {
                let alert_type_str: String = row.get(3)?;
                let alert_type = match alert_type_str.as_str() {
                    "above" => BalanceAlertType::Above,
                    "below" => BalanceAlertType::Below,
                    "equals" => BalanceAlertType::Equals,
                    _ => return Err(bdk_wallet::rusqlite::Error::InvalidColumnType(3, "alert_type".to_string(), bdk_wallet::rusqlite::types::Type::Text)),
                };

                Ok(BalanceAlert {
                    id: row.get(0)?,
                    wallet_checksum: row.get(1)?,
                    threshold_sats: row.get(2)?,
                    alert_type,
                    is_active: row.get(4)?,
                    last_triggered_at: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })?;

            Ok(alert)
        })
        .await?
    }

    pub async fn delete_balance_alert(&self, alert_id: &str) -> Result<()> {
        let pool = self.pool.clone();
        let alert_id = alert_id.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "DELETE FROM balance_alerts WHERE id = ?1",
                params![alert_id],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn check_duplicate_balance_alert(
        &self,
        wallet_checksum: &str,
        threshold_sats: i64,
        alert_type: BalanceAlertType,
    ) -> Result<Option<BalanceAlert>> {
        let pool = self.pool.clone();
        let wallet_checksum = wallet_checksum.to_string();
        let alert_type_str = alert_type.as_str().to_string();

        spawn_blocking(move || -> Result<Option<BalanceAlert>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, wallet_checksum, threshold_sats, alert_type, is_active, last_triggered_at, created_at
                 FROM balance_alerts
                 WHERE wallet_checksum = ?1 AND alert_type = ?2 AND threshold_sats = ?3
                 LIMIT 1"
            )?;

            let row = stmt.query_row(params![wallet_checksum, alert_type_str, threshold_sats], |row| {
                Ok(BalanceAlert {
                    id: row.get(0)?,
                    wallet_checksum: row.get(1)?,
                    threshold_sats: row.get(2)?,
                    alert_type: BalanceAlertType::from(row.get::<_, String>(3)?.as_str()),
                    is_active: row.get::<_, i64>(4)? != 0,
                    last_triggered_at: row.get::<_, Option<i64>>(5)?.map(|t| t as u64),
                    created_at: row.get(6)?,
                })
            });

            match row {
                Ok(alert) => Ok(Some(alert)),
                Err(bdk_wallet::rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
        .await?
    }

    pub async fn create_balance_alert_notification(
        &self,
        balance_alert_id: &str,
        wallet_checksum: &str,
        threshold_sats: i64,
        current_balance_sats: i64,
        alert_type: BalanceAlertType,
    ) -> Result<BalanceAlertNotification> {
        let pool = self.pool.clone();
        let notification_id = Uuid::new_v4().to_string();
        let balance_alert_id = balance_alert_id.to_string();
        let wallet_checksum = wallet_checksum.to_string();
        let alert_type_str = alert_type.as_str().to_string();
        let notification_sent_at = chrono::Utc::now().timestamp() as u64;

        spawn_blocking(move || -> Result<BalanceAlertNotification> {
            let conn = pool.get()?;
            let current_time = chrono::Utc::now().to_rfc3339();

            conn.execute(
                "INSERT INTO balance_alert_notifications
                 (id, balance_alert_id, wallet_checksum, threshold_sats, current_balance_sats, alert_type, notification_sent_at, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    notification_id,
                    balance_alert_id,
                    wallet_checksum,
                    threshold_sats,
                    current_balance_sats,
                    alert_type_str,
                    notification_sent_at as i64,
                    current_time
                ],
            )?;

            Ok(BalanceAlertNotification {
                id: notification_id,
                balance_alert_id,
                wallet_checksum,
                threshold_sats,
                current_balance_sats,
                alert_type,
                notification_sent_at,
                created_at: current_time,
            })
        })
        .await?
    }
}
