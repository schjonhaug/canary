use crate::wallet::WalletManager;
use crate::metadata::{TransactionEvent, EventType, EventInsert};
use tempfile::TempDir;
use tokio::sync::broadcast;
use std::fs;

fn create_temp_wallet_manager() -> (WalletManager, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let wallet_dir = temp_dir.path().join("wallets");
    fs::create_dir_all(&wallet_dir).unwrap();
    
    let (event_tx, _) = broadcast::channel(100);
    let metadata_db_path = temp_dir.path().join("txray.sqlite");
    
    let wallet_manager = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async {
            WalletManager::new(
                event_tx,
                wallet_dir,
                metadata_db_path.to_str().unwrap()
            ).await
        });
    
    (wallet_manager, temp_dir)
}

#[test]
fn test_parse_multipath_descriptor_valid() {
    let (wallet_manager, _temp_dir) = create_temp_wallet_manager();
    
    let descriptor = "wpkh(tpubD6NzVbkrYhZ4XgiXtGrdW5XDAPFCL9h7we1vwNCpn8tGbBcgfVYjXyhWo4E1xkh56hjod1RhGjxbaTLV3X4FyWuejifB9jusQ46QzG87VKp/<0;1>/*)";
    
    let result = wallet_manager.parse_multipath_descriptor(descriptor);
    assert!(result.is_ok());
    
    let (receive_desc, change_desc) = result.unwrap();
    // BDK splits multipath into /0/* and /1/*
    assert!(receive_desc.contains("/0/*"));
    assert!(change_desc.contains("/1/*"));
    assert_ne!(receive_desc, change_desc);
}

#[test]
fn test_parse_multipath_descriptor_invalid() {
    let (wallet_manager, _temp_dir) = create_temp_wallet_manager();
    
    // Test invalid descriptor
    let invalid_descriptor = "invalid_descriptor";
    let result = wallet_manager.parse_multipath_descriptor(invalid_descriptor);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Invalid descriptor"));
}

#[test]
fn test_parse_multipath_descriptor_not_multipath() {
    let (wallet_manager, _temp_dir) = create_temp_wallet_manager();
    
    // Test single path descriptor
    let single_path_descriptor = "wpkh(tpubD6NzVbkrYhZ4XgiXtGrdW5XDAPFCL9h7we1vwNCpn8tGbBcgfVYjXyhWo4E1xkh56hjod1RhGjxbaTLV3X4FyWuejifB9jusQ46QzG87VKp/*)";
    let result = wallet_manager.parse_multipath_descriptor(single_path_descriptor);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not a multipath descriptor"));
}

#[test]
fn test_get_network() {
    let (_wallet_manager, _temp_dir) = create_temp_wallet_manager();
    let network = WalletManager::get_network();
    assert_eq!(network, bdk_wallet::bitcoin::Network::Regtest);
}

#[test]
fn test_wallet_manager_creation() {
    let (wallet_manager, _temp_dir) = create_temp_wallet_manager();
    
    // Verify wallet manager is created with empty wallet list
    assert_eq!(wallet_manager.wallets.len(), 0);
    
    // Verify wallet directory exists
    assert!(wallet_manager.wallet_dir.exists());
    
    // Verify metadata database is initialized
    let wallets = wallet_manager.metadata_db.get_all_wallets().unwrap();
    assert_eq!(wallets.len(), 0);
}

#[test]
fn test_insert_and_broadcast_event_helper() {
    let (wallet_manager, _temp_dir) = create_temp_wallet_manager();
    
    // Create a wallet first
    let wallet_id = wallet_manager.metadata_db.insert_wallet(
        "Test Wallet",
        "test_descriptor",
        "test.sqlite"
    ).unwrap();
    
    // Create event insert
    let event_insert = EventInsert {
        wallet_id,
        event_type: EventType::Receive,
        amount_sats: 1000000,
        is_confirmed: true,
        is_rbf: false,
        is_cpfp: false,
        message: "Test transaction",
    };
    
    // Test the helper function
    let result = WalletManager::insert_and_broadcast_event_helper(
        &wallet_manager.metadata_db,
        &wallet_manager.event_sender,
        &event_insert
    );
    
    assert!(result.is_ok());
    
    // Verify event was inserted into database
    let events = wallet_manager.metadata_db.get_events_by_wallet(wallet_id).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::Receive);
    assert_eq!(events[0].amount_sats, 1000000);
    assert_eq!(events[0].message, "Test transaction");
}

