use crate::config::{AppConfig, NetworkConfig, OperatingMode};
use crate::metadata::{Language, MetadataDb};
use tempfile::tempdir;

async fn create_test_db() -> (MetadataDb, tempfile::TempDir) {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create test config
    let test_config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        temp_dir.path().to_string_lossy().to_string(),
        OperatingMode::SelfHosted,
        None,
        None, // No JWT secret needed for self-hosted mode
    );

    let db = MetadataDb::new(db_path.to_str().unwrap(), &test_config)
        .await
        .unwrap();
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
            None, // preferred_currency
            None, // preferred_language
        )
        .await
        .unwrap();
    let other_user_id = db
        .create_user(
            "other@example.com",
            "hashedpassword",
            Some("Other User"),
            true,
            None, // preferred_currency
            None, // preferred_language
        )
        .await
        .unwrap();

    // Create two wallets for different users
    let wallet1_checksum = db
        .insert_wallet("Wallet 1", "descriptor1", &user_id)
        .await
        .unwrap();
    let wallet2_checksum = db
        .insert_wallet("Wallet 2", "descriptor2", &other_user_id)
        .await
        .unwrap();

    // Add contacts to each wallet using the notification methods API
    let contact1_id = db
        .insert_contact_with_notification_methods(&wallet1_checksum, "Contact 1", vec![])
        .await
        .unwrap();
    let contact2_id = db
        .insert_contact_with_notification_methods(&wallet2_checksum, "Contact 2", vec![])
        .await
        .unwrap();

    // Test 1: User can delete their own contact
    let result = db
        .delete_wallet_contact(&wallet1_checksum, &contact1_id)
        .await
        .unwrap();
    assert!(result, "Should be able to delete own contact");

    // Test 2: User cannot delete contact from another user's wallet (IDOR protection)
    let result = db
        .delete_wallet_contact(&wallet1_checksum, &contact2_id)
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
    assert_eq!(contacts[0].id.clone().unwrap(), contact2_id);
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
            None, // preferred_currency
            None, // preferred_language
        )
        .await
        .unwrap();
    let wallet_checksum = db
        .insert_wallet("Test Wallet", "descriptor", &user_id)
        .await
        .unwrap();

    // Try to delete nonexistent contact
    let result = db
        .delete_wallet_contact(&wallet_checksum, "550e8400-e29b-41d4-a716-446655440999")
        .await
        .unwrap();
    assert!(!result, "Should return false for nonexistent contact");
}

