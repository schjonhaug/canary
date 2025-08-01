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
#[command(name = "canary")]
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
        if let Ok(network_env) = std::env::var("CANARY_NETWORK") {
            config.network = network_env.parse()?;
        }

        if let Ok(electrum_url_env) = std::env::var("CANARY_ELECTRUM_URL") {
            config.electrum_url = Some(electrum_url_env);
        }

        if let Ok(bind_address_env) = std::env::var("CANARY_BIND_ADDRESS") {
            config.bind_address = bind_address_env;
        }

        if let Ok(wallet_dir_env) = std::env::var("CANARY_WALLET_DIR") {
            config.wallet_dir = wallet_dir_env;
        }

        if let Ok(metadata_db_env) = std::env::var("CANARY_METADATA_DB") {
            config.metadata_db = metadata_db_env;
        }

        Ok(config)
    }

    /// Check if ntfy provider should be enabled
    pub fn is_ntfy_enabled(&self) -> bool {
        std::env::var("CANARY_ENABLE_NTFY")
            .map(|v| v.to_lowercase() == "true" || v == "1")
            .unwrap_or(true) // Default to enabled
    }

    /// Check if Twilio SMS provider should be enabled
    pub fn is_twilio_enabled(&self) -> bool {
        std::env::var("CANARY_ENABLE_TWILIO")
            .map(|v| v.to_lowercase() == "true" || v == "1")
            .unwrap_or(false) // Default to disabled unless explicitly enabled
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

    /// Get the effective wallet directory path
    /// Uses the configured path if it's absolute,
    /// otherwise uses network-specific path
    pub fn effective_wallet_dir(&self) -> String {
        if self.wallet_dir.starts_with('/') {
            // Absolute path - use as-is
            self.wallet_dir.clone()
        } else {
            // Relative path - use network-specific path
            self.wallet_dir_path()
        }
    }

    /// Get the effective metadata database path
    /// Uses the configured path if it's absolute,
    /// otherwise uses network-specific path
    pub fn effective_metadata_db(&self) -> String {
        if self.metadata_db.starts_with('/') {
            // Absolute path - use as-is
            self.metadata_db.clone()
        } else {
            // Relative path - use network-specific path
            self.metadata_db_path()
        }
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

    #[test]
    fn test_effective_paths() {
        // Test with relative paths (local development)
        let config = AppConfig {
            network: NetworkConfig::Regtest,
            electrum_url: None,
            bind_address: "127.0.0.1:3000".to_string(),
            wallet_dir: "./wallets".to_string(),
            metadata_db: "metadata.sqlite".to_string(),
        };
        assert_eq!(config.effective_wallet_dir(), "database/regtest/wallets");
        assert_eq!(config.effective_metadata_db(), "database/regtest/metadata.sqlite");

        // Test with absolute paths (Docker/Umbrel)
        let config = AppConfig {
            network: NetworkConfig::Mainnet,
            electrum_url: None,
            bind_address: "127.0.0.1:3000".to_string(),
            wallet_dir: "/app/data/database/mainnet/wallets".to_string(),
            metadata_db: "/app/data/database/mainnet/metadata.sqlite".to_string(),
        };
        assert_eq!(config.effective_wallet_dir(), "/app/data/database/mainnet/wallets");
        assert_eq!(config.effective_metadata_db(), "/app/data/database/mainnet/metadata.sqlite");
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
            let config = AppConfig {
                network,
                electrum_url: None,
                bind_address: "127.0.0.1:3000".to_string(),
                wallet_dir: "./wallets".to_string(),
                metadata_db: "metadata.sqlite".to_string(),
            };
            
            let wallet_path = config.wallet_dir_path();
            let metadata_path = config.metadata_db_path();
            
            // Ensure no duplicate paths
            assert!(!wallet_paths.contains(&wallet_path), "Wallet path '{}' is not unique", wallet_path);
            assert!(!metadata_paths.contains(&metadata_path), "Metadata path '{}' is not unique", metadata_path);
            
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
        let regtest_config = AppConfig {
            network: NetworkConfig::Regtest,
            electrum_url: None,
            bind_address: "127.0.0.1:3000".to_string(),
            wallet_dir: "./wallets".to_string(),
            metadata_db: "metadata.sqlite".to_string(),
        };
        assert_eq!(regtest_config.electrum_url(), "tcp://127.0.0.1:50001");
        
        let testnet_config = AppConfig {
            network: NetworkConfig::Testnet,
            electrum_url: None,
            bind_address: "127.0.0.1:3000".to_string(),
            wallet_dir: "./wallets".to_string(),
            metadata_db: "metadata.sqlite".to_string(),
        };
        assert_eq!(testnet_config.electrum_url(), "ssl://electrum.blockstream.info:60002");
        
        let mainnet_config = AppConfig {
            network: NetworkConfig::Mainnet,
            electrum_url: None,
            bind_address: "127.0.0.1:3000".to_string(),
            wallet_dir: "./wallets".to_string(),
            metadata_db: "metadata.sqlite".to_string(),
        };
        assert_eq!(mainnet_config.electrum_url(), "ssl://electrum.blockstream.info:50002");
    }

    #[test]
    fn test_custom_electrum_url_override() {
        let config = AppConfig {
            network: NetworkConfig::Mainnet,
            electrum_url: Some("ssl://custom.electrum.server:50002".to_string()),
            bind_address: "127.0.0.1:3000".to_string(),
            wallet_dir: "./wallets".to_string(),
            metadata_db: "metadata.sqlite".to_string(),
        };
        assert_eq!(config.electrum_url(), "ssl://custom.electrum.server:50002");
    }
}
