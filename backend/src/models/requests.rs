//! Request DTOs for API endpoints

use crate::metadata::{BalanceAlertType, ProviderType};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct CreateWalletRequest {
    /// The name of the wallet
    pub name: String,
    /// The multipath output descriptor for the wallet or extended public key (XPUB)
    pub descriptor: String,
    /// The user's preferred language for notifications (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_language: Option<String>,
    /// Whether this is a fresh wallet with no transaction history (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_fresh_wallet: Option<bool>,
    /// Script type for XPUB wallets (required when is_fresh_wallet=true and descriptor is XPUB, optional for advanced settings)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script_type: Option<String>,
    /// Stop gap for advanced users (auto, 250, 500, 750, 1000)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_gap: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct UpdateWalletRequest {
    /// The new name for the wallet
    pub name: String,
}

#[derive(Deserialize, Serialize)]
pub struct NotificationMethodRequest {
    /// The provider type (sms or ntfy)
    pub provider_type: ProviderType,
    /// The notification target (phone number or ntfy topic)
    pub notification_target: String,
}

#[derive(Deserialize, Serialize)]
pub struct CreateContactWithMethodsRequest {
    /// The name of the contact person
    pub name: String,
    /// List of notification methods for this contact
    pub notification_methods: Vec<NotificationMethodRequest>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SendContactVerificationRequest {
    /// Contact name
    pub name: String,
    /// Phone number to verify (optional)
    pub phone_number: Option<String>,
    /// Email address to verify (optional)
    pub email_address: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct UpdateContactRequest {
    /// Contact name
    pub name: String,
    /// Notification methods for the contact
    pub notification_methods: Vec<NotificationMethodRequest>,
}

#[derive(Deserialize, Serialize)]
pub struct VerifyContactRequest {
    /// Phone number being verified (optional)
    pub phone_number: Option<String>,
    /// Email address being verified (optional)
    pub email_address: Option<String>,
    /// Verification code
    pub code: String,
}

#[derive(Deserialize, Serialize)]
pub struct CreateBalanceAlertRequest {
    /// Balance threshold in satoshis (Option 1: BTC threshold)
    pub threshold_sats: Option<i64>,
    /// Alert type (above, below, equals)
    pub alert_type: BalanceAlertType,
    /// Fiat currency code (Option 2: Fiat threshold)
    pub threshold_currency: Option<String>, // e.g., "USD", "EUR"
    /// Fiat amount when currency is set
    pub threshold_fiat_amount: Option<f64>,
}

#[derive(Deserialize, Serialize)]
pub struct ContactFormRequest {
    /// The sender's email address
    pub email: String,
    /// The message content
    pub message: String,
}

#[derive(Deserialize, Serialize)]
pub struct CreateCheckoutSessionRequest {
    /// The subscription tier to purchase
    pub tier: String,
    /// Whether to use yearly billing (default is monthly)
    pub is_yearly: Option<bool>,
}

#[derive(Deserialize, Serialize)]
pub struct CreateCustomerPortalRequest {
    /// Return URL after customer portal session
    pub return_url: String,
}

#[derive(Deserialize, Serialize)]
pub struct DemoLoginRequest {
    /// Browser locale for language preference (e.g., "no-NO", "en-US")
    pub browser_locale: Option<String>,
}

#[derive(Deserialize)]
pub struct TestNtfyRequest {
    /// The ntfy topic to send the test notification to
    pub topic: String,
}
