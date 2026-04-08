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
const PERSONAL_USER_EMAIL: &str = "delivered+alice@resend.dev";
const VALID_TESTNET_DESCRIPTOR: &str = "wpkh(tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5/<0;1>/*)";
const SECOND_TESTNET_DESCRIPTOR: &str = "wpkh(tpubDCMRAYcH71Gagskm7E5peNMYB5sKaLLwtn2c4Rb3CMUTRVUk5dkpsskhspa5MEcVZ11LwTcM7R5mzndUCG9WabYcT5hfQHbYVoaLFBZHPCi/<0;1>/*)";

async fn create_cloud_test_app() -> (axum::Router, TempDir) {
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
        app_services,
        notification_manager,
        None,
        test_config,
        electrum_manager,
    );

    (router, temp_dir)
}

async fn body_to_json(body: Body) -> Value {
    let bytes = body.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn login_personal_user(app: &axum::Router) -> String {
    let request = Request::builder()
        .uri("/api/auth/login")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "email": PERSONAL_USER_EMAIL,
                "password": DEV_TEST_PASSWORD,
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_to_json(response.into_body()).await;
    body["token"].as_str().unwrap().to_string()
}

async fn create_wallet(app: &axum::Router, token: &str, name: &str, descriptor: &str) -> Value {
    let request = Request::builder()
        .uri("/api/wallets")
        .method("POST")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": name,
                "descriptor": descriptor,
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    body_to_json(response.into_body()).await
}

#[tokio::test]
async fn test_personal_user_wallet_limit_is_enforced() {
    let (app, _temp_dir) = create_cloud_test_app().await;
    let token = login_personal_user(&app).await;

    create_wallet(&app, &token, "First Wallet", VALID_TESTNET_DESCRIPTOR).await;

    let request = Request::builder()
        .uri("/api/wallets")
        .method("POST")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "Second Wallet",
                "descriptor": SECOND_TESTNET_DESCRIPTOR,
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["error_code"], "wallet_limit_reached");
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("Wallet limit reached"),);
}

#[tokio::test]
async fn test_personal_user_contact_limit_is_enforced() {
    let (app, _temp_dir) = create_cloud_test_app().await;
    let token = login_personal_user(&app).await;

    let wallet = create_wallet(
        &app,
        &token,
        "Wallet For Contacts",
        VALID_TESTNET_DESCRIPTOR,
    )
    .await;
    let checksum = wallet["wallet"]["checksum"].as_str().unwrap();

    // Wallet creation auto-creates a "Me" email contact, which already fills the Personal tier's
    // one-contact-per-wallet limit. Any additional contact should now be rejected through the
    // shared helper.
    let request = Request::builder()
        .uri(format!("/api/wallets/{checksum}/contacts"))
        .method("POST")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "Extra Contact",
                "notification_methods": [
                    {
                        "provider_type": "ntfy",
                        "notification_target": "personal-limit-topic-extra",
                    }
                ]
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["error_code"], "contact_limit_reached");
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("Contact limit reached"),);
}
