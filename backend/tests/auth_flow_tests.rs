use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use canary::{
    api::{create_router_with_services, AppServices},
    auth::AuthService,
    config::{AppConfig, NetworkConfig, OperatingMode},
    notifications::NotificationManager,
    wallet::{WalletCreationService, WalletManager},
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::sync::Arc;
use tempfile::{tempdir, TempDir};
use tokio::sync::{broadcast, Mutex};
use tower::ServiceExt;

struct TestApp {
    router: axum::Router,
    app_services: Arc<AppServices>,
    _temp_dir: TempDir,
}

const TEST_SELF_HOSTED_JWT_SECRET: &str = "test-self-hosted-jwt-secret";

async fn create_test_app(mode: OperatingMode) -> TestApp {
    let temp_dir = tempdir().unwrap();
    let temp_path = temp_dir.path().to_str().unwrap();
    let test_db_path = format!("{}/test_metadata.sqlite", temp_path);

    let frontend_url = Some("http://localhost:3001".to_string());

    let jwt_secret = match mode {
        OperatingMode::Cloud => Some("test-jwt-secret".to_string()),
        OperatingMode::SelfHosted => Some(TEST_SELF_HOSTED_JWT_SECRET.to_string()),
    };

    let config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        temp_path.to_string(),
        mode,
        frontend_url,
        jwt_secret.clone(),
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

    let notification_manager = Arc::new(Mutex::new(NotificationManager::new()));
    let router = create_router_with_services(
        app_services.clone(),
        notification_manager,
        None,
        config,
        None,
    );

    TestApp {
        router,
        app_services,
        _temp_dir: temp_dir,
    }
}

async fn body_to_json(body: Body) -> Value {
    let bytes = body.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn create_user(
    app_services: &AppServices,
    email: &str,
    password: &str,
    email_verified: bool,
) -> String {
    let auth_service = AuthService::new("test-jwt-secret".to_string(), None);
    let password_hash = auth_service.hash_password(password).unwrap();

    app_services
        .metadata_db
        .create_user(
            email,
            &password_hash,
            Some("Test User"),
            email_verified,
            Some("USD"),
            Some("en-US"),
        )
        .await
        .unwrap()
}

async fn demo_token(app: &axum::Router) -> String {
    let request = Request::builder()
        .uri("/api/auth/demo-login")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(json!({}).to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    body_to_json(response.into_body()).await["token"]
        .as_str()
        .unwrap()
        .to_string()
}

fn extract_auth_cookie(set_cookie: &str) -> &str {
    set_cookie.split(';').next().unwrap()
}

#[tokio::test]
async fn test_login_rejects_cross_site_origin() {
    let test_app = create_test_app(OperatingMode::Cloud).await;
    create_user(
        &test_app.app_services,
        "user@example.com",
        "correct-horse-battery",
        true,
    )
    .await;

    let login_request = Request::builder()
        .uri("/api/auth/login")
        .method("POST")
        .header("content-type", "application/json")
        .header("origin", "https://attacker.example")
        .body(Body::from(
            json!({
                "email": "user@example.com",
                "password": "correct-horse-battery"
            })
            .to_string(),
        ))
        .unwrap();

    let response = test_app.router.oneshot(login_request).await.unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn demo_session_cannot_send_or_verify_contact_otp() {
    let test_app = create_test_app(OperatingMode::Cloud).await;
    let token = demo_token(&test_app.router).await;

    for (path, payload) in [
        (
            "/api/wallets/any-wallet/contacts/send-verification",
            json!({
                "name": "Demo Contact",
                "email_address": "contact@example.com"
            }),
        ),
        (
            "/api/wallets/any-wallet/contacts/verify",
            json!({
                "email_address": "contact@example.com",
                "code": "123456"
            }),
        ),
    ] {
        let request = Request::builder()
            .uri(path)
            .method("POST")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap();

        let response = test_app.router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{path}");

        let response_body = body_to_json(response.into_body()).await;
        assert_eq!(response_body["error_code"], "demo_read_only");
    }
}

#[tokio::test]
async fn test_login_me_logout_invalidates_session() {
    let test_app = create_test_app(OperatingMode::Cloud).await;
    create_user(
        &test_app.app_services,
        "user@example.com",
        "correct-horse-battery",
        true,
    )
    .await;

    let login_request = Request::builder()
        .uri("/api/auth/login")
        .method("POST")
        .header("content-type", "application/json")
        .header("origin", "http://localhost:3001")
        .body(Body::from(
            json!({
                "email": "user@example.com",
                "password": "correct-horse-battery"
            })
            .to_string(),
        ))
        .unwrap();

    let login_response = test_app
        .router
        .clone()
        .oneshot(login_request)
        .await
        .unwrap();
    assert_eq!(login_response.status(), StatusCode::OK);

    let auth_cookie = extract_auth_cookie(
        login_response
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap(),
    )
    .to_string();

    let body = body_to_json(login_response.into_body()).await;
    assert_eq!(body["user"]["email"], "user@example.com");
    assert!(body["token"].as_str().is_some());

    let me_request = Request::builder()
        .uri("/api/auth/me")
        .method("GET")
        .header("cookie", auth_cookie.clone())
        .body(Body::empty())
        .unwrap();

    let me_response = test_app.router.clone().oneshot(me_request).await.unwrap();
    assert_eq!(me_response.status(), StatusCode::OK);

    let logout_request = Request::builder()
        .uri("/api/auth/logout")
        .method("POST")
        .header("cookie", auth_cookie.clone())
        .body(Body::empty())
        .unwrap();

    let logout_response = test_app
        .router
        .clone()
        .oneshot(logout_request)
        .await
        .unwrap();
    assert_eq!(logout_response.status(), StatusCode::OK);

    let me_after_logout_request = Request::builder()
        .uri("/api/auth/me")
        .method("GET")
        .header("cookie", auth_cookie)
        .body(Body::empty())
        .unwrap();

    let me_after_logout_response = test_app
        .router
        .clone()
        .oneshot(me_after_logout_request)
        .await
        .unwrap();
    assert_eq!(me_after_logout_response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_cloud_mode_accepts_valid_bearer_token_with_session() {
    let test_app = create_test_app(OperatingMode::Cloud).await;
    create_user(
        &test_app.app_services,
        "bearer-user@example.com",
        "password123",
        true,
    )
    .await;

    let login_request = Request::builder()
        .uri("/api/auth/login")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "email": "bearer-user@example.com",
                "password": "password123"
            })
            .to_string(),
        ))
        .unwrap();

    let login_response = test_app
        .router
        .clone()
        .oneshot(login_request)
        .await
        .unwrap();
    assert_eq!(login_response.status(), StatusCode::OK);

    let body = body_to_json(login_response.into_body()).await;
    let token = body["token"].as_str().unwrap();

    let me_request = Request::builder()
        .uri("/api/auth/me")
        .method("GET")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let me_response = test_app.router.oneshot(me_request).await.unwrap();
    assert_eq!(me_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_login_rejects_invalid_password_and_unverified_email() {
    let test_app = create_test_app(OperatingMode::Cloud).await;
    create_user(
        &test_app.app_services,
        "verified@example.com",
        "password123",
        true,
    )
    .await;
    create_user(
        &test_app.app_services,
        "pending@example.com",
        "password123",
        false,
    )
    .await;

    let invalid_password_request = Request::builder()
        .uri("/api/auth/login")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "email": "verified@example.com",
                "password": "wrong-password"
            })
            .to_string(),
        ))
        .unwrap();

    let invalid_password_response = test_app
        .router
        .clone()
        .oneshot(invalid_password_request)
        .await
        .unwrap();
    assert_eq!(invalid_password_response.status(), StatusCode::BAD_REQUEST);

    let invalid_password_body = body_to_json(invalid_password_response.into_body()).await;
    assert_eq!(invalid_password_body["error_code"], "invalid_credentials");

    let unverified_request = Request::builder()
        .uri("/api/auth/login")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "email": "pending@example.com",
                "password": "password123"
            })
            .to_string(),
        ))
        .unwrap();

    let unverified_response = test_app.router.oneshot(unverified_request).await.unwrap();
    assert_eq!(unverified_response.status(), StatusCode::FORBIDDEN);

    let unverified_body = body_to_json(unverified_response.into_body()).await;
    assert_eq!(unverified_body["error_code"], "email_not_verified");
}

