//! Pending contact verifications: expiry, single use and attempt limiting.

mod common;

use bdk_wallet::rusqlite::{Connection, params};
use canary::metadata::MetadataDb;
use common::{db_path, self_hosted_metadata_db};
use std::sync::Arc;
use tempfile::TempDir;

const TARGET: &str = "alice@example.com";

async fn wallet_with_pending_code(temp_dir: &TempDir) -> (MetadataDb, String, i64) {
    let metadata_db = self_hosted_metadata_db(temp_dir).await;
    let user_id = metadata_db
        .create_user("owner@example.com", "hash", None, false, None, None)
        .await
        .unwrap();
    let wallet_checksum = metadata_db
        .insert_wallet("Wallet", "descriptor", &user_id)
        .await
        .unwrap();
    let verification_id = metadata_db
        .create_pending_contact_verification(
            &wallet_checksum,
            "email",
            TARGET,
            "Alice",
            Some("code"),
        )
        .await
        .unwrap();
    (metadata_db, wallet_checksum, verification_id)
}

fn open_db(temp_dir: &TempDir) -> Connection {
    Connection::open(db_path(temp_dir.path())).unwrap()
}

#[tokio::test]
async fn pending_verification_expires_after_ttl() {
    let temp_dir = TempDir::new().unwrap();
    let (metadata_db, wallet_checksum, verification_id) = wallet_with_pending_code(&temp_dir).await;

    let minutes_left: f64 = open_db(&temp_dir)
        .query_row(
            "SELECT (julianday(expires_at) - julianday('now')) * 24 * 60,
                    expires_at = datetime(expires_at)
             FROM pending_contact_verifications WHERE id = ?1",
            params![verification_id],
            |row| {
                assert!(
                    row.get::<_, bool>(1)?,
                    "expires_at uses SQLite datetime format"
                );
                row.get(0)
            },
        )
        .unwrap();
    assert!(
        (9.0..=10.0).contains(&minutes_left),
        "expires in ~10 minutes, got {minutes_left}"
    );
    assert!(
        metadata_db
            .get_pending_verification(&wallet_checksum, TARGET)
            .await
            .unwrap()
            .is_some()
    );

    open_db(&temp_dir)
        .execute(
            "UPDATE pending_contact_verifications SET expires_at = datetime('now', '-1 minute')
             WHERE id = ?1",
            params![verification_id],
        )
        .unwrap();

    assert!(
        metadata_db
            .get_pending_verification(&wallet_checksum, TARGET)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        !metadata_db
            .mark_verification_completed(verification_id)
            .await
            .unwrap()
    );
    assert_eq!(
        metadata_db.cleanup_expired_verifications().await.unwrap(),
        1
    );
}

#[tokio::test]
async fn verification_code_can_only_be_redeemed_once() {
    let temp_dir = TempDir::new().unwrap();
    let (metadata_db, wallet_checksum, verification_id) = wallet_with_pending_code(&temp_dir).await;

    assert!(
        metadata_db
            .mark_verification_completed(verification_id)
            .await
            .unwrap()
    );
    assert!(
        !metadata_db
            .mark_verification_completed(verification_id)
            .await
            .unwrap()
    );

    assert!(
        metadata_db
            .get_pending_verification(&wallet_checksum, TARGET)
            .await
            .unwrap()
            .is_none(),
        "a redeemed code is no longer pending"
    );
    assert!(
        metadata_db
            .was_recently_verified(&wallet_checksum, TARGET)
            .await
            .unwrap(),
        "contact creation still sees the completed verification"
    );
}

#[tokio::test]
async fn concurrent_verification_attempts_are_all_counted() {
    let temp_dir = TempDir::new().unwrap();
    let metadata_db = Arc::new(self_hosted_metadata_db(&temp_dir).await);

    let attempts = (0..20).map(|_| {
        let metadata_db = Arc::clone(&metadata_db);
        tokio::spawn(async move { metadata_db.reserve_verification_attempt(TARGET).await })
    });
    let mut allowed = 0;
    for attempt in attempts {
        if attempt.await.unwrap().unwrap() {
            allowed += 1;
        }
    }
    assert_eq!(allowed, 5);
    assert!(
        !metadata_db
            .reserve_verification_attempt(TARGET)
            .await
            .unwrap()
    );
    assert!(
        metadata_db
            .reserve_verification_attempt("other@example.com")
            .await
            .unwrap(),
        "limits are per target"
    );

    metadata_db
        .clear_verification_rate_limit(TARGET)
        .await
        .unwrap();
    assert!(
        metadata_db
            .reserve_verification_attempt(TARGET)
            .await
            .unwrap()
    );
}
