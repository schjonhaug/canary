use axum::{
    body::Body,
    http::{HeaderMap, Request, StatusCode},
    routing::post,
    Router,
};
use canary::{
    api::{create_router_with_services, AppServices},
    auth::{AuthService, Claims},
    config::{AppConfig, NetworkConfig, NtfyServerConfig, OperatingMode},
    electrum::ElectrumClientManager,
    notifications::NotificationManager,
    wallet::{WalletCreationService, WalletManager},
    WebhookProvider,
};
use http_body_util::BodyExt;
use jsonwebtoken::{encode, EncodingKey, Header};
use serde_json::{json, Value};
use std::sync::Arc;
use tempfile::{tempdir, TempDir};
use tokio::sync::{broadcast, mpsc, Mutex};
use tower::ServiceExt;
const TEST_JWT_SECRET: &str = "test-jwt-secret";
async fn create_test_app(
    configure: impl FnOnce(AppConfig) -> AppConfig,
) -> (axum::Router, TempDir, String) {
    let temp_dir = tempdir().unwrap();
    let temp_path = temp_dir.path().to_str().unwrap();
    let test_db_path = format!("{}/test_metadata.sqlite", temp_path);

    let test_config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        temp_path.to_string(),
        OperatingMode::SelfHosted,
        None,
        Some(TEST_JWT_SECRET.to_string()),
    );

    let test_config = configure(test_config);

    let route_mode = test_config.operating_mode.clone();
    let mut test_config = test_config;
    test_config.operating_mode = OperatingMode::SelfHosted;
    let (event_tx, _event_rx) =
        broadcast::channel::<canary::metadata::TransactionNotification>(100);
    let wallet_manager = Arc::new(
        WalletManager::new(
            event_tx,
            temp_path.into(),
            &test_db_path,
            bdk_wallet::bitcoin::Network::Regtest,
            "tcp://127.0.0.1:50001",
            &test_config,
        )
        .await,
    );

    let app_services = {
        let electrum_client = wallet_manager.get_electrum_client().await;
        let wallet_creation_service = WalletCreationService::new(
            wallet_manager.wallet_dir.clone(),
            wallet_manager.metadata_db.clone(),
            electrum_client,
            wallet_manager.get_network(),
            wallet_manager.clone(),
        );
        Arc::new(AppServices {
            metadata_db: wallet_manager.metadata_db.clone(),
            wallet_creation_service,
        })
    };

    if test_config.is_self_hosted_mode() {
        create_self_hosted_admin_session(&app_services).await;
    }

    let mut manager = NotificationManager::new();
    if test_config.is_self_hosted_mode() {
        manager.register_provider(Arc::new(WebhookProvider::new()));
    }
    let notification_manager = Arc::new(Mutex::new(manager));
    let electrum_manager = Some(Arc::new(ElectrumClientManager::new_mock_connected()));

    test_config.operating_mode = route_mode;
    let router = create_router_with_services(
        app_services,
        notification_manager,
        None,
        test_config,
        electrum_manager,
    );

    (router, temp_dir, test_db_path)
}

fn self_hosted_admin_token() -> String {
    token_for_role(true)
}

fn token_for_role(is_admin: bool) -> String {
    let claims = Claims {
        sub: "foss-user".to_string(),
        email: "admin@local".to_string(),
        is_admin,
        is_demo: false,
        exp: 4_102_444_800,
        iat: 1_700_000_000,
        jti: "test-foss-user-admin".to_string(),
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(TEST_JWT_SECRET.as_ref()),
    )
    .unwrap()
}

async fn create_self_hosted_admin_session(app_services: &AppServices) {
    let token = self_hosted_admin_token();
    app_services
        .metadata_db
        .create_session(
            "foss-user",
            &AuthService::hash_token(&token),
            chrono::Utc::now() + chrono::Duration::days(7),
        )
        .await
        .unwrap();
}