#[tokio::test]
async fn test_verify_email_endpoint_marks_user_verified() {
    let test_app = create_test_app(OperatingMode::Cloud).await;
    let user_id = create_user(
        &test_app.app_services,
        "verify@example.com",
        "password123",
        false,
    )
    .await;

    let token = "verify-token";
    let token_hash = AuthService::hash_token(token);
    test_app
        .app_services
        .metadata_db
        .create_email_verification_token(&user_id, &token_hash)
        .await
        .unwrap();

    let verify_request = Request::builder()
        .uri(format!("/api/auth/verify-email/{token}"))
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let verify_response = test_app
        .router
        .clone()
        .oneshot(verify_request)
        .await
        .unwrap();
    assert_eq!(verify_response.status(), StatusCode::OK);

    let user = test_app
        .app_services
        .metadata_db
        .get_user_by_email("verify@example.com")
        .await
        .unwrap()
        .unwrap();
    assert!(user.email_verified);
}

#[tokio::test]
async fn test_reset_password_endpoint_updates_password_and_clears_token() {
    let test_app = create_test_app(OperatingMode::Cloud).await;
    let user_id = create_user(
        &test_app.app_services,
        "reset@example.com",
        "old-password",
        true,
    )
    .await;

    let token = "reset-token";
    let token_hash = AuthService::hash_token(token);
    test_app
        .app_services
        .metadata_db
        .create_password_reset_token(&user_id, &token_hash)
        .await
        .unwrap();

    let reset_request = Request::builder()
        .uri(format!("/api/auth/reset-password/{token}"))
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "password": "new-password"
            })
            .to_string(),
        ))
        .unwrap();

    let reset_response = test_app
        .router
        .clone()
        .oneshot(reset_request)
        .await
        .unwrap();
    assert_eq!(reset_response.status(), StatusCode::OK);

    let login_request = Request::builder()
        .uri("/api/auth/login")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "email": "reset@example.com",
                "password": "new-password"
            })
            .to_string(),
        ))
        .unwrap();

    let login_response = test_app
        .router
        .clone()
        .oneshot(login_request)
        .await
        .unwrap();
    assert_eq!(login_response.status(), StatusCode::OK);

    let token_lookup = test_app
        .app_services
        .metadata_db
        .verify_password_reset_token(&token_hash)
        .await
        .unwrap();
    assert!(token_lookup.is_none());
}

