use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use canary::{
    api::{create_router_with_services, AppServices},
    auth::DEV_TEST_PASSWORD,
    config::{AppConfig, NetworkConfig, OperatingMode},
    electrum::ElectrumClientManager,
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
const ADMIN_USER_EMAIL: &str = "delivered+admin@resend.dev";
const PERSONAL_USER_EMAIL: &str = "delivered+alice@resend.dev";
const TEAM_USER_EMAIL: &str = "delivered+bob@resend.dev";
const VALID_TESTNET_DESCRIPTOR: &str = "wpkh(tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5/<0;1>/*)";
const SECOND_TESTNET_DESCRIPTOR: &str = "wpkh(tpubDCMRAYcH71Gagskm7E5peNMYB5sKaLLwtn2c4Rb3CMUTRVUk5dkpsskhspa5MEcVZ11LwTcM7R5mzndUCG9WabYcT5hfQHbYVoaLFBZHPCi/<0;1>/*)";

async fn create_cloud_test_app() -> (axum::Router, TempDir, String, Arc<AppServices>) {
    let temp_dir = tempdir().unwrap();
    let temp_path = temp_dir.path().to_str().unwrap();
    let test_db_path = format!("{}/test_metadata.sqlite", temp_path);
    let test_config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        temp_path.to_string(),
        OperatingMode::Cloud,
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
    let notification_manager = Arc::new(Mutex::new(NotificationManager::new()));
    let electrum_manager = Some(Arc::new(ElectrumClientManager::new_mock_connected()));
    let router = create_router_with_services(
        app_services.clone(),
        notification_manager,
        None,
        test_config,
        electrum_manager,
    );
    (router, temp_dir, test_db_path, app_services)
}

async fn body_to_json(body: Body) -> Value {
    let bytes = body.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn login_user(app: &axum::Router, email: &str) -> String {
    let request = Request::builder()
        .uri("/api/auth/login")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "email": email,
                "password": DEV_TEST_PASSWORD,
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    body_to_json(response.into_body()).await["token"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn login_admin_user(app: &axum::Router, db_path: &str) -> String {
    use bdk_wallet::rusqlite::{params, Connection};
    use std::os::unix::fs::PermissionsExt;
    let connection = Connection::open(db_path).unwrap();
    let user_id: String = connection
        .query_row(
            "SELECT id FROM users WHERE email = ?1",
            params![ADMIN_USER_EMAIL],
            |row| row.get(0),
        )
        .unwrap();
    let secret = totp_rs::Secret::new(Box::from(*b"12345678901234567890"));
    let factor_path = std::path::Path::new(db_path).with_file_name("admin-mfa.json");
    std::fs::write(
        &factor_path,
        json!({user_id: secret.to_base32()}).to_string(),
    )
    .unwrap();
    std::fs::set_permissions(&factor_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::env::set_var("CANARY_ADMIN_MFA_SECRETS_FILE", &factor_path);
    let code = totp_rs::Builder::new()
        .with_secret(secret)
        .build()
        .unwrap()
        .generate(chrono::Utc::now().timestamp() as u64)
        .to_string();
    let request = Request::builder()
        .uri("/api/auth/login")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "email": ADMIN_USER_EMAIL,
                "password": DEV_TEST_PASSWORD,
                "mfa_code": code,
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    body_to_json(response.into_body()).await["token"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn send(app: &axum::Router, token: &str, request: Request<Body>) -> (StatusCode, Value) {
    let (mut parts, body) = request.into_parts();
    parts
        .headers
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    let response = app
        .clone()
        .oneshot(Request::from_parts(parts, body))
        .await
        .unwrap();
    let status = response.status();
    let body = body_to_json(response.into_body()).await;
    (status, body)
}

async fn create_wallet(app: &axum::Router, token: &str, name: &str, descriptor: &str) -> String {
    let (status, body) = send(
        app,
        token,
        Request::builder()
            .uri("/api/wallets")
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "name": name,
                    "descriptor": descriptor,
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["wallet"]["checksum"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn cloud_admin_support_access_is_customer_scoped_read_only_and_audited() {
    let (app, _temp_dir, db_path, _app_services) = create_cloud_test_app().await;
    let alice = login_user(&app, PERSONAL_USER_EMAIL).await;
    let bob = login_user(&app, TEAM_USER_EMAIL).await;
    let alice_checksum =
        create_wallet(&app, &alice, "Alice Wallet", VALID_TESTNET_DESCRIPTOR).await;
    let bob_checksum = create_wallet(&app, &bob, "Bob Wallet", SECOND_TESTNET_DESCRIPTOR).await;
    let admin = login_admin_user(&app, &db_path).await;

    let (status, body) = send(
        &app,
        &admin,
        Request::builder()
            .uri("/api/wallets")
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "name": "Admin Wallet",
                    "descriptor": VALID_TESTNET_DESCRIPTOR,
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error_code"], "admin_wallets_unsupported");

    let (status, _body) = send(
        &app,
        &admin,
        Request::builder()
            .uri(format!("/api/wallets/{alice_checksum}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _body) = send(
        &app,
        &alice,
        Request::builder()
            .uri("/api/admin/support-access")
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "email": PERSONAL_USER_EMAIL,
                    "reason": "customer asked about sync"
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, body) = send(
        &app,
        &admin,
        Request::builder()
            .uri("/api/admin/support-access")
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "email": PERSONAL_USER_EMAIL,
                    "reason": "customer asked about sync"
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["grant"]["target_email"], PERSONAL_USER_EMAIL);
    let wallets = body["wallets"].as_array().unwrap();
    assert_eq!(wallets.len(), 1);
    assert_eq!(wallets[0]["checksum"], alice_checksum);
    assert_eq!(wallets[0]["name"], "Alice Wallet");

    let (status, body) = send(
        &app,
        &admin,
        Request::builder()
            .uri(format!("/api/wallets/{alice_checksum}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["checksum"], alice_checksum);

    let (status, _body) = send(
        &app,
        &admin,
        Request::builder()
            .uri(format!("/api/wallets/{bob_checksum}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, body) = send(
        &app,
        &admin,
        Request::builder()
            .uri(format!("/api/wallets/{alice_checksum}"))
            .method("DELETE")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error_code"], "access_denied");

    use bdk_wallet::rusqlite::Connection;
    let connection = Connection::open(&db_path).unwrap();
    let (operation, target, details): (String, String, String) = connection
        .query_row(
            "SELECT operation, target, details_json FROM admin_audit_log
             WHERE operation = 'admin_support_access' ORDER BY created_at DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(operation, "admin_support_access");
    assert_ne!(target, PERSONAL_USER_EMAIL);
    assert!(!details.contains('@'));
    assert!(!details.to_lowercase().contains("tpub"));
    assert!(details.contains("granted"));
}

#[tokio::test]
async fn cloud_admin_cannot_open_admin_or_missing_support_targets() {
    let (app, _temp_dir, db_path, _app_services) = create_cloud_test_app().await;
    let admin = login_admin_user(&app, &db_path).await;
    let (status, body) = send(
        &app,
        &admin,
        Request::builder()
            .uri("/api/admin/support-access")
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "email": ADMIN_USER_EMAIL,
                    "reason": "checking my own wallets"
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error_code"], "support_target_invalid");

    let (status, body) = send(
        &app,
        &admin,
        Request::builder()
            .uri("/api/admin/support-access")
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "email": "missing-customer@example.com",
                    "reason": "customer asked about sync"
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error_code"], "support_target_not_found");
}
