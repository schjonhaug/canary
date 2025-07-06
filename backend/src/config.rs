use anyhow::{Result, anyhow};
use bdk_wallet::bitcoin::Network;
use clap::{Parser, ValueEnum};

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
#[command(name = "kanari")]
#[command(about = "Bitcoin wallet management service")]
pub struct AppConfig {
    /// Bitcoin network to use
    #[arg(long, value_enum, default_value = "regtest")]
    pub network: NetworkConfig,

    /// Electrum server URL to connect to
    #[arg(long)]
    pub electrum_url: Option<String>,

    /// Server bind address
    #[arg(long, default_value = "127.0.0.1:3000")]
    pub bind_address: String,

    /// Wallet directory path
    #[arg(long, default_value = "./wallets")]
    pub wallet_dir: String,

    /// Metadata database path
    #[arg(long, default_value = "metadata.sqlite")]
    pub metadata_db: String,
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        // Load environment variables from .env file if present
        if let Err(_) = dotenvy::dotenv() {
            // Ignore error if .env file doesn't exist
        }

        // Parse command line arguments
        let mut config = Self::parse();

        // Override with environment variables if present
        if let Ok(network_env) = std::env::var("KANARI_NETWORK") {
            config.network = network_env.parse()?;
        }

        if let Ok(electrum_url_env) = std::env::var("KANARI_ELECTRUM_URL") {
            config.electrum_url = Some(electrum_url_env);
        }

        if let Ok(bind_address_env) = std::env::var("KANARI_BIND_ADDRESS") {
            config.bind_address = bind_address_env;
        }

        if let Ok(wallet_dir_env) = std::env::var("KANARI_WALLET_DIR") {
            config.wallet_dir = wallet_dir_env;
        }

        if let Ok(metadata_db_env) = std::env::var("KANARI_METADATA_DB") {
            config.metadata_db = metadata_db_env;
        }

        Ok(config)
    }

    pub fn electrum_url(&self) -> String {
        self.electrum_url
            .clone()
            .unwrap_or_else(|| self.network.default_electrum_url().to_string())
    }

    pub fn network(&self) -> Network {
        self.network.to_bdk_network()
    }

    /// Get network-specific wallet directory path
    /// Returns: database/{network}/wallets
    pub fn wallet_dir_path(&self) -> String {
        let network_name = match self.network {
            NetworkConfig::Regtest => "regtest",
            NetworkConfig::Testnet => "testnet",
            NetworkConfig::Mainnet => "mainnet",
        };
        format!("database/{}/wallets", network_name)
    }

    /// Get network-specific metadata database path
    /// Returns: database/{network}/metadata.sqlite
    pub fn metadata_db_path(&self) -> String {
        let network_name = match self.network {
            NetworkConfig::Regtest => "regtest",
            NetworkConfig::Testnet => "testnet",
            NetworkConfig::Mainnet => "mainnet",
        };
        format!("database/{}/metadata.sqlite", network_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_network_specific_paths() {
        // Test regtest paths
        let config = AppConfig {
            network: NetworkConfig::Regtest,
            electrum_url: None,
            bind_address: "127.0.0.1:3000".to_string(),
            wallet_dir: "./wallets".to_string(),
            metadata_db: "metadata.sqlite".to_string(),
        };
        assert_eq!(config.wallet_dir_path(), "database/regtest/wallets");
        assert_eq!(
            config.metadata_db_path(),
            "database/regtest/metadata.sqlite"
        );

        // Test testnet paths
        let config = AppConfig {
            network: NetworkConfig::Testnet,
            electrum_url: None,
            bind_address: "127.0.0.1:3000".to_string(),
            wallet_dir: "./wallets".to_string(),
            metadata_db: "metadata.sqlite".to_string(),
        };
        assert_eq!(config.wallet_dir_path(), "database/testnet/wallets");
        assert_eq!(
            config.metadata_db_path(),
            "database/testnet/metadata.sqlite"
        );

        // Test mainnet paths
        let config = AppConfig {
            network: NetworkConfig::Mainnet,
            electrum_url: None,
            bind_address: "127.0.0.1:3000".to_string(),
            wallet_dir: "./wallets".to_string(),
            metadata_db: "metadata.sqlite".to_string(),
        };
        assert_eq!(config.wallet_dir_path(), "database/mainnet/wallets");
        assert_eq!(
            config.metadata_db_path(),
            "database/mainnet/metadata.sqlite"
        );
    }
}
