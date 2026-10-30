//! Application configuration. Everything the backend reads from the
//! environment is loaded once into [`AppConfig`].

use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};

use bdk_wallet::bitcoin::Network;

use crate::ntfy_provider::NtfyAuth;

mod load;
mod settings;
#[cfg(test)]
mod tests;
mod types;

pub(crate) use load::is_restore_drill_value;
pub use settings::{
    BtcPayCloudSettings, EmailSettings, SecretString, StripeSettings, SyncIntervalOverrides,
    SystemLocale, TwilioSettings,
};
use types::ManagedNtfyAccessToken;
pub use types::{
    BillingProvider, BtcPayPlanConfig, NetworkConfig, NtfyServerConfig, OperatingMode,
    TxExplorerConfig, PUBLIC_NTFY_SERVER_ID, STARTOS_NTFY_SERVER_ID, UMBREL_NTFY_SERVER_ID,
};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub network: NetworkConfig,
    pub electrum_url: Option<String>,
    pub bind_address: String,
    pub data_dir: String,
    pub operating_mode: OperatingMode,
    pub frontend_url: Option<String>,
    /// Additional browser origins allowed to make credentialed API requests.
    frontend_urls: Vec<String>,
    /// JWT secret for authentication
    jwt_secret: Option<String>,
    /// Required password for the built-in self-hosted admin account
    self_hosted_admin_password: Option<String>,
    /// Available self-hosted transaction explorers.
    tx_explorers: Vec<TxExplorerConfig>,
    /// Available self-hosted ntfy servers.
    ntfy_servers: Vec<NtfyServerConfig>,
    /// Default ntfy server URL used when no user or local server preference applies.
    ntfy_fallback_url: String,
    /// Scoped token for a package-provided ntfy server. Never serialized.
    managed_ntfy_access_token: Option<ManagedNtfyAccessToken>,
    /// `BTCPay` Server URL (e.g., <https://btcpay.enogtjue.no>)
    btcpay_url: Option<String>,
    /// `BTCPay` Server API key
    btcpay_api_key: Option<String>,
    /// `BTCPay` Server store ID
    btcpay_store_id: Option<String>,
    /// `BTCPay` Server offering ID (for recurring plan checkouts)
    btcpay_offering_id: Option<String>,
    /// `BTCPay` Server plan ID (for recurring plan checkouts)
    btcpay_plan_id: Option<String>,
    /// `BTCPay` cloud subscription plans and webhook secret.
    btcpay_cloud: BtcPayCloudSettings,
    stripe: StripeSettings,
    email: EmailSettings,
    twilio: TwilioSettings,
    /// `CANARY_RESTORE_DRILL`: billing and customer notifications are off.
    restore_drill: bool,
    /// Self-hosted per-wallet sync target (`CANARY_SYNC_INTERVAL`), in seconds.
    sync_interval: Option<u64>,
    sync_interval_overrides: SyncIntervalOverrides,
    /// Private ntfy topic for operator notifications (cloud mode).
    admin_notification_topic: Option<SecretString>,
    /// Proxies trusted to report the client address (`CANARY_TRUSTED_PROXY_IPS`).
    trusted_proxy_ips: HashSet<IpAddr>,
    /// Fallback Telegram bot token when none is saved in Settings.
    telegram_bot_token: Option<SecretString>,
    /// SOCKS proxy for `.onion` Nostr relays (`CANARY_NOSTR_SOCKS_PROXY`).
    nostr_socks_proxy: Option<SocketAddr>,
    /// Administrator TOTP secrets (`CANARY_ADMIN_MFA_SECRETS_FILE`).
    admin_mfa_secrets_file: PathBuf,
    system_locale: SystemLocale,
}

