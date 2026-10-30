mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use canary::{
    api::AppServices,
    auth::AuthService,
    config::{OperatingMode, TxExplorerConfig},
};
use common::{body_to_json, test_config, TestApp, LOCAL_FRONTEND_URL, TEST_JWT_SECRET};
use serde_json::{json, Value};
use tempfile::tempdir;
use tower::ServiceExt;

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
        .header("authorization", format!("Bearer {token}"))
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
        .header("authorization", format!("Bearer {token}"))
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
    let temp_dir = tempdir().unwrap();
    let config = test_config(temp_dir.path(), OperatingMode::SelfHosted, None, None)
        .with_tx_explorers(vec![TxExplorerConfig {
            id: "mempool".to_string(),
            name: "Mempool".to_string(),
            base_url: Some("http://umbrel.local:3006".to_string()),
            base_urls: vec!["http://umbrel.local:3006".to_string()],
            port: None,
            platform: Some("umbrel".to_string()),
        }]);

    let TestApp {
        router: app,
        temp_dir: _temp_dir,
        ..
    } = TestApp::new(temp_dir, config, false).await;

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
    assert_eq!(body["tx_explorers"][0]["platform"], "umbrel");
}

#[tokio::test]
async fn test_config_endpoint_self_hosted_serializes_tx_explorer_base_urls() {
    let temp_dir = tempdir().unwrap();
    let config = test_config(temp_dir.path(), OperatingMode::SelfHosted, None, None)
        .with_tx_explorers(vec![TxExplorerConfig {
            id: "mempool".to_string(),
            name: "Mempool".to_string(),
            base_url: None,
            base_urls: vec![
                "https://example-node.local:52127".to_string(),
                "https://203.0.113.10:52127".to_string(),
            ],
            port: None,
            platform: None,
        }]);

    let TestApp {
        router: app,
        temp_dir: _temp_dir,
        ..
    } = TestApp::new(temp_dir, config, false).await;

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
    assert!(body["tx_explorers"][0]["platform"].is_null());
}

#[tokio::test]
async fn test_config_endpoint_self_hosted_with_multiple_tx_explorers() {
    let temp_dir = tempdir().unwrap();
    let config = test_config(temp_dir.path(), OperatingMode::SelfHosted, None, None)
        .with_tx_explorers(vec![
            TxExplorerConfig {
                id: "mempool".to_string(),
                name: "Mempool".to_string(),
                base_url: None,
                base_urls: Vec::new(),
                port: Some(3006),
                platform: None,
            },
            TxExplorerConfig {
                id: "bitfeed".to_string(),
                name: "Bitfeed".to_string(),
                base_url: None,
                base_urls: Vec::new(),
                port: Some(8314),
                platform: None,
            },
        ]);

    let TestApp {
        router: app,
        temp_dir: _temp_dir,
        ..
    } = TestApp::new(temp_dir, config, false).await;

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
    let temp_dir = tempdir().unwrap();
    let config = test_config(temp_dir.path(), OperatingMode::SelfHosted, None, None);

    let TestApp {
        router: app,
        temp_dir: _temp_dir,
        ..
    } = TestApp::new(temp_dir, config, false).await;

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
    let temp_dir = tempdir().unwrap();
    let config = test_config(
        temp_dir.path(),
        OperatingMode::Cloud,
        Some(LOCAL_FRONTEND_URL),
        Some(TEST_JWT_SECRET),
    )
    .with_tx_explorers(vec![TxExplorerConfig {
        id: "mempool".to_string(),
        name: "Mempool".to_string(),
        base_url: Some("http://umbrel.local:3006".to_string()),
        base_urls: Vec::new(),
        port: None,
        platform: None,
    }]);

    let TestApp {
        router: app,
        temp_dir: _temp_dir,
        ..
    } = TestApp::new(temp_dir, config, false).await;

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
    let temp_dir = tempdir().unwrap();
    let config = test_config(
        temp_dir.path(),
        OperatingMode::SelfHosted,
        None,
        Some(TEST_JWT_SECRET),
    )
    .with_tx_explorers(vec![TxExplorerConfig {
        id: "bitfeed".to_string(),
        name: "Bitfeed".to_string(),
        base_url: None,
        base_urls: Vec::new(),
        port: Some(8314),
        platform: None,
    }]);
    let TestApp {
        router: app,
        app_services,
        temp_dir: _temp_dir,
        ..
    } = TestApp::new(temp_dir, config, false).await;
    let token = auth_token_for_user(
        &app_services,
        TEST_JWT_SECRET,
        "tx-explorer-user@example.com",
    )
    .await;

    let (status, body) = update_tx_explorer_preference(app.clone(), &token, "mempool-space").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["preferred_tx_explorer_id"], "mempool-space");

    let (status, body) = update_tx_explorer_preference(app, &token, " bitfeed ").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["preferred_tx_explorer_id"], "bitfeed");
}

