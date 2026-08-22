use anyhow::{anyhow, Result};
use bdk_wallet::bitcoin::Network;
use clap::{Parser, ValueEnum};
use serde::Serialize;
use std::str::FromStr;

use crate::ntfy_provider::NtfyAuth;

pub const PUBLIC_NTFY_SERVER_ID: &str = "ntfy-sh";
pub const STARTOS_NTFY_SERVER_ID: &str = "startos-ntfy";
pub const UMBREL_NTFY_SERVER_ID: &str = "umbrel-ntfy";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BillingProvider {
    Stripe,
    BtcPay,
}

impl BillingProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            BillingProvider::Stripe => "stripe",
            BillingProvider::BtcPay => "btcpay",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BtcPayPlanConfig {
    pub offering_id: String,
    pub personal_plan_id: String,
    pub team_plan_id: String,
    pub currency: String,
    pub personal_monthly_price: i64,
    pub team_monthly_price: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TxExplorerConfig {
    pub id: String,
    pub name: String,
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub base_urls: Vec<String>,
    pub port: Option<u16>,
    pub platform: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NtfyServerConfig {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub platform: Option<String>,
    pub default_topic: Option<String>,
    pub managed_auth: bool,
}

#[derive(Clone, PartialEq, Eq)]
struct ManagedNtfyAccessToken(String);

impl std::fmt::Debug for ManagedNtfyAccessToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ManagedNtfyAccessToken([redacted])")
    }
}

impl NtfyServerConfig {
    fn new(id: &str, name: &str, base_url: Option<String>, platform: Option<&str>) -> Option<Self> {
        Self::new_with_defaults(id, name, base_url, platform, None, false)
    }

    fn new_with_defaults(
        id: &str,
        name: &str,
        base_url: Option<String>,
        platform: Option<&str>,
        default_topic: Option<String>,
        managed_auth: bool,
    ) -> Option<Self> {
        let normalized_base_url = base_url
            .map(|url| url.trim().trim_end_matches('/').to_string())
            .filter(|url| !url.is_empty());

        normalized_base_url.map(|base_url| Self {
            id: id.to_string(),
            name: name.to_string(),
            base_url,
            platform: platform.map(str::to_string),
            default_topic,
            managed_auth,
        })
    }
}