#[test]
fn test_wallet_manager_get_wallet_by_id() {
    let (wallet_manager, _temp_dir) = create_temp_wallet_manager();
    
    // Create a wallet in metadata
    let wallet_id = wallet_manager.metadata_db.insert_wallet(
        "Test Wallet",
        "test_descriptor",
        "test.sqlite"
    ).unwrap();
    
    // Test getting wallet by ID
    let wallet = wallet_manager.get_wallet_by_id(wallet_id).unwrap();
    assert!(wallet.is_some());
    let wallet = wallet.unwrap();
    assert_eq!(wallet.name, "Test Wallet");
    assert_eq!(wallet.descriptor, "test_descriptor");
    
    // Test getting non-existent wallet
    let wallet = wallet_manager.get_wallet_by_id(999).unwrap();
    assert!(wallet.is_none());
}

#[test]
fn test_wallet_manager_get_all_wallets() {
    let (wallet_manager, _temp_dir) = create_temp_wallet_manager();
    
    // Initially should be empty
    let wallets = wallet_manager.get_all_wallets().unwrap();
    assert_eq!(wallets.len(), 0);
    
    // Create some wallets
    wallet_manager.metadata_db.insert_wallet(
        "Wallet 1",
        "descriptor1",
        "wallet1.sqlite"
    ).unwrap();
    
    wallet_manager.metadata_db.insert_wallet(
        "Wallet 2",
        "descriptor2",
        "wallet2.sqlite"
    ).unwrap();
    
    // Test getting all wallets
    let wallets = wallet_manager.get_all_wallets().unwrap();
    assert_eq!(wallets.len(), 2);
    assert_eq!(wallets[0].name, "Wallet 1");
    assert_eq!(wallets[1].name, "Wallet 2");
}

#[test]
fn test_event_insert_creation() {
    let event_insert = EventInsert {
        wallet_id: 1,
        event_type: EventType::Send,
        amount_sats: 500000,
        is_confirmed: false,
        is_rbf: true,
        is_cpfp: false,
        message: "Test RBF transaction",
    };
    
    assert_eq!(event_insert.wallet_id, 1);
    assert_eq!(event_insert.event_type, EventType::Send);
    assert_eq!(event_insert.amount_sats, 500000);
    assert!(!event_insert.is_confirmed);
    assert!(event_insert.is_rbf);
    assert!(!event_insert.is_cpfp);
    assert_eq!(event_insert.message, "Test RBF transaction");
}

#[test]
fn test_transaction_event_creation() {
    let event = TransactionEvent {
        id: Some(1),
        wallet_id: 1,
        event_type: EventType::Receive,
        amount_sats: 1000000,
        is_confirmed: true,
        is_rbf: false,
        is_cpfp: false,
        message: "Test confirmed receive".to_string(),
        created_at: "2024-01-01 12:00:00".to_string(),
    };
    
    assert_eq!(event.id, Some(1));
    assert_eq!(event.wallet_id, 1);
    assert_eq!(event.event_type, EventType::Receive);
    assert_eq!(event.amount_sats, 1000000);
    assert!(event.is_confirmed);
    assert!(!event.is_rbf);
    assert!(!event.is_cpfp);
    assert_eq!(event.message, "Test confirmed receive");
    assert_eq!(event.created_at, "2024-01-01 12:00:00");
}

#[test]
fn test_wallet_manager_wallet_dir_operations() {
    let (wallet_manager, _temp_dir) = create_temp_wallet_manager();
    
    // Test wallet directory path operations
    let wallet_dir = &wallet_manager.wallet_dir;
    assert!(wallet_dir.exists());
    assert!(wallet_dir.is_dir());
    
    // Test creating a wallet file path
    let wallet_filename = "test_wallet.sqlite";
    let wallet_path = wallet_dir.join(wallet_filename);
    assert_eq!(wallet_path.file_name().unwrap(), wallet_filename);
}

#[test]
fn test_event_type_default() {
    let default_event_type = EventType::default();
    assert_eq!(default_event_type, EventType::Send);
}

#[test]
fn test_multipath_descriptor_variations() {
    let (wallet_manager, _temp_dir) = create_temp_wallet_manager();
    
    // Test different multipath descriptor formats
    let descriptors = vec![
        "wpkh(tpubD6NzVbkrYhZ4XgiXtGrdW5XDAPFCL9h7we1vwNCpn8tGbBcgfVYjXyhWo4E1xkh56hjod1RhGjxbaTLV3X4FyWuejifB9jusQ46QzG87VKp/<0;1>/*)",
        "sh(wpkh(tpubD6NzVbkrYhZ4XgiXtGrdW5XDAPFCL9h7we1vwNCpn8tGbBcgfVYjXyhWo4E1xkh56hjod1RhGjxbaTLV3X4FyWuejifB9jusQ46QzG87VKp/<0;1>/*))",
    ];
    
    for descriptor in descriptors {
        let result = wallet_manager.parse_multipath_descriptor(descriptor);
        assert!(result.is_ok(), "Failed to parse descriptor: {}", descriptor);
        
        let (receive_desc, change_desc) = result.unwrap();
        assert!(!receive_desc.is_empty());
        assert!(!change_desc.is_empty());
        assert_ne!(receive_desc, change_desc);
    }
} 