#[tokio::test]
async fn test_user_preferences_rejects_unsupported_tx_explorer_id() {
    let temp_dir = tempdir().unwrap();
    let config = test_config(
        temp_dir.path(),
        OperatingMode::SelfHosted,
        None,
        Some(TEST_JWT_SECRET),
    );
    let TestApp {
        router: app,
        app_services,
        temp_dir: _temp_dir,
        ..
    } = TestApp::new(temp_dir, config, false).await;
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
async fn test_user_preferences_accepts_custom_tx_explorer_template() {
    let temp_dir = tempdir().unwrap();
    let config = test_config(
        temp_dir.path(),
        OperatingMode::SelfHosted,
        None,
        Some(TEST_JWT_SECRET),
    );
    let TestApp {
        router: app,
        app_services,
        temp_dir: _temp_dir,
        ..
    } = TestApp::new(temp_dir, config, false).await;
    let token = auth_token_for_user(
        &app_services,
        TEST_JWT_SECRET,
        "custom-explorer-user@example.com",
    )
    .await;

    let custom_preference = "custom:https://example.com/transaction/{txid}";
    let (status, body) =
        update_tx_explorer_preference(app.clone(), &token, custom_preference).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["preferred_tx_explorer_id"], custom_preference);

    let custom_preference_with_spaces = " custom:https://example.com/tx/{txid} ";
    let (status, body) =
        update_tx_explorer_preference(app.clone(), &token, custom_preference_with_spaces).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["preferred_tx_explorer_id"],
        "custom:https://example.com/tx/{txid}"
    );

    let custom_preference_with_inner_spaces = "custom: https://example.com/tx/{txid} ";
    let (status, body) =
        update_tx_explorer_preference(app, &token, custom_preference_with_inner_spaces).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["preferred_tx_explorer_id"],
        "custom:https://example.com/tx/{txid}"
    );
}

#[tokio::test]
async fn test_user_preferences_accepts_custom_tx_explorer_template_in_cloud_mode() {
    let temp_dir = tempdir().unwrap();
    let config = test_config(
        temp_dir.path(),
        OperatingMode::Cloud,
        Some(LOCAL_FRONTEND_URL),
        Some(TEST_JWT_SECRET),
    );
    let TestApp {
        router: app,
        app_services,
        temp_dir: _temp_dir,
        ..
    } = TestApp::new(temp_dir, config, false).await;
    let token = auth_token_for_user(
        &app_services,
        TEST_JWT_SECRET,
        "cloud-custom-explorer-user@example.com",
    )
    .await;

    let custom_preference = "custom:https://example.com/transaction/{txid}";
    let (status, body) = update_tx_explorer_preference(app, &token, custom_preference).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["preferred_tx_explorer_id"], custom_preference);
}

#[tokio::test]
async fn test_user_preferences_rejects_invalid_custom_tx_explorer_template() {
    let temp_dir = tempdir().unwrap();
    let config = test_config(
        temp_dir.path(),
        OperatingMode::SelfHosted,
        None,
        Some(TEST_JWT_SECRET),
    );
    let TestApp {
        router: app,
        app_services,
        temp_dir: _temp_dir,
        ..
    } = TestApp::new(temp_dir, config, false).await;
    let token = auth_token_for_user(
        &app_services,
        TEST_JWT_SECRET,
        "invalid-custom-explorer-user@example.com",
    )
    .await;

    for custom_preference in [
        "custom:https://example.com/transaction/",
        "custom:ftp://example.com/transaction/{txid}",
        "custom:",
        "custom:   ",
    ] {
        let (status, body) =
            update_tx_explorer_preference(app.clone(), &token, custom_preference).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            body["error"],
            format!("Unsupported tx explorer: {}", custom_preference.trim())
        );
    }
}

#[tokio::test]
async fn test_user_preferences_clears_tx_explorer_preference() {
    let temp_dir = tempdir().unwrap();
    let config = test_config(
        temp_dir.path(),
        OperatingMode::SelfHosted,
        None,
        Some(TEST_JWT_SECRET),
    );
    let TestApp {
        router: app,
        app_services,
        temp_dir: _temp_dir,
        ..
    } = TestApp::new(temp_dir, config, false).await;
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
    let temp_dir = tempdir().unwrap();
    let config = test_config(
        temp_dir.path(),
        OperatingMode::SelfHosted,
        None,
        Some(TEST_JWT_SECRET),
    );
    let TestApp {
        router: app,
        app_services,
        temp_dir: _temp_dir,
        ..
    } = TestApp::new(temp_dir, config, false).await;
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
    let temp_dir = tempdir().unwrap();
    let config = test_config(
        temp_dir.path(),
        OperatingMode::Cloud,
        Some(LOCAL_FRONTEND_URL),
        Some(TEST_JWT_SECRET),
    )
    .with_tx_explorers(vec![TxExplorerConfig {
        id: "bitfeed".to_string(),
        name: "Bitfeed".to_string(),
        base_url: Some("http://umbrel.local:8314".to_string()),
        base_urls: Vec::new(),
        port: None,
        platform: None,
    }]);
    let TestApp {
        router: app,
        app_services,
        temp_dir: _temp_dir,
        ..
    } = TestApp::new(temp_dir, config, false).await;
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
