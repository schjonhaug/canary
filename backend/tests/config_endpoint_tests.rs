use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use canary::{
    api::{create_router_with_services, AppServices},
    auth::AuthService,
    config::{AppConfig, NetworkConfig, OperatingMode, TxExplorerConfig},
    notifications::NotificationManager,
    wallet::{WalletCreationService, WalletManager},
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::sync::Arc;
use tempfile::{tempdir, TempDir};
use tokio::sync::{broadcast, Mutex};
use tower::ServiceExt;

const TEST_JWT_SECRET: &str = "test-jwt-secret";

async fn create_test_app_with_config(config: AppConfig) -> axum::Router {
    create_test_app_with_config_and_services(config).await.0
}

async fn create_test_app_with_config_and_services(
    config: AppConfig,
) -> (axum::Router, Arc<AppServices>, TempDir) {
    let temp_dir = tempdir().unwrap();
    let temp_path = temp_dir.path().to_str().unwrap();
    let test_db_path = format!("{}/test_metadata.sqlite", temp_path);

    let (event_tx, _event_rx) =
        broadcast::channel::<canary::metadata::TransactionNotification>(100);

    let wallet_manager = Arc::new(
        WalletManager::new(
            event_tx.clone(),
            temp_path.into(),
            &test_db_path,
            bdk_wallet::bitcoin::Network::Regtest,
            "tcp://127.0.0.1:50001",
            &config,
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

    let notification_manager = Arc::new(Mutex::new(NotificationManager::new()));

    let router = create_router_with_services(
        app_services.clone(),
        notification_manager,
        None,
        config,
        None,
    );

    (router, app_services, temp_dir)
}

async fn body_to_json(body: Body) -> Value {
    let bytes = body.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn auth_token_for_user(app_services: &AppServices, jwt_secret: &str, email: &str) -> String {
    let user_id = app_services
        .metadata_db
        .create_user(email, "password_hash", Some("Test User"), true, None, None)
        .await
        .unwrap();
    let token = AuthService::new(jwt_secret.to_string(), None)
        .generate_token(&user_id, email, false, false)
        .unwrap();
    let token_hash = AuthService::hash_token(&token);
    app_services
        .metadata_db
        .create_session(
            &user_id,
            &token_hash,
            chrono::Utc::now() + chrono::Duration::days(1),
        )
        .await
        .unwrap();
    token
}

async fn update_tx_explorer_preference(
    app: axum::Router,
    token: &str,
    preferred_tx_explorer_id: &str,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .uri("/api/user/preferences")
        .method("PUT")
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "preferred_tx_explorer_id": preferred_tx_explorer_id }).to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let body = body_to_json(response.into_body()).await;
    (status, body)
}

async fn clear_tx_explorer_preference_with_null(
    app: axum::Router,
    token: &str,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .uri("/api/user/preferences")
        .method("PUT")
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "preferred_tx_explorer_id": null }).to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let body = body_to_json(response.into_body()).await;
    (status, body)
}

#[tokio::test]
async fn test_config_endpoint_self_hosted_with_single_tx_explorer() {
    let config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        tempdir().unwrap().path().to_str().unwrap().to_string(),
        OperatingMode::SelfHosted,
        None,
        None,
    )
    .with_tx_explorers(vec![TxExplorerConfig {
        id: "mempool".to_string(),
        name: "Mempool".to_string(),
        base_url: Some("http://umbrel.local:3006".to_string()),
        base_urls: vec!["http://umbrel.local:3006".to_string()],
        port: None,
    }]);

    let app = create_test_app_with_config(config).await;

    let request = Request::builder()
        .uri("/api/config")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["default_tx_explorer_id"], "mempool");
    assert_eq!(body["tx_explorers"][0]["id"], "mempool");
    assert_eq!(
        body["tx_explorers"][0]["base_url"],
        "http://umbrel.local:3006"
    );
    assert_eq!(
        body["tx_explorers"][0]["base_urls"],
        json!(["http://umbrel.local:3006"])
    );
    assert!(body["tx_explorers"][0]["port"].is_null());
}

