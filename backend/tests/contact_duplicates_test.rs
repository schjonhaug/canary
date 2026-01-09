use canary::{
    config::{AppConfig, NetworkConfig, OperatingMode},
    metadata::{MetadataDb, ProviderType},
};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_duplicate_email_prevention() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create metadata database
    let config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        temp_dir.path().to_string_lossy().to_string(),
        OperatingMode::SelfHosted,
        None,
        None, // No JWT secret needed for self-hosted mode
    );

    let metadata_db = Arc::new(
        MetadataDb::new(db_path.to_str().unwrap(), &config)
            .await
            .unwrap(),
    );

    // Create a test user
    let user_id = metadata_db
        .create_user(
            "test@example.com",
            "hash",
            Some("Test User"),
            false,
            None,
            None,
        )
        .await
        .unwrap();

    // Create a test wallet
    let wallet_checksum = metadata_db
        .insert_wallet("Test Wallet", "descriptor", &user_id)
        .await
        .unwrap();

    // First, create a contact with an email
    let contact1_methods = vec![(ProviderType::Email, "john@example.com".to_string())];
    let contact1_id = metadata_db
        .insert_contact_with_notification_methods(&wallet_checksum, "John", contact1_methods)
        .await
        .unwrap();

    // Try to create another contact with the same email - should fail
    let duplicates = metadata_db
        .check_duplicate_notification_targets(
            &wallet_checksum,
            &[("email".to_string(), "john@example.com".to_string())],
            None,
        )
        .await
        .unwrap();

    assert!(!duplicates.is_empty(), "Should detect duplicate email");
    assert!(
        duplicates[0].contains("john@example.com"),
        "Error message should mention the duplicate email"
    );
    assert!(
        duplicates[0].contains("John"),
        "Error message should mention the existing contact name"
    );

    // Test case-insensitive email duplicate detection
    let case_duplicates = metadata_db
        .check_duplicate_notification_targets(
            &wallet_checksum,
            &[("email".to_string(), "JOHN@EXAMPLE.COM".to_string())],
            None,
        )
        .await
        .unwrap();

    assert!(
        !case_duplicates.is_empty(),
        "Should detect case-insensitive duplicate email"
    );

    // Test that same email in different wallet is allowed
    let wallet2_checksum = metadata_db
        .insert_wallet("Test Wallet 2", "descriptor2", &user_id)
        .await
        .unwrap();

    let cross_wallet_duplicates = metadata_db
        .check_duplicate_notification_targets(
            &wallet2_checksum,
            &[("email".to_string(), "john@example.com".to_string())],
            None,
        )
        .await
        .unwrap();

    assert!(
        cross_wallet_duplicates.is_empty(),
        "Same email should be allowed in different wallet"
    );

    // Test that updating a contact with its own email doesn't trigger duplicate error
    let self_update_duplicates = metadata_db
        .check_duplicate_notification_targets(
            &wallet_checksum,
            &[("email".to_string(), "john@example.com".to_string())],
            Some(&contact1_id),
        )
        .await
        .unwrap();

    assert!(
        self_update_duplicates.is_empty(),
        "Contact should be able to keep its own email on update"
    );
}

#[tokio::test]
async fn test_duplicate_phone_prevention() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create metadata database
    let config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        temp_dir.path().to_string_lossy().to_string(),
        OperatingMode::SelfHosted,
        None,
        None, // No JWT secret needed for self-hosted mode
    );

    let metadata_db = Arc::new(
        MetadataDb::new(db_path.to_str().unwrap(), &config)
            .await
            .unwrap(),
    );

    // Create a test user
    let user_id = metadata_db
        .create_user(
            "test@example.com",
            "hash",
            Some("Test User"),
            false,
            None,
            None,
        )
        .await
        .unwrap();

    // Create a test wallet
    let wallet_checksum = metadata_db
        .insert_wallet("Test Wallet", "descriptor", &user_id)
        .await
        .unwrap();

    // First, create a contact with a phone number
    let contact1_methods = vec![(ProviderType::Sms, "+4712345678".to_string())];
    let contact1_id = metadata_db
        .insert_contact_with_notification_methods(&wallet_checksum, "Alice", contact1_methods)
        .await
        .unwrap();

    // Try to create another contact with the same phone number - should fail
    let duplicates = metadata_db
        .check_duplicate_notification_targets(
            &wallet_checksum,
            &[("sms".to_string(), "+4712345678".to_string())],
            None,
        )
        .await
        .unwrap();

    assert!(
        !duplicates.is_empty(),
        "Should detect duplicate phone number"
    );
    assert!(
        duplicates[0].contains("+4712345678"),
        "Error message should mention the duplicate phone"
    );
    assert!(
        duplicates[0].contains("Alice"),
        "Error message should mention the existing contact name"
    );

    // Test that same phone in different wallet is allowed
    let wallet2_checksum = metadata_db
        .insert_wallet("Test Wallet 2", "descriptor2", &user_id)
        .await
        .unwrap();

    let cross_wallet_duplicates = metadata_db
        .check_duplicate_notification_targets(
            &wallet2_checksum,
            &[("sms".to_string(), "+4712345678".to_string())],
            None,
        )
        .await
        .unwrap();

    assert!(
        cross_wallet_duplicates.is_empty(),
        "Same phone should be allowed in different wallet"
    );

    // Test that updating a contact with its own phone doesn't trigger duplicate error
    let self_update_duplicates = metadata_db
        .check_duplicate_notification_targets(
            &wallet_checksum,
            &[("sms".to_string(), "+4712345678".to_string())],
            Some(&contact1_id),
        )
        .await
        .unwrap();

    assert!(
        self_update_duplicates.is_empty(),
        "Contact should be able to keep its own phone on update"
    );
}

