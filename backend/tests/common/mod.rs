//! Shared fixtures for the HTTP and database integration tests in `tests/`.
//!
//! Each integration test file is its own crate and pulls this module in with
//! `mod common;`. The builders here create a temporary data directory, a
//! regtest [`AppConfig`], the app services and the router, so tests only
//! describe what differs from the default.
#![allow(
    dead_code,
    reason = "each test binary compiles this module but uses a different subset"
)]

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use canary::{
    TelegramProvider, WebhookProvider,
    api::{AppServices, create_router_with_services},
    auth::{AuthService, Claims, DEV_TEST_PASSWORD},
    config::{AppConfig, NetworkConfig, OperatingMode},
    electrum::ElectrumClientManager,
    metadata::{MetadataDb, TransactionNotification},
    notifications::NotificationManager,
    wallet::{WalletCreationService, WalletManager},
};
use http_body_util::BodyExt;
use jsonwebtoken::{EncodingKey, Header, encode};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;
use tempfile::{TempDir, tempdir};
use tokio::sync::broadcast;
use tower::ServiceExt;

/// JWT secret for apps built with an explicit secret.
pub const TEST_JWT_SECRET: &str = "test-jwt-secret";
/// JWT secret that `AppConfig::new_for_test` falls back to in self-hosted mode.
pub const TEST_SELF_HOSTED_JWT_SECRET: &str = "test-self-hosted-jwt-secret";

/// User ID of the built-in self-hosted administrator.
pub const SELF_HOSTED_USER_ID: &str = "foss-user";
/// Email of the built-in self-hosted administrator.
pub const SELF_HOSTED_ADMIN_EMAIL: &str = "admin@local";

// Seeded cloud users. They all share `DEV_TEST_PASSWORD`.
pub const ADMIN_USER_EMAIL: &str = "delivered+admin@resend.dev";
pub const PERSONAL_USER_EMAIL: &str = "delivered+alice@resend.dev";
pub const TEAM_USER_EMAIL: &str = "delivered+bob@resend.dev";

// Two unrelated testnet descriptors that the regtest app accepts.
pub const VALID_TESTNET_DESCRIPTOR: &str = "wpkh(tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5/<0;1>/*)";
pub const SECOND_TESTNET_DESCRIPTOR: &str = "wpkh(tpubDCMRAYcH71Gagskm7E5peNMYB5sKaLLwtn2c4Rb3CMUTRVUk5dkpsskhspa5MEcVZ11LwTcM7R5mzndUCG9WabYcT5hfQHbYVoaLFBZHPCi/<0;1>/*)";

/// Frontend origin used by tests that exercise browser provenance checks.
pub const LOCAL_FRONTEND_URL: &str = "http://localhost:3001";

/// File name of the admin MFA secrets file that [`login_admin_user`] writes.
pub const ADMIN_MFA_SECRETS_FILE: &str = "admin-mfa.json";

/// Nothing listens here. Tests that need a connected Electrum use the mock manager.
const TEST_ELECTRUM_URL: &str = "tcp://127.0.0.1:50001";
const TEST_DB_FILE: &str = "test_metadata.sqlite";

/// A router together with the state a test needs to inspect it.
pub struct TestApp {
    pub router: Router,
    pub app_services: Arc<AppServices>,
    pub db_path: String,
    /// Owns the data directory; dropping it deletes the database.
    pub temp_dir: TempDir,
}

impl TestApp {
    /// Builds an app on `config` with no notification providers.
    pub async fn new(temp_dir: TempDir, config: AppConfig, mock_electrum: bool) -> Self {
        let app_services = create_app_services(temp_dir.path(), &config).await;
        let router = create_router(
            &app_services,
            NotificationManager::new(),
            config,
            mock_electrum,
        );
        Self {
            router,
            db_path: db_path(temp_dir.path()),
            app_services,
            temp_dir,
        }
    }
}