#[tokio::test]
async fn test_config_endpoint_self_hosted_serializes_tx_explorer_base_urls() {
    let config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        tempdir().unwrap().path().to_str().unwrap().to_string(),
        OperatingMode::SelfHosted,
        None,
        None,
    )
    .with_tx_explorers(vec![TxExplorerConfig {
        id: "mempool".to_string(),
        name: "Mempool".to_string(),
        base_url: None,
        base_urls: vec![
            "https://example-node.local:52127".to_string(),
            "https://203.0.113.10:52127".to_string(),
        ],
        port: None,
    }]);

    let app = create_test_app_with_config(config).await;

    let request = Request::builder()
        .uri("/api/config")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["tx_explorers"][0]["id"], "mempool");
    assert!(body["tx_explorers"][0]["base_url"].is_null());
    assert_eq!(
        body["tx_explorers"][0]["base_urls"],
        json!([
            "https://example-node.local:52127",
            "https://203.0.113.10:52127"
        ])
    );
}

#[tokio::test]
async fn test_config_endpoint_self_hosted_with_multiple_tx_explorers() {
    let config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        tempdir().unwrap().path().to_str().unwrap().to_string(),
        OperatingMode::SelfHosted,
        None,
        None,
    )
    .with_tx_explorers(vec![
        TxExplorerConfig {
            id: "mempool".to_string(),
            name: "Mempool".to_string(),
            base_url: None,
            base_urls: Vec::new(),
            port: Some(3006),
        },
        TxExplorerConfig {
            id: "bitfeed".to_string(),
            name: "Bitfeed".to_string(),
            base_url: None,
            base_urls: Vec::new(),
            port: Some(8314),
        },
    ]);

    let app = create_test_app_with_config(config).await;

    let request = Request::builder()
        .uri("/api/config")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["default_tx_explorer_id"], "mempool-space");
    assert_eq!(body["tx_explorers"][0]["port"], 3006);
    assert_eq!(body["tx_explorers"][1]["id"], "bitfeed");
    assert_eq!(body["tx_explorers"][1]["port"], 8314);
}

#[tokio::test]
async fn test_config_endpoint_self_hosted_no_mempool_config() {
    let config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        tempdir().unwrap().path().to_str().unwrap().to_string(),
        OperatingMode::SelfHosted,
        None,
        None,
    );

    let app = create_test_app_with_config(config).await;

    let request = Request::builder()
        .uri("/api/config")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_to_json(response.into_body()).await;
    assert!(body["tx_explorers"].as_array().unwrap().is_empty());
    assert_eq!(body["default_tx_explorer_id"], "mempool-space");
}

#[tokio::test]
async fn test_config_endpoint_cloud_mode_ignores_self_hosted_tx_explorers() {
    let config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        tempdir().unwrap().path().to_str().unwrap().to_string(),
        OperatingMode::Cloud,
        Some("http://localhost:3001".to_string()),
        Some("test-jwt-secret".to_string()),
    )
    .with_tx_explorers(vec![TxExplorerConfig {
        id: "mempool".to_string(),
        name: "Mempool".to_string(),
        base_url: Some("http://umbrel.local:3006".to_string()),
        base_urls: Vec::new(),
        port: None,
    }]);

    let app = create_test_app_with_config(config).await;

    let request = Request::builder()
        .uri("/api/config")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_to_json(response.into_body()).await;
    assert!(body["tx_explorers"].as_array().unwrap().is_empty());
    assert_eq!(body["default_tx_explorer_id"], "mempool-space");
}

#[tokio::test]
async fn test_user_preferences_accepts_supported_tx_explorer_ids() {
    let config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        tempdir().unwrap().path().to_str().unwrap().to_string(),
        OperatingMode::SelfHosted,
        None,
        Some(TEST_JWT_SECRET.to_string()),
    )
    .with_tx_explorers(vec![TxExplorerConfig {
        id: "bitfeed".to_string(),
        name: "Bitfeed".to_string(),
        base_url: None,
        base_urls: Vec::new(),
        port: Some(8314),
    }]);
    let (app, app_services, _temp_dir) = create_test_app_with_config_and_services(config).await;
    let token = auth_token_for_user(
        &app_services,
        TEST_JWT_SECRET,
        "tx-explorer-user@example.com",
    )
    .await;

    let (status, body) = update_tx_explorer_preference(app.clone(), &token, "mempool-space").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["preferred_tx_explorer_id"], "mempool-space");

    let (status, body) = update_tx_explorer_preference(app, &token, "bitfeed").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["preferred_tx_explorer_id"], "bitfeed");
}

