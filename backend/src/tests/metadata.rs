use crate::metadata::{ContactPerson, EventInsert, EventType, Language, MetadataDb, WalletMetadata};
use tempfile::NamedTempFile;

async fn create_temp_db() -> (MetadataDb, NamedTempFile) {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path().to_str().unwrap();
    let db = MetadataDb::new(db_path).unwrap();
    (db, temp_file)
}

#[tokio::test]
async fn test_create_database() {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path().to_str().unwrap();
    let db = MetadataDb::new(db_path).unwrap();

    // Verify tables exist by trying to insert and query data
    let wallet_id = db
        .insert_wallet("Test Wallet", "test_descriptor", "test.sqlite")
        .await.unwrap();
    assert_eq!(wallet_id, 1);
}

#[tokio::test]
async fn test_wallet_operations() {
    let (db, _temp_file) = create_temp_db().await;

    // Test insert wallet
    let wallet_id = db
        .insert_wallet("Test Wallet", "test_descriptor", "test.sqlite")
        .await.unwrap();
    assert_eq!(wallet_id, 1);

    // Test get wallet by id
    let wallet = db.get_wallet_by_id(wallet_id).await.unwrap().unwrap();
    assert_eq!(wallet.name, "Test Wallet");
    assert_eq!(wallet.descriptor, "test_descriptor");
    assert_eq!(wallet.wallet_filename, "test.sqlite");

    // Test get all wallets
    let wallets = db.get_all_wallets().await.unwrap();
    assert_eq!(wallets.len(), 1);
    assert_eq!(wallets[0].name, "Test Wallet");

    // Test descriptor exists
    assert!(db.descriptor_exists("test_descriptor").await.unwrap());
    assert!(!db.descriptor_exists("nonexistent").await.unwrap());

    // Test get wallet by descriptor
    let wallet = db
        .get_wallet_by_descriptor("test_descriptor")
        .await.unwrap()
        .unwrap();
    assert_eq!(wallet.name, "Test Wallet");

    // Test delete wallet
    let deleted = db.delete_wallet_by_id(wallet_id).await.unwrap();
    assert!(deleted.is_some());
    let (deleted_descriptor, deleted_filename) = deleted.unwrap();
    assert_eq!(deleted_descriptor, "test_descriptor");
    assert_eq!(deleted_filename, "test.sqlite");

    // Verify wallet is deleted
    let wallet = db.get_wallet_by_id(wallet_id).await.unwrap();
    assert!(wallet.is_none());
}

#[tokio::test]
async fn test_wallet_duplicate_descriptor() {
    let (db, _temp_file) = create_temp_db().await;

    // Insert first wallet
    db.insert_wallet("Wallet 1", "test_descriptor", "test1.sqlite")
        .await.unwrap();

    // Try to insert second wallet with same descriptor
    let result = db.insert_wallet("Wallet 2", "test_descriptor", "test2.sqlite").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_contact_operations() {
    let (db, _temp_file) = create_temp_db().await;

    // First create a wallet for the contact
    let wallet_id = db.insert_wallet("Test Wallet", "test_descriptor", "test_filename").await.unwrap();
    
    // Test insert contact
    let contact_id = db.insert_contact(wallet_id, "John Doe", "12345678", &Language::Norwegian).await.unwrap();
    assert_eq!(contact_id, 1);

    // Test get contacts for wallet
    let contacts = db.get_contacts_for_wallet(wallet_id).await.unwrap();
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0].name, "John Doe");
    assert_eq!(contacts[0].phone_number, "12345678");
    assert_eq!(contacts[0].wallet_id, wallet_id);

    // Test delete contact
    let deleted = db.delete_contact(contact_id).await.unwrap();
    assert!(deleted);

    // Verify contact is deleted
    let contacts = db.get_contacts_for_wallet(wallet_id).await.unwrap();
    assert_eq!(contacts.len(), 0);
}

#[tokio::test]
async fn test_wallet_specific_contact_operations() {
    let (db, _temp_file) = create_temp_db().await;

    // Create wallet 
    let wallet_id = db
        .insert_wallet("Test Wallet", "test_descriptor", "test.sqlite")
        .await.unwrap();
    
    // Create contact directly for wallet
    let contact_id = db.insert_contact(wallet_id, "John Doe", "12345678", &Language::Norwegian).await.unwrap();

    // Test get contacts for wallet
    let contacts = db.get_contacts_for_wallet(wallet_id).await.unwrap();
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0].name, "John Doe");
    assert_eq!(contacts[0].wallet_id, wallet_id);

    // Test delete contact
    let deleted = db.delete_contact(contact_id).await.unwrap();
    assert!(deleted);

    // Verify contact is removed
    let contacts = db.get_contacts_for_wallet(wallet_id).await.unwrap();
    assert_eq!(contacts.len(), 0);
}

