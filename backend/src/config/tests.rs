use super::load::{AppConfigArgs, Env};
use super::*;
use std::collections::HashMap;

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
    AppConfig::new_for_test(
        network,
        None,
        "127.0.0.1:3000".to_string(),
        "./database".to_string(),
        OperatingMode::Cloud,
        Some("http://localhost:3001".to_string()),
        Some("test-jwt-secret".to_string()),
    )
}

fn test_config_with_data_dir(network: NetworkConfig, data_dir: &str) -> AppConfig {
    AppConfig {
        data_dir: data_dir.to_string(),
        ..test_config(network)
    }
}

fn test_config_self_hosted(network: NetworkConfig) -> AppConfig {
    AppConfig {
        self_hosted_admin_password: Some("self-hosted-password".to_string()),
        ..AppConfig::new_for_test(
            network,
            None,
            "127.0.0.1:3000".to_string(),
            "./database".to_string(),
            OperatingMode::SelfHosted,
            None,
            Some("test-self-hosted-jwt-secret".to_string()),
        )
    }
}

fn args(network: NetworkConfig) -> AppConfigArgs {
    AppConfigArgs {
        network: Some(network),
        electrum_url: None,
        bind_address: Some("127.0.0.1:3000".to_string()),
        data_dir: Some("./database".to_string()),
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
            "Wallet path '{wallet_path}' is not unique"
        );
        assert!(
            !metadata_paths.contains(&metadata_path),
            "Metadata path '{metadata_path}' is not unique"
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
    let config = AppConfig {
        sync_interval: Some(42),
        ..test_config_self_hosted(NetworkConfig::Mainnet)
    };
    assert_eq!(config.sync_interval(), 42);

    let env = HashMap::from([
        ("CANARY_MODE", "self-hosted"),
        ("JWT_SECRET", "test-jwt-secret"),
        ("CANARY_SELF_HOSTED_ADMIN_PASSWORD", "test-admin-password"),
        ("CANARY_SYNC_INTERVAL", "42"),
    ]);
    let config = AppConfig::load_from(args(NetworkConfig::Mainnet), &env).unwrap();
    assert_eq!(config.sync_interval(), 42);
}

#[test]
fn test_self_hosted_mode_sync_interval_network_defaults() {
    let regtest_config = test_config_self_hosted(NetworkConfig::Regtest);
    assert_eq!(regtest_config.sync_interval(), 30);

    let mainnet_config = test_config_self_hosted(NetworkConfig::Mainnet);
    assert_eq!(mainnet_config.sync_interval(), 300);

    let invalid = AppConfig::load_from(
        args(NetworkConfig::Mainnet),
        &HashMap::from([
            ("CANARY_MODE", "self-hosted"),
            ("JWT_SECRET", "test-jwt-secret"),
            ("CANARY_SELF_HOSTED_ADMIN_PASSWORD", "test-admin-password"),
            ("CANARY_SYNC_INTERVAL", "soon"),
        ]),
    )
    .unwrap();
    assert_eq!(invalid.sync_interval(), 300);
}

#[test]
fn test_cloud_mode_sync_interval_fallback() {
    let config = test_config(NetworkConfig::Mainnet);

    // In cloud mode, should use network defaults as fallback
    assert_eq!(config.sync_interval(), 300);
}

#[test]
fn test_custom_electrum_url_override() {
    let config = AppConfig {
        electrum_url: Some("ssl://custom.electrum.server:50002".to_string()),
        ..test_config(NetworkConfig::Mainnet)
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
fn restore_drill_skips_billing_and_notification_requirements() {
    let config = AppConfig {
        restore_drill: true,
        jwt_secret: Some("restore-drill-jwt".to_string()),
        stripe: StripeSettings {
            secret_key: Some(SecretString::new("sk_test_present_but_ignored")),
            webhook_secret: Some(SecretString::new("whsec_present_but_ignored")),
        },
        twilio: TwilioSettings {
            account_sid: Some("ACpresent".to_string()),
            auth_token: Some(SecretString::new("present")),
            sender_id: Some("Canary".to_string()),
            verify_service_sid: None,
        },
        email: EmailSettings {
            resend_api_key: Some(SecretString::new("re_present")),
            ..EmailSettings::default()
        },
        ..test_config(NetworkConfig::Mainnet)
    };
    assert!(config.is_restore_drill());
    assert!(!config.is_stripe_enabled());
    assert!(config.active_billing_provider().is_none());
    assert!(!config.is_twilio_enabled());
    assert!(!config.is_email_enabled());
    assert!(!config.is_ntfy_enabled());
    assert!(!config.is_btcpay_enabled());
    assert!(config.validate_required_config().is_ok());
}

#[test]
fn restore_drill_still_requires_jwt_and_frontend_origin() {
    let config = AppConfig {
        restore_drill: true,
        jwt_secret: None,
        ..test_config(NetworkConfig::Mainnet)
    };
    let error = config.validate_required_config().unwrap_err();
    assert!(error.contains("JWT_SECRET"));
}

#[test]
fn test_detected_ntfy_server_becomes_self_hosted_default() {
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
}

#[test]
fn test_load_detects_umbrel_ntfy_url_in_self_hosted_mode() {
    let env = HashMap::from([
        ("CANARY_MODE", "self-hosted"),
        ("JWT_SECRET", "test-jwt-secret"),
        ("CANARY_SELF_HOSTED_ADMIN_PASSWORD", "test-admin-password"),
        ("CANARY_UMBREL_NTFY_URL", "http://ntfy_app_1/"),
    ]);

    let config = AppConfig::load_from(
        AppConfigArgs {
            network: Some(NetworkConfig::Regtest),
            electrum_url: None,
            bind_address: Some("127.0.0.1:3000".to_string()),
            data_dir: Some("./database".to_string()),
        },
        &env,
    )
    .unwrap();

    assert_eq!(config.default_ntfy_server_id(), "umbrel-ntfy");
    assert_eq!(config.ntfy_server_url(), "http://ntfy_app_1");
    assert_eq!(config.ntfy_servers().len(), 1);
    assert_eq!(config.ntfy_servers()[0].platform.as_deref(), Some("umbrel"));
}

#[test]
fn test_load_detects_startos_managed_ntfy_defaults_in_self_hosted_mode() {
    let env = HashMap::from([
        ("CANARY_MODE", "self-hosted"),
        ("JWT_SECRET", "test-jwt-secret"),
        ("CANARY_SELF_HOSTED_ADMIN_PASSWORD", "test-admin-password"),
        ("CANARY_NTFY_SERVER_URL", "http://ntfy.startos/"),
        ("CANARY_NTFY_TOKEN", " tk_test "),
        ("CANARY_NTFY_TOPIC", "canary"),
    ]);

    let config = AppConfig::load_from(
        AppConfigArgs {
            network: Some(NetworkConfig::Mainnet),
            electrum_url: None,
            bind_address: Some("127.0.0.1:3000".to_string()),
            data_dir: Some("./database".to_string()),
        },
        &env,
    )
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
        config
            .managed_ntfy_access_token_for_url("http://ntfy.startos", Some("http://ntfy.startos")),
        None
    );
    assert_eq!(
        config.managed_ntfy_access_token_for_url("https://ntfy.sh", None),
        None
    );
}

#[test]
fn test_load_ignores_startos_ntfy_token_without_server_url() {
    let env = HashMap::from([
        ("CANARY_MODE", "self-hosted"),
        ("JWT_SECRET", "test-jwt-secret"),
        ("CANARY_SELF_HOSTED_ADMIN_PASSWORD", "test-admin-password"),
        ("CANARY_NTFY_TOKEN", "tk_without_url"),
        ("CANARY_NTFY_TOPIC", "canary"),
    ]);

    let config = AppConfig::load_from(
        AppConfigArgs {
            network: Some(NetworkConfig::Mainnet),
            electrum_url: None,
            bind_address: Some("127.0.0.1:3000".to_string()),
            data_dir: Some("./database".to_string()),
        },
        &env,
    )
    .expect("self-hosted config should load");

    assert_eq!(config.default_ntfy_server_id(), PUBLIC_NTFY_SERVER_ID);
    assert_eq!(config.ntfy_server_url(), "https://ntfy.sh");
    assert!(config.ntfy_servers().is_empty());
    assert_eq!(
        config.managed_ntfy_access_token_for_url("http://ntfy.startos", None),
        None
    );
}

#[test]
fn test_load_ignores_invalid_startos_ntfy_topic() {
    let env = HashMap::from([
        ("CANARY_MODE", "self-hosted"),
        ("JWT_SECRET", "test-jwt-secret"),
        ("CANARY_SELF_HOSTED_ADMIN_PASSWORD", "test-admin-password"),
        ("CANARY_NTFY_SERVER_URL", "http://ntfy.startos/"),
        ("CANARY_NTFY_TOKEN", "tk_test"),
        ("CANARY_NTFY_TOPIC", "canary/topic"),
    ]);

    let config = AppConfig::load_from(
        AppConfigArgs {
            network: Some(NetworkConfig::Mainnet),
            electrum_url: None,
            bind_address: Some("127.0.0.1:3000".to_string()),
            data_dir: Some("./database".to_string()),
        },
        &env,
    )
    .expect("self-hosted config should load");

    assert_eq!(config.default_ntfy_server_id(), STARTOS_NTFY_SERVER_ID);
    assert_eq!(config.ntfy_servers().len(), 1);
    assert_eq!(config.ntfy_servers()[0].default_topic, None);
    assert!(config.ntfy_servers()[0].managed_auth);
}

#[test]
fn test_load_registers_startos_ntfy_url_without_managed_auth() {
    let env = HashMap::from([
        ("CANARY_MODE", "self-hosted"),
        ("JWT_SECRET", "test-jwt-secret"),
        ("CANARY_SELF_HOSTED_ADMIN_PASSWORD", "test-admin-password"),
        ("CANARY_NTFY_SERVER_URL", "http://ntfy.startos/"),
        ("CANARY_NTFY_TOPIC", "canary"),
    ]);

    let config = AppConfig::load_from(
        AppConfigArgs {
            network: Some(NetworkConfig::Mainnet),
            electrum_url: None,
            bind_address: Some("127.0.0.1:3000".to_string()),
            data_dir: Some("./database".to_string()),
        },
        &env,
    )
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
}

#[test]
fn test_load_ignores_startos_ntfy_defaults_in_cloud_mode() {
    let env = HashMap::from([
        ("CANARY_MODE", "cloud"),
        ("CANARY_NTFY_SERVER_URL", "http://ntfy.startos/"),
        ("CANARY_NTFY_TOKEN", "tk_test"),
        ("CANARY_NTFY_TOPIC", "canary"),
    ]);

    let config = AppConfig::load_from(
        AppConfigArgs {
            network: Some(NetworkConfig::Mainnet),
            electrum_url: None,
            bind_address: Some("127.0.0.1:3000".to_string()),
            data_dir: Some("./database".to_string()),
        },
        &env,
    )
    .expect("cloud config should load");

    assert_eq!(config.default_ntfy_server_id(), PUBLIC_NTFY_SERVER_ID);
    assert_eq!(config.ntfy_server_url(), "https://ntfy.sh");
    assert!(config.ntfy_servers().is_empty());
    assert_eq!(
        config.managed_ntfy_access_token_for_url("http://ntfy.startos", None),
        None
    );
}

#[test]
fn test_load_does_not_guess_default_when_multiple_local_ntfy_servers_exist() {
    let env = HashMap::from([
        ("CANARY_MODE", "self-hosted"),
        ("JWT_SECRET", "test-jwt-secret"),
        ("CANARY_SELF_HOSTED_ADMIN_PASSWORD", "test-admin-password"),
        ("CANARY_NTFY_SERVER_URL", "http://ntfy.startos/"),
        ("CANARY_NTFY_TOKEN", "tk_test"),
        ("CANARY_NTFY_TOPIC", "canary"),
        ("CANARY_UMBREL_NTFY_URL", "http://ntfy_app_1/"),
    ]);

    let config = AppConfig::load_from(
        AppConfigArgs {
            network: Some(NetworkConfig::Mainnet),
            electrum_url: None,
            bind_address: Some("127.0.0.1:3000".to_string()),
            data_dir: Some("./database".to_string()),
        },
        &env,
    )
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
}

#[test]
fn test_detected_umbrel_ntfy_url_takes_precedence_over_fallback_env() {
    let env = HashMap::from([
        ("CANARY_MODE", "self-hosted"),
        ("JWT_SECRET", "test-jwt-secret"),
        ("CANARY_SELF_HOSTED_ADMIN_PASSWORD", "test-admin-password"),
        ("CANARY_UMBREL_NTFY_URL", "http://ntfy_app_1"),
        ("NTFY_SERVER_URL", "https://ntfy.example.com"),
    ]);

    let config = AppConfig::load_from(
        AppConfigArgs {
            network: Some(NetworkConfig::Mainnet),
            electrum_url: None,
            bind_address: Some("127.0.0.1:3000".to_string()),
            data_dir: Some("./database".to_string()),
        },
        &env,
    )
    .expect("self-hosted config should load");

    assert_eq!(config.ntfy_server_url(), "http://ntfy_app_1");
    assert_eq!(config.default_ntfy_server_id(), UMBREL_NTFY_SERVER_ID);
}

#[test]
fn test_tx_explorer_url_list_env_validates_and_deduplicates_urls() {
    let env = HashMap::from([(
        "CANARY_MEMPOOL_URLS",
        " https://example-node.local:52127/ , not-a-url, https://example-node.local:52127, https://203.0.113.10:52127 ",
    )]);

    assert_eq!(
        Env(&env).url_list("CANARY_MEMPOOL_URLS"),
        vec![
            "https://example-node.local:52127".to_string(),
            "https://203.0.113.10:52127".to_string()
        ]
    );
}

#[test]
fn test_load_detects_tx_explorer_url_lists_in_self_hosted_mode() {
    let env = HashMap::from([
        ("CANARY_MODE", "self-hosted"),
        ("JWT_SECRET", "test-jwt-secret"),
        ("CANARY_SELF_HOSTED_ADMIN_PASSWORD", "test-admin-password"),
        ("CANARY_MEMPOOL_URL", "https://example-node.local:52127"),
        (
            "CANARY_MEMPOOL_URLS",
            "https://example-node.local:52127,https://203.0.113.10:52127",
        ),
        (
            "CANARY_BTC_RPC_EXPLORER_URLS",
            "https://example-node.local:49389,https://203.0.113.10:49389",
        ),
        ("CANARY_TX_EXPLORER_PLATFORM", "startos"),
    ]);

    let config = AppConfig::load_from(
        AppConfigArgs {
            network: Some(NetworkConfig::Mainnet),
            electrum_url: None,
            bind_address: Some("127.0.0.1:3000".to_string()),
            data_dir: Some("./database".to_string()),
        },
        &env,
    )
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
}

#[test]
fn test_tx_explorer_config_rejects_blank_base_url() {
    assert!(TxExplorerConfig::new(
        "mempool",
        "Mempool",
        Some("   ".to_string()),
        &[],
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
        &[],
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
    let url = |value| Env(&HashMap::from([("NTFY_SERVER_URL", value)])).url("NTFY_SERVER_URL");

    assert!(url("   ").is_none());
    assert!(url("ntfy.example.com").is_none());
    assert_eq!(
        url(" https://ntfy.example.com/ ").as_deref(),
        Some("https://ntfy.example.com")
    );
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
    assert!(
        !config.should_use_ntfy_auth_for_url("https://ntfy.sh", Some("https://ntfy.example.com"))
    );
    assert!(!config.should_use_ntfy_auth_for_url("https://ntfy.sh", Some("")));
}

#[test]
fn test_migrated_detected_ntfy_preference_remains_trusted() {
    let config = test_config_self_hosted(NetworkConfig::Regtest).with_ntfy_servers(vec![
        NtfyServerConfig::new(
            "umbrel-ntfy",
            "ntfy",
            Some("http://ntfy_app_1".to_string()),
            Some("umbrel"),
        )
        .unwrap(),
    ]);

    assert!(config.should_trust_ntfy_server_url("http://ntfy_app_1", Some("http://ntfy_app_1")));
    assert!(config.should_trust_ntfy_server_url(" http://ntfy_app_1/ ", Some("http://ntfy_app_1")));
    assert!(config.should_trust_ntfy_server_url("http://ntfy_app_1", None));
    assert!(!config
        .should_trust_ntfy_server_url("http://ntfy_app_1", Some("http://another-private-host")));
    assert!(!config.should_trust_ntfy_server_url(
        "http://another-private-host",
        Some("http://another-private-host")
    ));
    assert!(!config.should_trust_ntfy_server_url(
        "https://ntfy.example.com",
        Some("https://ntfy.example.com")
    ));
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
}

#[test]
fn test_get_jwt_secret_cloud_mode_with_secret() {
    let config = test_config(NetworkConfig::Regtest);
    assert_eq!(config.get_jwt_secret().unwrap(), "test-jwt-secret");
}

#[test]
fn test_get_jwt_secret_cloud_mode_without_secret() {
    let config = AppConfig {
        jwt_secret: None, // Missing JWT secret
        ..test_config(NetworkConfig::Regtest)
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
        jwt_secret: Some("   ".to_string()),
        ..test_config_self_hosted(NetworkConfig::Regtest)
    };

    assert_eq!(
        config.get_jwt_secret().unwrap_err(),
        "JWT_SECRET required for self-hosted mode - check your .env file"
    );
}

#[test]
fn test_get_self_hosted_admin_password_rejects_blank_password() {
    let config = AppConfig {
        self_hosted_admin_password: Some("   ".to_string()),
        ..test_config_self_hosted(NetworkConfig::Regtest)
    };

    assert_eq!(
        config.get_self_hosted_admin_password().unwrap_err(),
        "CANARY_SELF_HOSTED_ADMIN_PASSWORD required for self-hosted mode - check your .env file"
    );
}

#[test]
fn test_is_stripe_enabled_requires_non_empty_values() {
    let stripe = |secret_key: &str, webhook_secret: &str| StripeSettings {
        secret_key: Some(SecretString::new(secret_key)),
        webhook_secret: Some(SecretString::new(webhook_secret)),
    };

    let mut config = test_config(NetworkConfig::Regtest);
    config.stripe = stripe(" ", "");
    assert!(!config.is_stripe_enabled());

    config.stripe = stripe("sk_test_123", "whsec_123");
    assert!(config.is_stripe_enabled());
}

#[test]
fn test_active_billing_provider_prefers_valid_btcpay_when_stripe_empty() {
    let env = HashMap::from([
        ("BTCPAY_CLOUD_OFFERING_ID", "offering-1"),
        ("BTCPAY_CLOUD_PERSONAL_PLAN_ID", "personal-1"),
        ("BTCPAY_CLOUD_TEAM_PLAN_ID", "team-1"),
        ("BTCPAY_CLOUD_PERSONAL_PRICE", "500"),
        ("BTCPAY_CLOUD_TEAM_PRICE", "1500"),
    ]);

    let mut config = test_config(NetworkConfig::Regtest).with_btcpay(
        Some("https://btcpay.example.com".to_string()),
        Some("api-key".to_string()),
        Some("store-id".to_string()),
        Some("offering-fallback".to_string()),
        Some("plan-fallback".to_string()),
    );
    config.stripe = StripeSettings {
        secret_key: Some(SecretString::new("")),
        webhook_secret: Some(SecretString::new(" ")),
    };
    config.btcpay_cloud = load::load_btcpay_cloud(&Env(&env));

    assert_eq!(
        config.active_billing_provider(),
        Some(BillingProvider::BtcPay)
    );
}

#[test]
fn test_btcpay_cloud_plan_config_rejects_empty_ids() {
    let env = HashMap::from([
        ("BTCPAY_CLOUD_OFFERING_ID", " "),
        ("BTCPAY_CLOUD_PERSONAL_PLAN_ID", " "),
        ("BTCPAY_CLOUD_TEAM_PLAN_ID", ""),
        ("BTCPAY_CLOUD_PERSONAL_PRICE", "500"),
        ("BTCPAY_CLOUD_TEAM_PRICE", "1500"),
    ]);

    let mut config = test_config(NetworkConfig::Regtest).with_btcpay(
        Some("https://btcpay.example.com".to_string()),
        Some("api-key".to_string()),
        Some("store-id".to_string()),
        Some(" ".to_string()),
        Some("plan-fallback".to_string()),
    );
    config.btcpay_cloud = load::load_btcpay_cloud(&Env(&env));

    assert!(config.btcpay_cloud_plan_config().is_none());
}

fn cloud_env() -> HashMap<&'static str, &'static str> {
    HashMap::from([("CANARY_MODE", "cloud")])
}

#[test]
fn load_reads_integration_settings_once() {
    let mut env = cloud_env();
    env.extend([
        ("CANARY_RESTORE_DRILL", " yes "),
        ("RESEND_API_KEY", "re_key"),
        ("RESEND_FROM_EMAIL", "alerts@example.invalid"),
        ("RESEND_AUDIENCE_ID", "audience-1"),
        ("CONTACT_FORM_EMAIL", "support@example.invalid"),
        ("TWILIO_ACCOUNT_SID", "ACsid"),
        ("TWILIO_AUTH_TOKEN", "twilio-token"),
        ("TWILIO_SENDER_ID", "Canary"),
        ("STRIPE_SECRET_KEY", " sk_untrimmed "),
        ("BTCPAY_WEBHOOK_SECRET", "  whsec  "),
        ("BTCPAY_CLOUD_CURRENCY", "nok"),
        ("BTCPAY_CLOUD_PERSONAL_PRICE", "-5"),
        ("ADMIN_NOTIFICATION_TOPIC", "admin-topic"),
        ("CANARY_TRUSTED_PROXY_IPS", "10.0.0.1, not-an-ip,::1"),
        ("TELEGRAM_BOT_TOKEN", "  "),
        ("CANARY_ADMIN_MFA_SECRETS_FILE", "/tmp/mfa.json"),
        ("LANG", "nb_NO.UTF-8"),
    ]);

    let config = AppConfig::load_from(args(NetworkConfig::Mainnet), &env).unwrap();

    assert!(config.is_restore_drill());
    let email = config.email();
    assert_eq!(
        email.resend_api_key.as_ref().map(SecretString::expose),
        Some("re_key")
    );
    assert_eq!(
        email.resend_from_email.as_deref(),
        Some("alerts@example.invalid")
    );
    assert_eq!(email.resend_from_name, None);
    assert_eq!(email.resend_audience_id.as_deref(), Some("audience-1"));
    assert_eq!(
        email.contact_form_email.as_deref(),
        Some("support@example.invalid")
    );
    assert_eq!(
        config.twilio().sms_credentials(),
        Some(("ACsid", "twilio-token", "Canary"))
    );
    assert_eq!(config.twilio().verify_service_sid, None);
    assert_eq!(
        config
            .stripe()
            .secret_key
            .as_ref()
            .map(SecretString::expose),
        Some(" sk_untrimmed ")
    );
    assert_eq!(config.btcpay_webhook_secret(), Some("whsec"));
    assert_eq!(config.btcpay_cloud.currency.as_deref(), Some("nok"));
    assert_eq!(config.btcpay_cloud.personal_monthly_price, None);
    assert_eq!(config.admin_notification_topic(), Some("admin-topic"));
    assert_eq!(
        config.trusted_proxy_ips(),
        &HashSet::from(["10.0.0.1".parse().unwrap(), "::1".parse().unwrap()])
    );
    assert_eq!(config.telegram_bot_token(), None);
    assert_eq!(config.nostr_socks_proxy(), None);
    assert_eq!(config.admin_mfa_secrets_file(), Path::new("/tmp/mfa.json"));
    assert_eq!(
        config.system_locale().currency_locale(),
        Some("nb_NO.UTF-8")
    );
}

#[test]
fn load_uses_defaults_for_unset_integration_settings() {
    let config = AppConfig::load_from(args(NetworkConfig::Regtest), &cloud_env()).unwrap();

    assert!(!config.is_restore_drill());
    assert!(config.email().resend_api_key.is_none());
    assert!(config.twilio().sms_credentials().is_none());
    assert!(config.admin_notification_topic().is_none());
    assert!(config.trusted_proxy_ips().is_empty());
    assert_eq!(
        config.admin_mfa_secrets_file(),
        Path::new("/run/secrets/canary-admin-mfa.json")
    );
    assert_eq!(config.system_locale(), &SystemLocale::default());
}

#[test]
fn load_reads_telegram_token_trimmed_and_locale_fallback() {
    let mut env = cloud_env();
    env.extend([
        ("TELEGRAM_BOT_TOKEN", " 123:abc "),
        ("LC_ALL", "de_DE.UTF-8"),
    ]);

    let config = AppConfig::load_from(args(NetworkConfig::Regtest), &env).unwrap();

    assert_eq!(config.telegram_bot_token(), Some("123:abc"));
    assert_eq!(
        config.system_locale().currency_locale(),
        Some("de_DE.UTF-8")
    );
    assert_eq!(config.system_locale().language_locale(), None);
}

#[test]
fn load_reads_tier_sync_overrides_for_the_configured_network_only() {
    let mut env = cloud_env();
    env.extend([
        ("CANARY_SYNC_INTERVAL_PERSONAL_MAINNET", "15"),
        ("CANARY_SYNC_INTERVAL_TEAM_MAINNET", "30"),
        ("CANARY_SYNC_INTERVAL_TEAM_REGTEST", "5"),
    ]);

    let mainnet = AppConfig::load_from(args(NetworkConfig::Mainnet), &env).unwrap();
    assert_eq!(
        mainnet.sync_interval_overrides(),
        SyncIntervalOverrides {
            personal_secs: Some(15),
            team_secs: Some(30),
        }
    );

    let regtest = AppConfig::load_from(args(NetworkConfig::Regtest), &env).unwrap();
    assert_eq!(
        regtest.sync_interval_overrides(),
        SyncIntervalOverrides {
            personal_secs: None,
            team_secs: Some(5),
        }
    );
}

#[test]
fn load_rejects_invalid_nostr_socks_proxy() {
    let mut env = cloud_env();
    env.insert("CANARY_NOSTR_SOCKS_PROXY", "example.com:9050");

    let error = AppConfig::load_from(args(NetworkConfig::Regtest), &env).unwrap_err();
    assert!(error
        .to_string()
        .starts_with("Invalid CANARY_NOSTR_SOCKS_PROXY"));

    env.insert("CANARY_NOSTR_SOCKS_PROXY", "socks5h://127.0.0.1:9050");
    let config = AppConfig::load_from(args(NetworkConfig::Regtest), &env).unwrap();
    assert_eq!(
        config.nostr_socks_proxy(),
        Some("127.0.0.1:9050".parse().unwrap())
    );
}

#[test]
fn cloud_validation_lists_every_missing_provider_variable() {
    let error = test_config(NetworkConfig::Mainnet)
        .validate_required_config()
        .unwrap_err();

    assert_eq!(
        error,
        "Cloud mode requires the following environment variables:\n  \
         - Either Stripe billing (STRIPE_SECRET_KEY + STRIPE_WEBHOOK_SECRET) or BTCPay cloud billing (BTCPAY_CLOUD_* plan config) must be configured\n  \
         - TWILIO_ACCOUNT_SID - Required for SMS notifications\n  \
         - TWILIO_AUTH_TOKEN - Required for SMS notifications\n  \
         - TWILIO_SENDER_ID - Required for SMS notifications\n  \
         - TWILIO_VERIFY_SERVICE_SID - Required for SMS contact verification\n  \
         - RESEND_API_KEY - Required for email notifications and verification\n  \
         - RESEND_FROM_EMAIL - Required for sending emails\n  \
         - RESEND_FROM_NAME - Required for email sender name"
    );
}

#[test]
fn cloud_validation_treats_empty_provider_values_as_present() {
    let mut env = cloud_env();
    env.extend([
        ("JWT_SECRET", "jwt"),
        ("FRONTEND_URL", "https://canary.example"),
        ("STRIPE_SECRET_KEY", "sk_live"),
        ("STRIPE_WEBHOOK_SECRET", "whsec"),
        ("TWILIO_ACCOUNT_SID", ""),
        ("TWILIO_AUTH_TOKEN", ""),
        ("TWILIO_SENDER_ID", ""),
        ("TWILIO_VERIFY_SERVICE_SID", ""),
        ("RESEND_API_KEY", ""),
        ("RESEND_FROM_EMAIL", ""),
        ("RESEND_FROM_NAME", ""),
    ]);

    let config = AppConfig::load_from(args(NetworkConfig::Mainnet), &env).unwrap();
    assert_eq!(config.validate_required_config(), Ok(()));
}
