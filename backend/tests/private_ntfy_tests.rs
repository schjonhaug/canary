mod common;

use axum::{
    body::Body,
    http::{HeaderMap, Request, StatusCode},
    routing::post,
    Router,
};
use canary::{
    auth::AuthService,
    config::{AppConfig, NtfyServerConfig, OperatingMode},
    notifications::NotificationManager,
    WebhookProvider,
};
use common::{
    create_app_services, create_router, create_self_hosted_admin_session, db_path,
    self_hosted_admin_token, test_config, test_token, SELF_HOSTED_ADMIN_EMAIL, SELF_HOSTED_USER_ID,
    TEST_JWT_SECRET,
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::sync::Arc;
use tempfile::{tempdir, TempDir};
use tokio::sync::mpsc;
use tower::ServiceExt;

async fn create_test_app(
    configure: impl FnOnce(AppConfig) -> AppConfig,
) -> (axum::Router, TempDir, String) {
    let temp_dir = tempdir().unwrap();
    let mut config = configure(test_config(
        temp_dir.path(),
        OperatingMode::SelfHosted,
        None,
        Some(TEST_JWT_SECRET),
    ));

    // The services are always built in self-hosted mode, so the built-in
    // admin and its session exist even when the router runs in cloud mode.
    let route_mode = std::mem::replace(&mut config.operating_mode, OperatingMode::SelfHosted);
    let app_services = create_app_services(temp_dir.path(), &config).await;
    create_self_hosted_admin_session(&app_services, TEST_JWT_SECRET).await;

    let mut notification_manager = NotificationManager::new();
    notification_manager.register_provider(Arc::new(WebhookProvider::new()));

    config.operating_mode = route_mode;
    let router = create_router(&app_services, notification_manager, config, true);
    let db_path = db_path(temp_dir.path());
    (router, temp_dir, db_path)
}

async fn request(app: &Router, method: &str, path: &str, payload: Value) -> (StatusCode, Value) {
    request_with_token(
        app,
        method,
        path,
        payload,
        &self_hosted_admin_token(TEST_JWT_SECRET),
    )
    .await
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
    assert_eq!(headers["priority"], "urgent");
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
        assert_eq!(headers["priority"], "urgent");
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
    assert_eq!(headers["priority"], "urgent");
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
            &saved,
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
            created_at: String::new(),
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
                created_at: String::new(),
                is_enabled: true,
                content_fields: NotificationContentFields::detailed(true),
            }],
        };
        let notifications = [
            TransactionNotification::Pending(Transaction {
                txid: "a".repeat(64),
                wallet_checksum: "wallet".into(),
                transaction_type: EventType::Receive,
                amount_sats: 123_456,
                fee_sats: None,
                block_height: None,
                first_seen_at: 1_700_000_000,
                confirmed_at: None,
                parent_txid: None,
                transaction_status: TransactionStatus::Pending,
                replaced_by_txid: None,
                replaced_at: None,
                notification_status: vec![],
            }),
            TransactionNotification::BalanceAlert(BalanceAlertNotification {
                id: "alert-notification".into(),
                balance_alert_id: "alert".into(),
                wallet_checksum: "wallet".into(),
                contact_id: Some("contact".into()),
                threshold_sats: 100_000,
                current_balance_sats: 123_456,
                alert_type: BalanceAlertType::Above,
                notification_sent_at: 1_700_000_000,
                created_at: String::new(),
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
                    Some(123_456),
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
            NtfyProvider::new(&url),
            NtfyProvider::with_auth(&url, NtfyAuth::AccessToken("never-send".into())),
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
    let token = test_token(
        TEST_JWT_SECRET,
        SELF_HOSTED_USER_ID,
        SELF_HOSTED_ADMIN_EMAIL,
        false,
    );
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
