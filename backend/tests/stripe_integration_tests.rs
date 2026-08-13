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
use serde_json::json;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::sync::{broadcast, Mutex};
use tower::ServiceExt;

/// Test helper to create test application
async fn create_test_app() -> axum::Router {
    let temp_dir = tempdir().unwrap();
    let temp_path = temp_dir.path().to_str().unwrap();
    let test_db_path = format!("{}/test_metadata.sqlite", temp_path);

    // Create test config
    let test_config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        temp_path.to_string(),
        OperatingMode::Cloud, // Stripe tests need cloud mode
        Some("http://localhost:3001".to_string()),
        Some("test-jwt-secret".to_string()),
    );

    let (event_tx, _event_rx) =
        broadcast::channel::<canary::metadata::TransactionNotification>(100);
    let _current_block_header = Arc::new(Mutex::new(None::<canary::electrum::BlockHeader>));

    let wallet_manager = Arc::new(
        WalletManager::new(
            event_tx.clone(),
            temp_path.into(),
            &test_db_path,
            bdk_wallet::bitcoin::Network::Regtest,
            "tcp://127.0.0.1:50001", // Test electrum
            &test_config,
        )
        .await,
    );

    // Create AppServices for non-blocking architecture
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

    // No Stripe billing for basic tests (None means billing endpoints return 500)
    let stripe_billing = None;

    create_router_with_services(
        app_services,
        notification_manager,
        stripe_billing,
        test_config,
        None, // electrum_manager - not needed for Stripe tests
    )
}

/// Test pricing endpoint without Stripe billing
#[tokio::test]
async fn test_pricing_endpoint_without_stripe() {
    let app = create_test_app().await;

    let request = Request::builder()
        .uri("/api/billing/pricing")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // Should return 404 when Stripe billing routes are not mounted
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// Test checkout endpoint without Stripe billing
#[tokio::test]
async fn test_checkout_endpoint_without_stripe() {
    let app = create_test_app().await;

    let checkout_request = json!({
        "tier": "pro",
        "is_yearly": true
    });

    let request = Request::builder()
        .uri("/api/stripe/checkout")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(checkout_request.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // Should return 404 when Stripe billing routes are not mounted
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// Test session details endpoint without Stripe billing
#[tokio::test]
async fn test_session_details_without_stripe() {
    let app = create_test_app().await;

    let request = Request::builder()
        .uri("/api/billing/session/cs_test_123")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // Should return 404 when Stripe billing routes are not mounted
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// Test webhook endpoint without Stripe billing
#[tokio::test]
async fn test_webhook_endpoint_without_stripe() {
    let app = create_test_app().await;

    let request = Request::builder()
        .uri("/api/stripe/webhook")
        .method("POST")
        .header("content-type", "application/json")
        .header("stripe-signature", "t=123456,v1=fake_signature")
        .body(Body::from(r#"{"id":"evt_test_123","object":"event"}"#))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // Should return 404 when Stripe billing routes are not mounted
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// Test invalid JSON in checkout request
#[tokio::test]
async fn test_checkout_invalid_json() {
    let app = create_test_app().await;

    let request = Request::builder()
        .uri("/api/stripe/checkout")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from("invalid json"))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // Should return 404 when Stripe billing routes are not mounted
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// Test missing required fields in checkout request
#[tokio::test]
async fn test_checkout_missing_fields() {
    let app = create_test_app().await;

    let checkout_request = json!({
        "is_yearly": true
        // Missing "tier" field
    });

    let request = Request::builder()
        .uri("/api/stripe/checkout")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(checkout_request.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // Should return 404 when Stripe billing routes are not mounted
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// Test invalid tier value
#[tokio::test]
async fn test_checkout_invalid_tier() {
    let app = create_test_app().await;

    let checkout_request = json!({
        "tier": "invalid_tier",
        "is_yearly": false
    });

    let request = Request::builder()
        .uri("/api/stripe/checkout")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(checkout_request.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // Should return 404 when Stripe billing routes are not mounted
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// CORS only permits the configured frontend origin.
#[tokio::test]
async fn test_cors_headers_allow_only_configured_origin() {
    let app = create_test_app().await;

    // Test with a valid endpoint since billing endpoints aren't mounted
    let request = Request::builder()
        .uri("/api/wallets")
        .header("Origin", "http://localhost:3001")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response
            .headers()
            .get("access-control-allow-origin")
            .unwrap(),
        "http://localhost:3001"
    );

    let app = create_test_app().await;
    let request = Request::builder()
        .uri("/api/wallets")
        .header("Origin", "https://attacker.example")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_ne!(
        response
            .headers()
            .get("access-control-allow-origin")
            .unwrap(),
        "https://attacker.example"
    );
}