#[tokio::test]
async fn test_cloud_mode_rejects_invalid_jwt() {
    let test_app = create_test_app(OperatingMode::Cloud).await;

    let me_request = Request::builder()
        .uri("/api/auth/me")
        .method("GET")
        .header("authorization", "Bearer invalid.jwt.token")
        .body(Body::empty())
        .unwrap();

    let me_response = test_app.router.oneshot(me_request).await.unwrap();
    assert_eq!(me_response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_cookie_auth_ignores_malformed_authorization_header() {
    let test_app = create_test_app(OperatingMode::Cloud).await;
    create_user(
        &test_app.app_services,
        "cookie-user@example.com",
        "password123",
        true,
    )
    .await;

    let login_request = Request::builder()
        .uri("/api/auth/login")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "email": "cookie-user@example.com",
                "password": "password123"
            })
            .to_string(),
        ))
        .unwrap();

    let login_response = test_app
        .router
        .clone()
        .oneshot(login_request)
        .await
        .unwrap();
    assert_eq!(login_response.status(), StatusCode::OK);

    let auth_cookie = extract_auth_cookie(
        login_response
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap(),
    )
    .to_string();

    let me_request = Request::builder()
        .uri("/api/auth/me")
        .method("GET")
        .header("cookie", auth_cookie)
        .header("authorization", "bad")
        .body(Body::empty())
        .unwrap();

    let me_response = test_app.router.oneshot(me_request).await.unwrap();
    assert_eq!(me_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_self_hosted_rejects_jwt_without_session() {
    let test_app = create_test_app(OperatingMode::SelfHosted).await;

    let token = AuthService::new(TEST_SELF_HOSTED_JWT_SECRET.to_string(), None)
        .generate_token("foss-user", "admin@local", true, false)
        .unwrap();

    let request = Request::builder()
        .uri("/api/wallets")
        .method("GET")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = test_app.router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_self_hosted_login_me_logout_invalidates_session() {
    let test_app = create_test_app(OperatingMode::SelfHosted).await;

    let login_request = Request::builder()
        .uri("/api/auth/login")
        .method("POST")
        .header("content-type", "application/json")
        .header("origin", "http://localhost:3001")
        .body(Body::from(
            json!({
                "email": "admin@local",
                "password": "test-self-hosted-password"
            })
            .to_string(),
        ))
        .unwrap();

    let login_response = test_app
        .router
        .clone()
        .oneshot(login_request)
        .await
        .unwrap();
    assert_eq!(login_response.status(), StatusCode::OK);

    let auth_cookie = extract_auth_cookie(
        login_response
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap(),
    )
    .to_string();

    let body = body_to_json(login_response.into_body()).await;
    assert_eq!(body["user"]["email"], "admin@local");
    let bearer_token = body["token"].as_str().unwrap().to_string();

    let me_request = Request::builder()
        .uri("/api/auth/me")
        .method("GET")
        .header("cookie", auth_cookie.clone())
        .body(Body::empty())
        .unwrap();

    let me_response = test_app.router.clone().oneshot(me_request).await.unwrap();
    assert_eq!(me_response.status(), StatusCode::OK);

    let logout_request = Request::builder()
        .uri("/api/auth/logout")
        .method("POST")
        .header("cookie", auth_cookie.clone())
        .body(Body::empty())
        .unwrap();

    let logout_response = test_app
        .router
        .clone()
        .oneshot(logout_request)
        .await
        .unwrap();
    assert_eq!(logout_response.status(), StatusCode::OK);

    let me_after_logout_request = Request::builder()
        .uri("/api/auth/me")
        .method("GET")
        .header("cookie", auth_cookie)
        .body(Body::empty())
        .unwrap();

    let me_after_logout_response = test_app
        .router
        .clone()
        .oneshot(me_after_logout_request)
        .await
        .unwrap();
    assert_eq!(me_after_logout_response.status(), StatusCode::UNAUTHORIZED);

    let me_after_logout_bearer_request = Request::builder()
        .uri("/api/auth/me")
        .method("GET")
        .header("authorization", format!("Bearer {}", bearer_token))
        .body(Body::empty())
        .unwrap();

    let me_after_logout_bearer_response = test_app
        .router
        .clone()
        .oneshot(me_after_logout_bearer_request)
        .await
        .unwrap();
    assert_eq!(
        me_after_logout_bearer_response.status(),
        StatusCode::UNAUTHORIZED
    );
}
