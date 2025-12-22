//! Response DTOs for API endpoints

use crate::metadata::{BalanceAlert, WalletMetadata};
use crate::notifications::ProviderInfo;
use serde::{Deserialize, Serialize};

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
