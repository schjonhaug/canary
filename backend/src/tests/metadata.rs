use crate::metadata::{Language, MetadataDb};
use tempfile::tempdir;

async fn create_test_db() -> (MetadataDb, tempfile::TempDir) {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = MetadataDb::new(db_path.to_str().unwrap()).await.unwrap();
    (db, temp_dir) // Return both to keep temp_dir alive
}

#[tokio::test]
async fn test_delete_wallet_contact_authorization() {
    let (db, _temp_dir) = create_test_db().await;

    // Create test user
    let user_id = db
        .create_user(
            "test@example.com",
            "hashedpassword",
            Some("Test User"),
            true,
        )
        .await
        .unwrap();
    let other_user_id = db
        .create_user(
            "other@example.com",
            "hashedpassword",
            Some("Other User"),
            true,
        )
        .await
        .unwrap();

    // Create two wallets for different users
    let wallet1_checksum = db
        .insert_wallet("Wallet 1", "descriptor1", user_id)
        .await
        .unwrap();
    let wallet2_checksum = db
        .insert_wallet("Wallet 2", "descriptor2", other_user_id)
        .await
        .unwrap();

    // Add contacts to each wallet using the notification methods API
    let contact1_id = db
        .insert_contact_with_notification_methods(
            &wallet1_checksum,
            "Contact 1",
            &Language::English,
            vec![],
        )
        .await
        .unwrap();
    let contact2_id = db
        .insert_contact_with_notification_methods(
            &wallet2_checksum,
            "Contact 2",
            &Language::English,
            vec![],
        )
        .await
        .unwrap();

    // Test 1: User can delete their own contact
    let result = db
        .delete_wallet_contact(&wallet1_checksum, contact1_id)
        .await
        .unwrap();
    assert!(result, "Should be able to delete own contact");

    // Test 2: User cannot delete contact from another user's wallet (IDOR protection)
    let result = db
        .delete_wallet_contact(&wallet1_checksum, contact2_id)
        .await
        .unwrap();
    assert!(
        !result,
        "Should NOT be able to delete contact from different wallet"
    );

    // Test 3: Contact should still exist in its original wallet
    let contacts = db
        .get_contacts_with_notification_methods(&wallet2_checksum)
        .await
        .unwrap();
    assert_eq!(
        contacts.len(),
        1,
        "Contact should still exist in original wallet"
    );
    assert_eq!(contacts[0].id.unwrap(), contact2_id);
}

#[tokio::test]
async fn test_delete_wallet_contact_nonexistent() {
    let (db, _temp_dir) = create_test_db().await;

    // Create test user and wallet
    let user_id = db
        .create_user(
            "test@example.com",
            "hashedpassword",
            Some("Test User"),
            true,
        )
        .await
        .unwrap();
    let wallet_checksum = db
        .insert_wallet("Test Wallet", "descriptor", user_id)
        .await
        .unwrap();

    // Try to delete nonexistent contact
    let result = db
        .delete_wallet_contact(&wallet_checksum, 999)
        .await
        .unwrap();
    assert!(!result, "Should return false for nonexistent contact");
}

#[tokio::test]
async fn test_delete_wallet_contact_wrong_wallet() {
    let (db, _temp_dir) = create_test_db().await;

    // Create test users and wallets
    let user1_id = db
        .create_user("user1@example.com", "hashedpassword", Some("User 1"), true)
        .await
        .unwrap();
    let user2_id = db
        .create_user("user2@example.com", "hashedpassword", Some("User 2"), true)
        .await
        .unwrap();

    let wallet1_checksum = db
        .insert_wallet("Wallet 1", "descriptor1", user1_id)
        .await
        .unwrap();
    let wallet2_checksum = db
        .insert_wallet("Wallet 2", "descriptor2", user2_id)
        .await
        .unwrap();

    // Add contact to wallet1
    let contact_id = db
        .insert_contact_with_notification_methods(
            &wallet1_checksum,
            "Test Contact",
            &Language::English,
            vec![],
        )
        .await
        .unwrap();

    // Try to delete contact using wrong wallet checksum
    let result = db
        .delete_wallet_contact(&wallet2_checksum, contact_id)
        .await
        .unwrap();
    assert!(
        !result,
        "Should NOT be able to delete contact using wrong wallet checksum"
    );

    // Verify contact still exists in correct wallet
    let contacts = db
        .get_contacts_with_notification_methods(&wallet1_checksum)
        .await
        .unwrap();
    assert_eq!(
        contacts.len(),
        1,
        "Contact should still exist in original wallet"
    );
}