/// Returns a regtest config rooted at `data_dir`.
pub fn test_config(
    data_dir: &Path,
    mode: OperatingMode,
    frontend_url: Option<&str>,
    jwt_secret: Option<&str>,
) -> AppConfig {
    AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some(TEST_ELECTRUM_URL.to_string()),
        "127.0.0.1:3000".to_string(),
        data_dir.to_str().unwrap().to_string(),
        mode,
        frontend_url.map(str::to_string),
        jwt_secret.map(str::to_string),
    )
}

/// Returns the metadata database path inside `data_dir`.
pub fn db_path(data_dir: &Path) -> String {
    data_dir.join(TEST_DB_FILE).to_str().unwrap().to_string()
}

/// Opens a fresh metadata database inside `config.data_dir`.
pub async fn test_metadata_db(config: &AppConfig) -> MetadataDb {
    MetadataDb::new(&db_path(Path::new(&config.data_dir)), config)
        .await
        .unwrap()
}

/// Opens a fresh self-hosted metadata database inside `temp_dir`.
pub async fn self_hosted_metadata_db(temp_dir: &TempDir) -> MetadataDb {
    test_metadata_db(&test_config(
        temp_dir.path(),
        OperatingMode::SelfHosted,
        None,
        None,
    ))
    .await
}

/// Builds the wallet manager and app services with their database in `data_dir`.
pub async fn create_app_services(data_dir: &Path, config: &AppConfig) -> Arc<AppServices> {
    let (event_tx, _event_rx) = broadcast::channel::<TransactionNotification>(100);
    let wallet_manager = Arc::new(
        WalletManager::new(
            event_tx,
            data_dir.into(),
            &db_path(data_dir),
            bdk_wallet::bitcoin::Network::Regtest,
            TEST_ELECTRUM_URL,
            config,
        )
        .await,
    );
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
}

/// Builds the router without Stripe billing.
///
/// With `mock_electrum` the router sees an Electrum manager that reports
/// itself as connected; without it there is no Electrum manager at all.
pub fn create_router(
    app_services: &Arc<AppServices>,
    notification_manager: NotificationManager,
    config: AppConfig,
    mock_electrum: bool,
) -> Router {
    let electrum_manager =
        mock_electrum.then(|| Arc::new(ElectrumClientManager::new_mock_connected()));
    create_router_with_services(
        app_services,
        &Arc::new(notification_manager),
        &None,
        config,
        electrum_manager,
    )
}

/// Builds an app the way a deployment in `mode` would run.
///
/// It uses [`TEST_JWT_SECRET`] and the [`ADMIN_MFA_SECRETS_FILE`] that
/// [`login_admin_user`] writes. In self-hosted mode it also creates a session
/// for [`self_hosted_admin_token`] and registers the self-hosted webhook and
/// Telegram providers.
pub async fn create_test_app(mode: OperatingMode) -> TestApp {
    let temp_dir = tempdir().unwrap();
    let config = test_config(temp_dir.path(), mode, None, Some(TEST_JWT_SECRET))
        .with_admin_mfa_secrets_file(temp_dir.path().join(ADMIN_MFA_SECRETS_FILE));
    let app_services = create_app_services(temp_dir.path(), &config).await;

    let mut notification_manager = NotificationManager::new();
    if config.is_self_hosted_mode() {
        create_self_hosted_admin_session(&app_services, TEST_JWT_SECRET).await;
        notification_manager.register_provider(Arc::new(WebhookProvider::new()));
        if let Some(telegram_provider) =
            TelegramProvider::for_self_hosted(app_services.metadata_db.clone(), &config)
        {
            notification_manager.register_provider(Arc::new(telegram_provider));
        }
    }

    let router = create_router(&app_services, notification_manager, config, true);
    TestApp {
        router,
        db_path: db_path(temp_dir.path()),
        app_services,
        temp_dir,
    }
}

/// Collects a response body and parses it as JSON.
pub async fn body_to_json(body: Body) -> Value {
    let bytes = body.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

/// Signs a long-lived JWT for `user_id` with `secret`.
///
/// The token is deterministic, so a session created for it with
/// [`create_session`] stays valid for every later call with the same arguments.
pub fn test_token(secret: &str, user_id: &str, email: &str, is_admin: bool) -> String {
    let claims = Claims {
        sub: user_id.to_string(),
        email: email.to_string(),
        is_admin,
        is_demo: false,
        exp: 4_102_444_800,
        iat: 1_700_000_000,
        jti: format!("test-{user_id}-{email}-{is_admin}"),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_ref()),
    )
    .unwrap()
}

