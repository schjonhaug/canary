use crate::config::{AppConfig, NetworkConfig, OperatingMode};
use crate::metadata::{
    BalanceAlertType, ContactNotificationSettings, CreateBalanceAlertInput, MetadataDb,
    NotificationContentFields, ProviderType,
};
use crate::nostr_provider::canonicalize_nostr_public_key;
use crate::test_notification::{
    load_saved_test_config, notification_destination_matches, SavedTestConfigError, SavedTestIds,
};
use nostr_sdk::prelude::{Keys, ToBech32};
use tempfile::tempdir;

async fn create_test_db() -> (MetadataDb, tempfile::TempDir) {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let test_config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        temp_dir.path().to_string_lossy().to_string(),
        OperatingMode::SelfHosted,
        None,
        None,
    );
    let db = MetadataDb::new(db_path.to_str().unwrap(), &test_config)
        .await
        .unwrap();
    (db, temp_dir)
}

struct Fixture {
    user_id: String,
    other_user_id: String,
    wallet_checksum: String,
    contact_id: String,
    method_id: String,
}

async fn setup_ntfy_contact(db: &MetadataDb) -> Fixture {
    let user_id = db
        .create_user(
            "owner@example.com",
            "hashedpassword",
            Some("Owner"),
            true,
            None,
            None,
        )
        .await
        .unwrap();
    let other_user_id = db
        .create_user(
            "other@example.com",
            "hashedpassword",
            Some("Other"),
            true,
            None,
            None,
        )
        .await
        .unwrap();
    let wallet_checksum = db
        .insert_wallet("Wallet", "descriptor", &user_id)
        .await
        .unwrap();
    let contact_id = db
        .insert_contact_with_notification_settings(
            &wallet_checksum,
            "Alice",
            vec![(
                ProviderType::Ntfy,
                "alice-topic".to_string(),
                true,
                NotificationContentFields::standard(),
            )],
            ContactNotificationSettings {
                notify_sending: true,
                notify_sent: false,
                notify_receiving: true,
                notify_received: false,
                notify_cpfp: false,
                notify_rbf: true,
                include_wallet_balance_in_tx_notifications: false,
            },
        )
        .await
        .unwrap();
    let contact = db
        .get_single_contact_with_methods(&contact_id, &wallet_checksum)
        .await
        .unwrap()
        .unwrap();
    let method_id = contact.notification_methods[0].id.clone().unwrap();
    db.create_balance_alert_with_contact(CreateBalanceAlertInput {
        wallet_checksum: &wallet_checksum,
        contact_id: Some(&contact_id),
        threshold_sats: 1000,
        alert_type: BalanceAlertType::Below,
        threshold_currency: None,
        threshold_fiat_amount: None,
        current_balance_sats: None,
    })
    .await
    .unwrap();
    db.create_balance_alert(
        &wallet_checksum,
        5000,
        BalanceAlertType::Above,
        None,
        None,
        None,
    )
    .await
    .unwrap();

    Fixture {
        user_id,
        other_user_id,
        wallet_checksum,
        contact_id,
        method_id,
    }
}

#[tokio::test]
async fn loads_saved_ntfy_config_and_counts_contact_alerts() {
    let (db, _temp_dir) = create_test_db().await;
    let fixture = setup_ntfy_contact(&db).await;

    let config = load_saved_test_config(
        &db,
        &fixture.user_id,
        false,
        &SavedTestIds {
            wallet_checksum: fixture.wallet_checksum.clone(),
            contact_id: fixture.contact_id.clone(),
            method_id: fixture.method_id.clone(),
        },
        ProviderType::Ntfy,
        "alice-topic",
    )
    .await
    .unwrap();

    assert!(config.notify_sending);
    assert!(config.notify_receiving);
    assert!(!config.notify_sent);
    assert!(config.notify_rbf);
    assert!(!config.notify_cpfp);
    assert_eq!(config.balance_alert_count, 1);
    assert!(config.content_fields.wallet_name);
    assert!(!config.content_fields.transaction_amount);
}