#[tokio::test]
async fn test_delete_wallet_contact_wrong_wallet() {
    let (db, _temp_dir) = create_test_db().await;

    // Create test users and wallets
    let user1_id = db
        .create_user(
            "user1@example.com",
            "hashedpassword",
            Some("User 1"),
            true,
            None, // preferred_currency
            None, // preferred_language
        )
        .await
        .unwrap();
    let user2_id = db
        .create_user(
            "user2@example.com",
            "hashedpassword",
            Some("User 2"),
            true,
            None, // preferred_currency
            None, // preferred_language
        )
        .await
        .unwrap();

    let wallet1_checksum = db
        .insert_wallet("Wallet 1", "descriptor1", &user1_id)
        .await
        .unwrap();
    let wallet2_checksum = db
        .insert_wallet("Wallet 2", "descriptor2", &user2_id)
        .await
        .unwrap();

    // Add contact to wallet1
    let contact_id = db
        .insert_contact_with_notification_methods(&wallet1_checksum, "Test Contact", vec![])
        .await
        .unwrap();

    // Try to delete contact using wrong wallet checksum
    let result = db
        .delete_wallet_contact(&wallet2_checksum, &contact_id)
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

#[tokio::test]
async fn test_get_user_preferred_language_default() {
    let (db, _temp_dir) = create_test_db().await;

    // Create user without preferred language
    let user_id = db
        .create_user(
            "test@example.com",
            "hashedpassword",
            Some("Test User"),
            false,
            None, // preferred_currency
            None, // preferred_language - not set
        )
        .await
        .unwrap();

    // Should return English as default
    let language = db.get_user_preferred_language(&user_id).await.unwrap();
    assert_eq!(
        language,
        Language::English,
        "Should default to English when no preference set"
    );
}

#[tokio::test]
async fn test_get_user_preferred_language_norwegian() {
    let (db, _temp_dir) = create_test_db().await;

    // Create user with Norwegian preference
    let user_id = db
        .create_user(
            "nordic@example.com",
            "hashedpassword",
            Some("Nordic User"),
            false,
            None,       // preferred_currency
            Some("nb"), // preferred_language - Norwegian (Bokmål)
        )
        .await
        .unwrap();

    // Should return Norwegian
    let language = db.get_user_preferred_language(&user_id).await.unwrap();
    assert_eq!(
        language,
        Language::Norwegian,
        "Should return Norwegian when set"
    );
}

#[tokio::test]
async fn test_get_user_preferred_language_japanese() {
    let (db, _temp_dir) = create_test_db().await;

    // Create user with Japanese preference
    let user_id = db
        .create_user(
            "japan@example.com",
            "hashedpassword",
            Some("Japanese User"),
            false,
            None,       // preferred_currency
            Some("ja"), // preferred_language - Japanese
        )
        .await
        .unwrap();

    // Should return Japanese
    let language = db.get_user_preferred_language(&user_id).await.unwrap();
    assert_eq!(
        language,
        Language::Japanese,
        "Should return Japanese when set"
    );
}

#[tokio::test]
async fn test_get_user_preferred_language_nonexistent_user() {
    let (db, _temp_dir) = create_test_db().await;

    // Query for non-existent user should return English default
    let language = db
        .get_user_preferred_language("nonexistent-user-id")
        .await
        .unwrap();
    assert_eq!(
        language,
        Language::English,
        "Should default to English for non-existent user"
    );
}

// Unit tests for Language::to_twilio_locale()

#[test]
fn test_twilio_locale_english() {
    assert_eq!(Language::English.to_twilio_locale(), "en");
}

#[test]
fn test_twilio_locale_norwegian() {
    assert_eq!(Language::Norwegian.to_twilio_locale(), "nb");
}

#[test]
fn test_twilio_locale_spanish() {
    assert_eq!(Language::Spanish.to_twilio_locale(), "es");
}

#[test]
fn test_twilio_locale_portuguese() {
    // Twilio uses lowercase "pt-br" unlike BCP 47 "pt-BR"
    assert_eq!(Language::Portuguese.to_twilio_locale(), "pt-br");
}

#[test]
fn test_twilio_locale_german() {
    assert_eq!(Language::German.to_twilio_locale(), "de");
}

#[test]
fn test_twilio_locale_french() {
    assert_eq!(Language::French.to_twilio_locale(), "fr");
}

#[test]
fn test_twilio_locale_japanese() {
    assert_eq!(Language::Japanese.to_twilio_locale(), "ja");
}

#[test]
fn test_twilio_locale_danish() {
    assert_eq!(Language::Danish.to_twilio_locale(), "da");
}

#[test]
fn test_twilio_locale_swedish() {
    assert_eq!(Language::Swedish.to_twilio_locale(), "sv");
}

#[test]
fn test_twilio_locale_all_languages_are_valid() {
    // Verify all locales are non-empty and valid Twilio locale format
    let languages = [
        Language::English,
        Language::Norwegian,
        Language::Spanish,
        Language::Portuguese,
        Language::German,
        Language::French,
        Language::Japanese,
        Language::Danish,
        Language::Swedish,
    ];

    for lang in languages {
        let locale = lang.to_twilio_locale();
        assert!(
            !locale.is_empty(),
            "Locale should not be empty for {:?}",
            lang
        );
        assert!(
            locale.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
            "Locale should be lowercase ASCII for {:?}: {}",
            lang,
            locale
        );
    }
}
