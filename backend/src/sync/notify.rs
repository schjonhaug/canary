//! Transaction notifications published on the broadcast channel that feeds the
//! notification dispatcher.

use super::WalletSyncService;
use crate::metadata::{Transaction, TransactionInsert, TransactionNotification};
use tracing::debug;

impl WalletSyncService {
    /// Send notification for a new transaction (either pending or directly confirmed)
    pub(super) fn send_new_transaction_notification(&self, transaction: &TransactionInsert) {
        let tx = Transaction {
            txid: transaction.txid.clone(),
            wallet_checksum: transaction.wallet_checksum.clone(),
            transaction_type: transaction.transaction_type,
            amount_sats: transaction.amount_sats,
            fee_sats: transaction.fee_sats,
            block_height: transaction.block_height,
            first_seen_at: transaction.first_seen_at,
            confirmed_at: transaction.confirmed_at,
            parent_txid: transaction.parent_txid.clone(),
            transaction_status: transaction.transaction_status,
            replaced_by_txid: transaction.replaced_by_txid.clone(),
            replaced_at: transaction.replaced_at,
            notification_status: vec![], // Empty for new transactions
        };

        // Mined directly into a block, or still waiting in the mempool
        let notification = if tx.block_height.is_some() {
            TransactionNotification::Confirmed(tx)
        } else {
            TransactionNotification::Pending(tx)
        };
        self.broadcast_notification(notification, &transaction.wallet_checksum);
    }

    /// Send confirmation notification for a transaction that just got confirmed
    pub(super) fn send_confirmed_transaction_notification(&self, transaction: &Transaction) {
        self.broadcast_notification(
            TransactionNotification::Confirmed(transaction.clone()),
            &transaction.wallet_checksum,
        );
    }

    pub(super) fn send_replaced_transaction_notification(&self, transaction: &Transaction) {
        self.broadcast_notification(
            TransactionNotification::Pending(transaction.clone()),
            &transaction.wallet_checksum,
        );
    }

    /// Publishes a notification without failing the sync when nobody is listening.
    fn broadcast_notification(&self, notification: TransactionNotification, wallet_checksum: &str) {
        if self.notification_sender.send(notification).is_err() {
            debug!("[{wallet_checksum}] No notification listeners active");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::metadata::{
        Contact, EventType, Language, MetadataDb, NotificationMethod, ProviderType,
        TransactionStatus,
    };
    use crate::notifications::NotificationProvider;
    use crate::webhook_provider::{WebhookPayload, WebhookProvider};
    use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
    use std::sync::{Arc, Mutex as StdMutex};
    use tokio::sync::broadcast;

    async fn capture_sync_webhook(
        State(events): State<Arc<StdMutex<Vec<String>>>>,
        Json(payload): Json<WebhookPayload>,
    ) -> StatusCode {
        events.lock().unwrap().push(payload.event);
        StatusCode::NO_CONTENT
    }

    #[tokio::test]
    async fn sync_pending_and_confirmation_events_flow_to_webhook_delivery() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("metadata.sqlite");
        let config = AppConfig::new_for_test(
            crate::config::NetworkConfig::Regtest,
            Some("tcp://127.0.0.1:50001".to_string()),
            "127.0.0.1:3000".to_string(),
            temp_dir.path().to_string_lossy().to_string(),
            crate::config::OperatingMode::SelfHosted,
            None,
            None,
        );
        let metadata_db = MetadataDb::new(db_path.to_str().unwrap(), &config)
            .await
            .unwrap();
        let (notification_sender, mut notification_receiver) =
            broadcast::channel::<TransactionNotification>(10);
        let service = WalletSyncService::new(metadata_db, notification_sender, config);

        let captured_events = Arc::new(StdMutex::new(Vec::new()));
        let app = Router::new()
            .route("/hook", post(capture_sync_webhook))
            .with_state(captured_events.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let webhook_url = format!("http://{address}/hook");
        let webhook_provider = WebhookProvider::with_client(
            reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
        );
        let contact = Contact {
            id: Some("contact-1".to_string()),
            wallet_checksum: "wallet-checksum".to_string(),
            name: "Webhook Contact".to_string(),
            notification_methods: vec![NotificationMethod {
                id: Some("method-1".to_string()),
                contact_id: "contact-1".to_string(),
                provider_type: ProviderType::Webhook,
                notification_target: webhook_url,
                display_target: None,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                is_enabled: true,
                content_fields: crate::metadata::NotificationContentFields::detailed(false),
            }],
            created_at: "2026-01-01T00:00:00Z".to_string(),
            is_active: true,
            notify_sending: true,
            notify_sent: true,
            notify_receiving: true,
            notify_received: true,
            notify_cpfp: true,
            notify_rbf: true,
            include_wallet_balance_in_tx_notifications: false,
        };
        let pending_insert = TransactionInsert {
            txid: "55".repeat(32),
            wallet_checksum: "wallet-checksum".to_string(),
            transaction_type: EventType::Receive,
            amount_sats: 42_000,
            first_seen_at: 1_700_000_000,
            ..TransactionInsert::default()
        };

        service.send_new_transaction_notification(&pending_insert);
        let pending = notification_receiver.recv().await.unwrap();
        assert!(matches!(pending, TransactionNotification::Pending(_)));
        let pending_results = webhook_provider
            .send_notification(
                &pending,
                "Wallet",
                std::slice::from_ref(&contact),
                &Language::English,
                None,
            )
            .await;
        assert!(pending_results[0].1.success);

        let TransactionNotification::Pending(mut confirmed_transaction) = pending else {
            unreachable!()
        };
        confirmed_transaction.block_height = Some(900_000);
        confirmed_transaction.confirmed_at = Some(1_700_000_600);
        confirmed_transaction.transaction_status = TransactionStatus::Confirmed;
        service.send_confirmed_transaction_notification(&confirmed_transaction);
        let confirmed = notification_receiver.recv().await.unwrap();
        assert!(matches!(confirmed, TransactionNotification::Confirmed(_)));
        let confirmed_results = webhook_provider
            .send_notification(&confirmed, "Wallet", &[contact], &Language::English, None)
            .await;
        assert!(confirmed_results[0].1.success);

        assert_eq!(
            captured_events.lock().unwrap().as_slice(),
            ["receiving", "received"]
        );
    }
}