impl TxExplorerConfig {
    fn new(
        id: &str,
        name: &str,
        base_url: Option<String>,
        base_urls: Vec<String>,
        port: Option<u16>,
        platform: Option<&str>,
    ) -> Option<Self> {
        let normalized_base_url = base_url
            .map(|url| url.trim().trim_end_matches('/').to_string())
            .filter(|url| !url.is_empty());
        let normalized_platform = platform
            .map(str::trim)
            .filter(|platform| !platform.is_empty())
            .map(str::to_string);
        let mut normalized_base_urls = Vec::new();

        for url in normalized_base_url.iter().chain(base_urls.iter()) {
            let normalized_url = url.trim().trim_end_matches('/').to_string();
            if !normalized_url.is_empty() && !normalized_base_urls.contains(&normalized_url) {
                normalized_base_urls.push(normalized_url);
            }
        }

        if normalized_base_url.is_none() && normalized_base_urls.is_empty() && port.is_none() {
            return None;
        }

        Some(Self {
            id: id.to_string(),
            name: name.to_string(),
            base_url: normalized_base_url,
            base_urls: normalized_base_urls,
            port,
            platform: normalized_platform,
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
    fn normalize_url_env_value(var_name: &str, url: &str) -> Option<String> {
        let trimmed_url = url.trim();
        if trimmed_url.is_empty() {
            tracing::warn!("{} contains a blank URL and it will be ignored", var_name);
            None
        } else if trimmed_url.starts_with("http://") || trimmed_url.starts_with("https://") {
            Some(trimmed_url.trim_end_matches('/').to_string())
        } else {
            tracing::warn!(
                "{} contains a URL that does not start with http:// or https://: '{}' - ignoring",
                var_name,
                url
            );
            None
        }
    }

    fn parse_url_env(var_name: &str) -> Option<String> {
        std::env::var(var_name)
            .ok()
            .and_then(|url| Self::normalize_url_env_value(var_name, &url))
    }

    fn parse_url_list_env(var_name: &str) -> Vec<String> {
        let mut urls = Vec::new();

        if let Ok(raw_urls) = std::env::var(var_name) {
            for raw_url in raw_urls.split(',') {
                if let Some(url) = Self::normalize_url_env_value(var_name, raw_url) {
                    if !urls.contains(&url) {
                        urls.push(url);
                    }
                }
            }
        }

        urls
    }

    fn parse_non_empty_env(var_name: &str) -> Option<String> {
        std::env::var(var_name)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }

    fn parse_ntfy_topic_env(var_name: &str) -> Option<String> {
        let topic = Self::parse_non_empty_env(var_name)?;
        let is_valid_topic = topic.len() <= 64
            && topic
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');

        if is_valid_topic {
            Some(topic)
        } else {
            tracing::warn!(
                "{} contains an invalid ntfy topic and will be ignored",
                var_name
            );
            None
        }
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

    fn non_empty_env_var(key: &str) -> Option<String> {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
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

        // Load the canonical frontend URL plus any additional browser origins.
        let frontend_url = std::env::var("FRONTEND_URL").ok();
        let frontend_urls = Self::parse_url_list_env("FRONTEND_URLS");

        // Load authentication configuration
        let jwt_secret = std::env::var("JWT_SECRET").ok();
        let self_hosted_admin_password = std::env::var("CANARY_SELF_HOSTED_ADMIN_PASSWORD").ok();

        // Load self-hosted tx explorer configuration (optional)
        let mempool_url = Self::parse_url_env("CANARY_MEMPOOL_URL");
        let mempool_urls = Self::parse_url_list_env("CANARY_MEMPOOL_URLS");
        let mempool_port = std::env::var("CANARY_MEMPOOL_PORT")
            .ok()
            .and_then(|s| s.parse().ok());
        let bitfeed_url = Self::parse_url_env("CANARY_BITFEED_URL");
        let bitfeed_urls = Self::parse_url_list_env("CANARY_BITFEED_URLS");
        let bitfeed_port = std::env::var("CANARY_BITFEED_PORT")
            .ok()
            .and_then(|s| s.parse().ok());
        let btc_rpc_explorer_url = Self::parse_url_env("CANARY_BTC_RPC_EXPLORER_URL");
        let btc_rpc_explorer_urls = Self::parse_url_list_env("CANARY_BTC_RPC_EXPLORER_URLS");
        let btc_rpc_explorer_port = std::env::var("CANARY_BTC_RPC_EXPLORER_PORT")
            .ok()
            .and_then(|s| s.parse().ok());
        let tx_explorer_platform = std::env::var("CANARY_TX_EXPLORER_PLATFORM").ok();
        if let Some(platform) = tx_explorer_platform
            .as_deref()
            .map(str::trim)
            .filter(|platform| !platform.is_empty())
        {
            if !matches!(platform, "mynode" | "umbrel" | "startos") {
                tracing::warn!(
                    "CANARY_TX_EXPLORER_PLATFORM contains an unrecognized platform '{}' - local explorer settings will fall back to the generic local label",
                    platform
                );
            }
        }
        let tx_explorers = [
            TxExplorerConfig::new(
                "mempool",
                "Mempool",
                mempool_url,
                mempool_urls,
                mempool_port,
                tx_explorer_platform.as_deref(),
            ),
            TxExplorerConfig::new(
                "bitfeed",
                "Bitfeed",
                bitfeed_url,
                bitfeed_urls,
                bitfeed_port,
                tx_explorer_platform.as_deref(),
            ),
            TxExplorerConfig::new(
                "btc-rpc-explorer",
                "BTC RPC Explorer",
                btc_rpc_explorer_url,
                btc_rpc_explorer_urls,
                btc_rpc_explorer_port,
                tx_explorer_platform.as_deref(),
            ),
        ]
        .into_iter()
        .flatten()
        .collect();

        let startos_ntfy_url = Self::parse_url_env("CANARY_NTFY_SERVER_URL");
        let startos_ntfy_token = Self::parse_non_empty_env("CANARY_NTFY_TOKEN");
        let startos_ntfy_topic = Self::parse_ntfy_topic_env("CANARY_NTFY_TOPIC");
        let umbrel_ntfy_url = Self::parse_url_env("CANARY_UMBREL_NTFY_URL");
        let configured_ntfy_fallback_url = Self::parse_url_env("NTFY_SERVER_URL");
        if operating_mode == OperatingMode::Cloud
            && (startos_ntfy_url.is_some()
                || startos_ntfy_token.is_some()
                || startos_ntfy_topic.is_some())
        {
            tracing::warn!(
                "CANARY_NTFY_* variables are only used in self-hosted mode; ignoring provisioned ntfy defaults in cloud mode"
            );
        }
        if operating_mode == OperatingMode::Cloud && umbrel_ntfy_url.is_some() {
            tracing::warn!(
                "CANARY_UMBREL_NTFY_URL is only used in self-hosted mode; ignoring detected ntfy server in cloud mode"
            );
        }
        if operating_mode == OperatingMode::SelfHosted
            && startos_ntfy_url.is_none()
            && (startos_ntfy_token.is_some() || startos_ntfy_topic.is_some())
        {
            tracing::warn!(
                "CANARY_NTFY_TOKEN or CANARY_NTFY_TOPIC is set without CANARY_NTFY_SERVER_URL; provisioned ntfy defaults will be ignored"
            );
        }
        if operating_mode == OperatingMode::SelfHosted
            && startos_ntfy_url.is_some()
            && startos_ntfy_token.is_none()
        {
            tracing::warn!(
                "CANARY_NTFY_SERVER_URL is set without CANARY_NTFY_TOKEN; provisioned ntfy server will be available without managed auth"
            );
        }
        if operating_mode == OperatingMode::SelfHosted
            && startos_ntfy_url.is_some()
            && umbrel_ntfy_url.is_some()
        {
            tracing::warn!(
                "Both CANARY_NTFY_SERVER_URL and CANARY_UMBREL_NTFY_URL are set; multiple detected ntfy servers will be listed and no local ntfy default will be selected automatically"
            );
        }
        if operating_mode == OperatingMode::SelfHosted && configured_ntfy_fallback_url.is_some() {
            if startos_ntfy_url.is_some() {
                tracing::warn!(
                    "CANARY_NTFY_SERVER_URL is set; detected StartOS ntfy server will take precedence over NTFY_SERVER_URL"
                );
            }
            if umbrel_ntfy_url.is_some() {
                tracing::warn!(
                    "CANARY_UMBREL_NTFY_URL is set; detected Umbrel ntfy server will take precedence over NTFY_SERVER_URL"
                );
            }
        }
        let has_managed_startos_auth = startos_ntfy_url.is_some() && startos_ntfy_token.is_some();
        let ntfy_servers = if operating_mode == OperatingMode::SelfHosted {
            [
                NtfyServerConfig::new_with_defaults(
                    STARTOS_NTFY_SERVER_ID,
                    "ntfy",
                    startos_ntfy_url,
                    Some("startos"),
                    startos_ntfy_topic,
                    has_managed_startos_auth,
                ),
                NtfyServerConfig::new(
                    UMBREL_NTFY_SERVER_ID,
                    "ntfy",
                    umbrel_ntfy_url,
                    Some("umbrel"),
                ),
            ]
            .into_iter()
            .flatten()
            .collect()
        } else {
            Vec::new()
        };
        let managed_ntfy_access_token =
            if operating_mode == OperatingMode::SelfHosted && has_managed_startos_auth {
                startos_ntfy_token.map(ManagedNtfyAccessToken)
            } else {
                None
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
            frontend_urls,
            jwt_secret,
            self_hosted_admin_password,
            tx_explorers,
            ntfy_servers,
            ntfy_fallback_url,
            managed_ntfy_access_token,
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

    /// Get every configured HTTP(S) frontend origin used for CORS and browser
    /// request-origin validation. `FRONTEND_URL` remains the canonical URL for
    /// links, while `FRONTEND_URLS` can add alternate origins exposed by node
    /// platforms such as StartOS.
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
        self.btcpay_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
            && self
                .btcpay_api_key
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_some()
            && self
                .btcpay_store_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_some()
    }

    /// Check if Stripe billing is configured
    pub fn is_stripe_enabled(&self) -> bool {
        Self::non_empty_env_var("STRIPE_SECRET_KEY").is_some()
            && Self::non_empty_env_var("STRIPE_WEBHOOK_SECRET").is_some()
    }

    /// Determine which cloud billing provider should be used.
    /// Stripe wins if both are configured so existing deployments stay unchanged.
    pub fn active_billing_provider(&self) -> Option<BillingProvider> {
        if self.is_stripe_enabled() {
            Some(BillingProvider::Stripe)
        } else if self.btcpay_cloud_plan_config().is_some() {
            Some(BillingProvider::BtcPay)
        } else {
            None
        }
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

    /// Secret used to authenticate BTCPay webhook deliveries.
    pub fn btcpay_webhook_secret(&self) -> Option<String> {
        Self::non_empty_env_var("BTCPAY_WEBHOOK_SECRET")
    }

    pub fn btcpay_cloud_plan_config(&self) -> Option<BtcPayPlanConfig> {
        if !self.is_btcpay_enabled() {
            return None;
        }

        let offering_id = Self::non_empty_env_var("BTCPAY_CLOUD_OFFERING_ID").or_else(|| {
            self.btcpay_offering_id()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })?;
        let personal_plan_id = Self::non_empty_env_var("BTCPAY_CLOUD_PERSONAL_PLAN_ID")?;
        let team_plan_id = Self::non_empty_env_var("BTCPAY_CLOUD_TEAM_PLAN_ID")?;
        let currency = Self::non_empty_env_var("BTCPAY_CLOUD_CURRENCY")
            .unwrap_or_else(|| "USD".to_string())
            .to_uppercase();
        let personal_monthly_price = Self::non_empty_env_var("BTCPAY_CLOUD_PERSONAL_PRICE")
            .and_then(|value| i64::from_str(&value).ok())
            .filter(|value| *value > 0)?;
        let team_monthly_price = Self::non_empty_env_var("BTCPAY_CLOUD_TEAM_PRICE")
            .and_then(|value| i64::from_str(&value).ok())
            .filter(|value| *value > 0)?;

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
        // ntfy is always available. Self-hosted mode may also register local-only providers.
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
            .map(NtfyAuth::AccessToken)
            .unwrap_or(NtfyAuth::None)
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
        if std::env::var("JWT_SECRET").is_err() {
            missing.push("JWT_SECRET - Required for user authentication");
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

        // Frontend URL is required for email links and browser trust boundaries.
        if self.frontend_origin().is_none() {
            missing
                .push("FRONTEND_URL - Must be an HTTP(S) origin for email links and CORS security");
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

    /// Get sync interval based on mode and configuration.
    /// Self-hosted mode treats CANARY_SYNC_INTERVAL as a per-wallet freshness target and uses
    /// network defaults when it is absent.
    /// Cloud mode: Delegates to subscription tier logic
    pub fn get_sync_interval(&self) -> u64 {
        let sync_interval = std::env::var("CANARY_SYNC_INTERVAL").ok();
        self.resolve_sync_interval(sync_interval.as_deref())
    }

    fn resolve_sync_interval(&self, sync_interval: Option<&str>) -> u64 {
        if self.is_self_hosted_mode() {
            // Self-hosted mode: Use legacy CANARY_SYNC_INTERVAL or network-based defaults
            sync_interval
                .and_then(|value| value.parse().ok())
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
        }
    }

    /// Set tx explorers on a test config (builder pattern)
    pub fn with_tx_explorers(mut self, tx_explorers: Vec<TxExplorerConfig>) -> Self {
        self.tx_explorers = tx_explorers;
        self
    }

    /// Add alternate browser URLs to a test configuration.
    pub fn with_frontend_urls(mut self, frontend_urls: Vec<String>) -> Self {
        self.frontend_urls = frontend_urls;
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

    /// Set package-managed ntfy access token on a test config (builder pattern)
    #[cfg(test)]
    pub fn with_managed_ntfy_access_token(mut self, token: &str) -> Self {
        self.managed_ntfy_access_token = Some(ManagedNtfyAccessToken(token.to_string()));
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
    fn frontend_origin_accepts_only_http_urls_with_hosts() {
        let config = AppConfig::new_for_test(
            NetworkConfig::Regtest,
            None,
            "127.0.0.1:3000".to_string(),
            "./database".to_string(),
            OperatingMode::SelfHosted,
            Some("https://canary.example/settings".to_string()),
            None,
        );
        assert_eq!(
            config.frontend_origin().as_deref(),
            Some("https://canary.example")
        );

        for frontend_url in ["file:///tmp/canary", "data:text/html,canary", "https://"] {
            let config = AppConfig::new_for_test(
                NetworkConfig::Regtest,
                None,
                "127.0.0.1:3000".to_string(),
                "./database".to_string(),
                OperatingMode::SelfHosted,
                Some(frontend_url.to_string()),
                None,
            );
            assert!(config.frontend_origin().is_none(), "{frontend_url}");
        }
    }

    #[test]
    fn frontend_origins_include_canonical_and_additional_urls() {
        let config = AppConfig::new_for_test(
            NetworkConfig::Regtest,
            None,
            "127.0.0.1:3000".to_string(),
            "./database".to_string(),
            OperatingMode::SelfHosted,
            Some("https://canary.local:443/settings".to_string()),
            None,
        )
        .with_frontend_urls(vec![
            "http://192.168.1.10:3001/wallets".to_string(),
            "https://canary.local/settings".to_string(),
            "file:///tmp/canary".to_string(),
        ]);

        assert_eq!(
            config.frontend_origins(),
            vec![
                "https://canary.local".to_string(),
                "http://192.168.1.10:3001".to_string(),
            ]
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
            frontend_urls: Vec::new(),
            jwt_secret: Some("test-jwt-secret".to_string()),
            self_hosted_admin_password: None,
            tx_explorers: Vec::new(),
            ntfy_servers: Vec::new(),
            ntfy_fallback_url: "https://ntfy.sh".to_string(),
            managed_ntfy_access_token: None,
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
            frontend_urls: Vec::new(),
            jwt_secret: Some("test-jwt-secret".to_string()),
            self_hosted_admin_password: None,
            tx_explorers: Vec::new(),
            ntfy_servers: Vec::new(),
            ntfy_fallback_url: "https://ntfy.sh".to_string(),
            managed_ntfy_access_token: None,
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
            frontend_urls: Vec::new(),
            jwt_secret: Some("test-self-hosted-jwt-secret".to_string()),
            self_hosted_admin_password: Some("self-hosted-password".to_string()),
            tx_explorers: Vec::new(),
            ntfy_servers: Vec::new(),
            ntfy_fallback_url: "https://ntfy.sh".to_string(),
            managed_ntfy_access_token: None,
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
        let config = test_config_self_hosted(NetworkConfig::Mainnet);
        assert_eq!(config.resolve_sync_interval(Some("42")), 42);
    }

    #[test]
    fn test_self_hosted_mode_sync_interval_network_defaults() {
        let regtest_config = test_config_self_hosted(NetworkConfig::Regtest);
        assert_eq!(regtest_config.resolve_sync_interval(None), 30);

        let mainnet_config = test_config_self_hosted(NetworkConfig::Mainnet);
        assert_eq!(mainnet_config.resolve_sync_interval(None), 300);
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
            frontend_urls: Vec::new(),
            jwt_secret: Some("test-jwt-secret".to_string()),
            self_hosted_admin_password: None,
            tx_explorers: Vec::new(),
            ntfy_servers: Vec::new(),
            ntfy_fallback_url: "https://ntfy.sh".to_string(),
            managed_ntfy_access_token: None,
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
    fn test_load_detects_startos_managed_ntfy_defaults_in_self_hosted_mode() {
        let _guard = ENV_LOCK.lock().unwrap();
        let previous_mode = std::env::var("CANARY_MODE").ok();
        let previous_jwt = std::env::var("JWT_SECRET").ok();
        let previous_admin_password = std::env::var("CANARY_SELF_HOSTED_ADMIN_PASSWORD").ok();
        let previous_server_url = std::env::var("CANARY_NTFY_SERVER_URL").ok();
        let previous_token = std::env::var("CANARY_NTFY_TOKEN").ok();
        let previous_topic = std::env::var("CANARY_NTFY_TOPIC").ok();
        let previous_umbrel_ntfy_url = std::env::var("CANARY_UMBREL_NTFY_URL").ok();
        let previous_ntfy_server_url = std::env::var("NTFY_SERVER_URL").ok();

        std::env::set_var("CANARY_MODE", "self-hosted");
        std::env::set_var("JWT_SECRET", "test-jwt-secret");
        std::env::set_var("CANARY_SELF_HOSTED_ADMIN_PASSWORD", "test-admin-password");
        std::env::set_var("CANARY_NTFY_SERVER_URL", "http://ntfy.startos/");
        std::env::set_var("CANARY_NTFY_TOKEN", " tk_test ");
        std::env::set_var("CANARY_NTFY_TOPIC", "canary");
        std::env::remove_var("CANARY_UMBREL_NTFY_URL");
        std::env::remove_var("NTFY_SERVER_URL");

        let config = AppConfig::load_from_args(AppConfigArgs {
            network: Some(NetworkConfig::Mainnet),
            electrum_url: None,
            bind_address: Some("127.0.0.1:3000".to_string()),
            data_dir: Some("./database".to_string()),
        })
        .expect("self-hosted config should load");

        assert_eq!(config.default_ntfy_server_id(), STARTOS_NTFY_SERVER_ID);
        assert_eq!(config.ntfy_server_url(), "http://ntfy.startos");
        assert_eq!(config.ntfy_servers().len(), 1);
        assert_eq!(
            config.ntfy_servers()[0].platform.as_deref(),
            Some("startos")
        );
        assert_eq!(
            config.ntfy_servers()[0].default_topic.as_deref(),
            Some("canary")
        );
        assert!(config.ntfy_servers()[0].managed_auth);
        assert_eq!(
            config
                .managed_ntfy_access_token_for_url("http://ntfy.startos", None)
                .as_deref(),
            Some("tk_test")
        );
        assert_eq!(
            config.managed_ntfy_access_token_for_url(
                "http://ntfy.startos",
                Some("http://ntfy.startos")
            ),
            None
        );
        assert_eq!(
            config.managed_ntfy_access_token_for_url("https://ntfy.sh", None),
            None
        );

        restore_env_var("CANARY_MODE", previous_mode);
        restore_env_var("JWT_SECRET", previous_jwt);
        restore_env_var("CANARY_SELF_HOSTED_ADMIN_PASSWORD", previous_admin_password);
        restore_env_var("CANARY_NTFY_SERVER_URL", previous_server_url);
        restore_env_var("CANARY_NTFY_TOKEN", previous_token);
        restore_env_var("CANARY_NTFY_TOPIC", previous_topic);
        restore_env_var("CANARY_UMBREL_NTFY_URL", previous_umbrel_ntfy_url);
        restore_env_var("NTFY_SERVER_URL", previous_ntfy_server_url);
    }

    #[test]
    fn test_load_ignores_startos_ntfy_token_without_server_url() {
        let _guard = ENV_LOCK.lock().unwrap();
        let previous_mode = std::env::var("CANARY_MODE").ok();
        let previous_jwt = std::env::var("JWT_SECRET").ok();
        let previous_admin_password = std::env::var("CANARY_SELF_HOSTED_ADMIN_PASSWORD").ok();
        let previous_server_url = std::env::var("CANARY_NTFY_SERVER_URL").ok();
        let previous_token = std::env::var("CANARY_NTFY_TOKEN").ok();
        let previous_topic = std::env::var("CANARY_NTFY_TOPIC").ok();
        let previous_umbrel_ntfy_url = std::env::var("CANARY_UMBREL_NTFY_URL").ok();
        let previous_ntfy_server_url = std::env::var("NTFY_SERVER_URL").ok();

        std::env::set_var("CANARY_MODE", "self-hosted");
        std::env::set_var("JWT_SECRET", "test-jwt-secret");
        std::env::set_var("CANARY_SELF_HOSTED_ADMIN_PASSWORD", "test-admin-password");
        std::env::remove_var("CANARY_NTFY_SERVER_URL");
        std::env::set_var("CANARY_NTFY_TOKEN", "tk_without_url");
        std::env::set_var("CANARY_NTFY_TOPIC", "canary");
        std::env::remove_var("CANARY_UMBREL_NTFY_URL");
        std::env::remove_var("NTFY_SERVER_URL");

        let config = AppConfig::load_from_args(AppConfigArgs {
            network: Some(NetworkConfig::Mainnet),
            electrum_url: None,
            bind_address: Some("127.0.0.1:3000".to_string()),
            data_dir: Some("./database".to_string()),
        })
        .expect("self-hosted config should load");

        assert_eq!(config.default_ntfy_server_id(), PUBLIC_NTFY_SERVER_ID);
        assert_eq!(config.ntfy_server_url(), "https://ntfy.sh");
        assert!(config.ntfy_servers().is_empty());
        assert_eq!(
            config.managed_ntfy_access_token_for_url("http://ntfy.startos", None),
            None
        );

        restore_env_var("CANARY_MODE", previous_mode);
        restore_env_var("JWT_SECRET", previous_jwt);
        restore_env_var("CANARY_SELF_HOSTED_ADMIN_PASSWORD", previous_admin_password);
        restore_env_var("CANARY_NTFY_SERVER_URL", previous_server_url);
        restore_env_var("CANARY_NTFY_TOKEN", previous_token);
        restore_env_var("CANARY_NTFY_TOPIC", previous_topic);
        restore_env_var("CANARY_UMBREL_NTFY_URL", previous_umbrel_ntfy_url);
        restore_env_var("NTFY_SERVER_URL", previous_ntfy_server_url);
    }

    #[test]
    fn test_load_ignores_invalid_startos_ntfy_topic() {
        let _guard = ENV_LOCK.lock().unwrap();
        let previous_mode = std::env::var("CANARY_MODE").ok();
        let previous_jwt = std::env::var("JWT_SECRET").ok();
        let previous_admin_password = std::env::var("CANARY_SELF_HOSTED_ADMIN_PASSWORD").ok();
        let previous_server_url = std::env::var("CANARY_NTFY_SERVER_URL").ok();
        let previous_token = std::env::var("CANARY_NTFY_TOKEN").ok();
        let previous_topic = std::env::var("CANARY_NTFY_TOPIC").ok();
        let previous_umbrel_ntfy_url = std::env::var("CANARY_UMBREL_NTFY_URL").ok();
        let previous_ntfy_server_url = std::env::var("NTFY_SERVER_URL").ok();

        std::env::set_var("CANARY_MODE", "self-hosted");
        std::env::set_var("JWT_SECRET", "test-jwt-secret");
        std::env::set_var("CANARY_SELF_HOSTED_ADMIN_PASSWORD", "test-admin-password");
        std::env::set_var("CANARY_NTFY_SERVER_URL", "http://ntfy.startos/");
        std::env::set_var("CANARY_NTFY_TOKEN", "tk_test");
        std::env::set_var("CANARY_NTFY_TOPIC", "canary/topic");
        std::env::remove_var("CANARY_UMBREL_NTFY_URL");
        std::env::remove_var("NTFY_SERVER_URL");

        let config = AppConfig::load_from_args(AppConfigArgs {
            network: Some(NetworkConfig::Mainnet),
            electrum_url: None,
            bind_address: Some("127.0.0.1:3000".to_string()),
            data_dir: Some("./database".to_string()),
        })
        .expect("self-hosted config should load");

        assert_eq!(config.default_ntfy_server_id(), STARTOS_NTFY_SERVER_ID);
        assert_eq!(config.ntfy_servers().len(), 1);
        assert_eq!(config.ntfy_servers()[0].default_topic, None);
        assert!(config.ntfy_servers()[0].managed_auth);

        restore_env_var("CANARY_MODE", previous_mode);
        restore_env_var("JWT_SECRET", previous_jwt);
        restore_env_var("CANARY_SELF_HOSTED_ADMIN_PASSWORD", previous_admin_password);
        restore_env_var("CANARY_NTFY_SERVER_URL", previous_server_url);
        restore_env_var("CANARY_NTFY_TOKEN", previous_token);
        restore_env_var("CANARY_NTFY_TOPIC", previous_topic);
        restore_env_var("CANARY_UMBREL_NTFY_URL", previous_umbrel_ntfy_url);
        restore_env_var("NTFY_SERVER_URL", previous_ntfy_server_url);
    }

    #[test]
    fn test_load_registers_startos_ntfy_url_without_managed_auth() {
        let _guard = ENV_LOCK.lock().unwrap();
        let previous_mode = std::env::var("CANARY_MODE").ok();
        let previous_jwt = std::env::var("JWT_SECRET").ok();
        let previous_admin_password = std::env::var("CANARY_SELF_HOSTED_ADMIN_PASSWORD").ok();
        let previous_server_url = std::env::var("CANARY_NTFY_SERVER_URL").ok();
        let previous_token = std::env::var("CANARY_NTFY_TOKEN").ok();
        let previous_topic = std::env::var("CANARY_NTFY_TOPIC").ok();
        let previous_umbrel_ntfy_url = std::env::var("CANARY_UMBREL_NTFY_URL").ok();
        let previous_ntfy_server_url = std::env::var("NTFY_SERVER_URL").ok();

        std::env::set_var("CANARY_MODE", "self-hosted");
        std::env::set_var("JWT_SECRET", "test-jwt-secret");
        std::env::set_var("CANARY_SELF_HOSTED_ADMIN_PASSWORD", "test-admin-password");
        std::env::set_var("CANARY_NTFY_SERVER_URL", "http://ntfy.startos/");
        std::env::remove_var("CANARY_NTFY_TOKEN");
        std::env::set_var("CANARY_NTFY_TOPIC", "canary");
        std::env::remove_var("CANARY_UMBREL_NTFY_URL");
        std::env::remove_var("NTFY_SERVER_URL");

        let config = AppConfig::load_from_args(AppConfigArgs {
            network: Some(NetworkConfig::Mainnet),
            electrum_url: None,
            bind_address: Some("127.0.0.1:3000".to_string()),
            data_dir: Some("./database".to_string()),
        })
        .expect("self-hosted config should load");

        assert_eq!(config.default_ntfy_server_id(), STARTOS_NTFY_SERVER_ID);
        assert_eq!(config.ntfy_servers().len(), 1);
        assert_eq!(config.ntfy_servers()[0].base_url, "http://ntfy.startos");
        assert_eq!(
            config.ntfy_servers()[0].default_topic.as_deref(),
            Some("canary")
        );
        assert!(!config.ntfy_servers()[0].managed_auth);
        assert_eq!(
            config.managed_ntfy_access_token_for_url("http://ntfy.startos", None),
            None
        );

        restore_env_var("CANARY_MODE", previous_mode);
        restore_env_var("JWT_SECRET", previous_jwt);
        restore_env_var("CANARY_SELF_HOSTED_ADMIN_PASSWORD", previous_admin_password);
        restore_env_var("CANARY_NTFY_SERVER_URL", previous_server_url);
        restore_env_var("CANARY_NTFY_TOKEN", previous_token);
        restore_env_var("CANARY_NTFY_TOPIC", previous_topic);
        restore_env_var("CANARY_UMBREL_NTFY_URL", previous_umbrel_ntfy_url);
        restore_env_var("NTFY_SERVER_URL", previous_ntfy_server_url);
    }

    #[test]
    fn test_load_ignores_startos_ntfy_defaults_in_cloud_mode() {
        let _guard = ENV_LOCK.lock().unwrap();
        let previous_mode = std::env::var("CANARY_MODE").ok();
        let previous_server_url = std::env::var("CANARY_NTFY_SERVER_URL").ok();
        let previous_token = std::env::var("CANARY_NTFY_TOKEN").ok();
        let previous_topic = std::env::var("CANARY_NTFY_TOPIC").ok();
        let previous_umbrel_ntfy_url = std::env::var("CANARY_UMBREL_NTFY_URL").ok();
        let previous_ntfy_server_url = std::env::var("NTFY_SERVER_URL").ok();

        std::env::set_var("CANARY_MODE", "cloud");
        std::env::set_var("CANARY_NTFY_SERVER_URL", "http://ntfy.startos/");
        std::env::set_var("CANARY_NTFY_TOKEN", "tk_test");
        std::env::set_var("CANARY_NTFY_TOPIC", "canary");
        std::env::remove_var("CANARY_UMBREL_NTFY_URL");
        std::env::remove_var("NTFY_SERVER_URL");

        let config = AppConfig::load_from_args(AppConfigArgs {
            network: Some(NetworkConfig::Mainnet),
            electrum_url: None,
            bind_address: Some("127.0.0.1:3000".to_string()),
            data_dir: Some("./database".to_string()),
        })
        .expect("cloud config should load");

        assert_eq!(config.default_ntfy_server_id(), PUBLIC_NTFY_SERVER_ID);
        assert_eq!(config.ntfy_server_url(), "https://ntfy.sh");
        assert!(config.ntfy_servers().is_empty());
        assert_eq!(
            config.managed_ntfy_access_token_for_url("http://ntfy.startos", None),
            None
        );

        restore_env_var("CANARY_MODE", previous_mode);
        restore_env_var("CANARY_NTFY_SERVER_URL", previous_server_url);
        restore_env_var("CANARY_NTFY_TOKEN", previous_token);
        restore_env_var("CANARY_NTFY_TOPIC", previous_topic);
        restore_env_var("CANARY_UMBREL_NTFY_URL", previous_umbrel_ntfy_url);
        restore_env_var("NTFY_SERVER_URL", previous_ntfy_server_url);
    }

    #[test]
    fn test_load_does_not_guess_default_when_multiple_local_ntfy_servers_exist() {
        let _guard = ENV_LOCK.lock().unwrap();
        let previous_mode = std::env::var("CANARY_MODE").ok();
        let previous_jwt = std::env::var("JWT_SECRET").ok();
        let previous_admin_password = std::env::var("CANARY_SELF_HOSTED_ADMIN_PASSWORD").ok();
        let previous_server_url = std::env::var("CANARY_NTFY_SERVER_URL").ok();
        let previous_token = std::env::var("CANARY_NTFY_TOKEN").ok();
        let previous_topic = std::env::var("CANARY_NTFY_TOPIC").ok();
        let previous_umbrel_ntfy_url = std::env::var("CANARY_UMBREL_NTFY_URL").ok();
        let previous_ntfy_server_url = std::env::var("NTFY_SERVER_URL").ok();

        std::env::set_var("CANARY_MODE", "self-hosted");
        std::env::set_var("JWT_SECRET", "test-jwt-secret");
        std::env::set_var("CANARY_SELF_HOSTED_ADMIN_PASSWORD", "test-admin-password");
        std::env::set_var("CANARY_NTFY_SERVER_URL", "http://ntfy.startos/");
        std::env::set_var("CANARY_NTFY_TOKEN", "tk_test");
        std::env::set_var("CANARY_NTFY_TOPIC", "canary");
        std::env::set_var("CANARY_UMBREL_NTFY_URL", "http://ntfy_app_1/");
        std::env::remove_var("NTFY_SERVER_URL");

        let config = AppConfig::load_from_args(AppConfigArgs {
            network: Some(NetworkConfig::Mainnet),
            electrum_url: None,
            bind_address: Some("127.0.0.1:3000".to_string()),
            data_dir: Some("./database".to_string()),
        })
        .expect("self-hosted config should load");

        assert_eq!(config.default_ntfy_server_id(), PUBLIC_NTFY_SERVER_ID);
        assert_eq!(config.ntfy_server_url(), "https://ntfy.sh");
        assert_eq!(config.ntfy_servers().len(), 2);
        assert_eq!(config.ntfy_servers()[0].id, STARTOS_NTFY_SERVER_ID);
        assert_eq!(config.ntfy_servers()[1].id, UMBREL_NTFY_SERVER_ID);
        assert_eq!(
            config
                .managed_ntfy_access_token_for_url("http://ntfy.startos", None)
                .as_deref(),
            Some("tk_test")
        );

        restore_env_var("CANARY_MODE", previous_mode);
        restore_env_var("JWT_SECRET", previous_jwt);
        restore_env_var("CANARY_SELF_HOSTED_ADMIN_PASSWORD", previous_admin_password);
        restore_env_var("CANARY_NTFY_SERVER_URL", previous_server_url);
        restore_env_var("CANARY_NTFY_TOKEN", previous_token);
        restore_env_var("CANARY_NTFY_TOPIC", previous_topic);
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
    fn test_tx_explorer_url_list_env_validates_and_deduplicates_urls() {
        let _guard = ENV_LOCK.lock().unwrap();
        let previous_mempool_urls = std::env::var("CANARY_MEMPOOL_URLS").ok();

        std::env::set_var(
            "CANARY_MEMPOOL_URLS",
            " https://example-node.local:52127/ , not-a-url, https://example-node.local:52127, https://203.0.113.10:52127 ",
        );

        assert_eq!(
            AppConfig::parse_url_list_env("CANARY_MEMPOOL_URLS"),
            vec![
                "https://example-node.local:52127".to_string(),
                "https://203.0.113.10:52127".to_string()
            ]
        );

        restore_env_var("CANARY_MEMPOOL_URLS", previous_mempool_urls);
    }

    #[test]
    fn test_load_detects_tx_explorer_url_lists_in_self_hosted_mode() {
        let _guard = ENV_LOCK.lock().unwrap();
        let previous_mode = std::env::var("CANARY_MODE").ok();
        let previous_jwt = std::env::var("JWT_SECRET").ok();
        let previous_admin_password = std::env::var("CANARY_SELF_HOSTED_ADMIN_PASSWORD").ok();
        let previous_mempool_url = std::env::var("CANARY_MEMPOOL_URL").ok();
        let previous_mempool_urls = std::env::var("CANARY_MEMPOOL_URLS").ok();
        let previous_btc_rpc_explorer_urls = std::env::var("CANARY_BTC_RPC_EXPLORER_URLS").ok();
        let previous_tx_explorer_platform = std::env::var("CANARY_TX_EXPLORER_PLATFORM").ok();

        std::env::set_var("CANARY_MODE", "self-hosted");
        std::env::set_var("JWT_SECRET", "test-jwt-secret");
        std::env::set_var("CANARY_SELF_HOSTED_ADMIN_PASSWORD", "test-admin-password");
        std::env::set_var("CANARY_MEMPOOL_URL", "https://example-node.local:52127");
        std::env::set_var(
            "CANARY_MEMPOOL_URLS",
            "https://example-node.local:52127,https://203.0.113.10:52127",
        );
        std::env::set_var(
            "CANARY_BTC_RPC_EXPLORER_URLS",
            "https://example-node.local:49389,https://203.0.113.10:49389",
        );
        std::env::set_var("CANARY_TX_EXPLORER_PLATFORM", "startos");

        let config = AppConfig::load_from_args(AppConfigArgs {
            network: Some(NetworkConfig::Mainnet),
            electrum_url: None,
            bind_address: Some("127.0.0.1:3000".to_string()),
            data_dir: Some("./database".to_string()),
        })
        .expect("self-hosted config should load");

        assert_eq!(config.tx_explorers().len(), 2);
        assert_eq!(config.tx_explorers()[0].id, "mempool");
        assert_eq!(
            config.tx_explorers()[0].base_url,
            Some("https://example-node.local:52127".to_string())
        );
        assert_eq!(
            config.tx_explorers()[0].base_urls,
            vec![
                "https://example-node.local:52127".to_string(),
                "https://203.0.113.10:52127".to_string(),
            ]
        );
        assert_eq!(config.tx_explorers()[1].id, "btc-rpc-explorer");
        assert_eq!(
            config.tx_explorers()[0].platform.as_deref(),
            Some("startos")
        );
        assert_eq!(
            config.tx_explorers()[1].platform.as_deref(),
            Some("startos")
        );
        assert_eq!(
            config.tx_explorers()[1].base_urls,
            vec![
                "https://example-node.local:49389".to_string(),
                "https://203.0.113.10:49389".to_string(),
            ]
        );

        restore_env_var("CANARY_MODE", previous_mode);
        restore_env_var("JWT_SECRET", previous_jwt);
        restore_env_var("CANARY_SELF_HOSTED_ADMIN_PASSWORD", previous_admin_password);
        restore_env_var("CANARY_MEMPOOL_URL", previous_mempool_url);
        restore_env_var("CANARY_MEMPOOL_URLS", previous_mempool_urls);
        restore_env_var(
            "CANARY_BTC_RPC_EXPLORER_URLS",
            previous_btc_rpc_explorer_urls,
        );
        restore_env_var("CANARY_TX_EXPLORER_PLATFORM", previous_tx_explorer_platform);
    }

    #[test]
    fn test_tx_explorer_config_rejects_blank_base_url() {
        assert!(TxExplorerConfig::new(
            "mempool",
            "Mempool",
            Some("   ".to_string()),
            vec![],
            None,
            Some("umbrel"),
        )
        .is_none());
    }

    #[test]
    fn test_tx_explorer_config_ignores_blank_platform() {
        let config = TxExplorerConfig::new(
            "mempool",
            "Mempool",
            Some("http://umbrel.local:3006".to_string()),
            vec![],
            None,
            Some("   "),
        )
        .unwrap();

        assert_eq!(config.platform, None);
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
    fn test_with_managed_ntfy_auth_only_fills_empty_auth_for_managed_server() {
        let config = test_config_self_hosted(NetworkConfig::Mainnet)
            .with_ntfy_servers(vec![NtfyServerConfig::new_with_defaults(
                STARTOS_NTFY_SERVER_ID,
                "ntfy",
                Some("http://ntfy.startos".to_string()),
                Some("startos"),
                Some("canary".to_string()),
                true,
            )
            .unwrap()])
            .with_managed_ntfy_access_token("tk_managed");

        match config.with_managed_ntfy_auth(NtfyAuth::None, "http://ntfy.startos", None) {
            NtfyAuth::AccessToken(token) => assert_eq!(token, "tk_managed"),
            other => panic!("expected managed access token, got {other:?}"),
        }

        match config.with_managed_ntfy_auth(
            NtfyAuth::AccessToken("tk_user".to_string()),
            "http://ntfy.startos",
            None,
        ) {
            NtfyAuth::AccessToken(token) => assert_eq!(token, "tk_user"),
            other => panic!("expected user access token, got {other:?}"),
        }

        assert!(matches!(
            config.with_managed_ntfy_auth(NtfyAuth::None, "https://ntfy.sh", None),
            NtfyAuth::None
        ));
        assert!(matches!(
            config.with_managed_ntfy_auth(
                NtfyAuth::None,
                "http://ntfy.startos",
                Some("http://ntfy.startos")
            ),
            NtfyAuth::None
        ));
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
            frontend_urls: Vec::new(),
            jwt_secret: None, // Missing JWT secret
            self_hosted_admin_password: None,
            tx_explorers: Vec::new(),
            ntfy_servers: Vec::new(),
            ntfy_fallback_url: "https://ntfy.sh".to_string(),
            managed_ntfy_access_token: None,
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
            frontend_urls: Vec::new(),
            jwt_secret: Some("   ".to_string()),
            self_hosted_admin_password: Some("self-hosted-password".to_string()),
            tx_explorers: Vec::new(),
            ntfy_servers: Vec::new(),
            ntfy_fallback_url: "https://ntfy.sh".to_string(),
            managed_ntfy_access_token: None,
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
            frontend_urls: Vec::new(),
            jwt_secret: Some("test-self-hosted-jwt-secret".to_string()),
            self_hosted_admin_password: Some("   ".to_string()),
            tx_explorers: Vec::new(),
            ntfy_servers: Vec::new(),
            ntfy_fallback_url: "https://ntfy.sh".to_string(),
            managed_ntfy_access_token: None,
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

    #[test]
    fn test_is_stripe_enabled_requires_non_empty_values() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("STRIPE_SECRET_KEY", " ");
        std::env::set_var("STRIPE_WEBHOOK_SECRET", "");

        let config = test_config(NetworkConfig::Regtest);
        assert!(!config.is_stripe_enabled());

        std::env::set_var("STRIPE_SECRET_KEY", "sk_test_123");
        std::env::set_var("STRIPE_WEBHOOK_SECRET", "whsec_123");
        assert!(config.is_stripe_enabled());

        std::env::remove_var("STRIPE_SECRET_KEY");
        std::env::remove_var("STRIPE_WEBHOOK_SECRET");
    }

    #[test]
    fn test_active_billing_provider_prefers_valid_btcpay_when_stripe_empty() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("STRIPE_SECRET_KEY", "");
        std::env::set_var("STRIPE_WEBHOOK_SECRET", " ");
        std::env::set_var("BTCPAY_CLOUD_OFFERING_ID", "offering-1");
        std::env::set_var("BTCPAY_CLOUD_PERSONAL_PLAN_ID", "personal-1");
        std::env::set_var("BTCPAY_CLOUD_TEAM_PLAN_ID", "team-1");
        std::env::set_var("BTCPAY_CLOUD_PERSONAL_PRICE", "500");
        std::env::set_var("BTCPAY_CLOUD_TEAM_PRICE", "1500");

        let config = test_config(NetworkConfig::Regtest).with_btcpay(
            Some("https://btcpay.example.com".to_string()),
            Some("api-key".to_string()),
            Some("store-id".to_string()),
            Some("offering-fallback".to_string()),
            Some("plan-fallback".to_string()),
        );

        assert_eq!(
            config.active_billing_provider(),
            Some(BillingProvider::BtcPay)
        );

        std::env::remove_var("STRIPE_SECRET_KEY");
        std::env::remove_var("STRIPE_WEBHOOK_SECRET");
        std::env::remove_var("BTCPAY_CLOUD_OFFERING_ID");
        std::env::remove_var("BTCPAY_CLOUD_PERSONAL_PLAN_ID");
        std::env::remove_var("BTCPAY_CLOUD_TEAM_PLAN_ID");
        std::env::remove_var("BTCPAY_CLOUD_PERSONAL_PRICE");
        std::env::remove_var("BTCPAY_CLOUD_TEAM_PRICE");
    }

    #[test]
    fn test_btcpay_cloud_plan_config_rejects_empty_ids() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("BTCPAY_CLOUD_OFFERING_ID", " ");
        std::env::set_var("BTCPAY_CLOUD_PERSONAL_PLAN_ID", " ");
        std::env::set_var("BTCPAY_CLOUD_TEAM_PLAN_ID", "");
        std::env::set_var("BTCPAY_CLOUD_PERSONAL_PRICE", "500");
        std::env::set_var("BTCPAY_CLOUD_TEAM_PRICE", "1500");

        let config = test_config(NetworkConfig::Regtest).with_btcpay(
            Some("https://btcpay.example.com".to_string()),
            Some("api-key".to_string()),
            Some("store-id".to_string()),
            Some(" ".to_string()),
            Some("plan-fallback".to_string()),
        );

        assert!(config.btcpay_cloud_plan_config().is_none());

        std::env::remove_var("BTCPAY_CLOUD_OFFERING_ID");
        std::env::remove_var("BTCPAY_CLOUD_PERSONAL_PLAN_ID");
        std::env::remove_var("BTCPAY_CLOUD_TEAM_PLAN_ID");
        std::env::remove_var("BTCPAY_CLOUD_PERSONAL_PRICE");
        std::env::remove_var("BTCPAY_CLOUD_TEAM_PRICE");
    }
}
