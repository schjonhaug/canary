use anyhow::{anyhow, Result};
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

    /// Data directory path
    #[arg(long, default_value = "./database")]
    pub data_dir: String,
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


        if let Ok(data_dir_env) = std::env::var("CANARY_DATA_DIR") {
            config.data_dir = data_dir_env;
        }

        Ok(config)
    }

    /// Get the operating mode (saas or foss)
    pub fn operating_mode(&self) -> String {
        std::env::var("CANARY_MODE")
            .unwrap_or_else(|_| "saas".to_string()) // Default to SAAS if not specified
            .to_lowercase()
    }

    /// Check if running in SAAS mode
    pub fn is_saas_mode(&self) -> bool {
        self.operating_mode() == "saas"
    }

    /// Check if running in FOSS mode
    pub fn is_foss_mode(&self) -> bool {
        self.operating_mode() == "foss"
    }

    /// Check if ntfy provider should be enabled
    pub fn is_ntfy_enabled(&self) -> bool {
        // ntfy is always available, but in FOSS mode it's the only provider
        true
    }

    /// Check if Twilio SMS provider should be enabled
    pub fn is_twilio_enabled(&self) -> bool {
        // Only allow Twilio in SAAS mode, and only if configured
        if self.is_foss_mode() {
            return false;
        }
        
        // Check if Twilio environment variables are configured
        std::env::var("TWILIO_ACCOUNT_SID").is_ok() && 
        std::env::var("TWILIO_AUTH_TOKEN").is_ok() && 
        std::env::var("TWILIO_MESSAGING_SERVICE_SID").is_ok()
    }

    /// Check if email provider should be enabled
    pub fn is_email_enabled(&self) -> bool {
        // Only allow email in SAAS mode
        self.is_saas_mode()
    }

    /// Validate that all required environment variables are set for the current mode
    pub fn validate_required_config(&self) -> Result<(), String> {
        if self.is_saas_mode() {
            self.validate_saas_config()
        } else {
            // FOSS mode has no strict requirements beyond basic config
            Ok(())
        }
    }

    /// Validate required SAAS mode configuration
    fn validate_saas_config(&self) -> Result<(), String> {
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
        if std::env::var("TWILIO_MESSAGING_SERVICE_SID").is_err() {
            missing.push("TWILIO_MESSAGING_SERVICE_SID - Required for SMS notifications");
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
            missing.push("FRONTEND_URL - Required for email verification and reset links");
        }

        if missing.is_empty() {
            Ok(())
        } else {
            let error_msg = format!(
                "SAAS mode requires the following environment variables:\n{}",
                missing.into_iter()
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

    // Helper methods for tests
    #[cfg(test)]
    pub fn wallet_dir_path(&self) -> String {
        format!("database/{}/wallets", self.network_name())
    }

    #[cfg(test)]
    pub fn metadata_db_path(&self) -> String {
        format!("database/{}/metadata.sqlite", self.network_name())
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
            data_dir: "./database".to_string(),
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
            data_dir: "./database".to_string(),
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
            data_dir: "./database".to_string(),
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
            data_dir: "./database".to_string(),
        };
        assert_eq!(config.effective_wallet_dir(), "./database/regtest/wallets");
        assert_eq!(
            config.effective_metadata_db(),
            "./database/regtest/metadata.sqlite"
        );

        // Test with absolute paths (Docker/Umbrel)
        let config = AppConfig {
            network: NetworkConfig::Mainnet,
            electrum_url: None,
            bind_address: "127.0.0.1:3000".to_string(),
            data_dir: "/app/data".to_string(),
        };
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
            let config = AppConfig {
                network,
                electrum_url: None,
                bind_address: "127.0.0.1:3000".to_string(),
                data_dir: "./database".to_string(),
            };

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
        let regtest_config = AppConfig {
            network: NetworkConfig::Regtest,
            electrum_url: None,
            bind_address: "127.0.0.1:3000".to_string(),
            data_dir: "./database".to_string(),
        };
        assert_eq!(regtest_config.electrum_url(), "tcp://127.0.0.1:50001");

        let testnet_config = AppConfig {
            network: NetworkConfig::Testnet,
            electrum_url: None,
            bind_address: "127.0.0.1:3000".to_string(),
            data_dir: "./database".to_string(),
        };
        assert_eq!(
            testnet_config.electrum_url(),
            "ssl://electrum.blockstream.info:60002"
        );

        let mainnet_config = AppConfig {
            network: NetworkConfig::Mainnet,
            electrum_url: None,
            bind_address: "127.0.0.1:3000".to_string(),
            data_dir: "./database".to_string(),
        };
        assert_eq!(
            mainnet_config.electrum_url(),
            "ssl://electrum.blockstream.info:50002"
        );
    }

    #[test]
    fn test_custom_electrum_url_override() {
        let config = AppConfig {
            network: NetworkConfig::Mainnet,
            electrum_url: Some("ssl://custom.electrum.server:50002".to_string()),
            bind_address: "127.0.0.1:3000".to_string(),
            data_dir: "./database".to_string(),
        };
        assert_eq!(config.electrum_url(), "ssl://custom.electrum.server:50002");
    }
}
