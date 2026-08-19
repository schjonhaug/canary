use crate::metadata::{
    BalanceAlertNotification, BalanceAlertType, Contact, ContentPrivacyLevel, EventType, Language,
    NotificationMethod, ProviderType, Transaction, TransactionNotification,
};
use crate::notifications::NotificationProvider;
use crate::webhook_provider::{
    redact_webhook_url, validate_webhook_url, WebhookPayload, WebhookProvider,
    WEBHOOK_MAX_CONCURRENT_DELIVERIES, WEBHOOK_SCHEMA_VERSION,
};
use axum::{extract::State, http::StatusCode, response::Redirect, routing::post, Json, Router};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

fn transaction(direction: EventType, confirmed: bool) -> Transaction {
    Transaction {
        txid: "11".repeat(32),
        wallet_checksum: "wallet-checksum".to_string(),
        transaction_type: direction,
        amount_sats: 125_000,
        fee_sats: Some(500),
        block_height: confirmed.then_some(900_000),
        first_seen_at: 1_700_000_000,
        confirmed_at: confirmed.then_some(1_700_000_600),
        parent_txid: None,
        transaction_status: if confirmed { "confirmed" } else { "pending" }.to_string(),
        replaced_by_txid: None,
        replaced_at: None,
        notification_status: Vec::new(),
    }
}