#[tokio::test]
async fn test_user_preferences_rejects_unsupported_tx_explorer_id() {
    let config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        tempdir().unwrap().path().to_str().unwrap().to_string(),
        OperatingMode::SelfHosted,
        None,
        Some(TEST_JWT_SECRET.to_string()),
    );
    let (app, app_services, _temp_dir) = create_test_app_with_config_and_services(config).await;
    let token = auth_token_for_user(
        &app_services,
        TEST_JWT_SECRET,
        "unsupported-explorer-user@example.com",
    )
    .await;

    let (status, body) = update_tx_explorer_preference(app, &token, "unknown").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Unsupported tx explorer: unknown");
}

#[tokio::test]
async fn test_user_preferences_clears_tx_explorer_preference() {
    let config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        tempdir().unwrap().path().to_str().unwrap().to_string(),
        OperatingMode::SelfHosted,
        None,
        Some(TEST_JWT_SECRET.to_string()),
    );
    let (app, app_services, _temp_dir) = create_test_app_with_config_and_services(config).await;
    let token = auth_token_for_user(
        &app_services,
        TEST_JWT_SECRET,
        "clear-explorer-user@example.com",
    )
    .await;

    let (status, body) = update_tx_explorer_preference(app.clone(), &token, "mempool-space").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["preferred_tx_explorer_id"], "mempool-space");

    let (status, body) = update_tx_explorer_preference(app, &token, "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["preferred_tx_explorer_id"].is_null());
}

#[tokio::test]
async fn test_user_preferences_clears_tx_explorer_preference_with_null() {
    let config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        tempdir().unwrap().path().to_str().unwrap().to_string(),
        OperatingMode::SelfHosted,
        None,
        Some(TEST_JWT_SECRET.to_string()),
    );
    let (app, app_services, _temp_dir) = create_test_app_with_config_and_services(config).await;
    let token = auth_token_for_user(
        &app_services,
        TEST_JWT_SECRET,
        "clear-null-explorer-user@example.com",
    )
    .await;

    let (status, body) = update_tx_explorer_preference(app.clone(), &token, "mempool-space").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["preferred_tx_explorer_id"], "mempool-space");

    let (status, body) = clear_tx_explorer_preference_with_null(app, &token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["preferred_tx_explorer_id"].is_null());
}

#[tokio::test]
async fn test_user_preferences_cloud_mode_rejects_local_tx_explorer_ids() {
    let config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        tempdir().unwrap().path().to_str().unwrap().to_string(),
        OperatingMode::Cloud,
        Some("http://localhost:3001".to_string()),
        Some(TEST_JWT_SECRET.to_string()),
    )
    .with_tx_explorers(vec![TxExplorerConfig {
        id: "bitfeed".to_string(),
        name: "Bitfeed".to_string(),
        base_url: Some("http://umbrel.local:8314".to_string()),
        base_urls: Vec::new(),
        port: None,
    }]);
    let (app, app_services, _temp_dir) = create_test_app_with_config_and_services(config).await;
    let token = auth_token_for_user(
        &app_services,
        TEST_JWT_SECRET,
        "cloud-explorer-user@example.com",
    )
    .await;

    let (status, body) = update_tx_explorer_preference(app.clone(), &token, "bitfeed").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Unsupported tx explorer: bitfeed");

    let (status, body) = update_tx_explorer_preference(app.clone(), &token, "mempool-space").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["preferred_tx_explorer_id"], "mempool-space");

    let (status, body) = update_tx_explorer_preference(app.clone(), &token, "bitfeed-public").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["preferred_tx_explorer_id"], "bitfeed-public");

    let (status, body) =
        update_tx_explorer_preference(app, &token, "btc-rpc-explorer-public").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["preferred_tx_explorer_id"], "btc-rpc-explorer-public");
}
