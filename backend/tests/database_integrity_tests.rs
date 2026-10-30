#![expect(
    clippy::too_many_lines,
    reason = "end-to-end scenarios read top to bottom"
)]

mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use bdk_wallet::rusqlite::{Connection, params};
use canary::{
    auth::AuthService,
    config::OperatingMode,
    metadata::{ProviderType, WalletType},
};
use common::{
    SELF_HOSTED_ADMIN_EMAIL, SELF_HOSTED_USER_ID, TEST_SELF_HOSTED_JWT_SECRET, TestApp,
    body_to_json, create_session_for_user, self_hosted_admin_token, test_config, test_token,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tempfile::tempdir;
use tower::ServiceExt;

struct SeededValidRecords {
    contact_id: String,
    method_id: String,
}

/// Builds a self-hosted app with sessions for the built-in admin and for
/// `regular@example.com`. The router sees a mocked Electrum manager, and these
/// tests never sync.
async fn create_test_app() -> TestApp {
    let temp_dir = tempdir().unwrap();
    let config = test_config(
        temp_dir.path(),
        OperatingMode::SelfHosted,
        None,
        Some(TEST_SELF_HOSTED_JWT_SECRET),
    );
    let test_app = TestApp::new(temp_dir, config, true).await;
    let app_services = &test_app.app_services;

    create_session_for_user(
        app_services,
        TEST_SELF_HOSTED_JWT_SECRET,
        SELF_HOSTED_USER_ID,
        SELF_HOSTED_ADMIN_EMAIL,
        true,
    )
    .await;
    let regular_user_id = app_services
        .metadata_db
        .create_user(
            "regular@example.com",
            "hash",
            Some("Regular User"),
            true,
            None,
            None,
        )
        .await
        .unwrap();
    create_session_for_user(
        app_services,
        TEST_SELF_HOSTED_JWT_SECRET,
        &regular_user_id,
        "regular@example.com",
        false,
    )
    .await;

    test_app
}

async fn request_json(
    app: &axum::Router,
    method: &str,
    uri: &str,
    token: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"));

    let request_body = if let Some(body) = body {
        builder = builder.header("content-type", "application/json");
        Body::from(body.to_string())
    } else {
        Body::empty()
    };

    let response = app
        .clone()
        .oneshot(builder.body(request_body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = body_to_json(response.into_body()).await;
    (status, body)
}

async fn request_raw(
    app: &axum::Router,
    method: &str,
    uri: &str,
    token: &str,
    body: Body,
) -> (StatusCode, String) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(body)
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    (status, body)
}

async fn request_without_auth(app: &axum::Router, method: &str, uri: &str) -> StatusCode {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    response.status()
}

async fn seed_valid_records(app: &TestApp) -> SeededValidRecords {
    let user_id = app
        .app_services
        .metadata_db
        .create_user(
            "owner@example.com",
            "hash",
            Some("Owner"),
            false,
            None,
            None,
        )
        .await
        .unwrap();
    let wallet_checksum = app
        .app_services
        .metadata_db
        .insert_wallet_with_type_and_checksum(
            "Valid Wallet",
            "valid-descriptor",
            &user_id,
            WalletType::Descriptor,
            Some("valid-wallet"),
        )
        .await
        .unwrap();
    let contact_id = app
        .app_services
        .metadata_db
        .insert_contact_with_notification_methods(
            &wallet_checksum,
            "Valid Contact",
            vec![(ProviderType::Email, "owner@example.com".to_string())],
        )
        .await
        .unwrap();

    let conn = Connection::open(&app.db_path).unwrap();
    conn.execute(
        "INSERT INTO transactions (txid, wallet_checksum, transaction_type, amount_sats, first_seen_at)
         VALUES ('valid-tx', ?1, 'receive', 1000, 100)",
        params![&wallet_checksum],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO balance_alerts (id, wallet_checksum, threshold_sats, alert_type)
         VALUES ('valid-alert', ?1, 1000, 'above')",
        params![&wallet_checksum],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO balance_alert_notification_logs
         (id, balance_alert_id, wallet_checksum, provider_name, status, message_content)
         VALUES ('valid-alert-log', 'valid-alert', ?1, 'test', 'sent', 'valid')",
        params![&wallet_checksum],
    )
    .unwrap();

    let method_id: String = conn
        .query_row(
            "SELECT id FROM contact_notification_methods WHERE contact_id = ?1",
            params![&contact_id],
            |row| row.get(0),
        )
        .unwrap();
    conn.execute(
        "INSERT INTO notification_logs
         (id, transaction_txid, transaction_wallet_checksum, notification_method_id, provider_name, status, message_content)
         VALUES ('valid-log', 'valid-tx', ?1, ?2, 'test', 'sent', 'valid')",
        params![&wallet_checksum, &method_id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO notification_logs
         (id, transaction_txid, transaction_wallet_checksum, notification_method_id, provider_name, status, message_content)
         VALUES ('audit-record-log', 'valid-tx', ?1, NULL, 'test', 'sent', 'valid audit')",
        params![&wallet_checksum],
    )
    .unwrap();

    SeededValidRecords {
        contact_id,
        method_id,
    }
}

fn seed_orphaned_records(db_path: &str) {
    let conn = Connection::open(db_path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
    conn.execute(
        "INSERT INTO contacts (id, wallet_checksum, name)
         VALUES ('orphan-contact', 'missing-wallet', 'Orphan Contact')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO contact_notification_methods
         (id, contact_id, provider_type, notification_target, wallet_checksum)
         VALUES ('orphan-method', 'missing-contact', 'email', 'orphan@example.com', 'missing-wallet')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO notification_logs
         (id, transaction_txid, transaction_wallet_checksum, notification_method_id, provider_name, status, message_content)
         VALUES ('orphan-log', 'missing-tx', 'missing-wallet', 'missing-method', 'test', 'sent', 'orphan')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO notification_logs
         (id, transaction_txid, transaction_wallet_checksum, notification_method_id, provider_name, status, message_content)
         VALUES ('orphan-log-missing-tx', 'missing-tx-no-method', 'missing-wallet', NULL, 'test', 'sent', 'orphan')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO transactions (txid, wallet_checksum, transaction_type, amount_sats, first_seen_at)
         VALUES ('orphan-tx', 'missing-wallet', 'receive', 1000, 100)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO balance_alerts (id, wallet_checksum, threshold_sats, alert_type)
         VALUES ('orphan-alert', 'missing-wallet', 1000, 'above')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO balance_alert_notification_logs
         (id, balance_alert_id, wallet_checksum, provider_name, status, message_content)
         VALUES ('orphan-alert-log', 'missing-alert', 'missing-wallet', 'test', 'sent', 'orphan')",
        [],
    )
    .unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();
}

fn seed_soft_deleted_wallet_contact(db_path: &str) {
    let conn = Connection::open(db_path).unwrap();
    let user_id: String = conn
        .query_row("SELECT id FROM users LIMIT 1", [], |row| row.get(0))
        .unwrap();

    conn.execute(
        "INSERT INTO wallets (checksum, name, descriptor, hex_color, status, user_id)
         VALUES ('deleted-contact-wallet', 'Deleted Contact Wallet', 'deleted-contact-wallet-descriptor', '#222222', 'deleted', ?1)",
        params![user_id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO contacts (id, wallet_checksum, name)
         VALUES ('soft-deleted-wallet-contact', 'deleted-contact-wallet', 'Soft Deleted Wallet Contact')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO contact_notification_methods
         (id, contact_id, provider_type, notification_target, wallet_checksum)
         VALUES ('soft-deleted-wallet-contact-method', 'soft-deleted-wallet-contact', 'email', 'deleted-contact@example.com', 'deleted-contact-wallet')",
        [],
    )
    .unwrap();
}

fn seed_orphaned_transaction_log_with_valid_method(db_path: &str, method_id: &str) {
    let conn = Connection::open(db_path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
    conn.execute(
        "INSERT INTO notification_logs
         (id, transaction_txid, transaction_wallet_checksum, notification_method_id, provider_name, status, message_content)
         VALUES ('orphan-log-valid-method-missing-tx', 'missing-tx-with-valid-method', 'missing-wallet', ?1, 'test', 'sent', 'orphan')",
        params![method_id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO notification_logs
         (id, transaction_txid, transaction_wallet_checksum, notification_method_id, provider_name, status, message_content)
         VALUES ('orphan-log-valid-method-orphan-tx', 'orphan-tx', 'missing-wallet', ?1, 'test', 'sent', 'orphan')",
        params![method_id],
    )
    .unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();
}

fn seed_missing_method_log_for_valid_transaction(db_path: &str) {
    let conn = Connection::open(db_path).unwrap();
    let wallet_checksum: String = conn
        .query_row(
            "SELECT transaction_wallet_checksum FROM notification_logs WHERE id = 'valid-log'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
    conn.execute(
        "INSERT INTO notification_logs
         (id, transaction_txid, transaction_wallet_checksum, notification_method_id, provider_name, status, message_content)
         VALUES ('orphan-log-missing-method-valid-tx', 'valid-tx', ?1, 'missing-method-valid-tx', 'test', 'sent', 'orphan')",
        params![wallet_checksum],
    )
    .unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();
}

/// Must be called after `seed_orphaned_records`.
fn seed_orphaned_method_log_for_valid_transaction(db_path: &str, orphan_method_id: &str) {
    let conn = Connection::open(db_path).unwrap();
    let orphan_method_exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM contact_notification_methods WHERE id = ?1",
            params![orphan_method_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        orphan_method_exists, 1,
        "seed_orphaned_records must run before this helper"
    );
    let wallet_checksum: String = conn
        .query_row(
            "SELECT transaction_wallet_checksum FROM notification_logs WHERE id = 'valid-log'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    conn.execute(
        "INSERT INTO notification_logs
         (id, transaction_txid, transaction_wallet_checksum, notification_method_id, provider_name, status, message_content)
         VALUES ('orphan-method-valid-tx-log', 'valid-tx', ?1, ?2, 'test', 'sent', 'audit after method cleanup')",
        params![wallet_checksum, orphan_method_id],
    )
    .unwrap();
}

fn seed_soft_deleted_wallet_transaction_log(db_path: &str, method_id: &str) {
    let conn = Connection::open(db_path).unwrap();
    let user_id: String = conn
        .query_row("SELECT id FROM users LIMIT 1", [], |row| row.get(0))
        .unwrap();

    conn.execute(
        "INSERT INTO wallets (checksum, name, descriptor, hex_color, status, user_id)
         VALUES ('deleted-wallet', 'Deleted Wallet', 'deleted-wallet-descriptor', '#111111', 'deleted', ?1)",
        params![user_id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO transactions (txid, wallet_checksum, transaction_type, amount_sats, first_seen_at)
         VALUES ('deleted-wallet-tx', 'deleted-wallet', 'receive', 1000, 100)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO notification_logs
         (id, transaction_txid, transaction_wallet_checksum, notification_method_id, provider_name, status, message_content)
         VALUES ('deleted-wallet-log', 'deleted-wallet-tx', 'deleted-wallet', ?1, 'test', 'sent', 'deleted wallet log')",
        params![method_id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO notification_logs
         (id, transaction_txid, transaction_wallet_checksum, notification_method_id, provider_name, status, message_content)
         VALUES ('deleted-wallet-audit-log', 'deleted-wallet-tx', 'deleted-wallet', NULL, 'test', 'sent', 'deleted wallet audit log')",
        [],
    )
    .unwrap();
}

fn count_rows(db_path: &str, table: &str, id_column: &str, id: &str) -> i64 {
    let conn = Connection::open(db_path).unwrap();
    assert!(
        matches!(
            (table, id_column),
            (
                "balance_alert_notification_logs"
                    | "balance_alerts"
                    | "contact_notification_methods"
                    | "contacts"
                    | "notification_logs",
                "id"
            ) | ("transactions", "txid")
                | ("wallets", "checksum")
        ),
        "count_rows only allows known test table/column identifiers, got ({table:?}, {id_column:?})"
    );
    conn.query_row(
        &format!("SELECT COUNT(*) FROM {table} WHERE {id_column} = ?1"),
        params![id],
        |row| row.get(0),
    )
    .unwrap()
}

fn assert_notification_log_method_is_null(db_path: &str, id: &str) {
    let conn = Connection::open(db_path).unwrap();
    let method_id: Option<String> = conn
        .query_row(
            "SELECT notification_method_id FROM notification_logs WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(method_id.is_none());
}

#[tokio::test]
async fn database_health_and_integrity_require_admin_access() {
    let app = create_test_app().await;
    let regular_user = app
        .app_services
        .metadata_db
        .get_user_by_email("regular@example.com")
        .await
        .unwrap()
        .unwrap();
    let non_admin = test_token(
        TEST_SELF_HOSTED_JWT_SECRET,
        &regular_user.id,
        "regular@example.com",
        false,
    );

    let status = request_without_auth(&app.router, "GET", "/api/health/database").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let status = request_without_auth(&app.router, "POST", "/api/admin/database/integrity").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, body) =
        request_json(&app.router, "GET", "/api/health/database", &non_admin, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error_code"], "access_denied");

    let (status, body) = request_json(
        &app.router,
        "POST",
        "/api/admin/database/integrity",
        &non_admin,
        Some(json!({ "auto_fix": true })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error_code"], "access_denied");
}

#[tokio::test]
async fn database_health_reports_no_orphans_for_clean_database() {
    let app = create_test_app().await;
    let admin = self_hosted_admin_token(TEST_SELF_HOSTED_JWT_SECRET);

    let (status, body) =
        request_json(&app.router, "GET", "/api/health/database", &admin, None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "healthy");
    assert_ne!(body["schema_version"], "unknown");
    assert_eq!(body["pool"]["status"], "healthy");
    assert_eq!(body["checks"]["sqlite_integrity"]["status"], "pass");
    assert_eq!(body["checks"]["foreign_keys"]["status"], "pass");
    assert_eq!(body["checks"]["orphaned_records"]["status"], "pass");
    assert_eq!(body["checks"]["orphaned_records"]["notification_logs"], 0);
    assert_eq!(body["checks"]["orphaned_records"]["total"], 0);
    assert_eq!(body["checks"]["duplicates"]["status"], "pass");
    assert_eq!(body["checks"]["duplicates"]["total"], 0);
    assert!(body.get("cleanup").is_none());
}

#[tokio::test]
async fn database_health_reports_degraded_for_orphans_without_fk_violations() {
    let app = create_test_app().await;
    let _valid_records = seed_valid_records(&app).await;
    seed_soft_deleted_wallet_contact(&app.db_path);
    let admin = self_hosted_admin_token(TEST_SELF_HOSTED_JWT_SECRET);

    let (status, body) =
        request_json(&app.router, "GET", "/api/health/database", &admin, None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "degraded");
    assert_eq!(body["checks"]["foreign_keys"]["status"], "pass");
    assert_eq!(body["checks"]["orphaned_records"]["status"], "warn");
    assert_eq!(body["checks"]["orphaned_records"]["contacts"], 1);
    assert_eq!(
        body["checks"]["orphaned_records"]["notification_methods"],
        1
    );
    assert_eq!(body["checks"]["orphaned_records"]["total"], 2);
    assert_eq!(
        count_rows(
            &app.db_path,
            "contacts",
            "id",
            "soft-deleted-wallet-contact",
        ),
        1
    );
    assert_eq!(
        count_rows(
            &app.db_path,
            "wallets",
            "checksum",
            "deleted-contact-wallet",
        ),
        1
    );
    assert_eq!(
        count_rows(
            &app.db_path,
            "contact_notification_methods",
            "id",
            "soft-deleted-wallet-contact-method",
        ),
        1
    );
}

#[tokio::test]
async fn database_health_reports_orphans_without_cleanup() {
    let app = create_test_app().await;
    seed_orphaned_records(&app.db_path);
    let admin = self_hosted_admin_token(TEST_SELF_HOSTED_JWT_SECRET);

    let (status, body) =
        request_json(&app.router, "GET", "/api/health/database", &admin, None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "unhealthy");
    assert_eq!(body["checks"]["foreign_keys"]["status"], "fail");
    assert_eq!(body["checks"]["orphaned_records"]["status"], "warn");
    assert_eq!(body["checks"]["orphaned_records"]["contacts"], 1);
    assert_eq!(
        body["checks"]["orphaned_records"]["notification_methods"],
        1
    );
    assert_eq!(body["checks"]["orphaned_records"]["notification_logs"], 2);
    assert_eq!(body["checks"]["orphaned_records"]["transactions"], 1);
    assert_eq!(body["checks"]["orphaned_records"]["balance_alerts"], 1);
    assert_eq!(
        body["checks"]["orphaned_records"]["balance_alert_notification_logs"],
        1
    );
    assert_eq!(body["checks"]["orphaned_records"]["total"], 7);
    assert!(body.get("cleanup").is_none());
    assert_eq!(
        count_rows(&app.db_path, "contacts", "id", "orphan-contact"),
        1
    );
}

#[tokio::test]
async fn integrity_check_without_body_defaults_to_no_cleanup() {
    let app = create_test_app().await;
    seed_orphaned_records(&app.db_path);
    let admin = self_hosted_admin_token(TEST_SELF_HOSTED_JWT_SECRET);

    let (status, body) = request_json(
        &app.router,
        "POST",
        "/api/admin/database/integrity",
        &admin,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.get("cleanup").is_none());
    assert_eq!(body["checks"]["foreign_keys"]["status"], "fail");
    assert_eq!(body["checks"]["orphaned_records"]["total"], 7);
    assert_eq!(
        count_rows(&app.db_path, "contacts", "id", "orphan-contact"),
        1
    );
}

#[tokio::test]
async fn integrity_check_rejects_malformed_payload() {
    let app = create_test_app().await;
    seed_orphaned_records(&app.db_path);
    let admin = self_hosted_admin_token(TEST_SELF_HOSTED_JWT_SECRET);

    let (status, _body) = request_raw(
        &app.router,
        "POST",
        "/api/admin/database/integrity",
        &admin,
        Body::from("{"),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        count_rows(&app.db_path, "contacts", "id", "orphan-contact"),
        1
    );
}

#[tokio::test]
async fn integrity_check_with_auto_fix_on_clean_database_reports_zero_deletions() {
    let app = create_test_app().await;
    let admin = self_hosted_admin_token(TEST_SELF_HOSTED_JWT_SECRET);

    let (status, body) = request_json(
        &app.router,
        "POST",
        "/api/admin/database/integrity",
        &admin,
        Some(json!({ "auto_fix": true })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["cleanup"]["orphaned_contacts_deleted"], 0);
    assert_eq!(body["cleanup"]["orphaned_methods_deleted"], 0);
    assert_eq!(body["cleanup"]["orphaned_logs_deleted"], 0);
    assert_eq!(
        body["cleanup"]["orphaned_balance_alert_notification_logs_deleted"],
        0
    );
    assert_eq!(body["cleanup"]["orphaned_alerts_deleted"], 0);
    assert_eq!(body["cleanup"]["orphaned_transactions_deleted"], 0);
    assert_eq!(body["cleanup"]["total_deleted"], 0);
    assert_eq!(body["checks"]["orphaned_records"]["total"], 0);
}

#[tokio::test]
async fn integrity_check_without_auto_fix_reports_orphans_and_leaves_rows() {
    let app = create_test_app().await;
    let valid_records = seed_valid_records(&app).await;
    seed_orphaned_records(&app.db_path);
    seed_orphaned_transaction_log_with_valid_method(&app.db_path, &valid_records.method_id);
    seed_missing_method_log_for_valid_transaction(&app.db_path);
    seed_orphaned_method_log_for_valid_transaction(&app.db_path, "orphan-method");
    seed_soft_deleted_wallet_contact(&app.db_path);
    let admin = self_hosted_admin_token(TEST_SELF_HOSTED_JWT_SECRET);

    let (status, body) = request_json(
        &app.router,
        "POST",
        "/api/admin/database/integrity",
        &admin,
        Some(json!({ "auto_fix": false })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.get("cleanup").is_none());
    assert_eq!(body["checks"]["orphaned_records"]["status"], "warn");
    assert_eq!(body["checks"]["foreign_keys"]["status"], "fail");
    assert_eq!(body["checks"]["orphaned_records"]["contacts"], 2);
    assert_eq!(
        body["checks"]["orphaned_records"]["notification_methods"],
        2
    );
    // These five log orphans come from the base orphan fixture plus the
    // valid-method missing-transaction and missing-method valid-transaction fixtures.
    assert_eq!(body["checks"]["orphaned_records"]["notification_logs"], 5);
    assert_eq!(body["checks"]["orphaned_records"]["transactions"], 1);
    assert_eq!(body["checks"]["orphaned_records"]["balance_alerts"], 1);
    assert_eq!(
        body["checks"]["orphaned_records"]["balance_alert_notification_logs"],
        1
    );
    assert_eq!(body["checks"]["orphaned_records"]["total"], 12);

    let orphaned_logs = app
        .app_services
        .metadata_db
        .find_orphaned_notification_logs()
        .await
        .unwrap();
    let missing_tx_log = orphaned_logs
        .iter()
        .find(|record| record.id == "orphan-log-valid-method-missing-tx")
        .unwrap();
    assert_eq!(
        missing_tx_log.parent_ref,
        "missing-tx-with-valid-method:missing-wallet"
    );
    let double_orphan_log = orphaned_logs
        .iter()
        .find(|record| record.id == "orphan-log")
        .unwrap();
    assert_eq!(double_orphan_log.parent_ref, "missing-method");
    let null_method_missing_tx_log = orphaned_logs
        .iter()
        .find(|record| record.id == "orphan-log-missing-tx")
        .unwrap();
    assert_eq!(
        null_method_missing_tx_log.parent_ref,
        "missing-tx-no-method:missing-wallet"
    );
    let missing_method_valid_tx_log = orphaned_logs
        .iter()
        .find(|record| record.id == "orphan-log-missing-method-valid-tx")
        .unwrap();
    assert_eq!(
        missing_method_valid_tx_log.parent_ref,
        "missing-method-valid-tx"
    );
    let orphaned_tx_log = orphaned_logs
        .iter()
        .find(|record| record.id == "orphan-log-valid-method-orphan-tx")
        .unwrap();
    assert_eq!(orphaned_tx_log.parent_ref, "orphan-tx:missing-wallet");

    assert_eq!(
        count_rows(&app.db_path, "contacts", "id", "orphan-contact"),
        1
    );
    assert_eq!(
        count_rows(
            &app.db_path,
            "contact_notification_methods",
            "id",
            "orphan-method",
        ),
        1
    );
    assert_eq!(
        count_rows(&app.db_path, "notification_logs", "id", "orphan-log"),
        1
    );
    assert_eq!(
        count_rows(
            &app.db_path,
            "notification_logs",
            "id",
            "orphan-log-missing-tx",
        ),
        1
    );
    assert_eq!(
        count_rows(
            &app.db_path,
            "notification_logs",
            "id",
            "orphan-log-valid-method-missing-tx",
        ),
        1
    );
    assert_eq!(
        count_rows(
            &app.db_path,
            "notification_logs",
            "id",
            "orphan-log-valid-method-orphan-tx",
        ),
        1
    );
    assert_eq!(
        count_rows(
            &app.db_path,
            "notification_logs",
            "id",
            "orphan-log-missing-method-valid-tx",
        ),
        1
    );
    assert_eq!(
        count_rows(
            &app.db_path,
            "notification_logs",
            "id",
            "orphan-method-valid-tx-log",
        ),
        1
    );
    assert_eq!(
        count_rows(&app.db_path, "transactions", "txid", "orphan-tx"),
        1
    );
    assert_eq!(
        count_rows(&app.db_path, "balance_alerts", "id", "orphan-alert"),
        1
    );
    assert_eq!(
        count_rows(
            &app.db_path,
            "balance_alert_notification_logs",
            "id",
            "orphan-alert-log",
        ),
        1
    );
    assert_eq!(
        count_rows(&app.db_path, "contacts", "id", &valid_records.contact_id),
        1
    );
    assert_eq!(
        count_rows(
            &app.db_path,
            "contact_notification_methods",
            "id",
            &valid_records.method_id
        ),
        1
    );
}

#[tokio::test]
async fn integrity_check_handles_logs_for_soft_deleted_wallet_transactions() {
    let app = create_test_app().await;
    let valid_records = seed_valid_records(&app).await;
    seed_soft_deleted_wallet_transaction_log(&app.db_path, &valid_records.method_id);
    let admin = self_hosted_admin_token(TEST_SELF_HOSTED_JWT_SECRET);

    let orphaned_logs = app
        .app_services
        .metadata_db
        .find_orphaned_notification_logs()
        .await
        .unwrap();
    let deleted_wallet_log = orphaned_logs
        .iter()
        .find(|record| record.id == "deleted-wallet-log")
        .unwrap();
    assert_eq!(
        deleted_wallet_log.parent_ref,
        "deleted-wallet-tx:deleted-wallet"
    );
    let deleted_wallet_audit_log = orphaned_logs
        .iter()
        .find(|record| record.id == "deleted-wallet-audit-log")
        .unwrap();
    assert_eq!(
        deleted_wallet_audit_log.parent_ref,
        "deleted-wallet-tx:deleted-wallet"
    );

    let (status, body) = request_json(
        &app.router,
        "POST",
        "/api/admin/database/integrity",
        &admin,
        Some(json!({ "auto_fix": true })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["cleanup"]["orphaned_logs_deleted"], 2);
    assert_eq!(body["cleanup"]["orphaned_transactions_deleted"], 1);
    assert_eq!(body["cleanup"]["total_deleted"], 3);
    assert_eq!(body["checks"]["foreign_keys"]["status"], "pass");
    assert_eq!(body["checks"]["orphaned_records"]["status"], "pass");
    assert_eq!(body["checks"]["orphaned_records"]["total"], 0);
    assert_eq!(
        count_rows(
            &app.db_path,
            "notification_logs",
            "id",
            "deleted-wallet-log"
        ),
        0
    );
    assert_eq!(
        count_rows(
            &app.db_path,
            "notification_logs",
            "id",
            "deleted-wallet-audit-log"
        ),
        0
    );
    assert_eq!(
        count_rows(&app.db_path, "transactions", "txid", "deleted-wallet-tx"),
        0
    );
    assert_eq!(
        count_rows(&app.db_path, "wallets", "checksum", "deleted-wallet"),
        1
    );
    assert_eq!(
        count_rows(&app.db_path, "notification_logs", "id", "valid-log"),
        1
    );

    let (status, body) = request_json(
        &app.router,
        "POST",
        "/api/admin/database/integrity",
        &admin,
        Some(json!({ "auto_fix": true })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["cleanup"]["total_deleted"], 0);
    assert_eq!(body["checks"]["orphaned_records"]["notification_logs"], 0);
    assert_eq!(body["checks"]["orphaned_records"]["total"], 0);
}

#[tokio::test]
async fn integrity_check_with_auto_fix_deletes_orphans_and_preserves_valid_rows() {
    let app = create_test_app().await;
    let valid_records = seed_valid_records(&app).await;
    seed_orphaned_records(&app.db_path);
    seed_orphaned_transaction_log_with_valid_method(&app.db_path, &valid_records.method_id);
    seed_missing_method_log_for_valid_transaction(&app.db_path);
    seed_orphaned_method_log_for_valid_transaction(&app.db_path, "orphan-method");
    seed_soft_deleted_wallet_contact(&app.db_path);
    let admin = self_hosted_admin_token(TEST_SELF_HOSTED_JWT_SECRET);

    let (status, body) = request_json(
        &app.router,
        "POST",
        "/api/admin/database/integrity",
        &admin,
        Some(json!({ "auto_fix": true })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["cleanup"]["orphaned_contacts_deleted"], 2);
    assert_eq!(body["cleanup"]["orphaned_methods_deleted"], 2);
    assert_eq!(body["cleanup"]["orphaned_logs_deleted"], 5);
    assert_eq!(body["cleanup"]["orphaned_alerts_deleted"], 1);
    assert_eq!(
        body["cleanup"]["orphaned_balance_alert_notification_logs_deleted"],
        1
    );
    assert_eq!(body["cleanup"]["orphaned_transactions_deleted"], 1);
    assert_eq!(body["cleanup"]["total_deleted"], 12);
    assert_eq!(body["checks"]["foreign_keys"]["status"], "pass");
    assert_eq!(body["checks"]["orphaned_records"]["status"], "pass");
    assert_eq!(body["checks"]["orphaned_records"]["notification_logs"], 0);
    assert_eq!(body["checks"]["orphaned_records"]["total"], 0);

    let (status, body) = request_json(
        &app.router,
        "POST",
        "/api/admin/database/integrity",
        &admin,
        Some(json!({ "auto_fix": true })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["cleanup"]["total_deleted"], 0);
    assert_eq!(body["checks"]["orphaned_records"]["notification_logs"], 0);
    assert_eq!(body["checks"]["orphaned_records"]["total"], 0);

    let (status, health_body) =
        request_json(&app.router, "GET", "/api/health/database", &admin, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(health_body["status"], "healthy");
    assert_eq!(
        health_body["checks"]["orphaned_records"]["notification_logs"],
        0
    );
    assert_eq!(health_body["checks"]["orphaned_records"]["total"], 0);

    assert_eq!(
        count_rows(&app.db_path, "contacts", "id", "orphan-contact"),
        0
    );
    assert_eq!(
        count_rows(
            &app.db_path,
            "contact_notification_methods",
            "id",
            "orphan-method",
        ),
        0
    );
    assert_eq!(
        count_rows(
            &app.db_path,
            "contacts",
            "id",
            "soft-deleted-wallet-contact",
        ),
        0
    );
    assert_eq!(
        count_rows(
            &app.db_path,
            "contact_notification_methods",
            "id",
            "soft-deleted-wallet-contact-method",
        ),
        0
    );
    assert_eq!(
        count_rows(&app.db_path, "notification_logs", "id", "orphan-log"),
        0
    );
    assert_eq!(
        count_rows(
            &app.db_path,
            "notification_logs",
            "id",
            "orphan-log-missing-tx",
        ),
        0
    );
    assert_eq!(
        count_rows(
            &app.db_path,
            "notification_logs",
            "id",
            "orphan-log-valid-method-missing-tx",
        ),
        0
    );
    assert_eq!(
        count_rows(
            &app.db_path,
            "notification_logs",
            "id",
            "orphan-log-valid-method-orphan-tx",
        ),
        0
    );
    assert_eq!(
        count_rows(
            &app.db_path,
            "notification_logs",
            "id",
            "orphan-log-missing-method-valid-tx",
        ),
        0
    );
    assert_eq!(
        count_rows(
            &app.db_path,
            "notification_logs",
            "id",
            "orphan-method-valid-tx-log",
        ),
        1
    );
    assert_notification_log_method_is_null(&app.db_path, "orphan-method-valid-tx-log");
    assert_eq!(
        count_rows(&app.db_path, "transactions", "txid", "orphan-tx"),
        0
    );
    assert_eq!(
        count_rows(&app.db_path, "balance_alerts", "id", "orphan-alert"),
        0
    );
    assert_eq!(
        count_rows(
            &app.db_path,
            "balance_alert_notification_logs",
            "id",
            "orphan-alert-log",
        ),
        0
    );

    assert_eq!(
        count_rows(&app.db_path, "contacts", "id", &valid_records.contact_id),
        1
    );
    assert_eq!(
        count_rows(
            &app.db_path,
            "contact_notification_methods",
            "id",
            &valid_records.method_id
        ),
        1
    );
    assert_eq!(
        count_rows(&app.db_path, "notification_logs", "id", "valid-log"),
        1
    );
    assert_eq!(
        count_rows(&app.db_path, "notification_logs", "id", "audit-record-log",),
        1
    );
    assert_eq!(
        count_rows(&app.db_path, "transactions", "txid", "valid-tx"),
        1
    );
    assert_eq!(
        count_rows(&app.db_path, "balance_alerts", "id", "valid-alert"),
        1
    );
    assert_eq!(
        count_rows(
            &app.db_path,
            "balance_alert_notification_logs",
            "id",
            "valid-alert-log",
        ),
        1
    );
}

#[tokio::test]
async fn administrator_cannot_read_or_mutate_another_customers_wallet() {
    let app = create_test_app().await;
    seed_valid_records(&app).await;
    let admin = self_hosted_admin_token(TEST_SELF_HOSTED_JWT_SECRET);
    let (status, body) = request_json(&app.router, "GET", "/api/wallets", &admin, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["wallets"].as_array().unwrap().len(), 0);
    for (method, uri) in [
        ("GET", "/api/wallets/valid-wallet"),
        ("GET", "/api/wallets/valid-wallet/detail"),
        ("GET", "/api/wallets/valid-wallet/contacts"),
        ("GET", "/api/wallets/valid-wallet/balance-alerts"),
        ("DELETE", "/api/balance-alerts/valid-alert"),
        ("DELETE", "/api/wallets/valid-wallet"),
    ] {
        let (status, body) = request_json(&app.router, method, uri, &admin, None).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}");
        assert_eq!(body["error_code"], "access_denied");
        assert!(!body.to_string().contains("valid-descriptor"));
    }
    assert_eq!(
        count_rows(&app.db_path, "balance_alerts", "id", "valid-alert"),
        1
    );
}

#[tokio::test]
async fn role_changes_permanently_revoke_existing_sessions() {
    let app = create_test_app().await;
    let token = self_hosted_admin_token(TEST_SELF_HOSTED_JWT_SECRET);
    let (status, _) = request_json(&app.router, "GET", "/api/health/database", &token, None).await;
    assert_eq!(status, StatusCode::OK);
    let connection = Connection::open(&app.db_path).unwrap();
    connection
        .execute("UPDATE users SET is_admin = 0 WHERE id = 'foss-user'", [])
        .unwrap();
    let (status, _) = request_json(&app.router, "GET", "/api/health/database", &token, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    connection
        .execute("UPDATE users SET is_admin = 1 WHERE id = 'foss-user'", [])
        .unwrap();
    let (status, _) = request_json(&app.router, "GET", "/api/health/database", &token, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // Even a valid signed token cannot be attached to a different session subject.
    let regular = app
        .app_services
        .metadata_db
        .get_user_by_email("regular@example.com")
        .await
        .unwrap()
        .unwrap();
    app.app_services
        .metadata_db
        .create_session(
            &regular.id,
            &AuthService::hash_token(&token),
            chrono::Utc::now() + chrono::Duration::days(1),
        )
        .await
        .unwrap();
    let (status, _) = request_json(&app.router, "GET", "/api/wallets", &token, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