#[tokio::test]
async fn test_twilio_config_operations() {
    let (db, _temp_file) = create_temp_db().await;

    // Test get config when none exists
    let config = db.get_twilio_config().await.unwrap();
    assert!(config.is_none());

    // Test upsert config
    let config_id = db
        .upsert_twilio_config("test_sid", "test_token", "test_messaging_sid")
        .await.unwrap();
    assert!(config_id >= 1);

    // Test get config
    let config = db.get_twilio_config().await.unwrap().unwrap();
    assert_eq!(config.account_sid, "test_sid");
    assert_eq!(config.auth_token, "test_token");
    assert_eq!(config.messaging_service_sid, "test_messaging_sid");

    // Test update config (upsert should update existing)
    let _config_id2 = db
        .upsert_twilio_config("new_sid", "new_token", "new_messaging_sid")
        .await.unwrap();
    // Accept either same or new id, but config should be updated
    let config = db.get_twilio_config().await.unwrap().unwrap();
    assert_eq!(config.account_sid, "new_sid");
    assert_eq!(config.auth_token, "new_token");
    assert_eq!(config.messaging_service_sid, "new_messaging_sid");
}

#[tokio::test]
async fn test_event_operations() {
    let (db, _temp_file) = create_temp_db().await;

    // Create a wallet first
    let wallet_id = db
        .insert_wallet("Test Wallet", "test_descriptor", "test.sqlite")
        .await.unwrap();

    // Test insert event
    let event = EventInsert {
        wallet_id,
        event_type: EventType::Receive,
        amount_sats: 1000000, // 0.01 BTC
        is_confirmed: true,
        is_rbf: false,
        is_cpfp: false,
        balance_total: Some(1000000),
        transaction_time: 1672574400, // 2023-01-01 12:00:00 UTC
    };

    let event_id = db.insert_event(&event).await.unwrap();
    assert_eq!(event_id, 1);

    // Test get all events with wallets
    let all_events = db.get_all_events_with_wallets().await.unwrap();
    assert_eq!(all_events.len(), 1);
    assert_eq!(all_events[0].event_type, EventType::Receive);
    assert_eq!(all_events[0].amount_sats, 1000000);
    assert_eq!(all_events[0].wallet_id, wallet_id);
}

#[tokio::test]
async fn test_sms_log_operations() {
    let (db, _temp_file) = create_temp_db().await;

    // Create wallet, contact, and event
    let wallet_id = db
        .insert_wallet("Test Wallet", "test_descriptor", "test.sqlite")
        .await.unwrap();
    let contact_id = db.insert_contact(wallet_id, "John Doe", "12345678", &Language::Norwegian).await.unwrap();

    let event = EventInsert {
        wallet_id,
        event_type: EventType::Receive,
        amount_sats: 1000000,
        is_confirmed: true,
        is_rbf: false,
        is_cpfp: false,
        balance_total: Some(1000000),
        transaction_time: 1672574400, // 2023-01-01 12:00:00 UTC
    };
    let event_id = db.insert_event(&event).await.unwrap();

    // Test insert SMS log
    let log_id = db
        .insert_sms_log(event_id, contact_id, "Test message", "sent", Some("test_sid"), None)
        .await.unwrap();
    assert_eq!(log_id, 1);

    // SMS recipients functionality has been moved to SSE dashboard updates
}

#[tokio::test]
async fn test_event_type_conversions() {
    // Test string to EventType conversion
    assert_eq!(EventType::from("send"), EventType::Send);
    assert_eq!(EventType::from("receive"), EventType::Receive);

    // Test EventType to string conversion
    assert_eq!(EventType::Send.as_str(), "send");
    assert_eq!(EventType::Receive.as_str(), "receive");
}

#[tokio::test]
async fn test_wallet_metadata_serialization() {
    let wallet = WalletMetadata {
        id: Some(1),
        name: "Test Wallet".to_string(),
        descriptor: "test_descriptor".to_string(),
        wallet_filename: "test.sqlite".to_string(),
        created_at: "2024-01-01 12:00:00".to_string(),
        balance_total: Some(100000000), // 1 BTC in satoshis
        last_activity: Some("2024-01-01 12:30:00".to_string()),
        contact_count: Some(0),
        hex_color: "#ff0000".to_string(),
    };

    // Test serialization
    let json = serde_json::to_string(&wallet).unwrap();
    assert!(json.contains("Test Wallet"));
    assert!(json.contains("test_descriptor"));

    // Test deserialization
    let deserialized: WalletMetadata = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.name, wallet.name);
    assert_eq!(deserialized.descriptor, wallet.descriptor);
}

#[tokio::test]
async fn test_contact_person_serialization() {
    let contact = ContactPerson {
        id: Some(1),
        wallet_id: 1,
        name: "John Doe".to_string(),
        phone_number: "12345678".to_string(),
        language: Language::Norwegian,
        created_at: "2024-01-01 12:00:00".to_string(),
    };

    // Test serialization
    let json = serde_json::to_string(&contact).unwrap();
    assert!(json.contains("John Doe"));
    assert!(json.contains("12345678"));

    // Test deserialization
    let deserialized: ContactPerson = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.name, contact.name);
    assert_eq!(deserialized.phone_number, contact.phone_number);
}