/// Signs a JWT for the built-in self-hosted administrator.
pub fn self_hosted_admin_token(secret: &str) -> String {
    test_token(secret, SELF_HOSTED_USER_ID, SELF_HOSTED_ADMIN_EMAIL, true)
}

/// Stores a seven-day session for `token`, so the auth middleware accepts it.
pub async fn create_session(app_services: &AppServices, user_id: &str, token: &str) {
    app_services
        .metadata_db
        .create_session(
            user_id,
            &AuthService::hash_token(token),
            chrono::Utc::now() + chrono::Duration::days(7),
        )
        .await
        .unwrap();
}

/// Stores a session for the [`test_token`] with the same arguments.
pub async fn create_session_for_user(
    app_services: &AppServices,
    secret: &str,
    user_id: &str,
    email: &str,
    is_admin: bool,
) {
    let token = test_token(secret, user_id, email, is_admin);
    create_session(app_services, user_id, &token).await;
}

/// Stores a session for [`self_hosted_admin_token`].
pub async fn create_self_hosted_admin_session(app_services: &AppServices, secret: &str) {
    create_session_for_user(
        app_services,
        secret,
        SELF_HOSTED_USER_ID,
        SELF_HOSTED_ADMIN_EMAIL,
        true,
    )
    .await;
}

async fn login(app: &Router, credentials: Value) -> String {
    let request = Request::builder()
        .uri("/api/auth/login")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(credentials.to_string()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    body_to_json(response.into_body()).await["token"]
        .as_str()
        .unwrap()
        .to_string()
}

/// Logs a seeded cloud user in with the shared development password.
pub async fn login_user(app: &Router, email: &str) -> String {
    login(
        app,
        json!({
            "email": email,
            "password": DEV_TEST_PASSWORD,
        }),
    )
    .await
}

/// Provisions a TOTP factor for [`ADMIN_USER_EMAIL`] and logs in with it.
///
/// Cloud administrators need an externally provisioned factor. The factor is
/// written to [`ADMIN_MFA_SECRETS_FILE`] next to the database at `db_path`, so
/// the app must be configured to read it from there.
pub async fn login_admin_user(app: &Router, db_path: &str) -> String {
    use bdk_wallet::rusqlite::{Connection, params};
    use std::os::unix::fs::PermissionsExt;

    let connection = Connection::open(db_path).unwrap();
    let user_id: String = connection
        .query_row(
            "SELECT id FROM users WHERE email = ?1",
            params![ADMIN_USER_EMAIL],
            |row| row.get(0),
        )
        .unwrap();
    // Synthetic fixture secret; it never leaves the temporary test directory.
    let secret = totp_rs::Secret::new(Box::from(*b"12345678901234567890"));
    let factor_path = Path::new(db_path).with_file_name(ADMIN_MFA_SECRETS_FILE);
    std::fs::write(
        &factor_path,
        json!({user_id: secret.to_base32()}).to_string(),
    )
    .unwrap();
    std::fs::set_permissions(&factor_path, std::fs::Permissions::from_mode(0o600)).unwrap();

    let code = totp_rs::Builder::new()
        .with_secret(secret)
        .build()
        .unwrap()
        .generate(u64::try_from(chrono::Utc::now().timestamp()).unwrap())
        .to_string();
    login(
        app,
        json!({
            "email": ADMIN_USER_EMAIL,
            "password": DEV_TEST_PASSWORD,
            "mfa_code": code,
        }),
    )
    .await
}

/// Creates a wallet through the API, asserts `201 Created` and returns the body.
pub async fn create_wallet(app: &Router, token: &str, name: &str, descriptor: &str) -> Value {
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
    let status = response.status();
    let body = body_to_json(response.into_body()).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body
}
