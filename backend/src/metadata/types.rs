use crate::subscription::SubscriptionTier;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::num::Wrapping;

/// Error type for type conversion failures
#[derive(Debug)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Default, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub enum EventType {
    #[serde(rename = "send")]
    #[default]
    Send,
    #[serde(rename = "receive")]
    Receive,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub enum Language {
    #[serde(rename = "en-US")]
    English,
    #[serde(rename = "nb")]
    Norwegian,
    #[serde(rename = "es-419")]
    Spanish,
    #[serde(rename = "pt-BR")]
    Portuguese,
    #[serde(rename = "de-DE")]
    German,
    #[serde(rename = "fr-FR")]
    French,
    #[serde(rename = "ja")]
    Japanese,
    #[serde(rename = "da")]
    Danish,
    #[serde(rename = "sv")]
    Swedish,
}

impl Language {
    pub fn as_str(&self) -> &'static str {
        match self {
            Language::English => "en-US",
            Language::Norwegian => "nb",
            Language::Spanish => "es-419",
            Language::Portuguese => "pt-BR",
            Language::German => "de-DE",
            Language::French => "fr-FR",
            Language::Japanese => "ja",
            Language::Danish => "da",
            Language::Swedish => "sv",
        }
    }

    /// Returns the Twilio Verify locale code for this language.
    ///
    /// Note: Twilio uses lowercase locale codes (e.g., `pt-br`) while our i18n
    /// system uses standard BCP 47 codes (e.g., `pt-BR`). This method returns
    /// the Twilio-specific format.
    ///
    /// See: https://www.twilio.com/docs/verify/supported-languages
    pub fn twilio_locale(self) -> &'static str {
        match self {
            Language::English => "en",
            Language::Norwegian => "nb",
            Language::Spanish => "es",
            Language::Portuguese => "pt-br",
            Language::German => "de",
            Language::French => "fr",
            Language::Japanese => "ja",
            Language::Danish => "da",
            Language::Swedish => "sv",
        }
    }
}

