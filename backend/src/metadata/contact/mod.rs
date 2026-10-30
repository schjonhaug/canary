//! Wallet contacts, their notification methods, contact verification and OTP rate limits.
//!
//! The submodules add `impl MetadataDb` blocks; this module owns the per-contact
//! notification settings type they share.

mod duplicates;
mod otp_rate_limit;
mod query;
mod verification;
mod write;

#[derive(Debug, Clone, Copy)]
pub struct ContactNotificationSettings {
    pub notify_sending: bool,
    pub notify_sent: bool,
    pub notify_receiving: bool,
    pub notify_received: bool,
    pub notify_cpfp: bool,
    pub notify_rbf: bool,
    pub include_wallet_balance_in_tx_notifications: bool,
}

impl ContactNotificationSettings {
    pub fn defaults_for_new_contact() -> Self {
        // Legacy backend insertion paths preserve the pre-filtering behavior:
        // notify on every transaction event unless a newer client sends
        // explicit per-contact settings.
        Self {
            notify_sending: true,
            notify_sent: true,
            notify_receiving: true,
            notify_received: true,
            notify_cpfp: true,
            notify_rbf: true,
            include_wallet_balance_in_tx_notifications: false,
        }
    }
}
