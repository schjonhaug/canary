mod common;

use bdk_wallet::rusqlite::Connection;
use canary::metadata::{
    BalanceAlertType, CreateBalanceAlertInput, EventType, NotificationDeliveryStatus,
    NotificationLogParams, ProviderType, TransactionInsert,
};
use common::{db_path, self_hosted_metadata_db};
use tempfile::TempDir;

#[tokio::test]
async fn webhook_notification_log_snapshots_store_only_the_origin() {
    let temp_dir = TempDir::new().unwrap();
    let db = self_hosted_metadata_db(&temp_dir).await;
    let user_id = db
        .create_user(
            "webhook@example.com",
            "hash",
            Some("User"),
            false,
            None,
            None,
        )
        .await
        .unwrap();
    let wallet_checksum = db
        .insert_wallet("Wallet", "descriptor", &user_id)
        .await
        .unwrap();
    let full_url = "https://hooks.example.com:8443/canary/events?token=super-secret";
    let contact_id = db
        .insert_contact_with_notification_methods(
            &wallet_checksum,
            "Webhook Contact",
            vec![(ProviderType::Webhook, full_url.to_string())],
        )
        .await
        .unwrap();
    let contact = db
        .get_contacts_with_notification_methods(&wallet_checksum)
        .await
        .unwrap()
        .into_iter()
        .find(|contact| contact.id.as_deref() == Some(&contact_id))
        .unwrap();
    let method_id = contact.notification_methods[0].id.as_deref().unwrap();
    assert_eq!(
        contact.notification_methods[0].notification_target,
        full_url
    );
    assert_eq!(
        contact.notification_methods[0].display_target.as_deref(),
        Some("https://hooks.example.com:8443")
    );

    let txid = "44".repeat(32);
    db.insert_transaction(&TransactionInsert {
        txid: txid.clone(),
        wallet_checksum: wallet_checksum.clone(),
        transaction_type: EventType::Receive,
        amount_sats: 1_000,
        first_seen_at: 1_700_000_000,
        ..TransactionInsert::default()
    })
    .await
    .unwrap();
    let log_params = NotificationLogParams {
        notification_method_id: method_id,
        provider_name: "webhook",
        provider_message_id: None,
        status: NotificationDeliveryStatus::Sent,
        error_message: None,
        message_content: "message",
    };
    db.insert_notification_log_for_transaction(&txid, &wallet_checksum, &log_params, "receiving")
        .await
        .unwrap();

    let alert = db
        .create_balance_alert_with_contact(CreateBalanceAlertInput {
            wallet_checksum: &wallet_checksum,
            contact_id: Some(&contact_id),
            threshold_sats: 10_000,
            alert_type: BalanceAlertType::Below,
            threshold_currency: None,
            threshold_fiat_amount: None,
            current_balance_sats: Some(20_000),
        })
        .await
        .unwrap();
    db.insert_notification_log_for_balance_alert(&alert.id, &wallet_checksum, &log_params)
        .await
        .unwrap();

    let conn = Connection::open(db_path(temp_dir.path())).unwrap();
    for table in ["notification_logs", "balance_alert_notification_logs"] {
        let snapshot: String = conn
            .query_row(
                &format!("SELECT notification_target_snapshot FROM {table} LIMIT 1"),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(snapshot, "https://hooks.example.com:8443");
        assert!(!snapshot.contains("super-secret"));
    }
}
