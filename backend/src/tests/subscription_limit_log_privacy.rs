use crate::api::AppServices;
use crate::config::{AppConfig, NetworkConfig, OperatingMode};
use crate::metadata::{ContactNotificationSettings, NotificationContentFields, ProviderType};
use crate::wallet::{WalletCreationService, WalletManager};
use bdk_wallet::bitcoin::Network;
use std::io::Write;
use std::sync::{Arc, Mutex};
use tempfile::tempdir;
use tokio::sync::broadcast;

#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn subscription_limit_updates_do_not_log_user_wallet_or_contact_identifiers() {
    let directory = tempdir().unwrap();
    let config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        directory.path().to_string_lossy().to_string(),
        OperatingMode::Cloud,
        None,
        Some("test-jwt-secret".to_string()),
    );
    let (event_tx, _event_rx) = broadcast::channel(8);
    let wallet_manager = Arc::new(
        WalletManager::new(
            event_tx,
            directory.path().to_path_buf(),
            directory.path().join("test.sqlite").to_str().unwrap(),
            Network::Regtest,
            "tcp://127.0.0.1:50001",
            &config,
        )
        .await,
    );
    let app_services = AppServices {
        metadata_db: wallet_manager.metadata_db.clone(),
        wallet_creation_service: WalletCreationService::new(
            wallet_manager.wallet_dir.clone(),
            wallet_manager.metadata_db.clone(),
            wallet_manager.get_electrum_client().await,
            wallet_manager.get_network(),
            wallet_manager.clone(),
        ),
    };

    let user_id = wallet_manager
        .metadata_db
        .create_user(
            "synthetic-limit@example.invalid",
            "hashedpassword",
            Some("Synthetic Limit User"),
            false,
            None,
            None,
        )
        .await
        .unwrap();
    let first_wallet = wallet_manager
        .metadata_db
        .insert_wallet("Synthetic Wallet One", "descriptor-one", &user_id)
        .await
        .unwrap();
    let second_wallet = wallet_manager
        .metadata_db
        .insert_wallet("Synthetic Wallet Two", "descriptor-two", &user_id)
        .await
        .unwrap();
    let first_contact = wallet_manager
        .metadata_db
        .insert_contact_with_notification_methods(
            &first_wallet,
            "Synthetic Contact One",
            vec![(
                ProviderType::Email,
                "synthetic-contact@example.invalid".to_string(),
            )],
        )
        .await
        .unwrap();
    let second_contact = wallet_manager
        .metadata_db
        .insert_contact_with_notification_settings(
            &first_wallet,
            "Synthetic Contact Two",
            vec![(
                ProviderType::Ntfy,
                "synthetic-topic".to_string(),
                true,
                NotificationContentFields::standard(),
            )],
            ContactNotificationSettings::defaults_for_new_contact(),
        )
        .await
        .unwrap();

    let captured = Arc::new(Mutex::new(Vec::new()));
    let writer = Capture(captured.clone());
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    app_services
        .apply_subscription_limits(&user_id, "personal", "active", false, None, None)
        .await
        .unwrap();
    wallet_manager
        .apply_subscription_limits(&user_id, "personal", "past_due", false, None, None)
        .await
        .unwrap();

    let logs = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
    assert!(logs.contains("Applying subscription tier limits"));
    assert!(logs.contains("Deactivating wallets for an inactive subscription"));
    for sensitive in [
        user_id.as_str(),
        first_wallet.as_str(),
        second_wallet.as_str(),
        first_contact.as_str(),
        second_contact.as_str(),
        "synthetic-limit@example.invalid",
        "Synthetic Limit User",
        "Synthetic Wallet One",
        "Synthetic Wallet Two",
        "Synthetic Contact One",
        "Synthetic Contact Two",
        "synthetic-contact@example.invalid",
        "synthetic-topic",
        "personal",
        "past_due",
    ] {
        assert!(
            !logs.contains(sensitive),
            "Sensitive fixture appeared in captured logs: {sensitive}\n{logs}"
        );
    }
}
