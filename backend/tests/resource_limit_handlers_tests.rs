use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use bdk_wallet::bitcoin::Network;
use bdk_wallet::keys::DescriptorPublicKey;
use canary::{
    api::{create_router_with_services, AppServices},
    auth::{AuthService, Claims, DEV_TEST_PASSWORD},
    config::{AppConfig, NetworkConfig, OperatingMode},
    electrum::ElectrumClientManager,
    notifications::NotificationManager,
    wallet::{WalletCreationService, WalletManager},
    WebhookProvider,
};
use http_body_util::BodyExt;
use jsonwebtoken::{encode, EncodingKey, Header};
use nostr_sdk::prelude::{Keys, ToBech32};
use serde_json::{json, Value};
use std::sync::Arc;
use tempfile::{tempdir, TempDir};
use tokio::sync::{broadcast, Mutex};
use tokio::time::{sleep, Duration};
use tower::ServiceExt;

const TEST_JWT_SECRET: &str = "test-jwt-secret";
const ADMIN_USER_EMAIL: &str = "delivered+admin@resend.dev";
const PERSONAL_USER_EMAIL: &str = "delivered+alice@resend.dev";
const VALID_TESTNET_DESCRIPTOR: &str = "wpkh(tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5/<0;1>/*)";
const SECOND_TESTNET_DESCRIPTOR: &str = "wpkh(tpubDCMRAYcH71Gagskm7E5peNMYB5sKaLLwtn2c4Rb3CMUTRVUk5dkpsskhspa5MEcVZ11LwTcM7R5mzndUCG9WabYcT5hfQHbYVoaLFBZHPCi/<0;1>/*)";
const VALID_TESTNET_XPUB: &str = "tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5";

async fn create_test_app(
    operating_mode: OperatingMode,
    jwt_secret: Option<&str>,
) -> (axum::Router, TempDir, String) {
    let temp_dir = tempdir().unwrap();
    let temp_path = temp_dir.path().to_str().unwrap();
    let test_db_path = format!("{}/test_metadata.sqlite", temp_path);

    let test_config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        temp_path.to_string(),
        operating_mode,
        None,
        jwt_secret.map(str::to_string),
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

    if test_config.is_self_hosted_mode() {
        create_self_hosted_admin_session(&app_services).await;
    }

    let mut manager = NotificationManager::new();
    if test_config.is_self_hosted_mode() {
        manager.register_provider(Arc::new(WebhookProvider::new()));
    }
    let notification_manager = Arc::new(Mutex::new(manager));
    let electrum_manager = Some(Arc::new(ElectrumClientManager::new_mock_connected()));

    let router = create_router_with_services(
        app_services,
        notification_manager,
        None,
        test_config,
        electrum_manager,
    );

    (router, temp_dir, test_db_path)
}

async fn create_cloud_test_app() -> (axum::Router, TempDir, String) {
    create_test_app(OperatingMode::Cloud, Some(TEST_JWT_SECRET)).await
}

async fn create_self_hosted_test_app() -> (axum::Router, TempDir, String) {
    create_test_app(OperatingMode::SelfHosted, Some(TEST_JWT_SECRET)).await
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

    let body = body_to_json(response.into_body()).await;
    body["token"].as_str().unwrap().to_string()
}

async fn login_personal_user(app: &axum::Router) -> String {
    login_user(app, PERSONAL_USER_EMAIL).await
}

async fn login_admin_user(app: &axum::Router) -> String {
    login_user(app, ADMIN_USER_EMAIL).await
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

async fn wait_for_auto_contact(app: &axum::Router, token: &str, checksum: &str) {
    for _ in 0..100 {
        let request = Request::builder()
            .uri(format!("/api/wallets/{checksum}/contacts"))
            .method("GET")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = body_to_json(response.into_body()).await;
        if body.as_array().is_some_and(|contacts| !contacts.is_empty()) {
            return;
        }

        sleep(Duration::from_millis(50)).await;
    }

    panic!("timed out waiting for auto-created contact");
}

async fn create_contact(
    app: &axum::Router,
    token: Option<&str>,
    checksum: &str,
    name: &str,
    topic: &str,
) -> StatusCode {
    let mut builder = Request::builder()
        .uri(format!("/api/wallets/{checksum}/contacts"))
        .method("POST")
        .header("content-type", "application/json");

    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }

    let request = builder
        .body(Body::from(
            json!({
                "name": name,
                "notification_methods": [
                    {
                        "provider_type": "ntfy",
                        "notification_target": topic,
                    }
                ]
            })
            .to_string(),
        ))
        .unwrap();

    app.clone().oneshot(request).await.unwrap().status()
}