#[tokio::test]
async fn test_ntfy_topics_excluded_from_duplicate_check() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create metadata database
    let config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        temp_dir.path().to_string_lossy().to_string(),
        OperatingMode::SelfHosted,
        None,
        None, // No JWT secret needed for self-hosted mode
    );

    let metadata_db = Arc::new(
        MetadataDb::new(db_path.to_str().unwrap(), &config)
            .await
            .unwrap(),
    );

    // Create a test user
    let user_id = metadata_db
        .create_user(
            "test@example.com",
            "hash",
            Some("Test User"),
            false,
            None,
            None,
        )
        .await
        .unwrap();

    // Create a test wallet
    let wallet_checksum = metadata_db
        .insert_wallet("Test Wallet", "descriptor", &user_id)
        .await
        .unwrap();

    // Create contact with ntfy topic
    let contact1_methods = vec![(ProviderType::Ntfy, "some-topic".to_string())];
    metadata_db
        .insert_contact_with_notification_methods(&wallet_checksum, "Bob", contact1_methods)
        .await
        .unwrap();

    // Try to create another contact with the same ntfy topic - should succeed (ntfy excluded from duplicates)
    let duplicates = metadata_db
        .check_duplicate_notification_targets(
            &wallet_checksum,
            &[("ntfy".to_string(), "some-topic".to_string())],
            None,
        )
        .await
        .unwrap();

    assert!(
        duplicates.is_empty(),
        "ntfy topics should be excluded from duplicate checking"
    );
}

#[tokio::test]
async fn test_mixed_provider_types_allowed() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create metadata database
    let config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        temp_dir.path().to_string_lossy().to_string(),
        OperatingMode::SelfHosted,
        None,
        None, // No JWT secret needed for self-hosted mode
    );

    let metadata_db = Arc::new(
        MetadataDb::new(db_path.to_str().unwrap(), &config)
            .await
            .unwrap(),
    );

    // Create a test user
    let user_id = metadata_db
        .create_user(
            "test@example.com",
            "hash",
            Some("Test User"),
            false,
            None,
            None,
        )
        .await
        .unwrap();

    // Create a test wallet
    let wallet_checksum = metadata_db
        .insert_wallet("Test Wallet", "descriptor", &user_id)
        .await
        .unwrap();

    // Create contact with email
    let contact1_methods = vec![(ProviderType::Email, "+123456789".to_string())]; // Phone number as email (weird but allowed)
    metadata_db
        .insert_contact_with_notification_methods(&wallet_checksum, "Contact 1", contact1_methods)
        .await
        .unwrap();

    // Create contact with same string but as SMS - should be allowed (different provider types)
    let duplicates = metadata_db
        .check_duplicate_notification_targets(
            &wallet_checksum,
            &[("sms".to_string(), "+123456789".to_string())],
            None,
        )
        .await
        .unwrap();

    assert!(
        duplicates.is_empty(),
        "Same string should be allowed for different provider types"
    );
}

#[tokio::test]
async fn test_verification_endpoint_rejects_duplicates() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create metadata database
    let config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        temp_dir.path().to_string_lossy().to_string(),
        OperatingMode::SelfHosted,
        None,
        None, // No JWT secret needed for self-hosted mode
    );

    let metadata_db = Arc::new(
        MetadataDb::new(db_path.to_str().unwrap(), &config)
            .await
            .unwrap(),
    );

    // Create a test user
    let user_id = metadata_db
        .create_user(
            "test@example.com",
            "hash",
            Some("Test User"),
            false,
            None,
            None,
        )
        .await
        .unwrap();

    // Create a test wallet
    let wallet_checksum = metadata_db
        .insert_wallet("Test Wallet", "descriptor", &user_id)
        .await
        .unwrap();

    // First, create a contact with an email
    let contact1_methods = vec![(ProviderType::Email, "existing@example.com".to_string())];
    metadata_db
        .insert_contact_with_notification_methods(
            &wallet_checksum,
            "Existing Contact",
            contact1_methods,
        )
        .await
        .unwrap();

    // Now try to check for duplicate during verification - should find the existing contact
    let duplicate_check_result = metadata_db
        .check_duplicate_notification_target(
            &wallet_checksum,
            "email",
            "existing@example.com",
            None,
        )
        .await
        .unwrap();

    assert!(
        duplicate_check_result.is_some(),
        "Should detect duplicate email during verification"
    );
    assert_eq!(
        duplicate_check_result.unwrap(),
        "Existing Contact",
        "Should return the existing contact name"
    );

    // Test that different email is allowed
    let no_duplicate_result = metadata_db
        .check_duplicate_notification_target(&wallet_checksum, "email", "new@example.com", None)
        .await
        .unwrap();

    assert!(
        no_duplicate_result.is_none(),
        "Should not detect duplicate for different email"
    );
}