impl From<&str> for Language {
    fn from(s: &str) -> Self {
        match s {
            "en-US" => Language::English,
            "nb" => Language::Norwegian,
            "es-419" => Language::Spanish,
            "pt-BR" => Language::Portuguese,
            "de-DE" => Language::German,
            "fr-FR" => Language::French,
            "ja" => Language::Japanese,
            "da" => Language::Danish,
            "sv" => Language::Swedish,
            _ => Language::English, // Default fallback
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserRecord {
    pub id: String, // UUIDv4
    pub email: String,
    pub password_hash: String,
    pub name: Option<String>,
    pub is_admin: bool,
    pub is_demo: bool,
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
    pub preferred_language: Option<String>,
}

impl UserRecord {}

/// Convert browser locale to language code for emails and notifications
/// Returns the exact locale code that matches our YAML translation files
pub fn locale_to_language(locale: &str) -> &'static str {
    let lang_code = locale.split(['-', '_']).next().unwrap_or("").to_lowercase();
    match lang_code.as_str() {
        "nb" | "nn" | "no" => "nb",
        "es" => "es-419",
        "pt" => "pt-BR",
        "de" => "de-DE",
        "fr" => "fr-FR",
        "ja" => "ja",
        "da" => "da",
        "sv" => "sv",
        _ => "en-US",
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TwilioConfig {
    pub id: Option<i64>,
    pub account_sid: String,
    pub auth_token: String,
    pub sender_id: String,
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

impl TryFrom<&str> for EventType {
    type Error = ParseError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "send" => Ok(EventType::Send),
            "receive" => Ok(EventType::Receive),
            _ => Err(ParseError(format!("Invalid event type: {}", s))),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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
    pub wallet_type: String, // 'descriptor' or 'address'
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_synced_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Contact {
    pub id: Option<String>, // UUIDv4
    pub wallet_checksum: String,
    pub name: String,
    pub notification_methods: Vec<NotificationMethod>,
    pub created_at: String,
    pub is_active: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NotificationMethod {
    pub id: Option<String>, // UUIDv4
    pub contact_id: String, // UUIDv4
    pub provider_type: ProviderType,
    pub notification_target: String, // phone number or ntfy topic
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_target: Option<String>, // formatted version for display
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum ProviderType {
    #[serde(rename = "sms")]
    Sms,
    #[serde(rename = "ntfy")]
    Ntfy,
    #[serde(rename = "email")]
    Email,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub enum BalanceAlertType {
    #[serde(rename = "above")]
    Above,
    #[serde(rename = "below")]
    Below,
    #[serde(rename = "equals")]
    Equals,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BalanceAlert {
    pub id: String, // UUIDv4
    pub wallet_checksum: String,
    pub threshold_sats: i64,
    pub alert_type: BalanceAlertType,
    pub is_active: bool,
    pub last_triggered_at: Option<u64>, // Unix timestamp
    pub created_at: String,
    // Fiat threshold support (migration 013)
    pub threshold_currency: Option<String>, // e.g., "USD", "EUR", None for BTC
    pub threshold_fiat_amount: Option<f64>, // Fiat amount when currency is set
    // Crossing detection support (migration 013)
    pub last_checked_balance_sats: Option<i64>, // For threshold crossing detection
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BalanceAlertNotification {
    pub id: String, // UUIDv4
    pub balance_alert_id: String,
    pub wallet_checksum: String,
    pub threshold_sats: i64,
    pub current_balance_sats: i64,
    pub alert_type: BalanceAlertType,
    pub notification_sent_at: u64, // Unix timestamp
    pub created_at: String,
    // Fiat threshold snapshot (migration 013)
    pub threshold_currency: Option<String>,
    pub threshold_fiat_amount: Option<f64>,
    pub exchange_rate_snapshot: Option<f64>, // BTC/fiat rate at trigger time
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

impl TryFrom<&str> for BalanceAlertType {
    type Error = ParseError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "above" => Ok(BalanceAlertType::Above),
            "below" => Ok(BalanceAlertType::Below),
            "equals" => Ok(BalanceAlertType::Equals),
            _ => Err(ParseError(format!("Invalid balance alert type: {}", s))),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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

#[derive(Debug, Serialize, Deserialize, Clone)]
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

#[derive(Debug, Serialize, Deserialize, Clone)]
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WalletsListResponse {
    pub timestamp: u64,
    pub wallets: Vec<WalletMetadata>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WalletDetailResponse {
    pub timestamp: u64,
    pub wallet: WalletMetadata,
    pub transactions: Vec<TransactionWithWallet>,
    pub contacts: Vec<Contact>,
    pub balance_alerts: Vec<BalanceAlert>,
}

#[derive(Debug, Clone)]
pub struct NonSyncingWalletsSummary {
    pub expired_trials: usize,
    pub cancelled_subscriptions: usize,
    pub expired_subscriptions: usize,
    pub past_due_subscriptions: usize,
    pub inactive_wallets: usize,
    pub total_non_syncing: usize,
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

/// Extract checksum from a Bitcoin descriptor
pub fn extract_checksum(descriptor: &str) -> String {
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

/// Parameters for creating a notification log entry
#[derive(Debug, Clone)]
pub struct NotificationLogParams<'a> {
    pub notification_method_id: &'a str,
    pub provider_name: &'a str,
    pub provider_message_id: Option<&'a str>,
    pub status: &'a str,
    pub error_message: Option<&'a str>,
    pub message_content: &'a str,
}

/// Parameters for triggering a balance alert notification
#[derive(Debug, Clone)]
pub struct BalanceAlertTriggerParams {
    pub threshold_sats: i64,
    pub current_balance_sats: i64,
    pub alert_type: BalanceAlertType,
    pub threshold_currency: Option<String>,
    pub threshold_fiat_amount: Option<f64>,
    pub exchange_rate_snapshot: Option<f64>,
}

/// Parameters for updating a user's subscription
#[derive(Debug, Clone, Default)]
pub struct SubscriptionUpdateParams<'a> {
    pub subscription_tier: &'a str,
    pub subscription_status: &'a str,
    pub stripe_subscription_id: Option<&'a str>,
    pub subscription_started_at: Option<&'a str>,
    pub subscription_ends_at: Option<&'a str>,
    pub trial_ends_at: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct PendingBillingCheckout {
    pub token: String,
    pub user_id: String,
    pub provider: String,
    pub subscription_tier: String,
    pub billing_period: String,
    pub completed_at: Option<String>,
}