async fn request(app: &Router, method: &str, path: &str, payload: Value) -> (StatusCode, Value) {
    request_with_token(app, method, path, payload, &self_hosted_admin_token()).await
}
async fn request_with_token(
    app: &Router,
    method: &str,
    path: &str,
    payload: Value,
    token: &str,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(path)
                .method(method)
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn self_hosted_saves_private_ntfy_addresses() {
    let (app, _dir, _) = create_test_app(|c| c).await;
    for url in [
        "http://10.0.0.2",
        "http://172.16.0.2",
        "http://192.168.1.2",
        "http://127.0.0.1:8080",
        "http://[::1]:8080",
        "http://[fd00::1]",
        "http://100.64.0.1",
        "http://[::ffff:192.168.1.2]",
    ] {
        let (status, body) = request(
            &app,
            "PUT",
            "/api/user/preferences",
            json!({"ntfy_server_url": url}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{url}: {body}");
        assert_eq!(body["ntfy_server_url"], url);
    }
}

async fn receiver(
    status: StatusCode,
) -> (
    String,
    mpsc::Receiver<(HeaderMap, String)>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel(8);
    let app = Router::new().route(
        "/private-topic",
        post(move |headers: HeaderMap, body: String| {
            let tx = tx.clone();
            async move {
                tx.send((headers, body)).await.unwrap();
                (
                    status,
                    [("location", "http://127.0.0.1:1/redirect")],
                    axum::Json(json!({"id":"mock-id"})),
                )
            }
        }),
    );
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, rx, task)
}

#[tokio::test]
async fn preexisting_private_preference_delivers_test_notification() {
    let (url, mut rx, task) = receiver(StatusCode::OK).await;
    let (app, _dir, db) = create_test_app(|c| c).await;
    let conn = bdk_wallet::rusqlite::Connection::open(db).unwrap();
    conn.execute("UPDATE users SET ntfy_server_url = ?1, ntfy_access_token = 'private-token' WHERE id = 'foss-user'", [&url]).unwrap();
    let (status, body) = request(
        &app,
        "POST",
        "/api/ntfy/test",
        json!({"topic":"private-topic"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true, "{body}");
    let (headers, body) = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(headers["authorization"], "Bearer private-token");
    assert!(
        body.contains("test notification from Canary Wallet"),
        "{body}"
    );
    assert_eq!(headers["title"], "Test Notification");
    task.abort();
}

#[tokio::test]
async fn rejected_destinations_preserve_saved_url_and_credentials() {
    let (app, _dir, db) = create_test_app(|c| c).await;
    let conn = bdk_wallet::rusqlite::Connection::open(db).unwrap();
    conn.execute("UPDATE users SET ntfy_server_url = 'http://192.168.1.2', ntfy_access_token = 'keep-token' WHERE id = 'foss-user'", []).unwrap();
    for url in [
        "http://169.254.169.254",
        "http://0.0.0.0",
        "http://224.0.0.1",
        "http://255.255.255.255",
        "http://[fe80::1]",
        "http://[::]",
        "http://[ff02::1]",
    ] {
        let (status, body) = request(
            &app,
            "PUT",
            "/api/user/preferences",
            json!({"ntfy_server_url":url,"ntfy_access_token":"replace-token"}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{url}: {body}");
        assert_eq!(body["error_code"], "invalid_ntfy_url", "{body}");
        let saved: (String, String) = conn
            .query_row(
                "SELECT ntfy_server_url, ntfy_access_token FROM users WHERE id = 'foss-user'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(saved, ("http://192.168.1.2".into(), "keep-token".into()));
    }
}

fn integration(url: &str, platform: &str, managed_auth: bool) -> NtfyServerConfig {
    NtfyServerConfig {
        id: "local".into(),
        name: "Local ntfy".into(),
        base_url: url.into(),
        platform: Some(platform.into()),
        default_topic: None,
        managed_auth,
    }
}

#[tokio::test]
async fn deployment_defaults_and_saved_preferences_keep_auth_scoped() {
    // Umbrel migrated preference, StartOS managed/default and explicit override,
    // and standalone/myNode fallback defaults all use the same backend routing.
    for (platform, saved, expected_auth) in [
        ("umbrel", true, None),
        ("startos", false, Some("Bearer managed-token")),
        ("startos", true, None),
        ("docker", false, None),
        ("mynode", false, None),
    ] {
        let (url, mut rx, task) = receiver(StatusCode::OK).await;
        let (app, _dir, db) = create_test_app(|c| {
            let c = c
                .with_ntfy_fallback_url(&url)
                .with_managed_ntfy_access_token("managed-token");
            if matches!(platform, "umbrel" | "startos") {
                c.with_ntfy_servers(vec![integration(&url, platform, platform == "startos")])
            } else {
                c
            }
        })
        .await;
        if saved {
            let conn = bdk_wallet::rusqlite::Connection::open(db).unwrap();
            conn.execute(
                "UPDATE users SET ntfy_server_url = ?1 WHERE id = 'foss-user'",
                [&url],
            )
            .unwrap();
        }
        let (_, body) = request(
            &app,
            "POST",
            "/api/ntfy/test",
            json!({"topic":"private-topic"}),
        )
        .await;
        assert_eq!(body["success"], true, "{platform}: {body}");
        let (headers, body) = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            headers.get("authorization").map(|v| v.to_str().unwrap()),
            expected_auth,
            "{platform}"
        );
        assert!(
            body.contains("test notification from Canary Wallet"),
            "{body}"
        );
        assert_eq!(headers["title"], "Test Notification");
        task.abort();
    }
}

#[tokio::test]
async fn newly_saved_override_uses_basic_auth_without_managed_token() {
    let (url, mut rx, task) = receiver(StatusCode::OK).await;
    let (app, _dir, _) = create_test_app(|c| {
        c.with_ntfy_servers(vec![integration("http://127.0.0.1:1", "startos", true)])
            .with_managed_ntfy_access_token("managed-secret")
    })
    .await;
    let (status, body) = request(
        &app,
        "PUT",
        "/api/user/preferences",
        json!({"ntfy_server_url":url,"ntfy_username":"alice","ntfy_password":"secret"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, body) = request(
        &app,
        "POST",
        "/api/ntfy/test",
        json!({"topic":"private-topic"}),
    )
    .await;
    assert_eq!(body["success"], true, "{body}");
    let (headers, body) = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(headers["authorization"], "Basic YWxpY2U6c2VjcmV0");
    assert!(
        body.contains("test notification from Canary Wallet"),
        "{body}"
    );
    assert_eq!(headers["title"], "Test Notification");
    task.abort();
}

#[tokio::test]
async fn private_receiver_failures_and_redirects_remain_visible() {
    for status in [
        StatusCode::UNAUTHORIZED,
        StatusCode::FORBIDDEN,
        StatusCode::TEMPORARY_REDIRECT,
    ] {
        let (url, mut rx, task) = receiver(status).await;
        let (app, _dir, db) = create_test_app(|c| c).await;
        let conn = bdk_wallet::rusqlite::Connection::open(db).unwrap();
        conn.execute(
            "UPDATE users SET ntfy_server_url = ?1 WHERE id = 'foss-user'",
            [&url],
        )
        .unwrap();
        let (_, body) = request(
            &app,
            "POST",
            "/api/ntfy/test",
            json!({"topic":"private-topic"}),
        )
        .await;
        assert_eq!(body["success"], false);
        assert_eq!(body["error"], format!("HTTP {}", status.as_u16()));
        tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        task.abort();
    }
}

#[tokio::test]
async fn regular_transactions_and_balance_alerts_use_saved_private_server() {
    use canary::{
        metadata::*,
        notifications::NotificationProvider,
        ntfy_provider::{NtfyAuth, NtfyProvider},
    };
    for newly_saved in [false, true] {
        let (url, mut rx, task) = receiver(StatusCode::OK).await;
        let mut config = None;
        let (app, _dir, db) = create_test_app(|c| {
            config = Some(c.clone());
            c
        })
        .await;
        let conn = bdk_wallet::rusqlite::Connection::open(db).unwrap();
        if !newly_saved {
            conn.execute(
                "UPDATE users SET ntfy_server_url = ?1 WHERE id = 'foss-user'",
                [&url],
            )
            .unwrap();
        }
        if newly_saved {
            let (status, body) = request(
                &app,
                "PUT",
                "/api/user/preferences",
                json!({"ntfy_server_url":url,"ntfy_username":"alice","ntfy_password":"secret"}),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{body}");
        }
        let saved: String = conn
            .query_row(
                "SELECT ntfy_server_url FROM users WHERE id = 'foss-user'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let config = config.unwrap();
        let provider = config.ntfy_provider(
            saved.clone(),
            if newly_saved {
                NtfyAuth::BasicAuth {
                    username: "alice".into(),
                    password: "secret".into(),
                }
            } else {
                NtfyAuth::AccessToken("delivery-token".into())
            },
            Some(&saved),
        );
        let contact = Contact {
            id: Some("contact".into()),
            wallet_checksum: "wallet".into(),
            name: "Alice".into(),
            created_at: "".into(),
            is_active: true,
            notify_sending: true,
            notify_sent: true,
            notify_receiving: true,
            notify_received: true,
            notify_cpfp: true,
            notify_rbf: true,
            include_wallet_balance_in_tx_notifications: true,
            notification_methods: vec![NotificationMethod {
                id: Some("method".into()),
                contact_id: "contact".into(),
                provider_type: ProviderType::Ntfy,
                notification_target: "private-topic".into(),
                display_target: None,
                created_at: "".into(),
                is_enabled: true,
                content_fields: NotificationContentFields::detailed(true),
            }],
        };
        let notifications = [
            TransactionNotification::Pending(Transaction {
                txid: "a".repeat(64),
                wallet_checksum: "wallet".into(),
                transaction_type: EventType::Receive,
                amount_sats: 123456,
                fee_sats: None,
                block_height: None,
                first_seen_at: 1700000000,
                confirmed_at: None,
                parent_txid: None,
                transaction_status: "pending".into(),
                replaced_by_txid: None,
                replaced_at: None,
                notification_status: vec![],
            }),
            TransactionNotification::BalanceAlert(BalanceAlertNotification {
                id: "alert-notification".into(),
                balance_alert_id: "alert".into(),
                wallet_checksum: "wallet".into(),
                contact_id: Some("contact".into()),
                threshold_sats: 100000,
                current_balance_sats: 123456,
                alert_type: BalanceAlertType::Above,
                notification_sent_at: 1700000000,
                created_at: "".into(),
                threshold_currency: None,
                threshold_fiat_amount: None,
                exchange_rate_snapshot: None,
            }),
        ];
        for (notification, priority) in notifications.iter().zip(["high", "urgent"]) {
            let results = provider
                .send_notification(
                    notification,
                    "Private wallet",
                    std::slice::from_ref(&contact),
                    &Language::English,
                    Some(123456),
                )
                .await;
            assert_eq!(results.len(), 1);
            assert!(results[0].1.success, "{:?}", results[0]);
            let (headers, body) =
                tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
                    .await
                    .unwrap()
                    .unwrap();
            assert_eq!(
                headers["authorization"],
                if newly_saved {
                    "Basic YWxpY2U6c2VjcmV0"
                } else {
                    "Bearer delivery-token"
                }
            );
            assert_eq!(headers["priority"], priority);
            assert!(body.contains("Private wallet"), "{body}");
            assert_eq!(body, results[0].2);
        }
        // Public-only constructors must never inherit the self-hosted allowance.
        for provider in [
            NtfyProvider::new(url.clone()),
            NtfyProvider::with_auth(url, NtfyAuth::AccessToken("never-send".into())),
        ] {
            let results = provider
                .send_notification(
                    &notifications[0],
                    "Private wallet",
                    std::slice::from_ref(&contact),
                    &Language::English,
                    None,
                )
                .await;
            assert!(!results[0].1.success);
            assert_eq!(
                results[0].1.error_message.as_deref(),
                Some("ntfy server URL is invalid or not allowed.")
            );
            assert!(rx.try_recv().is_err());
        }
        task.abort();
    }
}

#[tokio::test]
async fn unavailable_private_server_reports_connection_failure() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let (app, _dir, db) = create_test_app(|c| c).await;
    let conn = bdk_wallet::rusqlite::Connection::open(db).unwrap();
    conn.execute(
        "UPDATE users SET ntfy_server_url = ?1 WHERE id = 'foss-user'",
        [&url],
    )
    .unwrap();
    let (_, body) = request(
        &app,
        "POST",
        "/api/ntfy/test",
        json!({"topic":"private-topic"}),
    )
    .await;
    assert_eq!(body["success"], false);
    assert_eq!(body["error"], "Request to ntfy server failed");
}

#[tokio::test]
async fn unauthenticated_custom_override_never_receives_managed_token() {
    let (url, mut rx, task) = receiver(StatusCode::OK).await;
    let (app, _dir, _) = create_test_app(|c| {
        c.with_ntfy_servers(vec![integration("http://127.0.0.1:1", "startos", true)])
            .with_managed_ntfy_access_token("managed-secret")
    })
    .await;
    let (status, body) = request(
        &app,
        "PUT",
        "/api/user/preferences",
        json!({"ntfy_server_url":url}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, body) = request(
        &app,
        "POST",
        "/api/ntfy/test",
        json!({"topic":"private-topic"}),
    )
    .await;
    assert_eq!(body["success"], true, "{body}");
    let (headers, _) = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(headers.get("authorization").is_none());
    task.abort();
}

#[tokio::test]
async fn cloud_rejects_custom_servers_and_test_delivery() {
    // Provision an authenticated user using the self-hosted fixture, then route
    // requests with cloud configuration to isolate the outbound mode restriction.
    let (app, _dir, db) = create_test_app(|mut c| {
        c.operating_mode = OperatingMode::Cloud;
        c
    })
    .await;
    let conn = bdk_wallet::rusqlite::Connection::open(&db).unwrap();
    conn.execute("UPDATE users SET is_admin = 0 WHERE id = 'foss-user'", [])
        .unwrap();
    let token = token_for_role(false);
    conn.execute(
        "INSERT INTO sessions (id, user_id, token_hash, expires_at) VALUES ('cloud-test', 'foss-user', ?1, '2100-01-01 00:00:00')",
        [AuthService::hash_token(&token)],
    )
    .unwrap();
    for url in ["http://192.168.1.2", "https://8.8.8.8"] {
        let (status, body) = request_with_token(
            &app,
            "PUT",
            "/api/user/preferences",
            json!({"ntfy_server_url":url}),
            &token,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(body["error_code"], "custom_ntfy_server_unsupported");
    }
    let conn = bdk_wallet::rusqlite::Connection::open(db).unwrap();
    let saved: Option<String> = conn
        .query_row(
            "SELECT ntfy_server_url FROM users WHERE id = 'foss-user'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(saved.is_none());
    let (status, _) = request_with_token(
        &app,
        "POST",
        "/api/ntfy/test",
        json!({"topic":"private-topic"}),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