#[tokio::test]
async fn rejects_destination_mismatch_disabled_method_and_wrong_provider() {
    let (db, _temp_dir) = create_test_db().await;
    let enabled = setup_ntfy_contact(&db).await;
    let mismatch = load_saved_test_config(
        &db,
        &enabled.user_id,
        false,
        &SavedTestIds {
            wallet_checksum: enabled.wallet_checksum.clone(),
            contact_id: enabled.contact_id.clone(),
            method_id: enabled.method_id.clone(),
        },
        ProviderType::Ntfy,
        "other-topic",
    )
    .await
    .unwrap_err();
    assert_eq!(mismatch, SavedTestConfigError::DestinationMismatch);

    let provider_mismatch = load_saved_test_config(
        &db,
        &enabled.user_id,
        false,
        &SavedTestIds {
            wallet_checksum: enabled.wallet_checksum.clone(),
            contact_id: enabled.contact_id.clone(),
            method_id: enabled.method_id.clone(),
        },
        ProviderType::Webhook,
        "alice-topic",
    )
    .await
    .unwrap_err();
    assert_eq!(provider_mismatch, SavedTestConfigError::ProviderMismatch);

    let disabled_contact_id = db
        .insert_contact_with_notification_settings(
            &enabled.wallet_checksum,
            "Disabled",
            vec![(
                ProviderType::Ntfy,
                "alice-topic".to_string(),
                false,
                NotificationContentFields::standard(),
            )],
            ContactNotificationSettings::defaults_for_new_contact(),
        )
        .await
        .unwrap();
    let disabled_contact = db
        .get_single_contact_with_methods(&disabled_contact_id, &enabled.wallet_checksum)
        .await
        .unwrap()
        .unwrap();
    let disabled_error = load_saved_test_config(
        &db,
        &enabled.user_id,
        false,
        &SavedTestIds {
            wallet_checksum: enabled.wallet_checksum,
            contact_id: disabled_contact_id,
            method_id: disabled_contact.notification_methods[0].id.clone().unwrap(),
        },
        ProviderType::Ntfy,
        "alice-topic",
    )
    .await
    .unwrap_err();
    assert_eq!(disabled_error, SavedTestConfigError::MethodDisabled);
}

#[tokio::test]
async fn rejects_access_for_another_users_wallet() {
    let (db, _temp_dir) = create_test_db().await;
    let fixture = setup_ntfy_contact(&db).await;
    let error = load_saved_test_config(
        &db,
        &fixture.other_user_id,
        false,
        &SavedTestIds {
            wallet_checksum: fixture.wallet_checksum,
            contact_id: fixture.contact_id,
            method_id: fixture.method_id,
        },
        ProviderType::Ntfy,
        "alice-topic",
    )
    .await
    .unwrap_err();
    assert_eq!(error, SavedTestConfigError::AccessDenied);
}

#[tokio::test]
async fn matches_nostr_npub_to_stored_hex() {
    let keys = Keys::generate();
    let hex = keys.public_key().to_hex();
    let npub = keys.public_key().to_bech32().unwrap();
    let method = crate::metadata::NotificationMethod {
        id: Some("method".into()),
        contact_id: "contact".into(),
        provider_type: ProviderType::Nostr,
        notification_target: hex.clone(),
        display_target: Some(npub.clone()),
        created_at: "2024-01-01T00:00:00Z".into(),
        is_enabled: true,
        content_fields: NotificationContentFields::standard(),
    };

    assert!(notification_destination_matches(
        ProviderType::Nostr,
        &npub,
        &method
    ));
    assert!(notification_destination_matches(
        ProviderType::Nostr,
        &hex,
        &method
    ));
    assert!(!notification_destination_matches(
        ProviderType::Nostr,
        Keys::generate().public_key().to_hex().as_str(),
        &method
    ));
    assert_eq!(
        canonicalize_nostr_public_key(&npub).unwrap().0,
        hex.to_lowercase()
    );
}
