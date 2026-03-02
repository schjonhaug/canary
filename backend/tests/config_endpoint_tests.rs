use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use canary::{
    api::{create_router_with_services, AppServices},
    config::{AppConfig, NetworkConfig, OperatingMode},
    notifications::NotificationManager,
    wallet::{WalletCreationService, WalletManager},
};
use http_body_util::BodyExt;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::sync::{broadcast, Mutex};
use tower::ServiceExt;

async fn create_test_app_with_config(config: AppConfig) -> axum::Router {
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

    create_router_with_services(app_services, notification_manager, None, config, None)
}

async fn body_to_json(body: Body) -> serde_json::Value {
    let bytes = body.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn test_config_endpoint_self_hosted_with_mempool_url() {
    let config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        tempdir().unwrap().path().to_str().unwrap().to_string(),
        OperatingMode::SelfHosted,
        None,
        None,
    )
    .with_mempool_url(Some("http://umbrel.local:3006".to_string()));

    let app = create_test_app_with_config(config).await;

    let request = Request::builder()
        .uri("/api/config")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["mempool_url"], "http://umbrel.local:3006");
    assert!(body["mempool_port"].is_null());
}

#[tokio::test]
async fn test_config_endpoint_self_hosted_with_mempool_port() {
    let config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        tempdir().unwrap().path().to_str().unwrap().to_string(),
        OperatingMode::SelfHosted,
        None,
        None,
    )
    .with_mempool_port(Some(3006));

    let app = create_test_app_with_config(config).await;

    let request = Request::builder()
        .uri("/api/config")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_to_json(response.into_body()).await;
    assert!(body["mempool_url"].is_null());
    assert_eq!(body["mempool_port"], 3006);
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
    assert!(body["mempool_url"].is_null());
    assert!(body["mempool_port"].is_null());
}

#[tokio::test]
async fn test_config_endpoint_cloud_mode_ignores_mempool_config() {
    let config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        tempdir().unwrap().path().to_str().unwrap().to_string(),
        OperatingMode::Cloud,
        Some("http://localhost:3001".to_string()),
        Some("test-jwt-secret".to_string()),
    )
    .with_mempool_url(Some("http://umbrel.local:3006".to_string()))
    .with_mempool_port(Some(3006));

    let app = create_test_app_with_config(config).await;

    let request = Request::builder()
        .uri("/api/config")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_to_json(response.into_body()).await;
    // Cloud mode should always return null regardless of config
    assert!(body["mempool_url"].is_null());
    assert!(body["mempool_port"].is_null());
}
