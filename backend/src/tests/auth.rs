use crate::auth::{authenticate_user, AuthService};
use crate::config::{AppConfig, NetworkConfig, OperatingMode};
use crate::handlers::auth::update_password_and_revoke_sessions;
use crate::metadata::MetadataDb;
use tempfile::tempdir;

async fn create_test_db(mode: OperatingMode) -> (MetadataDb, tempfile::TempDir) {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let frontend_url =
        matches!(mode, OperatingMode::Cloud).then_some("http://localhost:3001".to_string());

    let test_config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        temp_dir.path().to_string_lossy().to_string(),
        mode,
        frontend_url,
        Some("test-jwt-secret".to_string()),
    );

    let db = MetadataDb::new(db_path.to_str().unwrap(), &test_config)
        .await
        .unwrap();
    (db, temp_dir)
}

async fn create_cloud_test_db() -> (MetadataDb, tempfile::TempDir) {
    create_test_db(OperatingMode::Cloud).await
}

async fn create_self_hosted_test_db() -> (MetadataDb, tempfile::TempDir) {
    create_test_db(OperatingMode::SelfHosted).await
}

#[tokio::test]
async fn authenticate_user_rejects_token_without_active_session() {
    let (db, _temp_dir) = create_cloud_test_db().await;
    let auth_service = AuthService::new("test-jwt-secret".to_string(), None);
    let user_id = db
        .create_user(
            "test@example.com",
            "hashedpassword",
            Some("Test User"),
            true,
            None,
            None,
        )
        .await
        .unwrap();

    let token = auth_service
        .generate_token(&user_id, "test@example.com", false, false)
        .unwrap();
    let auth_header = format!("Bearer {token}");

    let err = authenticate_user(&db, Some(&auth_header), None, "test-jwt-secret")
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("Authentication required"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn authenticate_user_rejects_token_after_logout() {
    let (db, _temp_dir) = create_cloud_test_db().await;
    let auth_service = AuthService::new("test-jwt-secret".to_string(), None);
    let user_id = db
        .create_user(
            "test@example.com",
            "hashedpassword",
            Some("Test User"),
            true,
            None,
            None,
        )
        .await
        .unwrap();

    let token = auth_service
        .generate_token(&user_id, "test@example.com", false, false)
        .unwrap();
    let token_hash = AuthService::hash_token(&token);
    let expires_at = chrono::Utc::now() + chrono::Duration::days(7);
    let auth_header = format!("Bearer {token}");

    db.create_session(&user_id, &token_hash, expires_at)
        .await
        .unwrap();

    let user = authenticate_user(&db, Some(&auth_header), None, "test-jwt-secret")
        .await
        .unwrap();
    assert_eq!(user.user_id, user_id);

    db.delete_session(&token_hash).await.unwrap();

    let err = authenticate_user(&db, Some(&auth_header), None, "test-jwt-secret")
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("Authentication required"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn password_reset_helper_revokes_all_existing_tokens() {
    let (db, _temp_dir) = create_cloud_test_db().await;
    let auth_service = AuthService::new("test-jwt-secret".to_string(), None);
    let user_id = db
        .create_user(
            "test@example.com",
            "old-hash",
            Some("Test User"),
            true,
            None,
            None,
        )
        .await
        .unwrap();

    let token_a = auth_service
        .generate_token(&user_id, "test@example.com", false, false)
        .unwrap();
    let token_b = auth_service
        .generate_token(&user_id, "test@example.com", false, false)
        .unwrap();
    let auth_header_a = format!("Bearer {token_a}");
    let auth_header_b = format!("Bearer {token_b}");

    db.create_session(
        &user_id,
        &AuthService::hash_token(&token_a),
        chrono::Utc::now() + chrono::Duration::days(7),
    )
    .await
    .unwrap();
    db.create_session(
        &user_id,
        &AuthService::hash_token(&token_b),
        chrono::Utc::now() + chrono::Duration::days(7),
    )
    .await
    .unwrap();

    update_password_and_revoke_sessions(&db, &user_id, "new-hash")
        .await
        .unwrap();

    let err_a = authenticate_user(&db, Some(&auth_header_a), None, "test-jwt-secret")
        .await
        .unwrap_err();
    let err_b = authenticate_user(&db, Some(&auth_header_b), None, "test-jwt-secret")
        .await
        .unwrap_err();

    assert!(err_a.to_string().contains("Authentication required"));
    assert!(err_b.to_string().contains("Authentication required"));

    let user = db.get_user_by_id(&user_id).await.unwrap().unwrap();
    assert_eq!(user.password_hash, "new-hash");
}

#[tokio::test]
async fn authenticate_user_rejects_self_hosted_token_without_active_session() {
    let (db, _temp_dir) = create_self_hosted_test_db().await;
    let auth_service = AuthService::new("test-jwt-secret".to_string(), None);

    let token = auth_service
        .generate_token("foss-user", "admin@local", true, false)
        .unwrap();
    let auth_header = format!("Bearer {token}");

    let err = authenticate_user(&db, Some(&auth_header), None, "test-jwt-secret")
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("Authentication required"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn authenticate_user_accepts_self_hosted_token_with_active_session() {
    let (db, _temp_dir) = create_self_hosted_test_db().await;
    let auth_service = AuthService::new("test-jwt-secret".to_string(), None);

    let token = auth_service
        .generate_token("foss-user", "admin@local", true, false)
        .unwrap();
    db.create_session(
        "foss-user",
        &AuthService::hash_token(&token),
        chrono::Utc::now() + chrono::Duration::days(7),
    )
    .await
    .unwrap();

    let auth_header = format!("Bearer {token}");
    let user = authenticate_user(&db, Some(&auth_header), None, "test-jwt-secret")
        .await
        .unwrap();

    assert_eq!(user.user_id, "foss-user");
    assert!(user.is_admin);
}

#[tokio::test]
async fn authenticate_user_rejects_token_with_expired_session() {
    let (db, _temp_dir) = create_self_hosted_test_db().await;
    let auth_service = AuthService::new("test-jwt-secret".to_string(), None);

    let token = auth_service
        .generate_token("foss-user", "admin@local", true, false)
        .unwrap();
    db.create_session(
        "foss-user",
        &AuthService::hash_token(&token),
        chrono::Utc::now() - chrono::Duration::minutes(1),
    )
    .await
    .unwrap();

    let auth_header = format!("Bearer {token}");
    let err = authenticate_user(&db, Some(&auth_header), None, "test-jwt-secret")
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("Authentication required"),
        "unexpected error: {err}"
    );
}
