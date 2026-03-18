//! Response DTOs for API endpoints

use crate::metadata::{BalanceAlert, WalletMetadata};
use crate::notifications::ProviderInfo;
use serde::{Deserialize, Serialize};

// ============================
// DATABASE HEALTH & INTEGRITY
// ============================

#[derive(Serialize)]
pub struct DatabaseHealthResponse {
    pub status: String,
    pub schema_version: String,
    pub pool: PoolHealth,
    pub checks: IntegrityChecks,
    pub timestamp: String,
}

#[derive(Serialize)]
pub struct PoolHealth {
    pub total_connections: u32,
    pub idle_connections: u32,
    pub max_connections: u32,
    pub status: String,
}

#[derive(Serialize)]
pub struct IntegrityChecks {
    pub sqlite_integrity: CheckResult,
    pub foreign_keys: CheckResult,
    pub orphaned_records: OrphanedRecordsReport,
    pub duplicates: DuplicatesReport,
}

#[derive(Serialize)]
pub struct CheckResult {
    pub status: String,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<String>,
}

#[derive(Serialize)]
pub struct OrphanedRecordsReport {
    pub status: String,
    pub contacts: usize,
    pub notification_methods: usize,
    pub notification_logs: usize,
    pub transactions: usize,
    pub balance_alerts: usize,
    pub balance_alert_notification_logs: usize,
    pub total: usize,
}

#[derive(Serialize)]
pub struct DuplicatesReport {
    pub status: String,
    pub duplicate_notification_methods: usize,
    pub total: usize,
}

#[derive(Serialize)]
pub struct IntegrityReportResponse {
    #[serde(flatten)]
    pub health: DatabaseHealthResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleanup: Option<CleanupReport>,
}

#[derive(Serialize)]
pub struct CleanupReport {
    pub orphaned_contacts_deleted: usize,
    pub orphaned_methods_deleted: usize,
    pub orphaned_logs_deleted: usize,
    pub orphaned_balance_alert_notification_logs_deleted: usize,
    pub orphaned_alerts_deleted: usize,
    pub orphaned_transactions_deleted: usize,
    pub total_deleted: usize,
}

#[derive(Serialize)]
pub struct CreateWalletResponse {
    /// Success message
    pub message: String,
    /// Created wallet metadata
    pub wallet: WalletMetadata,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    /// Error description
    pub error: String,
    /// Machine-readable error code for frontend translation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

impl ErrorResponse {
    /// Create an error response without an error code (internal/generic errors)
    pub fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            error_code: None,
        }
    }

    /// Create an error response with a machine-readable error code (user-facing errors)
    pub fn coded(code: &str, error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            error_code: Some(code.to_string()),
        }
    }
}

#[derive(Serialize)]
pub struct BlockHeaderResponse {
    /// Block height
    pub height: u32,
    /// Block timestamp
    pub timestamp: u64,
    /// Network name (mainnet, testnet, regtest)
    pub network: String,
}

#[derive(Serialize)]
pub struct CreateContactResponse {
    /// Success message
    pub message: String,
    /// Contact ID
    pub contact_id: String, // UUIDv4
}

#[derive(Serialize)]
pub struct ProvidersResponse {
    /// Available notification providers
    pub providers: Vec<ProviderInfo>,
}

#[derive(Serialize)]
pub struct ContactFormResponse {
    /// Success message
    pub message: String,
}

#[derive(Serialize)]
pub struct BalanceAlertsResponse {
    /// List of balance alerts for the wallet
    pub alerts: Vec<BalanceAlert>,
}

#[derive(Deserialize, Serialize)]
pub struct VerifyContactResponse {
    pub valid: bool,
    pub message: String,
}

#[derive(Serialize)]
pub struct BillingTierLimits {
    pub max_wallets: i32,             // -1 for unlimited
    pub max_contacts_per_wallet: i32, // -1 for unlimited
    pub sync_interval_seconds: u64,
}

#[derive(Serialize)]
pub struct BillingStatusResponse {
    /// User ID
    pub user_id: String,
    /// Current subscription tier
    pub subscription_tier: String,
    /// Subscription status (trial, active, expired, cancelled)
    pub subscription_status: String,
    /// Trial end date (if in trial)
    pub trial_ends_at: Option<String>,
    /// Subscription started date (if active)
    pub subscription_started_at: Option<String>,
    /// Subscription end date (for cancelled subscriptions)
    pub subscription_ends_at: Option<String>,
    /// Stripe customer ID
    pub stripe_customer_id: Option<String>,
    /// Current wallet count
    pub wallet_count: usize,
    /// Current contact count across all wallets
    pub contact_count: usize,
    /// Subscription tier limits
    pub limits: BillingTierLimits,
}

#[derive(Serialize)]
pub struct TestNtfyResponse {
    /// Whether the test notification was sent successfully
    pub success: bool,
    /// Error message if the notification failed
    pub error: Option<String>,
}
