mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use canary::config::OperatingMode;
use common::{
    body_to_json, create_test_app, create_wallet, login_admin_user, login_user,
    self_hosted_admin_token, TestApp, PERSONAL_USER_EMAIL, SECOND_TESTNET_DESCRIPTOR,
    TEAM_USER_EMAIL, TEST_JWT_SECRET, VALID_TESTNET_DESCRIPTOR,
};
use nostr_sdk::prelude::{Keys, ToBech32};
use serde_json::{json, Value};
use tokio::time::{sleep, Duration};
use tower::ServiceExt;

async fn login_personal_user(app: &axum::Router) -> String {
    login_user(app, PERSONAL_USER_EMAIL).await
}

async fn login_team_user(app: &axum::Router) -> String {
    login_user(app, TEAM_USER_EMAIL).await
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

async fn telegram_settings(
    app: &axum::Router,
    token: &str,
    method: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .uri("/api/telegram/settings")
        .method(method)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(body.map_or_else(Body::empty, |value| Body::from(value.to_string())))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = body_to_json(response.into_body()).await;
    (status, body)
}

async fn get_providers(app: &axum::Router) -> (StatusCode, Value) {
    let response = app
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
    let status = response.status();
    let body = body_to_json(response.into_body()).await;
    (status, body)
}

fn provider_names(body: &Value) -> Vec<&str> {
    body["providers"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|provider| provider["name"].as_str())
        .collect()
}

#[tokio::test]
async fn test_personal_user_wallet_limit_is_enforced() {
    let TestApp {
        router: app,
        temp_dir: _temp_dir,
        ..
    } = create_test_app(OperatingMode::Cloud).await;
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
    let TestApp {
        router: app,
        temp_dir: _temp_dir,
        ..
    } = create_test_app(OperatingMode::Cloud).await;
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
    let TestApp {
        router: app,
        temp_dir: _temp_dir,
        ..
    } = create_test_app(OperatingMode::SelfHosted).await;
    let token = self_hosted_admin_token(TEST_JWT_SECRET);

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
    let TestApp {
        router: app,
        temp_dir: _temp_dir,
        ..
    } = create_test_app(OperatingMode::SelfHosted).await;
    let token = self_hosted_admin_token(TEST_JWT_SECRET);

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
async fn test_cloud_admin_cannot_add_contacts_to_customer_wallets() {
    let TestApp {
        router: app,
        db_path,
        temp_dir: _temp_dir,
        ..
    } = create_test_app(OperatingMode::Cloud).await;
    let owner = login_personal_user(&app).await;
    let admin = login_admin_user(&app, &db_path).await;

    let wallet = create_wallet(
        &app,
        &owner,
        "Customer Contact Wallet",
        VALID_TESTNET_DESCRIPTOR,
    )
    .await;
    let checksum = wallet["wallet"]["checksum"].as_str().unwrap();

    let status = create_contact(
        &app,
        Some(&admin),
        checksum,
        "Admin Extra Contact",
        "admin-extra-topic",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_cloud_mode_rejects_nostr_contact_method() {
    let TestApp {
        router: app,
        temp_dir: _temp_dir,
        ..
    } = create_test_app(OperatingMode::Cloud).await;
    let token = login_team_user(&app).await;

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
    let TestApp {
        router: app,
        temp_dir: _temp_dir,
        ..
    } = create_test_app(OperatingMode::Cloud).await;
    let token = login_team_user(&app).await;
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
async fn test_cloud_mode_rejects_telegram_contacts_and_settings() {
    let TestApp {
        router: app,
        temp_dir: _temp_dir,
        ..
    } = create_test_app(OperatingMode::Cloud).await;
    let token = login_team_user(&app).await;
    let wallet = create_wallet(
        &app,
        &token,
        "Cloud Telegram Wallet",
        VALID_TESTNET_DESCRIPTOR,
    )
    .await;
    let checksum = wallet["wallet"]["checksum"].as_str().unwrap();

    let (status, body) =
        create_contact_with_provider(&app, &token, checksum, "telegram", "123456789").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error_code"], "telegram_self_hosted_only");

    let (status, body) = telegram_settings(&app, &token, "GET", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error_code"], "telegram_self_hosted_only");

    let (status, body) = telegram_settings(
        &app,
        &token,
        "PUT",
        Some(json!({ "bot_token": "123456:ABC-DEF" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error_code"], "telegram_self_hosted_only");
}

#[tokio::test]
async fn test_self_hosted_telegram_settings_gate_provider_and_never_echo_token() {
    let TestApp {
        router: app,
        temp_dir: _temp_dir,
        ..
    } = create_test_app(OperatingMode::SelfHosted).await;
    let token = self_hosted_admin_token(TEST_JWT_SECRET);

    let (status, providers) = get_providers(&app).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!provider_names(&providers).contains(&"telegram"));

    let (status, body) = telegram_settings(&app, &token, "GET", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["configured"], false);
    assert!(body.get("bot_token").is_none());

    let saved_token = "123456:settings-secret-token";
    let (status, body) = telegram_settings(
        &app,
        &token,
        "PUT",
        Some(json!({ "bot_token": saved_token })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["configured"], true);
    assert!(body.get("bot_token").is_none());
    assert!(!body.to_string().contains(saved_token));

    let (status, providers) = get_providers(&app).await;
    assert_eq!(status, StatusCode::OK);
    assert!(provider_names(&providers).contains(&"telegram"));

    let wallet = create_wallet(
        &app,
        &token,
        "Self-hosted Telegram Wallet",
        VALID_TESTNET_DESCRIPTOR,
    )
    .await;
    let checksum = wallet["wallet"]["checksum"].as_str().unwrap();
    let (status, contact) =
        create_contact_with_provider(&app, &token, checksum, "telegram", "123456789").await;
    assert_eq!(status, StatusCode::CREATED, "{contact}");
    let contact_id = contact["contact_id"].as_str().unwrap();

    let (status, body) =
        telegram_settings(&app, &token, "PUT", Some(json!({ "bot_token": "" }))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["configured"], false);

    let (status, body) =
        update_contact_with_provider(&app, &token, checksum, contact_id, "telegram", "123456789")
            .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) =
        update_contact_with_provider(&app, &token, checksum, contact_id, "telegram", "987654321")
            .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error_code"], "telegram_not_configured");

    let (status, body) = update_contact_with_provider(
        &app,
        &token,
        checksum,
        contact_id,
        "ntfy",
        "replacement-topic",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn test_self_hosted_webhook_provider_validation_reuse_and_redacted_display() {
    let TestApp {
        router: app,
        temp_dir: _temp_dir,
        ..
    } = create_test_app(OperatingMode::SelfHosted).await;
    let token = self_hosted_admin_token(TEST_JWT_SECRET);
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
        let (status, body) =
            create_contact_with_provider(&app, &token, checksum, "webhook", full_url).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
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
    assert!(webhook_methods.iter().all(|method| method["content_fields"]
        == json!({
            "wallet_name": true,
            "event_type": true,
            "transaction_amount": false,
            "transaction_balance": false,
            "balance_alert_condition": false,
            "balance_alert_threshold": false,
            "balance_alert_balance": false,
        })));

    let (status, _) = post_webhook_test(&app, None, full_url).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, body) = post_webhook_test(
        &app,
        Some(&token),
        "http://169.254.169.254/hook?token=secret",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error_code"], "invalid_webhook_url");

    let lan_url = "http://192.168.1.10/hooks/canary?token=secret";
    let (status, body) =
        create_contact_with_provider(&app, &token, checksum, "webhook", lan_url).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, body) = post_webhook_test(&app, Some(&token), "http://127.0.0.1:1/hook").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["success"], false);
}

#[tokio::test]
async fn test_legacy_privacy_and_custom_fields_follow_independent_methods() {
    let TestApp {
        router: app,
        temp_dir: _temp_dir,
        ..
    } = create_test_app(OperatingMode::SelfHosted).await;
    let token = self_hosted_admin_token(TEST_JWT_SECRET);
    let wallet = create_wallet(
        &app,
        &token,
        "Privacy Association Wallet",
        VALID_TESTNET_DESCRIPTOR,
    )
    .await;
    let checksum = wallet["wallet"]["checksum"].as_str().unwrap();

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/wallets/{checksum}/contacts"))
                .method("POST")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "Privacy Contact",
                        "notification_methods": [
                            {
                                "provider_type": "ntfy",
                                "notification_target": "privacy-minimal",
                                "content_privacy_level": "minimal",
                            },
                            {
                                "provider_type": "ntfy",
                                "notification_target": "privacy-detailed",
                                "content_privacy_level": "detailed",
                            },
                        ],
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::CREATED);
    let created = body_to_json(create_response.into_body()).await;
    let contact_id = created["contact_id"].as_str().unwrap();

    let update_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/wallets/{checksum}/contacts/{contact_id}"))
                .method("PUT")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "Privacy Contact",
                        "notification_methods": [
                            {
                                "provider_type": "ntfy",
                                "notification_target": "privacy-detailed",
                            },
                            {
                                "provider_type": "ntfy",
                                "notification_target": "privacy-minimal",
                                "content_fields": {
                                    "wallet_name": false,
                                    "event_type": true,
                                    "transaction_amount": true,
                                    "transaction_balance": false,
                                    "balance_alert_condition": false,
                                    "balance_alert_threshold": true,
                                    "balance_alert_balance": false
                                },
                            },
                        ],
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(update_response.status(), StatusCode::OK);

    let contacts_response = app
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
    let methods = contacts
        .as_array()
        .unwrap()
        .iter()
        .find(|contact| contact["id"] == contact_id)
        .unwrap()["notification_methods"]
        .as_array()
        .unwrap();

    assert!(methods.iter().any(|method| {
        method["notification_target"] == "privacy-detailed"
            && method["content_fields"]["wallet_name"] == true
            && method["content_fields"]["transaction_amount"] == true
            && method["content_fields"]["balance_alert_balance"] == true
    }));
    assert!(methods.iter().any(|method| {
        method["notification_target"] == "privacy-minimal"
            && method["content_fields"]
                == json!({
                    "wallet_name": false,
                    "event_type": true,
                    "transaction_amount": true,
                    "transaction_balance": false,
                    "balance_alert_condition": false,
                    "balance_alert_threshold": true,
                    "balance_alert_balance": false,
                })
    }));
}

#[tokio::test]
async fn test_cloud_admin_cannot_create_wallets() {
    let TestApp {
        router: app,
        db_path,
        temp_dir: _temp_dir,
        ..
    } = create_test_app(OperatingMode::Cloud).await;
    let token = login_admin_user(&app, &db_path).await;

    let request = Request::builder()
        .uri("/api/wallets")
        .method("POST")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "Admin Wallet",
                "descriptor": VALID_TESTNET_DESCRIPTOR,
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = body_to_json(response.into_body()).await;
    assert_eq!(body["error_code"], "admin_wallets_unsupported");
}