impl AppConfig {
    fn require_non_empty_config<'a>(
        value: Option<&'a str>,
        missing_message: &'static str,
    ) -> Result<&'a str, &'static str> {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or(missing_message)
    }

    fn has_non_empty(value: Option<&SecretString>) -> bool {
        value.is_some_and(|value| !value.expose().trim().is_empty())
    }

    /// Get the operating mode as a string (cloud or self-hosted)
    pub fn operating_mode(&self) -> String {
        self.operating_mode.to_string()
    }

    /// Check if running in cloud mode (hosted service with authentication and billing)
    pub fn is_cloud_mode(&self) -> bool {
        self.operating_mode == OperatingMode::Cloud
    }

    /// Check if running in self-hosted mode (single-user, no billing)
    pub fn is_self_hosted_mode(&self) -> bool {
        self.operating_mode == OperatingMode::SelfHosted
    }

    /// Get the frontend URL for email links (verification, password reset, etc.)
    /// Returns None in self-hosted mode or if not configured
    pub fn frontend_url(&self) -> Option<&str> {
        self.frontend_url.as_deref()
    }

    /// Get every configured HTTP(S) frontend origin used for CORS and browser
    /// request-origin validation. `FRONTEND_URL` remains the canonical URL for
    /// links, while `FRONTEND_URLS` can add alternate origins exposed by node
    /// platforms such as `StartOS`.
    pub fn frontend_origins(&self) -> Vec<String> {
        let mut origins = Vec::new();

        for frontend_url in self.frontend_url.iter().chain(self.frontend_urls.iter()) {
            let Some(origin) = Self::http_origin(frontend_url) else {
                continue;
            };
            if !origins.contains(&origin) {
                origins.push(origin);
            }
        }

        origins
    }

    fn http_origin(value: &str) -> Option<String> {
        let url = url::Url::parse(value).ok()?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return None;
        }

        Some(url.origin().ascii_serialization())
    }

    /// Get JWT secret for authentication.
    /// Returns an error if `JWT_SECRET` is not configured for the active mode.
    pub fn get_jwt_secret(&self) -> Result<&str, &'static str> {
        Self::require_non_empty_config(
            self.jwt_secret.as_deref(),
            if self.is_self_hosted_mode() {
                "JWT_SECRET required for self-hosted mode - check your .env file"
            } else {
                "JWT_SECRET required for cloud mode - check your .env file"
            },
        )
    }

    /// Get the configured self-hosted admin password.
    /// Returns an error when not running in self-hosted mode or when the password is missing.
    pub fn get_self_hosted_admin_password(&self) -> Result<&str, &'static str> {
        if !self.is_self_hosted_mode() {
            return Err("Self-hosted admin password is only available in self-hosted mode");
        }

        Self::require_non_empty_config(
            self.self_hosted_admin_password.as_deref(),
            "CANARY_SELF_HOSTED_ADMIN_PASSWORD required for self-hosted mode - check your .env file",
        )
    }

    /// Get configured self-hosted transaction explorers.
    pub fn tx_explorers(&self) -> &[TxExplorerConfig] {
        &self.tx_explorers
    }

    /// Get configured self-hosted ntfy servers.
    pub fn ntfy_servers(&self) -> &[NtfyServerConfig] {
        &self.ntfy_servers
    }

    fn single_detected_ntfy_server(&self) -> Option<&NtfyServerConfig> {
        if !self.is_self_hosted_mode() {
            return None;
        }

        match self.ntfy_servers.as_slice() {
            [server] => Some(server),
            servers if servers.len() > 1 => {
                tracing::debug!(
                    count = servers.len(),
                    "Multiple detected ntfy servers configured; falling back to explicit default selection"
                );
                None
            }
            _ => None,
        }
    }

    /// Get the default ntfy server id for API config responses.
    pub fn default_ntfy_server_id(&self) -> String {
        if let Some(server) = self.single_detected_ntfy_server() {
            server.id.clone()
        } else {
            PUBLIC_NTFY_SERVER_ID.to_string()
        }
    }

    /// Isolated restore drills disable billing and customer notifications even
    /// when production credentials are present in the environment.
    pub fn is_restore_drill(&self) -> bool {
        self.restore_drill
    }

    /// Check if `BTCPay` Server integration is fully configured
    pub fn is_btcpay_enabled(&self) -> bool {
        !self.is_restore_drill()
            && self
                .btcpay_url
                .as_deref()
                .map(str::trim)
                .as_ref()
                .is_some_and(|value| !value.is_empty())
            && self
                .btcpay_api_key
                .as_deref()
                .map(str::trim)
                .as_ref()
                .is_some_and(|value| !value.is_empty())
            && self
                .btcpay_store_id
                .as_deref()
                .map(str::trim)
                .as_ref()
                .is_some_and(|value| !value.is_empty())
    }

    /// Check if Stripe billing is configured
    pub fn is_stripe_enabled(&self) -> bool {
        !self.is_restore_drill()
            && Self::has_non_empty(self.stripe.secret_key.as_ref())
            && Self::has_non_empty(self.stripe.webhook_secret.as_ref())
    }

    /// Determine which cloud billing provider should be used.
    /// Stripe wins if both are configured so existing deployments stay unchanged.
    pub fn active_billing_provider(&self) -> Option<BillingProvider> {
        if self.is_restore_drill() {
            return None;
        }
        if self.is_stripe_enabled() {
            Some(BillingProvider::Stripe)
        } else if self.btcpay_cloud_plan_config().is_some() {
            Some(BillingProvider::BtcPay)
        } else {
            None
        }
    }

    /// Check if recurring `BTCPay` donations are fully configured.
    pub fn is_btcpay_recurring_enabled(&self) -> bool {
        self.is_btcpay_enabled()
            && self
                .btcpay_offering_id
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty())
            && self
                .btcpay_plan_id
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty())
    }

    /// Get `BTCPay` Server URL
    pub fn btcpay_url(&self) -> Option<&str> {
        self.btcpay_url.as_deref()
    }

    /// Get `BTCPay` Server API key
    pub fn btcpay_api_key(&self) -> Option<&str> {
        self.btcpay_api_key.as_deref()
    }

    /// Get `BTCPay` Server store ID
    pub fn btcpay_store_id(&self) -> Option<&str> {
        self.btcpay_store_id.as_deref()
    }

    /// Get `BTCPay` Server offering ID (for recurring plan checkouts)
    pub fn btcpay_offering_id(&self) -> Option<&str> {
        self.btcpay_offering_id.as_deref()
    }

    /// Get `BTCPay` Server plan ID (for recurring plan checkouts)
    pub fn btcpay_plan_id(&self) -> Option<&str> {
        self.btcpay_plan_id.as_deref()
    }

    /// Secret used to authenticate `BTCPay` webhook deliveries.
    pub fn btcpay_webhook_secret(&self) -> Option<&str> {
        self.btcpay_cloud
            .webhook_secret
            .as_ref()
            .map(SecretString::expose)
    }

    pub fn btcpay_cloud_plan_config(&self) -> Option<BtcPayPlanConfig> {
        if !self.is_btcpay_enabled() {
            return None;
        }

        let cloud = &self.btcpay_cloud;
        let offering_id = cloud.offering_id.clone().or_else(|| {
            self.btcpay_offering_id()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })?;
        let personal_plan_id = cloud.personal_plan_id.clone()?;
        let team_plan_id = cloud.team_plan_id.clone()?;
        let currency = cloud.currency.as_deref().unwrap_or("USD").to_uppercase();
        let personal_monthly_price = cloud.personal_monthly_price?;
        let team_monthly_price = cloud.team_monthly_price?;

        Some(BtcPayPlanConfig {
            offering_id,
            personal_plan_id,
            team_plan_id,
            currency,
            personal_monthly_price,
            team_monthly_price,
        })
    }

    /// Check if ntfy provider should be enabled
    pub fn is_ntfy_enabled(&self) -> bool {
        // ntfy is always available except during isolated restore drills.
        // Self-hosted mode may also register local-only providers.
        !self.is_restore_drill()
    }

    /// Get the default ntfy server URL.
    /// Detected self-hosted integrations take precedence over environment fallback.
    pub fn ntfy_server_url(&self) -> String {
        if let Some(server) = self.single_detected_ntfy_server() {
            return server.base_url.clone();
        }

        self.ntfy_fallback_url.clone()
    }

    /// Check whether a URL is one of the currently detected self-hosted ntfy servers.
    pub fn is_detected_ntfy_server_url(&self, server_url: &str) -> bool {
        let normalized_server_url = server_url.trim().trim_end_matches('/');
        self.is_self_hosted_mode()
            && self
                .ntfy_servers
                .iter()
                .any(|server| server.base_url.trim_end_matches('/') == normalized_server_url)
    }

    /// Private ntfy destinations are supported on every self-hosted deployment.
    pub fn ntfy_outbound_policy(&self) -> crate::outbound_target::OutboundTargetPolicy {
        use crate::outbound_target::OutboundTargetPolicy;
        if self.is_self_hosted_mode() {
            OutboundTargetPolicy::SelfHostedNtfy
        } else {
            OutboundTargetPolicy::PublicOnly
        }
    }

    pub fn ntfy_provider(
        &self,
        server_url: &str,
        auth: NtfyAuth,
        user_configured_server_url: Option<&str>,
    ) -> crate::ntfy_provider::NtfyProvider {
        use crate::ntfy_provider::NtfyProvider;
        if self.should_trust_ntfy_server_url(server_url, user_configured_server_url) {
            NtfyProvider::with_trusted_auth(server_url, auth)
        } else {
            NtfyProvider::with_policy(server_url, auth, self.ntfy_outbound_policy())
        }
    }

    /// Whether an ntfy server may bypass public-network validation.
    ///
    /// Operator-configured defaults are trusted when the user has not selected
    /// another server. A saved user preference is trusted only when it exactly
    /// matches a detected self-hosted integration. This keeps migrated Umbrel
    /// and `StartOS` preferences working with their existing trusted-client behavior.
    pub fn should_trust_ntfy_server_url(
        &self,
        server_url: &str,
        user_configured_server_url: Option<&str>,
    ) -> bool {
        let Some(user_configured_server_url) = user_configured_server_url else {
            return true;
        };

        let normalized_server_url = server_url.trim().trim_end_matches('/');
        let normalized_user_url = user_configured_server_url.trim().trim_end_matches('/');
        normalized_server_url == normalized_user_url
            && self.is_detected_ntfy_server_url(normalized_server_url)
    }

    /// Return package-managed ntfy auth only for the exact detected server URL it belongs to.
    pub fn managed_ntfy_access_token_for_url(
        &self,
        server_url: &str,
        user_configured_server_url: Option<&str>,
    ) -> Option<String> {
        let normalized_server_url = server_url.trim().trim_end_matches('/');
        let matches_user_configured_url = user_configured_server_url
            .map(|url| url.trim().trim_end_matches('/'))
            .filter(|url| !url.is_empty())
            .is_some_and(|url| url == normalized_server_url);

        if matches_user_configured_url {
            return None;
        }

        let matches_managed_server = self.is_self_hosted_mode()
            && self.ntfy_servers.iter().any(|server| {
                server.managed_auth
                    && server.base_url.trim_end_matches('/') == normalized_server_url
            });

        if matches_managed_server {
            self.managed_ntfy_access_token
                .as_ref()
                .map(|token| token.0.clone())
        } else {
            None
        }
    }

    /// Apply package-managed ntfy auth when no explicit user auth applies.
    pub fn with_managed_ntfy_auth(
        &self,
        ntfy_auth: NtfyAuth,
        server_url: &str,
        user_configured_server_url: Option<&str>,
    ) -> NtfyAuth {
        if !matches!(ntfy_auth, NtfyAuth::None) {
            return ntfy_auth;
        }

        self.managed_ntfy_access_token_for_url(server_url, user_configured_server_url)
            .map_or(NtfyAuth::None, NtfyAuth::AccessToken)
    }

    /// Auth may be sent only to explicitly configured user URLs or detected local integrations.
    pub fn should_use_ntfy_auth_for_url(
        &self,
        server_url: &str,
        user_configured_server_url: Option<&str>,
    ) -> bool {
        let normalized_server_url = server_url.trim().trim_end_matches('/');
        let matches_user_configured_url = user_configured_server_url
            .map(|url| url.trim().trim_end_matches('/'))
            .filter(|url| !url.is_empty())
            .is_some_and(|url| url == normalized_server_url);

        matches_user_configured_url || self.is_detected_ntfy_server_url(server_url)
    }

    /// Check if Twilio SMS provider should be enabled
    pub fn is_twilio_enabled(&self) -> bool {
        // Only allow Twilio in cloud mode, and only if configured
        if self.is_self_hosted_mode() || self.is_restore_drill() {
            return false;
        }

        self.twilio.sms_credentials().is_some()
    }

    /// Check if email provider should be enabled
    pub fn is_email_enabled(&self) -> bool {
        // Only allow email in cloud mode
        self.is_cloud_mode() && !self.is_restore_drill()
    }

    /// Resend email settings.
    pub fn email(&self) -> &EmailSettings {
        &self.email
    }

    /// Twilio SMS and Verify settings.
    pub fn twilio(&self) -> &TwilioSettings {
        &self.twilio
    }

    /// Stripe billing credentials.
    pub fn stripe(&self) -> &StripeSettings {
        &self.stripe
    }

    /// Private ntfy topic for operator notifications, if configured.
    pub fn admin_notification_topic(&self) -> Option<&str> {
        self.admin_notification_topic
            .as_ref()
            .map(SecretString::expose)
    }

    /// Proxies whose `X-Forwarded-For` header is trusted.
    pub fn trusted_proxy_ips(&self) -> &HashSet<IpAddr> {
        &self.trusted_proxy_ips
    }

    /// Telegram bot token from the environment, used when Settings has none.
    pub fn telegram_bot_token(&self) -> Option<&str> {
        self.telegram_bot_token.as_ref().map(SecretString::expose)
    }

    /// SOCKS proxy for `.onion` Nostr inbox relays.
    pub fn nostr_socks_proxy(&self) -> Option<SocketAddr> {
        self.nostr_socks_proxy
    }

    /// File holding the cloud administrators' TOTP secrets.
    pub fn admin_mfa_secrets_file(&self) -> &Path {
        &self.admin_mfa_secrets_file
    }

    /// Operating system locale used for the built-in self-hosted user.
    pub fn system_locale(&self) -> &SystemLocale {
        &self.system_locale
    }

    /// Validate that all required environment variables are set for the current mode
    pub fn validate_required_config(&self) -> Result<(), String> {
        if self.is_cloud_mode() {
            self.validate_cloud_config()
        } else {
            self.validate_self_hosted_config()
        }
    }

    fn validate_self_hosted_config(&self) -> Result<(), String> {
        if self.frontend_origins().is_empty() {
            return Err(
                "Missing or invalid configuration:\n  - FRONTEND_URL or FRONTEND_URLS - Must contain an HTTP(S) origin for browser origin validation"
                    .to_string(),
            );
        }

        Ok(())
    }

    pub fn frontend_origin(&self) -> Option<String> {
        self.frontend_url().and_then(Self::http_origin)
    }

    /// Validate required cloud mode configuration
    fn validate_cloud_config(&self) -> Result<(), String> {
        let mut missing = Vec::new();

        // JWT Secret is required for authentication
        if self.jwt_secret.is_none() {
            missing.push("JWT_SECRET - Required for user authentication");
        }

        // Frontend URL is required for email links and browser trust boundaries.
        if self.frontend_origin().is_none() {
            missing
                .push("FRONTEND_URL - Must be an HTTP(S) origin for email links and CORS security");
        }

        if self.is_restore_drill() {
            return if missing.is_empty() {
                Ok(())
            } else {
                Err(format!(
                    "Restore drill requires the following environment variables:\n{}",
                    missing
                        .into_iter()
                        .map(|var| format!("  - {var}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                ))
            };
        }

        // Billing configuration is required in cloud mode.
        if !self.is_stripe_enabled() && self.btcpay_cloud_plan_config().is_none() {
            missing.push(
                "Either Stripe billing (STRIPE_SECRET_KEY + STRIPE_WEBHOOK_SECRET) or BTCPay cloud billing (BTCPAY_CLOUD_* plan config) must be configured",
            );
        }

        if self.active_billing_provider() == Some(BillingProvider::BtcPay)
            && self.btcpay_webhook_secret().is_none()
        {
            missing.push("BTCPAY_WEBHOOK_SECRET - Required to verify BTCPay subscription webhooks");
        }

        if self.active_billing_provider() == Some(BillingProvider::BtcPay)
            && self
                .btcpay_url()
                .and_then(|value| url::Url::parse(value).ok())
                .is_none_or(|url| url.scheme() != "https" || url.host_str().is_none())
        {
            missing.push("BTCPAY_URL - Must be an HTTPS URL for cloud billing");
        }

        // Twilio configuration is required for SMS notifications
        if self.twilio.account_sid.is_none() {
            missing.push("TWILIO_ACCOUNT_SID - Required for SMS notifications");
        }
        if self.twilio.auth_token.is_none() {
            missing.push("TWILIO_AUTH_TOKEN - Required for SMS notifications");
        }
        if self.twilio.sender_id.is_none() {
            missing.push("TWILIO_SENDER_ID - Required for SMS notifications");
        }
        if self.twilio.verify_service_sid.is_none() {
            missing.push("TWILIO_VERIFY_SERVICE_SID - Required for SMS contact verification");
        }

        // Resend configuration is required for email notifications and auth emails
        if self.email.resend_api_key.is_none() {
            missing.push("RESEND_API_KEY - Required for email notifications and verification");
        }
        if self.email.resend_from_email.is_none() {
            missing.push("RESEND_FROM_EMAIL - Required for sending emails");
        }
        if self.email.resend_from_name.is_none() {
            missing.push("RESEND_FROM_NAME - Required for email sender name");
        }

        if missing.is_empty() {
            Ok(())
        } else {
            let error_msg = format!(
                "Cloud mode requires the following environment variables:\n{}",
                missing
                    .into_iter()
                    .map(|var| format!("  - {var}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            Err(error_msg)
        }
    }

    pub fn electrum_url(&self) -> String {
        self.electrum_url
            .clone()
            .unwrap_or_else(|| self.network.default_electrum_url().to_string())
    }

    pub fn network(&self) -> Network {
        self.network.to_bdk_network()
    }

    /// Get the network name as a string
    fn network_name(&self) -> &'static str {
        match self.network {
            NetworkConfig::Regtest => "regtest",
            NetworkConfig::Testnet => "testnet",
            NetworkConfig::Mainnet => "mainnet",
        }
    }

    /// Get the effective wallet directory path
    /// Returns: {`data_dir}/{network}/wallets`
    pub fn effective_wallet_dir(&self) -> String {
        format!("{}/{}/wallets", self.data_dir, self.network_name())
    }

    /// Get the effective metadata database path
    /// Returns: {`data_dir}/{network}/metadata.sqlite`
    pub fn effective_metadata_db(&self) -> String {
        format!("{}/{}/metadata.sqlite", self.data_dir, self.network_name())
    }

    /// Self-hosted per-wallet sync target, in seconds: `CANARY_SYNC_INTERVAL`,
    /// or a network default when it is absent or invalid. Cloud mode uses the
    /// subscription tier intervals instead; this is only its fallback.
    pub fn sync_interval(&self) -> u64 {
        if self.is_self_hosted_mode() {
            self.sync_interval
                .unwrap_or_else(|| self.network_default_sync_interval())
        } else {
            self.network_default_sync_interval()
        }
    }

    /// Cloud tier sync interval overrides for the configured network.
    pub fn sync_interval_overrides(&self) -> SyncIntervalOverrides {
        self.sync_interval_overrides
    }

    /// Cloud subscription tier sync intervals: network defaults with the
    /// configured overrides applied.
    pub fn tier_sync_intervals(&self) -> crate::subscription::TierSyncIntervals {
        crate::subscription::TierSyncIntervals::defaults_for(&self.network)
            .with_overrides(self.sync_interval_overrides)
    }

    /// Get network-appropriate sync interval defaults
    fn network_default_sync_interval(&self) -> u64 {
        match self.network {
            NetworkConfig::Regtest => 30,  // 30s for regtest (fast local network)
            NetworkConfig::Testnet => 60,  // 60s for testnet
            NetworkConfig::Mainnet => 300, // 5 minutes for mainnet (conservative default)
        }
    }

    // Helper methods for tests
    #[cfg(test)]
    pub fn wallet_dir_path(&self) -> String {
        format!("database/{}/wallets", self.network_name())
    }

    #[cfg(test)]
    pub fn metadata_db_path(&self) -> String {
        format!("database/{}/metadata.sqlite", self.network_name())
    }

    /// Create an `AppConfig` for testing purposes.
    /// This allows external tests to construct `AppConfig` with all required fields.
    pub fn new_for_test(
        network: NetworkConfig,
        electrum_url: Option<String>,
        bind_address: String,
        data_dir: String,
        operating_mode: OperatingMode,
        frontend_url: Option<String>,
        jwt_secret: Option<String>,
    ) -> Self {
        let jwt_secret = if operating_mode == OperatingMode::SelfHosted {
            Some(jwt_secret.unwrap_or_else(|| "test-self-hosted-jwt-secret".to_string()))
        } else {
            jwt_secret
        };
        let self_hosted_admin_password = if operating_mode == OperatingMode::SelfHosted {
            Some("test-self-hosted-password".to_string())
        } else {
            None
        };

        Self {
            network,
            electrum_url,
            bind_address,
            data_dir,
            operating_mode,
            frontend_url,
            frontend_urls: Vec::new(),
            jwt_secret,
            self_hosted_admin_password,
            tx_explorers: Vec::new(),
            ntfy_servers: Vec::new(),
            ntfy_fallback_url: "https://ntfy.sh".to_string(),
            managed_ntfy_access_token: None,
            btcpay_url: None,
            btcpay_api_key: None,
            btcpay_store_id: None,
            btcpay_offering_id: None,
            btcpay_plan_id: None,
            btcpay_cloud: BtcPayCloudSettings::default(),
            stripe: StripeSettings::default(),
            email: EmailSettings::default(),
            twilio: TwilioSettings::default(),
            restore_drill: false,
            sync_interval: None,
            sync_interval_overrides: SyncIntervalOverrides::default(),
            admin_notification_topic: None,
            trusted_proxy_ips: HashSet::new(),
            telegram_bot_token: None,
            nostr_socks_proxy: None,
            admin_mfa_secrets_file: PathBuf::from(load::DEFAULT_ADMIN_MFA_SECRETS_FILE),
            system_locale: SystemLocale::default(),
        }
    }

    /// Set tx explorers on a test config (builder pattern)
    #[must_use]
    pub fn with_tx_explorers(mut self, tx_explorers: Vec<TxExplorerConfig>) -> Self {
        self.tx_explorers = tx_explorers;
        self
    }

    /// Add alternate browser URLs to a test configuration.
    #[must_use]
    pub fn with_frontend_urls(mut self, frontend_urls: Vec<String>) -> Self {
        self.frontend_urls = frontend_urls;
        self
    }

    /// Set ntfy servers on a test config (builder pattern)
    #[must_use]
    pub fn with_ntfy_servers(mut self, ntfy_servers: Vec<NtfyServerConfig>) -> Self {
        self.ntfy_servers = ntfy_servers;
        self
    }

    /// Set ntfy fallback URL on a test config (builder pattern)
    #[must_use]
    pub fn with_ntfy_fallback_url(mut self, ntfy_fallback_url: &str) -> Self {
        self.ntfy_fallback_url = ntfy_fallback_url.to_string();
        self
    }

    /// Set package-managed ntfy access token on a test config (builder pattern)
    #[must_use]
    pub fn with_managed_ntfy_access_token(mut self, token: &str) -> Self {
        self.managed_ntfy_access_token = Some(ManagedNtfyAccessToken(token.to_string()));
        self
    }

    /// Turn the restore drill on or off on a test config.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn with_restore_drill(mut self, restore_drill: bool) -> Self {
        self.restore_drill = restore_drill;
        self
    }

    /// Set Resend settings on a test config.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn with_email(mut self, email: EmailSettings) -> Self {
        self.email = email;
        self
    }

    /// Set Twilio settings on a test config.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn with_twilio(mut self, twilio: TwilioSettings) -> Self {
        self.twilio = twilio;
        self
    }

    /// Set the admin notification topic on a test config.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn with_admin_notification_topic(mut self, topic: &str) -> Self {
        self.admin_notification_topic = Some(SecretString::new(topic));
        self
    }

    /// Set the Telegram fallback bot token on a test config.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn with_telegram_bot_token(mut self, token: &str) -> Self {
        self.telegram_bot_token = Some(SecretString::new(token));
        self
    }

    /// Set the administrator TOTP secrets file on a test config.
    #[must_use]
    pub fn with_admin_mfa_secrets_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.admin_mfa_secrets_file = path.into();
        self
    }

    /// Set `BTCPay` config on a test config (builder pattern)
    #[must_use]
    pub fn with_btcpay(
        mut self,
        url: Option<String>,
        api_key: Option<String>,
        store_id: Option<String>,
        offering_id: Option<String>,
        plan_id: Option<String>,
    ) -> Self {
        self.btcpay_url = url;
        self.btcpay_api_key = api_key;
        self.btcpay_store_id = store_id;
        self.btcpay_offering_id = offering_id;
        self.btcpay_plan_id = plan_id;
        self
    }
}