async fn create_contact_with_provider(
    app: &axum::Router,
    token: &str,
    checksum: &str,
    provider_type: &str,
    target: &str,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .uri(format!("/api/wallets/{checksum}/contacts"))
        .method("POST")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "Provider Contact",
                "notification_methods": [
                    {
                        "provider_type": provider_type,
                        "notification_target": target,
                    }
                ]
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = body_to_json(response.into_body()).await;
    (status, body)
}

async fn update_contact_with_provider(
    app: &axum::Router,
    token: &str,
    checksum: &str,
    contact_id: &str,
    provider_type: &str,
    target: &str,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .uri(format!("/api/wallets/{checksum}/contacts/{contact_id}"))
        .method("PUT")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "Updated Provider Contact",
                "notification_methods": [
                    {
                        "provider_type": provider_type,
                        "notification_target": target,
                    }
                ]
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = body_to_json(response.into_body()).await;
    (status, body)
}

async fn post_webhook_test(
    app: &axum::Router,
    token: Option<&str>,
    url: &str,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .uri("/api/webhook/test")
        .method("POST")
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    let response = app
        .clone()
        .oneshot(
            builder
                .body(Body::from(json!({ "url": url }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = body_to_json(response.into_body()).await;
    (status, body)
}

fn derive_regtest_address(script_type: &str, index: u32) -> String {
    let descriptor = match script_type {
        "p2pkh" => format!("pkh({}/0/*)", VALID_TESTNET_XPUB),
        "p2sh" => format!("sh(wpkh({}/0/*))", VALID_TESTNET_XPUB),
        "p2wpkh" => format!("wpkh({}/0/*)", VALID_TESTNET_XPUB),
        "p2tr" => format!("tr({}/0/*)", VALID_TESTNET_XPUB),
        _ => panic!("unsupported script type"),
    };

    let descriptor: miniscript::descriptor::Descriptor<DescriptorPublicKey> =
        descriptor.parse().unwrap();
    descriptor
        .at_derivation_index(index)
        .unwrap()
        .address(Network::Regtest)
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn test_personal_user_wallet_limit_is_enforced() {
    let (app, _temp_dir, _db_path) = create_cloud_test_app().await;
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
    let (app, _temp_dir, _db_path) = create_cloud_test_app().await;
    let token = login_personal_user(&app).await;

    let wallet = create_wallet(
        &app,
        &token,
        "Wallet For Contacts",
        VALID_TESTNET_DESCRIPTOR,
    )
    .await;
    let checksum = wallet["wallet"]["checksum"].as_str().unwrap();
    wait_for_auto_contact(&app, &token, checksum).await;

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

#[tokio::test]
async fn test_self_hosted_mode_bypasses_wallet_limits() {
    let (app, _temp_dir, _db_path) = create_self_hosted_test_app().await;
    let token = self_hosted_admin_token();

    let first_request = Request::builder()
        .uri("/api/wallets")
        .method("POST")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "First Wallet",
                "descriptor": VALID_TESTNET_DESCRIPTOR,
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(first_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let second_request = Request::builder()
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

    let response = app.oneshot(second_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn test_self_hosted_mode_bypasses_contact_limits() {
    let (app, _temp_dir, _db_path) = create_self_hosted_test_app().await;
    let token = self_hosted_admin_token();

    let wallet_request = Request::builder()
        .uri("/api/wallets")
        .method("POST")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "Self-Hosted Contact Wallet",
                "descriptor": VALID_TESTNET_DESCRIPTOR,
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(wallet_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = body_to_json(response.into_body()).await;
    let checksum = body["wallet"]["checksum"].as_str().unwrap();

    // Self-hosted mode uses the hardcoded admin user, so extra contacts should bypass limits.
    let status = create_contact(
        &app,
        Some(&token),
        checksum,
        "Self-Hosted Extra Contact",
        "self-hosted-extra-topic",
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn test_admin_user_bypasses_contact_limits() {
    let (app, _temp_dir, _db_path) = create_cloud_test_app().await;
    let token = login_admin_user(&app).await;

    let wallet = create_wallet(
        &app,
        &token,
        "Admin Contact Wallet",
        VALID_TESTNET_DESCRIPTOR,
    )
    .await;
    let checksum = wallet["wallet"]["checksum"].as_str().unwrap();
    wait_for_auto_contact(&app, &token, checksum).await;

    for index in 0..5 {
        let status = create_contact(
            &app,
            Some(&token),
            checksum,
            &format!("Admin Extra Contact {}", index + 1),
            &format!("admin-extra-topic-{}", index + 1),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
    }
}

#[tokio::test]
async fn test_cloud_mode_rejects_nostr_contact_method() {
    let (app, _temp_dir, _db_path) = create_cloud_test_app().await;
    let token = login_admin_user(&app).await;

    let wallet = create_wallet(&app, &token, "Cloud Nostr Wallet", VALID_TESTNET_DESCRIPTOR).await;
    let checksum = wallet["wallet"]["checksum"].as_str().unwrap();
    let valid_npub = Keys::generate().public_key().to_bech32().unwrap();

    let (status, body) =
        create_contact_with_provider(&app, &token, checksum, "nostr", &valid_npub).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error_code"], "nostr_self_hosted_only");

    let (status, created_contact) =
        create_contact_with_provider(&app, &token, checksum, "ntfy", "cloud-safe-topic").await;
    assert_eq!(status, StatusCode::CREATED);
    let contact_id = created_contact["contact_id"].as_str().unwrap();

    let (status, body) =
        update_contact_with_provider(&app, &token, checksum, contact_id, "nostr", &valid_npub)
            .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error_code"], "nostr_self_hosted_only");
}

#[tokio::test]
async fn test_cloud_mode_rejects_webhook_create_update_and_test() {
    let (app, _temp_dir, _db_path) = create_cloud_test_app().await;
    let token = login_admin_user(&app).await;
    let wallet = create_wallet(
        &app,
        &token,
        "Cloud Webhook Wallet",
        VALID_TESTNET_DESCRIPTOR,
    )
    .await;
    let checksum = wallet["wallet"]["checksum"].as_str().unwrap();

    let (status, body) = create_contact_with_provider(
        &app,
        &token,
        checksum,
        "webhook",
        "https://example.com/canary?token=secret",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error_code"], "webhook_self_hosted_only");

    let (status, created_contact) =
        create_contact_with_provider(&app, &token, checksum, "ntfy", "cloud-webhook-update").await;
    assert_eq!(status, StatusCode::CREATED);
    let contact_id = created_contact["contact_id"].as_str().unwrap();
    let (status, body) = update_contact_with_provider(
        &app,
        &token,
        checksum,
        contact_id,
        "webhook",
        "https://example.com/canary?token=secret",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error_code"], "webhook_self_hosted_only");

    let (status, body) = post_webhook_test(&app, Some(&token), "https://example.com/canary").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error_code"], "webhook_self_hosted_only");
}

#[tokio::test]
async fn test_self_hosted_webhook_provider_validation_reuse_and_redacted_display() {
    let (app, _temp_dir, _db_path) = create_self_hosted_test_app().await;
    let token = self_hosted_admin_token();
    let wallet = create_wallet(
        &app,
        &token,
        "Self-hosted Webhook Wallet",
        VALID_TESTNET_DESCRIPTOR,
    )
    .await;
    let checksum = wallet["wallet"]["checksum"].as_str().unwrap();

    let providers_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/providers")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(providers_response.status(), StatusCode::OK);
    let providers = body_to_json(providers_response.into_body()).await;
    assert!(providers["providers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|provider| provider["name"] == "webhook"));

    let (status, body) =
        create_contact_with_provider(&app, &token, checksum, "webhook", "ftp://example.com/hook")
            .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error_code"], "invalid_webhook_url");

    let full_url = "https://example.com/hooks/canary?token=secret";
    for _ in 0..2 {
        let (status, _) =
            create_contact_with_provider(&app, &token, checksum, "webhook", full_url).await;
        assert_eq!(status, StatusCode::CREATED);
    }

    let contacts_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/wallets/{checksum}/contacts"))
                .method("GET")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let contacts = body_to_json(contacts_response.into_body()).await;
    let webhook_methods: Vec<_> = contacts
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|contact| contact["notification_methods"].as_array().unwrap())
        .filter(|method| method["provider_type"] == "webhook")
        .collect();
    assert_eq!(webhook_methods.len(), 2);
    assert!(webhook_methods
        .iter()
        .all(|method| method["notification_target"] == full_url));
    assert!(webhook_methods
        .iter()
        .all(|method| method["display_target"] == "https://example.com"));

    let (status, _) = post_webhook_test(&app, None, full_url).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, body) = post_webhook_test(
        &app,
        Some(&token),
        "http://127.0.0.1:8080/hook?token=secret",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error_code"], "invalid_webhook_url");
}

#[tokio::test]
async fn test_admin_user_bypasses_wallet_limits() {
    let (app, _temp_dir, _db_path) = create_cloud_test_app().await;
    let token = login_admin_user(&app).await;

    let wallet_inputs = [
        VALID_TESTNET_DESCRIPTOR.to_string(),
        SECOND_TESTNET_DESCRIPTOR.to_string(),
        derive_regtest_address("p2pkh", 0),
        derive_regtest_address("p2sh", 0),
        derive_regtest_address("p2wpkh", 0),
        derive_regtest_address("p2tr", 0),
    ];

    for (index, descriptor) in wallet_inputs.iter().enumerate() {
        let request = Request::builder()
            .uri("/api/wallets")
            .method("POST")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "name": format!("Admin Wallet {}", index + 1),
                    "descriptor": descriptor,
                })
                .to_string(),
            ))
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }
}