fn contact(url: &str, suffix: usize) -> Contact {
    let contact_id = format!("contact-{suffix}");
    Contact {
        id: Some(contact_id.clone()),
        wallet_checksum: "wallet-checksum".to_string(),
        name: format!("Contact {suffix}"),
        notification_methods: vec![NotificationMethod {
            id: Some(format!("method-{suffix}")),
            contact_id,
            provider_type: ProviderType::Webhook,
            notification_target: url.to_string(),
            display_target: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            is_enabled: true,
            content_privacy_level: ContentPrivacyLevel::Detailed,
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
    }
}

fn payload_for(notification: TransactionNotification) -> WebhookPayload {
    payload_for_level(notification, ContentPrivacyLevel::Detailed)
}

fn payload_for_level(
    notification: TransactionNotification,
    content_privacy_level: ContentPrivacyLevel,
) -> WebhookPayload {
    WebhookPayload::for_notification(
        &notification,
        "Cold Storage",
        &contact("http://localhost/hook", 1),
        &Language::English,
        Some(250_000),
        content_privacy_level,
    )
}

fn test_provider() -> WebhookProvider {
    WebhookProvider::with_client(
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap(),
    )
}

fn balance_alert() -> BalanceAlertNotification {
    BalanceAlertNotification {
        id: "notification-id".to_string(),
        balance_alert_id: "alert-id".to_string(),
        wallet_checksum: "wallet-checksum".to_string(),
        contact_id: Some("contact-1".to_string()),
        threshold_sats: 100_000_000,
        current_balance_sats: 150_000_000,
        alert_type: BalanceAlertType::Above,
        notification_sent_at: 1_700_000_000,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        threshold_currency: Some("USD".to_string()),
        threshold_fiat_amount: Some(50_000.0),
        exchange_rate_snapshot: Some(60_000.0),
    }
}

#[tokio::test]
async fn validates_and_canonicalizes_public_http_urls() {
    assert_eq!(
        validate_webhook_url(" https://example.com/hooks/canary?token=secret ")
            .await
            .unwrap(),
        "https://example.com/hooks/canary?token=secret"
    );
    assert!(validate_webhook_url("http://127.0.0.1:8080/hook")
        .await
        .is_err());
    assert!(validate_webhook_url("http://[::1]:8080/hook")
        .await
        .is_err());

    for invalid in [
        "",
        "/relative",
        "ftp://example.com/hook",
        "https://user:secret@example.com/hook",
        "https://example.com/hook#fragment",
        "http:///missing-host",
    ] {
        assert!(
            validate_webhook_url(invalid).await.is_err(),
            "accepted {invalid}"
        );
    }
    assert!(
        validate_webhook_url(&format!("https://example.com/{}", "x".repeat(2_100)))
            .await
            .is_err()
    );
}

#[test]
fn redacts_webhook_secrets_to_the_origin() {
    assert_eq!(
        redact_webhook_url("https://hooks.example.com:8443/canary?token=super-secret"),
        "https://hooks.example.com:8443"
    );
}

#[test]
fn builds_every_transaction_payload_variant() {
    let mut sending = transaction(EventType::Send, false);
    assert_eq!(
        payload_for(TransactionNotification::Pending(sending.clone())).event,
        "sending"
    );

    let sent = transaction(EventType::Send, true);
    assert_eq!(
        payload_for(TransactionNotification::Confirmed(sent)).event,
        "sent"
    );

    let receiving = transaction(EventType::Receive, false);
    assert_eq!(
        payload_for(TransactionNotification::Pending(receiving)).event,
        "receiving"
    );

    let received = transaction(EventType::Receive, true);
    let received_payload = payload_for(TransactionNotification::Confirmed(received));
    assert_eq!(received_payload.event, "received");
    assert_eq!(received_payload.schema_version, WEBHOOK_SCHEMA_VERSION);
    assert_eq!(
        received_payload.wallet.as_ref().unwrap().name,
        Some("Cold Storage".to_string())
    );
    assert_eq!(received_payload.contact.as_ref().unwrap().name, "Contact 1");
    assert_eq!(
        received_payload.transaction.as_ref().unwrap().amount_sats,
        Some(125_000)
    );
    assert!(chrono::DateTime::parse_from_rfc3339(&received_payload.sent_at).is_ok());

    sending.transaction_status = "replaced".to_string();
    sending.replaced_by_txid = Some("22".repeat(32));
    assert_eq!(
        payload_for(TransactionNotification::Pending(sending)).event,
        "rbf"
    );

    let mut cpfp = transaction(EventType::Send, false);
    cpfp.parent_txid = Some("33".repeat(32));
    assert_eq!(
        payload_for(TransactionNotification::Pending(cpfp)).event,
        "cpfp"
    );
}

#[test]
fn builds_balance_alert_and_test_payloads() {
    let payload = payload_for(TransactionNotification::BalanceAlert(balance_alert()));
    assert_eq!(payload.event, "balance_alert");
    assert!(payload.transaction.is_none());
    assert_eq!(
        payload.balance_alert.as_ref().unwrap().current_fiat_amount,
        Some(90_000.0)
    );

    let test = WebhookPayload::test(&Language::English);
    assert_eq!(test.event, "test");
    assert!(test.wallet.is_none());
    assert!(test.contact.is_none());
    assert!(test.transaction.is_none());
    assert!(test.balance_alert.is_none());
}

#[test]
fn privacy_levels_omit_excluded_webhook_fields_for_every_event() {
    let mut rbf = transaction(EventType::Send, false);
    rbf.transaction_status = "replaced".to_string();
    rbf.replaced_by_txid = Some("22".repeat(32));
    let mut cpfp = transaction(EventType::Send, false);
    cpfp.parent_txid = Some("33".repeat(32));
    let notifications = vec![
        TransactionNotification::Pending(transaction(EventType::Send, false)),
        TransactionNotification::Confirmed(transaction(EventType::Send, true)),
        TransactionNotification::Pending(transaction(EventType::Receive, false)),
        TransactionNotification::Confirmed(transaction(EventType::Receive, true)),
        TransactionNotification::Pending(rbf),
        TransactionNotification::Pending(cpfp),
        TransactionNotification::BalanceAlert(balance_alert()),
    ];

    for notification in notifications {
        let minimal = serde_json::to_value(payload_for_level(
            notification.clone(),
            ContentPrivacyLevel::Minimal,
        ))
        .unwrap();
        assert!(minimal.get("wallet").is_none());
        assert!(minimal.get("contact").is_none());
        assert!(minimal.get("transaction").is_none());
        assert!(minimal.get("balance_alert").is_none());
        let minimal_text = minimal.to_string();
        for excluded in [
            "Cold Storage",
            "wallet-checksum",
            "Contact 1",
            "125000",
            "250000",
            &"11".repeat(32),
            &"22".repeat(32),
            &"33".repeat(32),
        ] {
            assert!(
                !minimal_text.contains(excluded),
                "Minimal leaked {excluded}"
            );
        }

        let standard = serde_json::to_value(payload_for_level(
            notification,
            ContentPrivacyLevel::Standard,
        ))
        .unwrap();
        assert_eq!(standard["wallet"]["name"], "Cold Storage");
        assert!(standard["wallet"].get("checksum").is_none());
        assert!(standard["wallet"].get("balance_sats").is_none());
        assert!(standard.get("contact").is_none());
        if let Some(transaction) = standard.get("transaction") {
            assert!(transaction.get("direction").is_some());
            assert!(transaction.get("status").is_some());
            for key in [
                "txid",
                "amount_sats",
                "fee_sats",
                "block_height",
                "first_seen_at",
                "confirmed_at",
                "parent_txid",
                "replaced_by_txid",
                "replaced_at",
            ] {
                assert!(transaction.get(key).is_none(), "Standard included {key}");
            }
        }
        if let Some(alert) = standard.get("balance_alert") {
            assert!(alert.get("alert_type").is_some());
            for key in [
                "id",
                "alert_id",
                "threshold_sats",
                "current_balance_sats",
                "threshold_currency",
                "threshold_fiat_amount",
                "exchange_rate_snapshot",
                "current_fiat_amount",
            ] {
                assert!(alert.get(key).is_none(), "Standard included {key}");
            }
        }
        let standard_text = standard.to_string();
        for excluded in ["wallet-checksum", "Contact 1", "125000", "250000"] {
            assert!(
                !standard_text.contains(excluded),
                "Standard leaked {excluded}"
            );
        }
    }
}

async fn capture_payload(
    State(captured): State<Arc<Mutex<Vec<WebhookPayload>>>>,
    Json(payload): Json<WebhookPayload>,
) -> StatusCode {
    captured.lock().unwrap().push(payload);
    StatusCode::NO_CONTENT
}

#[tokio::test]
async fn posts_json_and_accepts_any_2xx_status() {
    let captured = Arc::new(Mutex::new(Vec::new()));
    let app = Router::new()
        .route("/hook", post(capture_payload))
        .route("/created", post(|| async { StatusCode::CREATED }))
        .route("/accepted", post(|| async { StatusCode::ACCEPTED }))
        .route("/bad-request", post(|| async { StatusCode::BAD_REQUEST }))
        .route(
            "/server-error",
            post(|| async { StatusCode::INTERNAL_SERVER_ERROR }),
        )
        .with_state(captured.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let provider = test_provider();
    let result = provider
        .send_payload(
            &format!("http://{address}/hook"),
            &WebhookPayload::test(&Language::English),
        )
        .await;
    assert!(result.success);
    assert_eq!(captured.lock().unwrap().len(), 1);

    for path in ["created", "accepted"] {
        let result = provider
            .send_payload(
                &format!("http://{address}/{path}"),
                &WebhookPayload::test(&Language::English),
            )
            .await;
        assert!(result.success, "expected {path} to succeed");
    }
    for (path, expected_error) in [("bad-request", "HTTP 400"), ("server-error", "HTTP 500")] {
        let result = provider
            .send_payload(
                &format!("http://{address}/{path}"),
                &WebhookPayload::test(&Language::English),
            )
            .await;
        assert!(!result.success);
        assert_eq!(result.error_message.as_deref(), Some(expected_error));
    }
}

#[tokio::test]
async fn delivers_every_event_variant_to_a_disposable_receiver() {
    let captured = Arc::new(Mutex::new(Vec::new()));
    let app = Router::new()
        .route("/hook", post(capture_payload))
        .with_state(captured.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let url = format!("http://{address}/hook");

    let mut rbf = transaction(EventType::Send, false);
    rbf.transaction_status = "replaced".to_string();
    rbf.replaced_by_txid = Some("22".repeat(32));
    let mut cpfp = transaction(EventType::Send, false);
    cpfp.parent_txid = Some("33".repeat(32));
    let payloads = vec![
        payload_for(TransactionNotification::Pending(transaction(
            EventType::Send,
            false,
        ))),
        payload_for(TransactionNotification::Confirmed(transaction(
            EventType::Send,
            true,
        ))),
        payload_for(TransactionNotification::Pending(transaction(
            EventType::Receive,
            false,
        ))),
        payload_for(TransactionNotification::Confirmed(transaction(
            EventType::Receive,
            true,
        ))),
        payload_for(TransactionNotification::Pending(rbf)),
        payload_for(TransactionNotification::Pending(cpfp)),
        payload_for(TransactionNotification::BalanceAlert(balance_alert())),
        WebhookPayload::test(&Language::English),
    ];

    let provider = test_provider();
    for payload in &payloads {
        assert!(provider.send_payload(&url, payload).await.success);
    }

    let events: Vec<_> = captured
        .lock()
        .unwrap()
        .iter()
        .map(|payload| payload.event.clone())
        .collect();
    assert_eq!(
        events,
        [
            "sending",
            "sent",
            "receiving",
            "received",
            "rbf",
            "cpfp",
            "balance_alert",
            "test",
        ]
    );
}

#[tokio::test]
async fn does_not_follow_redirects() {
    let followed = Arc::new(AtomicUsize::new(0));
    let followed_state = followed.clone();
    let app = Router::new()
        .route("/hook", post(|| async { Redirect::temporary("/accepted") }))
        .route(
            "/accepted",
            post(move || {
                let followed = followed_state.clone();
                async move {
                    followed.fetch_add(1, Ordering::SeqCst);
                    StatusCode::NO_CONTENT
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let result = test_provider()
        .send_payload(
            &format!("http://{address}/hook"),
            &WebhookPayload::test(&Language::English),
        )
        .await;
    assert!(!result.success);
    assert_eq!(result.error_message.as_deref(), Some("HTTP 307"));
    assert_eq!(followed.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn does_not_retry_failed_deliveries() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_state = attempts.clone();
    let app = Router::new().route(
        "/hook",
        post(move || {
            let attempts = attempts_state.clone();
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let result = test_provider()
        .send_payload(
            &format!("http://{address}/hook"),
            &WebhookPayload::test(&Language::English),
        )
        .await;
    assert!(!result.success);
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn reports_timeouts_without_exposing_url_secrets() {
    let app = Router::new().route(
        "/hook",
        post(|| async {
            tokio::time::sleep(Duration::from_millis(200)).await;
            StatusCode::NO_CONTENT
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(50))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    let result = WebhookProvider::with_client(client)
        .send_payload(
            &format!("http://{address}/hook?token=secret"),
            &WebhookPayload::test(&Language::English),
        )
        .await;
    assert!(!result.success);
    let error = result.error_message.unwrap();
    assert!(error.contains("timed out"));
    assert!(!error.contains("token"));
    assert!(!error.contains("secret"));
}

#[derive(Clone)]
struct ConcurrencyState {
    active: Arc<AtomicUsize>,
    maximum: Arc<AtomicUsize>,
}

async fn track_concurrency(State(state): State<ConcurrencyState>) -> StatusCode {
    let active = state.active.fetch_add(1, Ordering::SeqCst) + 1;
    state.maximum.fetch_max(active, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(60)).await;
    state.active.fetch_sub(1, Ordering::SeqCst);
    StatusCode::NO_CONTENT
}

#[tokio::test]
async fn bounds_concurrent_deliveries() {
    let state = ConcurrencyState {
        active: Arc::new(AtomicUsize::new(0)),
        maximum: Arc::new(AtomicUsize::new(0)),
    };
    let app = Router::new()
        .route("/hook", post(track_concurrency))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let url = format!("http://{address}/hook");
    let contacts: Vec<_> = (0..9).map(|index| contact(&url, index)).collect();

    let results = test_provider()
        .send_notification(
            &TransactionNotification::Pending(transaction(EventType::Receive, false)),
            "Wallet",
            &contacts,
            &Language::English,
            None,
        )
        .await;
    assert_eq!(results.len(), 9);
    assert!(results.iter().all(|(_, result, _)| result.success));
    let maximum = state.maximum.load(Ordering::SeqCst);
    assert!(maximum > 1);
    assert!(maximum <= WEBHOOK_MAX_CONCURRENT_DELIVERIES);
}
