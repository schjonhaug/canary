mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use canary::config::OperatingMode;
use common::{
    body_to_json, create_test_app, login_admin_user, login_user, TestApp, ADMIN_USER_EMAIL,
    PERSONAL_USER_EMAIL, SECOND_TESTNET_DESCRIPTOR, TEAM_USER_EMAIL, VALID_TESTNET_DESCRIPTOR,
};
use serde_json::{json, Value};
use tower::ServiceExt;

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

async fn create_wallet_checksum(
    app: &axum::Router,
    token: &str,
    name: &str,
    descriptor: &str,
) -> String {
    common::create_wallet(app, token, name, descriptor).await["wallet"]["checksum"]
        .as_str()
        .unwrap()
        .to_string()
}
#[tokio::test]
async fn cloud_admin_support_access_is_customer_scoped_read_only_and_audited() {
    let TestApp {
        router: app,
        db_path,
        temp_dir: _temp_dir,
        ..
    } = create_test_app(OperatingMode::Cloud).await;
    let alice = login_user(&app, PERSONAL_USER_EMAIL).await;
    let bob = login_user(&app, TEAM_USER_EMAIL).await;
    let alice_checksum =
        create_wallet_checksum(&app, &alice, "Alice Wallet", VALID_TESTNET_DESCRIPTOR).await;
    let bob_checksum =
        create_wallet_checksum(&app, &bob, "Bob Wallet", SECOND_TESTNET_DESCRIPTOR).await;
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

    let connection = bdk_wallet::rusqlite::Connection::open(&db_path).unwrap();
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

    connection
        .execute("DELETE FROM admin_mfa_replay", [])
        .unwrap();
    drop(connection);
    let admin = login_admin_user(&app, &db_path).await;
    let (status, body) = send(
        &app,
        &admin,
        Request::builder()
            .uri("/api/admin/support-access")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["grant"].is_null());
}

#[tokio::test]
async fn cloud_admin_cannot_open_admin_or_missing_support_targets() {
    let TestApp {
        router: app,
        db_path,
        temp_dir: _temp_dir,
        ..
    } = create_test_app(OperatingMode::Cloud).await;
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
