use anyhow::{anyhow, Result};
use bdk_wallet::bitcoin::Network;
use clap::{Parser, ValueEnum};
use serde::Serialize;

pub const PUBLIC_NTFY_SERVER_ID: &str = "ntfy-sh";
pub const UMBREL_NTFY_SERVER_ID: &str = "umbrel-ntfy";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TxExplorerConfig {
    pub id: String,
    pub name: String,
    pub base_url: Option<String>,
    pub port: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NtfyServerConfig {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub platform: Option<String>,
}

impl NtfyServerConfig {
    fn new(id: &str, name: &str, base_url: Option<String>, platform: Option<&str>) -> Option<Self> {
        let normalized_base_url = base_url
            .map(|url| url.trim().trim_end_matches('/').to_string())
            .filter(|url| !url.is_empty());

        normalized_base_url.map(|base_url| Self {
            id: id.to_string(),
            name: name.to_string(),
            base_url,
            platform: platform.map(str::to_string),
        })
    }
}

impl TxExplorerConfig {
    fn new(id: &str, name: &str, base_url: Option<String>, port: Option<u16>) -> Option<Self> {
        let normalized_base_url = base_url.map(|url| url.trim_end_matches('/').to_string());

        if normalized_base_url.is_none() && port.is_none() {
            return None;
        }

        Some(Self {
            id: id.to_string(),
            name: name.to_string(),
            base_url: normalized_base_url,
            port,
        })
    }
}

#[derive(Debug, Clone, ValueEnum)]
pub enum NetworkConfig {
    Regtest,
    Testnet,
    Mainnet,
}

impl NetworkConfig {
    pub fn to_bdk_network(&self) -> Network {
        match self {
            NetworkConfig::Regtest => Network::Regtest,
            NetworkConfig::Testnet => Network::Testnet,
            NetworkConfig::Mainnet => Network::Bitcoin,
        }
    }

    pub fn from_network(network: Network) -> Self {
        match network {
            Network::Regtest => NetworkConfig::Regtest,
            Network::Testnet => NetworkConfig::Testnet,
            Network::Bitcoin => NetworkConfig::Mainnet,
            _ => NetworkConfig::Mainnet, // Default fallback
        }
    }

    pub fn default_electrum_url(&self) -> &'static str {
        match self {
            NetworkConfig::Regtest => "tcp://127.0.0.1:50001",
            NetworkConfig::Testnet => "ssl://electrum.blockstream.info:60002",
            NetworkConfig::Mainnet => "ssl://electrum.blockstream.info:50002",
        }
    }
}

impl std::str::FromStr for NetworkConfig {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "regtest" => Ok(NetworkConfig::Regtest),
            "testnet" => Ok(NetworkConfig::Testnet),
            "mainnet" => Ok(NetworkConfig::Mainnet),
            _ => Err(anyhow!(
                "Invalid network: {}. Valid options: regtest, testnet, mainnet",
                s
            )),
        }
    }
}

#[derive(Debug, Clone, Parser)]
#[command(name = "canary")]
#[command(about = "Bitcoin wallet management service")]
pub(crate) struct AppConfigArgs {
    /// Bitcoin network to use
    #[arg(long, value_enum)]
    pub network: Option<NetworkConfig>,

    /// Electrum server URL to connect to
    #[arg(long)]
    pub electrum_url: Option<String>,

    /// Server bind address
    #[arg(long)]
    pub bind_address: Option<String>,

    /// Data directory path
    #[arg(long)]
    pub data_dir: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperatingMode {
    Cloud,
    SelfHosted,
}

impl std::str::FromStr for OperatingMode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "cloud" => Ok(OperatingMode::Cloud),
            "self-hosted" => Ok(OperatingMode::SelfHosted),
            _ => Err(anyhow!(
                "Invalid CANARY_MODE: '{}'. Valid options: cloud, self-hosted",
                s
            )),
        }
    }
}

impl std::fmt::Display for OperatingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OperatingMode::Cloud => write!(f, "cloud"),
            OperatingMode::SelfHosted => write!(f, "self-hosted"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub network: NetworkConfig,
    pub electrum_url: Option<String>,
    pub bind_address: String,
    pub data_dir: String,
    pub operating_mode: OperatingMode,
    pub frontend_url: Option<String>,
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
    /// BTCPay Server URL (e.g., https://btcpay.enogtjue.no)
    btcpay_url: Option<String>,
    /// BTCPay Server API key
    btcpay_api_key: Option<String>,
    /// BTCPay Server store ID
    btcpay_store_id: Option<String>,
    /// BTCPay Server offering ID (for recurring plan checkouts)
    btcpay_offering_id: Option<String>,
    /// BTCPay Server plan ID (for recurring plan checkouts)
    btcpay_plan_id: Option<String>,
}

impl AppConfig {
    fn parse_url_env(var_name: &str) -> Option<String> {
        std::env::var(var_name).ok().and_then(|url| {
            let trimmed_url = url.trim();
            if trimmed_url.is_empty() {
                tracing::warn!("{} is blank and will be ignored", var_name);
                None
            } else if trimmed_url.starts_with("http://") || trimmed_url.starts_with("https://") {
                Some(trimmed_url.trim_end_matches('/').to_string())
            } else {
                tracing::warn!(
                    "{} must start with http:// or https://: '{}' - ignoring",
                    var_name,
                    url
                );
                None
            }
        })
    }

