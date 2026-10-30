mod common;

use axum::{
    body::Body,
    http::{header, HeaderMap, Request, StatusCode},
};
use bdk_wallet::rusqlite::{params, Connection};
use canary::config::OperatingMode;
use common::{
    body_to_json, create_self_hosted_admin_session, create_session, self_hosted_admin_token,
    test_config, test_token, TestApp, TEST_JWT_SECRET,
};
use serde_json::Value;
use tempfile::tempdir;
use tower::ServiceExt;

async fn create_self_hosted_test_app() -> TestApp {
    let temp_dir = tempdir().unwrap();
    let config = test_config(
        temp_dir.path(),
        OperatingMode::SelfHosted,
        None,
        Some(TEST_JWT_SECRET),
    );
    let test_app = TestApp::new(temp_dir, config, false).await;
    create_self_hosted_admin_session(&test_app.app_services, TEST_JWT_SECRET).await;
    test_app
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
    headers: &HeaderMap,
    body: &Value,
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
    let token = self_hosted_admin_token(TEST_JWT_SECRET);

    for _ in 0..6 {
        let (status, _headers, _body) = get_database_health(&test_app.router, &token).await;
        assert_eq!(status, StatusCode::OK);
    }

    let (status, headers, body) = get_database_health(&test_app.router, &token).await;
    assert_rate_limited(status, &headers, &body, "300");
}

#[tokio::test]
async fn database_integrity_endpoint_has_separate_rate_limit_scope() {
    let test_app = create_self_hosted_test_app().await;
    let token = self_hosted_admin_token(TEST_JWT_SECRET);

    for _ in 0..6 {
        let (status, _headers, _body) = get_database_health(&test_app.router, &token).await;
        assert_eq!(status, StatusCode::OK);
    }
    let (status, headers, body) = get_database_health(&test_app.router, &token).await;
    assert_rate_limited(status, &headers, &body, "300");

    let (status, _headers, _body) = post_database_integrity(&test_app.router, &token).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _headers, _body) = post_database_integrity(&test_app.router, &token).await;
    assert_eq!(status, StatusCode::OK);
    let (status, headers, body) = post_database_integrity(&test_app.router, &token).await;
    assert_rate_limited(status, &headers, &body, "600");
}

#[tokio::test]
async fn limiter_backend_failure_uses_per_user_per_scope_fallback() {
    let test_app = create_self_hosted_test_app().await;
    let primary_token = self_hosted_admin_token(TEST_JWT_SECRET);
    let secondary_email = "secondary-admin@example.com";
    let secondary_user_id = test_app
        .app_services
        .metadata_db
        .create_user(
            secondary_email,
            "hash",
            Some("Secondary Admin"),
            true,
            None,
            None,
        )
        .await
        .unwrap();
    let secondary_token = test_token(TEST_JWT_SECRET, &secondary_user_id, secondary_email, true);

    let conn = Connection::open(&test_app.db_path).unwrap();
    conn.execute(
        "UPDATE users SET is_admin = 1 WHERE id = ?1",
        params![&secondary_user_id],
    )
    .unwrap();
    conn.execute("DROP TABLE auth_rate_limits", []).unwrap();
    drop(conn);
    create_session(&test_app.app_services, &secondary_user_id, &secondary_token).await;

    for _ in 0..6 {
        let (status, _, _) = get_database_health(&test_app.router, &primary_token).await;
        assert_eq!(status, StatusCode::OK);
    }
    let (status, headers, body) = get_database_health(&test_app.router, &primary_token).await;
    assert_rate_limited(status, &headers, &body, "300");

    let (status, _, _) = get_database_health(&test_app.router, &secondary_token).await;
    assert_eq!(status, StatusCode::OK);

    for _ in 0..2 {
        let (status, _, _) = post_database_integrity(&test_app.router, &primary_token).await;
        assert_eq!(status, StatusCode::OK);
    }
    let (status, headers, body) = post_database_integrity(&test_app.router, &primary_token).await;
    assert_rate_limited(status, &headers, &body, "600");
}

#[tokio::test]
async fn fallback_preserves_quota_consumed_before_limiter_backend_failure() {
    let test_app = create_self_hosted_test_app().await;
    let token = self_hosted_admin_token(TEST_JWT_SECRET);

    for _ in 0..6 {
        let (status, _, _) = get_database_health(&test_app.router, &token).await;
        assert_eq!(status, StatusCode::OK);
    }

    let conn = Connection::open(&test_app.db_path).unwrap();
    conn.execute("DROP TABLE auth_rate_limits", []).unwrap();
    drop(conn);

    let (status, headers, body) = get_database_health(&test_app.router, &token).await;
    assert_rate_limited(status, &headers, &body, "300");
}
