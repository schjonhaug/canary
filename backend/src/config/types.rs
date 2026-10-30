//! Value types shared by the configuration: networks, operating modes and
//! the self-hosted explorer and ntfy server descriptions.

use anyhow::{Result, anyhow};
use bdk_wallet::bitcoin::Network;
use clap::ValueEnum;
use serde::Serialize;

pub const PUBLIC_NTFY_SERVER_ID: &str = "ntfy-sh";
pub const STARTOS_NTFY_SERVER_ID: &str = "startos-ntfy";
pub const UMBREL_NTFY_SERVER_ID: &str = "umbrel-ntfy";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BillingProvider {
    Stripe,
    BtcPay,
}

impl BillingProvider {
    pub fn as_str(self) -> &'static str {
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
pub(super) struct ManagedNtfyAccessToken(pub(super) String);

impl std::fmt::Debug for ManagedNtfyAccessToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ManagedNtfyAccessToken([redacted])")
    }
}

impl NtfyServerConfig {
    pub(super) fn new(
        id: &str,
        name: &str,
        base_url: Option<String>,
        platform: Option<&str>,
    ) -> Option<Self> {
        Self::new_with_defaults(id, name, base_url, platform, None, false)
    }

    pub(super) fn new_with_defaults(
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
    pub(super) fn new(
        id: &str,
        name: &str,
        base_url: Option<String>,
        base_urls: &[String],
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
            // Mainnet, and any network Canary does not support explicitly
            _ => NetworkConfig::Mainnet,
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
                "Invalid network: {s}. Valid options: regtest, testnet, mainnet"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
                "Invalid CANARY_MODE: '{s}'. Valid options: cloud, self-hosted"
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
