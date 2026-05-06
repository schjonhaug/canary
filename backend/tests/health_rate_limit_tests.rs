use axum::{
    body::Body,
    http::{header, HeaderMap, Request, StatusCode},
};
use canary::{
    api::{create_router_with_services, AppServices},
    auth::{AuthService, Claims},
    config::{AppConfig, NetworkConfig, OperatingMode},
    notifications::NotificationManager,
    wallet::{WalletCreationService, WalletManager},
};
use http_body_util::BodyExt;
use jsonwebtoken::{encode, EncodingKey, Header};
use serde_json::Value;
use std::sync::Arc;
use tempfile::{tempdir, TempDir};
use tokio::sync::{broadcast, Mutex};
use tower::ServiceExt;

const TEST_JWT_SECRET: &str = "test-jwt-secret";

struct TestApp {
    router: axum::Router,
    _temp_dir: TempDir,
}

async fn create_self_hosted_test_app() -> TestApp {
    let temp_dir = tempdir().unwrap();
    let temp_path = temp_dir.path().to_str().unwrap();
    let test_db_path = format!("{}/test_metadata.sqlite", temp_path);

    let config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        temp_path.to_string(),
        OperatingMode::SelfHosted,
        None,
        Some(TEST_JWT_SECRET.to_string()),
    );

    let (event_tx, _event_rx) =
        broadcast::channel::<canary::metadata::TransactionNotification>(100);
    let wallet_manager = Arc::new(
        WalletManager::new(
            event_tx,
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
    create_self_hosted_admin_session(&app_services).await;

    let notification_manager = Arc::new(Mutex::new(NotificationManager::new()));
    let router =
        create_router_with_services(app_services, notification_manager, None, config, None);

    TestApp {
        router,
        _temp_dir: temp_dir,
    }
}

fn self_hosted_admin_token() -> String {
    let claims = Claims {
        sub: "foss-user".to_string(),
        email: "admin@local".to_string(),
        is_admin: true,
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

async fn body_to_json(body: Body) -> Value {
    let bytes = body.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn get_database_health(router: &axum::Router, token: &str) -> (StatusCode, HeaderMap, Value) {
    let request = Request::builder()
        .uri("/api/health/database")
        .method("GET")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = body_to_json(response.into_body()).await;
    (status, headers, body)
}

async fn post_database_integrity(
    router: &axum::Router,
    token: &str,
) -> (StatusCode, HeaderMap, Value) {
    let request = Request::builder()
        .uri("/api/admin/database/integrity")
        .method("POST")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"auto_fix":false}"#))
        .unwrap();

    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = body_to_json(response.into_body()).await;
    (status, headers, body)
}

fn assert_rate_limited(
    status: StatusCode,
    headers: HeaderMap,
    body: Value,
    expected_retry_after: &str,
) {
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        headers.get(header::RETRY_AFTER).unwrap().to_str().unwrap(),
        expected_retry_after
    );
    assert_eq!(body["error_code"], "admin_endpoint_rate_limit");
}

#[tokio::test]
async fn database_health_endpoint_is_rate_limited() {
    let test_app = create_self_hosted_test_app().await;
    let token = self_hosted_admin_token();

    for _ in 0..6 {
        let (status, _headers, _body) = get_database_health(&test_app.router, &token).await;
        assert_eq!(status, StatusCode::OK);
    }

    let (status, headers, body) = get_database_health(&test_app.router, &token).await;
    assert_rate_limited(status, headers, body, "300");
}

#[tokio::test]
async fn database_integrity_endpoint_has_separate_rate_limit_scope() {
    let test_app = create_self_hosted_test_app().await;
    let token = self_hosted_admin_token();

    for _ in 0..6 {
        let (status, _headers, _body) = get_database_health(&test_app.router, &token).await;
        assert_eq!(status, StatusCode::OK);
    }
    let (status, headers, body) = get_database_health(&test_app.router, &token).await;
    assert_rate_limited(status, headers, body, "300");

    let (status, _headers, _body) = post_database_integrity(&test_app.router, &token).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _headers, _body) = post_database_integrity(&test_app.router, &token).await;
    assert_eq!(status, StatusCode::OK);
    let (status, headers, body) = post_database_integrity(&test_app.router, &token).await;
    assert_rate_limited(status, headers, body, "600");
}
