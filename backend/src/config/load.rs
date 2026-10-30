//! Reads `AppConfig` from command line arguments and the environment.

use anyhow::{anyhow, Result};
use clap::Parser;

use super::{
    AppConfig, ManagedNtfyAccessToken, NetworkConfig, NtfyServerConfig, OperatingMode,
    TxExplorerConfig, STARTOS_NTFY_SERVER_ID, UMBREL_NTFY_SERVER_ID,
};

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

    pub(super) fn parse_url_env(var_name: &str) -> Option<String> {
        std::env::var(var_name)
            .ok()
            .and_then(|url| Self::normalize_url_env_value(var_name, &url))
    }

    pub(super) fn parse_url_list_env(var_name: &str) -> Vec<String> {
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

    pub fn load() -> Result<Self> {
        // Note: .env file is loaded at the start of main() before logging init

        // Parse command line arguments
        let args = AppConfigArgs::parse();
        Self::load_from_args(args)
    }

    pub(crate) fn load_from_args(args: AppConfigArgs) -> Result<Self> {
        // Resolve network configuration (CLI args override env vars)
        let Some(network) = args.network.or_else(|| {
            std::env::var("CANARY_NETWORK")
                .ok()
                .and_then(|v| v.parse().ok())
        }) else {
            return Err(anyhow!(
                    "CANARY_NETWORK is required. Set it via --network CLI arg or CANARY_NETWORK environment variable.\nValid values: regtest, testnet, mainnet"
                ));
        };

        // Resolve electrum URL (CLI args override env vars)
        let electrum_url = args
            .electrum_url
            .or_else(|| std::env::var("CANARY_ELECTRUM_URL").ok());

        // Resolve bind address
        let Some(bind_address) = args
            .bind_address
            .or_else(|| std::env::var("CANARY_BIND_ADDRESS").ok())
        else {
            return Err(anyhow!(
                    "CANARY_BIND_ADDRESS is required. Set it via --bind-address CLI arg or CANARY_BIND_ADDRESS environment variable.\nExample: 127.0.0.1:3000"
                ));
        };

        // Resolve data directory
        let Some(data_dir) = args
            .data_dir
            .or_else(|| std::env::var("CANARY_DATA_DIR").ok())
        else {
            return Err(anyhow!(
                    "CANARY_DATA_DIR is required. Set it via --data-dir CLI arg or CANARY_DATA_DIR environment variable.\nExample: ./database"
                ));
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
                &mempool_urls,
                mempool_port,
                tx_explorer_platform.as_deref(),
            ),
            TxExplorerConfig::new(
                "bitfeed",
                "Bitfeed",
                bitfeed_url,
                &bitfeed_urls,
                bitfeed_port,
                tx_explorer_platform.as_deref(),
            ),
            TxExplorerConfig::new(
                "btc-rpc-explorer",
                "BTC RPC Explorer",
                btc_rpc_explorer_url,
                &btc_rpc_explorer_urls,
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

        if let Err(error) = crate::nostr_provider::nostr_onion_socks_proxy_from_env() {
            return Err(anyhow!(error));
        }

        if operating_mode == OperatingMode::SelfHosted {
            if jwt_secret
                .as_deref()
                .map(str::trim)
                .as_ref()
                .is_none_or(|v| v.is_empty())
            {
                return Err(anyhow!(
                    "JWT_SECRET required for self-hosted mode - check your .env file"
                ));
            }

            if self_hosted_admin_password
                .as_deref()
                .map(str::trim)
                .as_ref()
                .is_none_or(|v| v.is_empty())
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
}
