//! Settings for external integrations, grouped by provider.
//!
//! Values are kept as they were configured. Presence checks such as
//! [`AppConfig::validate_required_config`](super::AppConfig::validate_required_config)
//! and the provider constructors decide what counts as usable.

/// A configured secret. `Debug` never prints the value.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The secret value, for handing to the client that needs it.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretString([redacted])")
    }
}

/// Resend email delivery (`RESEND_*`) and contact form recipient.
#[derive(Debug, Clone, Default)]
pub struct EmailSettings {
    pub resend_api_key: Option<SecretString>,
    pub resend_from_email: Option<String>,
    pub resend_from_name: Option<String>,
    /// Marketing audience that opted-in users are added to.
    pub resend_audience_id: Option<String>,
    /// Recipient of contact form submissions (`CONTACT_FORM_EMAIL`).
    pub contact_form_email: Option<String>,
}

/// Twilio SMS and Verify credentials (`TWILIO_*`).
#[derive(Debug, Clone, Default)]
pub struct TwilioSettings {
    pub account_sid: Option<String>,
    pub auth_token: Option<SecretString>,
    pub sender_id: Option<String>,
    pub verify_service_sid: Option<String>,
}

impl TwilioSettings {
    /// Account SID, auth token and sender, when all three are configured.
    pub fn sms_credentials(&self) -> Option<(&str, &str, &str)> {
        Some((
            self.account_sid.as_deref()?,
            self.auth_token.as_ref()?.expose(),
            self.sender_id.as_deref()?,
        ))
    }
}

/// Stripe billing credentials (`STRIPE_SECRET_KEY`, `STRIPE_WEBHOOK_SECRET`).
#[derive(Debug, Clone, Default)]
pub struct StripeSettings {
    pub secret_key: Option<SecretString>,
    pub webhook_secret: Option<SecretString>,
}

/// `BTCPay` cloud subscription plans (`BTCPAY_CLOUD_*`) and webhook secret.
///
/// Every value is trimmed and blank values are treated as unset.
#[derive(Debug, Clone, Default)]
pub struct BtcPayCloudSettings {
    pub offering_id: Option<String>,
    pub personal_plan_id: Option<String>,
    pub team_plan_id: Option<String>,
    pub currency: Option<String>,
    /// Monthly price; only positive values are kept.
    pub personal_monthly_price: Option<i64>,
    /// Monthly price; only positive values are kept.
    pub team_monthly_price: Option<i64>,
    pub webhook_secret: Option<SecretString>,
}

/// Cloud tier sync interval overrides for the configured network
/// (`CANARY_SYNC_INTERVAL_{PERSONAL,TEAM}_{NETWORK}`), in seconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncIntervalOverrides {
    pub personal_secs: Option<u64>,
    pub team_secs: Option<u64>,
}

/// Operating system locale (`LANG`, `LC_ALL`), used to pick the currency and
/// language of the built-in self-hosted user when it is first created.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemLocale {
    pub lang: Option<String>,
    pub lc_all: Option<String>,
}

impl SystemLocale {
    /// Locale used for the default currency: `LANG`, then `LC_ALL`.
    pub fn currency_locale(&self) -> Option<&str> {
        self.lang.as_deref().or(self.lc_all.as_deref())
    }

    /// Locale used for the default language: `LANG` only.
    pub fn language_locale(&self) -> Option<&str> {
        self.lang.as_deref()
    }
}