    fn require_non_empty_config<'a>(
        value: Option<&'a str>,
        missing_message: &'static str,
    ) -> Result<&'a str, &'static str> {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or(missing_message)
    }

    pub fn load() -> Result<Self> {
        // Note: .env file is loaded at the start of main() before logging init

        // Parse command line arguments
        let args = AppConfigArgs::parse();
        Self::load_from_args(args)
    }

    pub(crate) fn load_from_args(args: AppConfigArgs) -> Result<Self> {
        // Resolve network configuration (CLI args override env vars)
        let network = match args.network.or_else(|| {
            std::env::var("CANARY_NETWORK")
                .ok()
                .and_then(|v| v.parse().ok())
        }) {
            Some(network) => network,
            None => {
                return Err(anyhow!(
                    "CANARY_NETWORK is required. Set it via --network CLI arg or CANARY_NETWORK environment variable.\nValid values: regtest, testnet, mainnet"
                ));
            }
        };

        // Resolve electrum URL (CLI args override env vars)
        let electrum_url = args
            .electrum_url
            .or_else(|| std::env::var("CANARY_ELECTRUM_URL").ok());

        // Resolve bind address
        let bind_address = match args
            .bind_address
            .or_else(|| std::env::var("CANARY_BIND_ADDRESS").ok())
        {
            Some(addr) => addr,
            None => {
                return Err(anyhow!(
                    "CANARY_BIND_ADDRESS is required. Set it via --bind-address CLI arg or CANARY_BIND_ADDRESS environment variable.\nExample: 127.0.0.1:3000"
                ));
            }
        };

        // Resolve data directory
        let data_dir = match args
            .data_dir
            .or_else(|| std::env::var("CANARY_DATA_DIR").ok())
        {
            Some(dir) => dir,
            None => {
                return Err(anyhow!(
                    "CANARY_DATA_DIR is required. Set it via --data-dir CLI arg or CANARY_DATA_DIR environment variable.\nExample: ./database"
                ));
            }
        };

        // Resolve operating mode (required, no default)
        let operating_mode = match std::env::var("CANARY_MODE") {
            Ok(mode_str) => mode_str.parse::<OperatingMode>()?,
            Err(_) => {
                return Err(anyhow!(
                    "CANARY_MODE is required. Set it via CANARY_MODE environment variable.\nValid values: cloud, self-hosted\n\nTo get started:\n  - For self-hosted mode: cp .env.example.self-hosted .env\n  - For cloud mode: cp .env.example.cloud .env"
                ));
            }
        };

        // Load frontend URL (optional in self-hosted mode, validated in cloud mode)
        let frontend_url = std::env::var("FRONTEND_URL").ok();

        // Load authentication configuration
        let jwt_secret = std::env::var("JWT_SECRET").ok();
        let self_hosted_admin_password = std::env::var("CANARY_SELF_HOSTED_ADMIN_PASSWORD").ok();

        // Load self-hosted tx explorer configuration (optional)
        let mempool_url = Self::parse_url_env("CANARY_MEMPOOL_URL");
        let mempool_port = std::env::var("CANARY_MEMPOOL_PORT")
            .ok()
            .and_then(|s| s.parse().ok());
        let bitfeed_url = Self::parse_url_env("CANARY_BITFEED_URL");
        let bitfeed_port = std::env::var("CANARY_BITFEED_PORT")
            .ok()
            .and_then(|s| s.parse().ok());
        let btc_rpc_explorer_url = Self::parse_url_env("CANARY_BTC_RPC_EXPLORER_URL");
        let btc_rpc_explorer_port = std::env::var("CANARY_BTC_RPC_EXPLORER_PORT")
            .ok()
            .and_then(|s| s.parse().ok());
        let tx_explorers = [
            TxExplorerConfig::new("mempool", "Mempool", mempool_url, mempool_port),
            TxExplorerConfig::new("bitfeed", "Bitfeed", bitfeed_url, bitfeed_port),
            TxExplorerConfig::new(
                "btc-rpc-explorer",
                "BTC RPC Explorer",
                btc_rpc_explorer_url,
                btc_rpc_explorer_port,
            ),
        ]
        .into_iter()
        .flatten()
        .collect();

        let umbrel_ntfy_url = Self::parse_url_env("CANARY_UMBREL_NTFY_URL");
        let configured_ntfy_fallback_url = Self::parse_url_env("NTFY_SERVER_URL");
        if operating_mode == OperatingMode::Cloud && umbrel_ntfy_url.is_some() {
            tracing::warn!(
                "CANARY_UMBREL_NTFY_URL is only used in self-hosted mode; ignoring detected ntfy server in cloud mode"
            );
        }
        if operating_mode == OperatingMode::SelfHosted
            && umbrel_ntfy_url.is_some()
            && configured_ntfy_fallback_url.is_some()
        {
            tracing::warn!(
                "CANARY_UMBREL_NTFY_URL is set; detected Umbrel ntfy server will take precedence over NTFY_SERVER_URL"
            );
        }
        let ntfy_servers = if operating_mode == OperatingMode::SelfHosted {
            [NtfyServerConfig::new(
                UMBREL_NTFY_SERVER_ID,
                "ntfy",
                umbrel_ntfy_url,
                Some("umbrel"),
            )]
            .into_iter()
            .flatten()
            .collect()
        } else {
            Vec::new()
        };
        let ntfy_fallback_url =
            configured_ntfy_fallback_url.unwrap_or_else(|| "https://ntfy.sh".to_string());

        // Load BTCPay configuration (optional, cloud mode only)
        let btcpay_url = std::env::var("BTCPAY_URL").ok();
        let btcpay_api_key = std::env::var("BTCPAY_API_KEY").ok();
        let btcpay_store_id = std::env::var("BTCPAY_STORE_ID").ok();
        let btcpay_offering_id = std::env::var("BTCPAY_OFFERING_ID").ok();
        let btcpay_plan_id = std::env::var("BTCPAY_PLAN_ID").ok();

        if operating_mode == OperatingMode::SelfHosted {
            if jwt_secret
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .is_none()
            {
                return Err(anyhow!(
                    "JWT_SECRET required for self-hosted mode - check your .env file"
                ));
            }

            if self_hosted_admin_password
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .is_none()
            {
                return Err(anyhow!(
                    "CANARY_SELF_HOSTED_ADMIN_PASSWORD required for self-hosted mode - check your .env file"
                ));
            }
        }

        Ok(AppConfig {
            network,
            electrum_url,
            bind_address,
            data_dir,
            operating_mode,
            frontend_url,
            jwt_secret,
            self_hosted_admin_password,
            tx_explorers,
            ntfy_servers,
            ntfy_fallback_url,
            btcpay_url,
            btcpay_api_key,
            btcpay_store_id,
            btcpay_offering_id,
            btcpay_plan_id,
        })
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

    /// Get JWT secret for authentication.
    /// Returns an error if JWT_SECRET is not configured for the active mode.
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

    /// Check if BTCPay Server integration is fully configured
    pub fn is_btcpay_enabled(&self) -> bool {
        self.btcpay_url.is_some() && self.btcpay_api_key.is_some() && self.btcpay_store_id.is_some()
    }

    /// Check if recurring BTCPay donations are fully configured.
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

    /// Get BTCPay Server URL
    pub fn btcpay_url(&self) -> Option<&str> {
        self.btcpay_url.as_deref()
    }

    /// Get BTCPay Server API key
    pub fn btcpay_api_key(&self) -> Option<&str> {
        self.btcpay_api_key.as_deref()
    }

    /// Get BTCPay Server store ID
    pub fn btcpay_store_id(&self) -> Option<&str> {
        self.btcpay_store_id.as_deref()
    }

    /// Get BTCPay Server offering ID (for recurring plan checkouts)
    pub fn btcpay_offering_id(&self) -> Option<&str> {
        self.btcpay_offering_id.as_deref()
    }

    /// Get BTCPay Server plan ID (for recurring plan checkouts)
    pub fn btcpay_plan_id(&self) -> Option<&str> {
        self.btcpay_plan_id.as_deref()
    }

    /// Check if ntfy provider should be enabled
    pub fn is_ntfy_enabled(&self) -> bool {
        // ntfy is always available, but in self-hosted mode it's the only provider
        true
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
        if self.is_self_hosted_mode() {
            return false;
        }

        // Check if Twilio environment variables are configured
        std::env::var("TWILIO_ACCOUNT_SID").is_ok()
            && std::env::var("TWILIO_AUTH_TOKEN").is_ok()
            && std::env::var("TWILIO_SENDER_ID").is_ok()
    }

    /// Check if email provider should be enabled
    pub fn is_email_enabled(&self) -> bool {
        // Only allow email in cloud mode
        self.is_cloud_mode()
    }

    /// Validate that all required environment variables are set for the current mode
    pub fn validate_required_config(&self) -> Result<(), String> {
        if self.is_cloud_mode() {
            self.validate_cloud_config()
        } else {
            // Self-hosted mode has no strict requirements beyond basic config
            Ok(())
        }
    }

    /// Validate required cloud mode configuration
    fn validate_cloud_config(&self) -> Result<(), String> {
        let mut missing = Vec::new();

        // JWT Secret is required for authentication
        if std::env::var("JWT_SECRET").is_err() {
            missing.push("JWT_SECRET - Required for user authentication");
        }

        // Stripe configuration is required for billing
        if std::env::var("STRIPE_SECRET_KEY").is_err() {
            missing.push("STRIPE_SECRET_KEY - Required for subscription billing");
        }
        if std::env::var("STRIPE_WEBHOOK_SECRET").is_err() {
            missing.push("STRIPE_WEBHOOK_SECRET - Required for webhook verification");
        }

        // Twilio configuration is required for SMS notifications
        if std::env::var("TWILIO_ACCOUNT_SID").is_err() {
            missing.push("TWILIO_ACCOUNT_SID - Required for SMS notifications");
        }
        if std::env::var("TWILIO_AUTH_TOKEN").is_err() {
            missing.push("TWILIO_AUTH_TOKEN - Required for SMS notifications");
        }
        if std::env::var("TWILIO_SENDER_ID").is_err() {
            missing.push("TWILIO_SENDER_ID - Required for SMS notifications");
        }
        if std::env::var("TWILIO_VERIFY_SERVICE_SID").is_err() {
            missing.push("TWILIO_VERIFY_SERVICE_SID - Required for SMS contact verification");
        }

        // Resend configuration is required for email notifications and auth emails
        if std::env::var("RESEND_API_KEY").is_err() {
            missing.push("RESEND_API_KEY - Required for email notifications and verification");
        }
        if std::env::var("RESEND_FROM_EMAIL").is_err() {
            missing.push("RESEND_FROM_EMAIL - Required for sending emails");
        }
        if std::env::var("RESEND_FROM_NAME").is_err() {
            missing.push("RESEND_FROM_NAME - Required for email sender name");
        }

        // Frontend URL is required for email links
        if std::env::var("FRONTEND_URL").is_err() {
            missing.push("FRONTEND_URL - Required for email links and CORS security");
        }

        // BTCPay Server configuration is required for donation redirects
        if std::env::var("BTCPAY_URL").is_err() {
            missing.push("BTCPAY_URL - Required for donation page redirects");
        }
        if std::env::var("BTCPAY_API_KEY").is_err() {
            missing.push("BTCPAY_API_KEY - Required for donation page redirects");
        }
        if std::env::var("BTCPAY_STORE_ID").is_err() {
            missing.push("BTCPAY_STORE_ID - Required for donation page redirects");
        }
        if std::env::var("BTCPAY_OFFERING_ID").is_err() {
            missing.push("BTCPAY_OFFERING_ID - Required for recurring donation redirects");
        }
        if std::env::var("BTCPAY_PLAN_ID").is_err() {
            missing.push("BTCPAY_PLAN_ID - Required for recurring donation redirects");
        }

        if missing.is_empty() {
            Ok(())
        } else {
            let error_msg = format!(
                "Cloud mode requires the following environment variables:\n{}",
                missing
                    .into_iter()
                    .map(|var| format!("  - {}", var))
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
    /// Returns: {data_dir}/{network}/wallets
    pub fn effective_wallet_dir(&self) -> String {
        format!("{}/{}/wallets", self.data_dir, self.network_name())
    }

    /// Get the effective metadata database path
    /// Returns: {data_dir}/{network}/metadata.sqlite
    pub fn effective_metadata_db(&self) -> String {
        format!("{}/{}/metadata.sqlite", self.data_dir, self.network_name())
    }

    /// Get sync interval based on mode and configuration
    /// Self-hosted mode: Uses CANARY_SYNC_INTERVAL with network defaults
    /// Cloud mode: Delegates to subscription tier logic
    pub fn get_sync_interval(&self) -> u64 {
        if self.is_self_hosted_mode() {
            // Self-hosted mode: Use legacy CANARY_SYNC_INTERVAL or network-based defaults
            std::env::var("CANARY_SYNC_INTERVAL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| self.get_network_default_sync_interval())
        } else {
            // Cloud mode: Use subscription tier logic (handled elsewhere)
            // This is a fallback - normally subscription tiers handle this
            self.get_network_default_sync_interval()
        }
    }

    /// Get network-appropriate sync interval defaults
    fn get_network_default_sync_interval(&self) -> u64 {
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

    /// Create an AppConfig for testing purposes.
    /// This allows external tests to construct AppConfig with all required fields.
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
            jwt_secret,
            self_hosted_admin_password,
            tx_explorers: Vec::new(),
            ntfy_servers: Vec::new(),
            ntfy_fallback_url: "https://ntfy.sh".to_string(),
            btcpay_url: None,
            btcpay_api_key: None,
            btcpay_store_id: None,
            btcpay_offering_id: None,
            btcpay_plan_id: None,
        }
    }

    /// Set tx explorers on a test config (builder pattern)
    pub fn with_tx_explorers(mut self, tx_explorers: Vec<TxExplorerConfig>) -> Self {
        self.tx_explorers = tx_explorers;
        self
    }

    /// Set ntfy servers on a test config (builder pattern)
    pub fn with_ntfy_servers(mut self, ntfy_servers: Vec<NtfyServerConfig>) -> Self {
        self.ntfy_servers = ntfy_servers;
        self
    }

    /// Set ntfy fallback URL on a test config (builder pattern)
    pub fn with_ntfy_fallback_url(mut self, ntfy_fallback_url: &str) -> Self {
        self.ntfy_fallback_url = ntfy_fallback_url.to_string();
        self
    }

    /// Set BTCPay config on a test config (builder pattern)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn restore_env_var(name: &str, value: Option<String>) {
        if let Some(value) = value {
            std::env::set_var(name, value);
        } else {
            std::env::remove_var(name);
        }
    }

    #[test]
    fn test_network_config_parsing() {
        assert!(matches!(
            "regtest".parse::<NetworkConfig>().unwrap(),
            NetworkConfig::Regtest
        ));
        assert!(matches!(
            "testnet".parse::<NetworkConfig>().unwrap(),
            NetworkConfig::Testnet
        ));
        assert!(matches!(
            "mainnet".parse::<NetworkConfig>().unwrap(),
            NetworkConfig::Mainnet
        ));
        assert!("invalid".parse::<NetworkConfig>().is_err());
    }

    #[test]
    fn test_default_electrum_urls() {
        assert_eq!(
            NetworkConfig::Regtest.default_electrum_url(),
            "tcp://127.0.0.1:50001"
        );
        assert_eq!(
            NetworkConfig::Testnet.default_electrum_url(),
            "ssl://electrum.blockstream.info:60002"
        );
        assert_eq!(
            NetworkConfig::Mainnet.default_electrum_url(),
            "ssl://electrum.blockstream.info:50002"
        );
    }

    #[test]
    fn test_bdk_network_conversion() {
        assert_eq!(NetworkConfig::Regtest.to_bdk_network(), Network::Regtest);
        assert_eq!(NetworkConfig::Testnet.to_bdk_network(), Network::Testnet);
        assert_eq!(NetworkConfig::Mainnet.to_bdk_network(), Network::Bitcoin);
    }

    // Helper function to create test configs with default mode
    fn test_config(network: NetworkConfig) -> AppConfig {
        AppConfig {
            network,
            electrum_url: None,
            bind_address: "127.0.0.1:3000".to_string(),
            data_dir: "./database".to_string(),
            operating_mode: OperatingMode::Cloud, // Default for tests
            frontend_url: Some("http://localhost:3001".to_string()),
            jwt_secret: Some("test-jwt-secret".to_string()),
            self_hosted_admin_password: None,
            tx_explorers: Vec::new(),
            ntfy_servers: Vec::new(),
            ntfy_fallback_url: "https://ntfy.sh".to_string(),
            btcpay_url: None,
            btcpay_api_key: None,
            btcpay_store_id: None,
            btcpay_offering_id: None,
            btcpay_plan_id: None,
        }
    }

    fn test_config_with_data_dir(network: NetworkConfig, data_dir: &str) -> AppConfig {
        AppConfig {
            network,
            electrum_url: None,
            bind_address: "127.0.0.1:3000".to_string(),
            data_dir: data_dir.to_string(),
            operating_mode: OperatingMode::Cloud,
            frontend_url: Some("http://localhost:3001".to_string()),
            jwt_secret: Some("test-jwt-secret".to_string()),
            self_hosted_admin_password: None,
            tx_explorers: Vec::new(),
            ntfy_servers: Vec::new(),
            ntfy_fallback_url: "https://ntfy.sh".to_string(),
            btcpay_url: None,
            btcpay_api_key: None,
            btcpay_store_id: None,
            btcpay_offering_id: None,
            btcpay_plan_id: None,
        }
    }

    fn test_config_self_hosted(network: NetworkConfig) -> AppConfig {
        AppConfig {
            network,
            electrum_url: None,
            bind_address: "127.0.0.1:3000".to_string(),
            data_dir: "./database".to_string(),
            operating_mode: OperatingMode::SelfHosted,
            frontend_url: None,
            jwt_secret: Some("test-self-hosted-jwt-secret".to_string()),
            self_hosted_admin_password: Some("self-hosted-password".to_string()),
            tx_explorers: Vec::new(),
            ntfy_servers: Vec::new(),
            ntfy_fallback_url: "https://ntfy.sh".to_string(),
            btcpay_url: None,
            btcpay_api_key: None,
            btcpay_store_id: None,
            btcpay_offering_id: None,
            btcpay_plan_id: None,
        }
    }

    #[test]
    fn test_network_specific_paths() {
        // Test regtest paths
        let config = test_config(NetworkConfig::Regtest);
        assert_eq!(config.wallet_dir_path(), "database/regtest/wallets");
        assert_eq!(
            config.metadata_db_path(),
            "database/regtest/metadata.sqlite"
        );

        // Test testnet paths
        let config = test_config(NetworkConfig::Testnet);
        assert_eq!(config.wallet_dir_path(), "database/testnet/wallets");
        assert_eq!(
            config.metadata_db_path(),
            "database/testnet/metadata.sqlite"
        );

        // Test mainnet paths
        let config = test_config(NetworkConfig::Mainnet);
        assert_eq!(config.wallet_dir_path(), "database/mainnet/wallets");
        assert_eq!(
            config.metadata_db_path(),
            "database/mainnet/metadata.sqlite"
        );
    }

    #[test]
    fn test_effective_paths() {
        // Test with relative paths (local development)
        let config = test_config(NetworkConfig::Regtest);
        assert_eq!(config.effective_wallet_dir(), "./database/regtest/wallets");
        assert_eq!(
            config.effective_metadata_db(),
            "./database/regtest/metadata.sqlite"
        );

        // Test with absolute paths (Docker/Umbrel)
        let config = test_config_with_data_dir(NetworkConfig::Mainnet, "/app/data");
        assert_eq!(config.effective_wallet_dir(), "/app/data/mainnet/wallets");
        assert_eq!(
            config.effective_metadata_db(),
            "/app/data/mainnet/metadata.sqlite"
        );
    }

    #[test]
    fn test_network_database_isolation() {
        let networks = vec![
            NetworkConfig::Regtest,
            NetworkConfig::Testnet,
            NetworkConfig::Mainnet,
        ];

        let mut wallet_paths = Vec::new();
        let mut metadata_paths = Vec::new();

        for network in networks {
            let config = test_config(network);

            let wallet_path = config.wallet_dir_path();
            let metadata_path = config.metadata_db_path();

            // Ensure no duplicate paths
            assert!(
                !wallet_paths.contains(&wallet_path),
                "Wallet path '{}' is not unique",
                wallet_path
            );
            assert!(
                !metadata_paths.contains(&metadata_path),
                "Metadata path '{}' is not unique",
                metadata_path
            );

            wallet_paths.push(wallet_path);
            metadata_paths.push(metadata_path);
        }

        // Verify all paths are different
        assert_eq!(wallet_paths.len(), 3);
        assert_eq!(metadata_paths.len(), 3);

        // Verify expected paths
        assert!(wallet_paths.contains(&"database/regtest/wallets".to_string()));
        assert!(wallet_paths.contains(&"database/testnet/wallets".to_string()));
        assert!(wallet_paths.contains(&"database/mainnet/wallets".to_string()));

        assert!(metadata_paths.contains(&"database/regtest/metadata.sqlite".to_string()));
        assert!(metadata_paths.contains(&"database/testnet/metadata.sqlite".to_string()));
        assert!(metadata_paths.contains(&"database/mainnet/metadata.sqlite".to_string()));
    }

    #[test]
    fn test_network_electrum_url_defaults() {
        let regtest_config = test_config(NetworkConfig::Regtest);
        assert_eq!(regtest_config.electrum_url(), "tcp://127.0.0.1:50001");

        let testnet_config = test_config(NetworkConfig::Testnet);
        assert_eq!(
            testnet_config.electrum_url(),
            "ssl://electrum.blockstream.info:60002"
        );

        let mainnet_config = test_config(NetworkConfig::Mainnet);
        assert_eq!(
            mainnet_config.electrum_url(),
            "ssl://electrum.blockstream.info:50002"
        );
    }

    #[test]
    fn test_self_hosted_mode_sync_interval_legacy_fallback() {
        let _guard = ENV_LOCK.lock().unwrap();
        let previous_sync_interval = std::env::var("CANARY_SYNC_INTERVAL").ok();

        // Set CANARY_SYNC_INTERVAL for self-hosted mode
        std::env::set_var("CANARY_SYNC_INTERVAL", "42");

        let config = test_config_self_hosted(NetworkConfig::Mainnet);
        assert_eq!(config.get_sync_interval(), 42);

        restore_env_var("CANARY_SYNC_INTERVAL", previous_sync_interval);
    }

    #[test]
    fn test_self_hosted_mode_sync_interval_network_defaults() {
        let _guard = ENV_LOCK.lock().unwrap();
        let previous_sync_interval = std::env::var("CANARY_SYNC_INTERVAL").ok();

        // No CANARY_SYNC_INTERVAL, should use network defaults
        std::env::remove_var("CANARY_SYNC_INTERVAL");

        let regtest_config = test_config_self_hosted(NetworkConfig::Regtest);
        assert_eq!(regtest_config.get_sync_interval(), 30);

        let mainnet_config = test_config_self_hosted(NetworkConfig::Mainnet);
        assert_eq!(mainnet_config.get_sync_interval(), 300);

        restore_env_var("CANARY_SYNC_INTERVAL", previous_sync_interval);
    }

    #[test]
    fn test_cloud_mode_sync_interval_fallback() {
        let config = test_config(NetworkConfig::Mainnet);

        // In cloud mode, should use network defaults as fallback
        assert_eq!(config.get_sync_interval(), 300);
    }

    #[test]
    fn test_custom_electrum_url_override() {
        let config = AppConfig {
            network: NetworkConfig::Mainnet,
            electrum_url: Some("ssl://custom.electrum.server:50002".to_string()),
            bind_address: "127.0.0.1:3000".to_string(),
            data_dir: "./database".to_string(),
            operating_mode: OperatingMode::Cloud,
            frontend_url: Some("http://localhost:3001".to_string()),
            jwt_secret: Some("test-jwt-secret".to_string()),
            self_hosted_admin_password: None,
            tx_explorers: Vec::new(),
            ntfy_servers: Vec::new(),
            ntfy_fallback_url: "https://ntfy.sh".to_string(),
            btcpay_url: None,
            btcpay_api_key: None,
            btcpay_store_id: None,
            btcpay_offering_id: None,
            btcpay_plan_id: None,
        };
        assert_eq!(config.electrum_url(), "ssl://custom.electrum.server:50002");
    }

    #[test]
    fn test_operating_mode_parsing() {
        assert!(matches!(
            "cloud".parse::<OperatingMode>().unwrap(),
            OperatingMode::Cloud
        ));
        assert!(matches!(
            "self-hosted".parse::<OperatingMode>().unwrap(),
            OperatingMode::SelfHosted
        ));
        assert!(matches!(
            "CLOUD".parse::<OperatingMode>().unwrap(),
            OperatingMode::Cloud
        ));
        assert!(matches!(
            "Self-Hosted".parse::<OperatingMode>().unwrap(),
            OperatingMode::SelfHosted
        ));
        assert!("invalid".parse::<OperatingMode>().is_err());
    }

    #[test]
    fn test_is_cloud_mode() {
        let cloud_config = test_config(NetworkConfig::Regtest);
        assert!(cloud_config.is_cloud_mode());
        assert!(!cloud_config.is_self_hosted_mode());

        let self_hosted_config = test_config_self_hosted(NetworkConfig::Regtest);
        assert!(!self_hosted_config.is_cloud_mode());
        assert!(self_hosted_config.is_self_hosted_mode());
    }

    #[test]
    fn test_detected_ntfy_server_becomes_self_hosted_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        let previous_ntfy_server_url = std::env::var("NTFY_SERVER_URL").ok();

        std::env::remove_var("NTFY_SERVER_URL");
        let config = test_config_self_hosted(NetworkConfig::Regtest).with_ntfy_servers(vec![
            NtfyServerConfig::new(
                "umbrel-ntfy",
                "ntfy",
                Some("http://ntfy_app_1/".to_string()),
                Some("umbrel"),
            )
            .unwrap(),
        ]);

        assert_eq!(config.default_ntfy_server_id(), "umbrel-ntfy");
        assert_eq!(config.ntfy_server_url(), "http://ntfy_app_1");
        assert_eq!(config.ntfy_servers()[0].platform.as_deref(), Some("umbrel"));
        assert!(config.is_detected_ntfy_server_url("http://ntfy_app_1/"));
        assert!(config.is_detected_ntfy_server_url(" http://ntfy_app_1/"));
        assert!(!config.is_detected_ntfy_server_url("https://ntfy.sh"));

        restore_env_var("NTFY_SERVER_URL", previous_ntfy_server_url);
    }

    #[test]
    fn test_load_detects_umbrel_ntfy_url_in_self_hosted_mode() {
        let _guard = ENV_LOCK.lock().unwrap();
        let previous_mode = std::env::var("CANARY_MODE").ok();
        let previous_jwt = std::env::var("JWT_SECRET").ok();
        let previous_admin_password = std::env::var("CANARY_SELF_HOSTED_ADMIN_PASSWORD").ok();
        let previous_umbrel_ntfy_url = std::env::var("CANARY_UMBREL_NTFY_URL").ok();
        let previous_ntfy_server_url = std::env::var("NTFY_SERVER_URL").ok();

        std::env::set_var("CANARY_MODE", "self-hosted");
        std::env::set_var("JWT_SECRET", "test-jwt-secret");
        std::env::set_var("CANARY_SELF_HOSTED_ADMIN_PASSWORD", "test-admin-password");
        std::env::set_var("CANARY_UMBREL_NTFY_URL", "http://ntfy_app_1/");
        std::env::remove_var("NTFY_SERVER_URL");

        let config = AppConfig::load_from_args(AppConfigArgs {
            network: Some(NetworkConfig::Regtest),
            electrum_url: None,
            bind_address: Some("127.0.0.1:3000".to_string()),
            data_dir: Some("./database".to_string()),
        })
        .unwrap();

        assert_eq!(config.default_ntfy_server_id(), "umbrel-ntfy");
        assert_eq!(config.ntfy_server_url(), "http://ntfy_app_1");
        assert_eq!(config.ntfy_servers().len(), 1);
        assert_eq!(config.ntfy_servers()[0].platform.as_deref(), Some("umbrel"));

        restore_env_var("CANARY_MODE", previous_mode);
        restore_env_var("JWT_SECRET", previous_jwt);
        restore_env_var("CANARY_SELF_HOSTED_ADMIN_PASSWORD", previous_admin_password);
        restore_env_var("CANARY_UMBREL_NTFY_URL", previous_umbrel_ntfy_url);
        restore_env_var("NTFY_SERVER_URL", previous_ntfy_server_url);
    }

    #[test]
    fn test_detected_umbrel_ntfy_url_takes_precedence_over_fallback_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        let previous_mode = std::env::var("CANARY_MODE").ok();
        let previous_jwt = std::env::var("JWT_SECRET").ok();
        let previous_admin_password = std::env::var("CANARY_SELF_HOSTED_ADMIN_PASSWORD").ok();
        let previous_umbrel_ntfy_url = std::env::var("CANARY_UMBREL_NTFY_URL").ok();
        let previous_ntfy_server_url = std::env::var("NTFY_SERVER_URL").ok();

        std::env::set_var("CANARY_MODE", "self-hosted");
        std::env::set_var("JWT_SECRET", "test-jwt-secret");
        std::env::set_var("CANARY_SELF_HOSTED_ADMIN_PASSWORD", "test-admin-password");
        std::env::set_var("CANARY_UMBREL_NTFY_URL", "http://ntfy_app_1");
        std::env::set_var("NTFY_SERVER_URL", "https://ntfy.example.com");

        let config = AppConfig::load_from_args(AppConfigArgs {
            network: Some(NetworkConfig::Mainnet),
            electrum_url: None,
            bind_address: Some("127.0.0.1:3000".to_string()),
            data_dir: Some("./database".to_string()),
        })
        .expect("self-hosted config should load");

        assert_eq!(config.ntfy_server_url(), "http://ntfy_app_1");
        assert_eq!(config.default_ntfy_server_id(), UMBREL_NTFY_SERVER_ID);

        restore_env_var("CANARY_MODE", previous_mode);
        restore_env_var("JWT_SECRET", previous_jwt);
        restore_env_var("CANARY_SELF_HOSTED_ADMIN_PASSWORD", previous_admin_password);
        restore_env_var("CANARY_UMBREL_NTFY_URL", previous_umbrel_ntfy_url);
        restore_env_var("NTFY_SERVER_URL", previous_ntfy_server_url);
    }

    #[test]
    fn test_ntfy_server_config_rejects_blank_url() {
        assert!(NtfyServerConfig::new(
            "umbrel-ntfy",
            "ntfy",
            Some("   ".to_string()),
            Some("umbrel")
        )
        .is_none());
    }

    #[test]
    fn test_ntfy_fallback_url_env_validates_configured_url() {
        let _guard = ENV_LOCK.lock().unwrap();
        let previous_ntfy_server_url = std::env::var("NTFY_SERVER_URL").ok();

        std::env::set_var("NTFY_SERVER_URL", "   ");
        assert!(AppConfig::parse_url_env("NTFY_SERVER_URL").is_none());

        std::env::set_var("NTFY_SERVER_URL", "ntfy.example.com");
        assert!(AppConfig::parse_url_env("NTFY_SERVER_URL").is_none());

        std::env::set_var("NTFY_SERVER_URL", " https://ntfy.example.com/ ");
        assert_eq!(
            AppConfig::parse_url_env("NTFY_SERVER_URL").as_deref(),
            Some("https://ntfy.example.com")
        );

        restore_env_var("NTFY_SERVER_URL", previous_ntfy_server_url);
    }

    #[test]
    fn test_ntfy_server_url_falls_back_to_env_without_detected_local() {
        let config = test_config_self_hosted(NetworkConfig::Regtest)
            .with_ntfy_fallback_url("https://ntfy.example.com");

        assert_eq!(config.default_ntfy_server_id(), "ntfy-sh");
        assert_eq!(config.ntfy_server_url(), "https://ntfy.example.com");
    }

    #[test]
    fn test_detected_ntfy_server_takes_precedence_over_fallback_url() {
        let _guard = ENV_LOCK.lock().unwrap();
        let config = test_config_self_hosted(NetworkConfig::Regtest)
            .with_ntfy_fallback_url("https://ntfy.example.com")
            .with_ntfy_servers(vec![NtfyServerConfig::new(
                "umbrel-ntfy",
                "ntfy",
                Some("http://ntfy_app_1".to_string()),
                Some("umbrel"),
            )
            .unwrap()]);

        assert_eq!(config.ntfy_server_url(), "http://ntfy_app_1");
    }

    #[test]
    fn test_ntfy_auth_only_allowed_for_matching_saved_or_detected_url() {
        let config = test_config_self_hosted(NetworkConfig::Regtest).with_ntfy_servers(vec![
            NtfyServerConfig::new(
                "umbrel-ntfy",
                "ntfy",
                Some("http://ntfy_app_1".to_string()),
                Some("umbrel"),
            )
            .unwrap(),
        ]);

        assert!(config.should_use_ntfy_auth_for_url(
            "https://ntfy.example.com/",
            Some(" https://ntfy.example.com ")
        ));
        assert!(config.should_use_ntfy_auth_for_url("http://ntfy_app_1", None));
        assert!(!config
            .should_use_ntfy_auth_for_url("https://ntfy.sh", Some("https://ntfy.example.com")));
        assert!(!config.should_use_ntfy_auth_for_url("https://ntfy.sh", Some("")));
    }

    #[test]
    fn test_ntfy_auth_allowed_for_matching_configured_url_in_cloud_mode() {
        let config = test_config(NetworkConfig::Mainnet);

        assert!(config.should_use_ntfy_auth_for_url("https://ntfy.sh", Some("https://ntfy.sh")));
    }

    #[test]
    fn test_cloud_mode_ignores_detected_ntfy_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        let previous_ntfy_server_url = std::env::var("NTFY_SERVER_URL").ok();

        std::env::remove_var("NTFY_SERVER_URL");
        let config =
            test_config(NetworkConfig::Regtest).with_ntfy_servers(vec![NtfyServerConfig::new(
                "umbrel-ntfy",
                "ntfy",
                Some("http://ntfy_app_1".to_string()),
                Some("umbrel"),
            )
            .unwrap()]);

        assert_eq!(config.default_ntfy_server_id(), "ntfy-sh");
        assert_eq!(config.ntfy_server_url(), "https://ntfy.sh");

        restore_env_var("NTFY_SERVER_URL", previous_ntfy_server_url);
    }

    #[test]
    fn test_get_jwt_secret_cloud_mode_with_secret() {
        let config = test_config(NetworkConfig::Regtest);
        assert_eq!(config.get_jwt_secret().unwrap(), "test-jwt-secret");
    }

    #[test]
    fn test_get_jwt_secret_cloud_mode_without_secret() {
        let config = AppConfig {
            network: NetworkConfig::Regtest,
            electrum_url: None,
            bind_address: "127.0.0.1:3000".to_string(),
            data_dir: "./database".to_string(),
            operating_mode: OperatingMode::Cloud,
            frontend_url: Some("http://localhost:3001".to_string()),
            jwt_secret: None, // Missing JWT secret
            self_hosted_admin_password: None,
            tx_explorers: Vec::new(),
            ntfy_servers: Vec::new(),
            ntfy_fallback_url: "https://ntfy.sh".to_string(),
            btcpay_url: None,
            btcpay_api_key: None,
            btcpay_store_id: None,
            btcpay_offering_id: None,
            btcpay_plan_id: None,
        };
        assert_eq!(
            config.get_jwt_secret().unwrap_err(),
            "JWT_SECRET required for cloud mode - check your .env file"
        );
    }

    #[test]
    fn test_get_jwt_secret_self_hosted_mode() {
        let config = test_config_self_hosted(NetworkConfig::Regtest);
        assert_eq!(
            config.get_jwt_secret().unwrap(),
            "test-self-hosted-jwt-secret"
        );
    }

    #[test]
    fn test_get_self_hosted_admin_password_self_hosted_mode() {
        let config = test_config_self_hosted(NetworkConfig::Regtest);
        assert_eq!(
            config.get_self_hosted_admin_password().unwrap(),
            "self-hosted-password"
        );
    }

    #[test]
    fn test_get_self_hosted_admin_password_cloud_mode() {
        let config = test_config(NetworkConfig::Regtest);
        assert_eq!(
            config.get_self_hosted_admin_password().unwrap_err(),
            "Self-hosted admin password is only available in self-hosted mode"
        );
    }

    #[test]
    fn test_get_jwt_secret_rejects_blank_self_hosted_secret() {
        let config = AppConfig {
            network: NetworkConfig::Regtest,
            electrum_url: None,
            bind_address: "127.0.0.1:3000".to_string(),
            data_dir: "./database".to_string(),
            operating_mode: OperatingMode::SelfHosted,
            frontend_url: None,
            jwt_secret: Some("   ".to_string()),
            self_hosted_admin_password: Some("self-hosted-password".to_string()),
            tx_explorers: Vec::new(),
            ntfy_servers: Vec::new(),
            ntfy_fallback_url: "https://ntfy.sh".to_string(),
            btcpay_url: None,
            btcpay_api_key: None,
            btcpay_store_id: None,
            btcpay_offering_id: None,
            btcpay_plan_id: None,
        };

        assert_eq!(
            config.get_jwt_secret().unwrap_err(),
            "JWT_SECRET required for self-hosted mode - check your .env file"
        );
    }

    #[test]
    fn test_get_self_hosted_admin_password_rejects_blank_password() {
        let config = AppConfig {
            network: NetworkConfig::Regtest,
            electrum_url: None,
            bind_address: "127.0.0.1:3000".to_string(),
            data_dir: "./database".to_string(),
            operating_mode: OperatingMode::SelfHosted,
            frontend_url: None,
            jwt_secret: Some("test-self-hosted-jwt-secret".to_string()),
            self_hosted_admin_password: Some("   ".to_string()),
            tx_explorers: Vec::new(),
            ntfy_servers: Vec::new(),
            ntfy_fallback_url: "https://ntfy.sh".to_string(),
            btcpay_url: None,
            btcpay_api_key: None,
            btcpay_store_id: None,
            btcpay_offering_id: None,
            btcpay_plan_id: None,
        };

        assert_eq!(
            config.get_self_hosted_admin_password().unwrap_err(),
            "CANARY_SELF_HOSTED_ADMIN_PASSWORD required for self-hosted mode - check your .env file"
        );
    }
}
