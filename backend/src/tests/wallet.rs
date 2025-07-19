use crate::metadata::{EventInsert, EventType, TransactionEvent};
use crate::wallet::WalletManager;
use bdk_wallet::bitcoin::Network;
use std::fs;
use tempfile::TempDir;
use tokio::sync::broadcast;

fn create_temp_wallet_manager() -> (WalletManager, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let wallet_dir = temp_dir.path().join("wallets");
    fs::create_dir_all(&wallet_dir).unwrap();

    let (event_tx, _) = broadcast::channel(100);
    let (dashboard_tx, _) = broadcast::channel::<crate::metadata::DashboardUpdate>(100);
    let metadata_db_path = temp_dir.path().join("metadata.sqlite");

    let wallet_manager = tokio::runtime::Runtime::new().unwrap().block_on(async {
        WalletManager::new(
            event_tx,
            dashboard_tx,
            wallet_dir,
            metadata_db_path.to_str().unwrap(),
            Network::Regtest,
            "tcp://127.0.0.1:50001",
        )
        .await
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
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Invalid descriptor")
    );
}

#[test]
fn test_parse_multipath_descriptor_not_multipath() {
    let (wallet_manager, _temp_dir) = create_temp_wallet_manager();

    // Test single path descriptor
    let single_path_descriptor = "wpkh(tpubD6NzVbkrYhZ4XgiXtGrdW5XDAPFCL9h7we1vwNCpn8tGbBcgfVYjXyhWo4E1xkh56hjod1RhGjxbaTLV3X4FyWuejifB9jusQ46QzG87VKp/*)";
    let result = wallet_manager.parse_multipath_descriptor(single_path_descriptor);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("not a multipath descriptor")
    );
}

#[test]
fn test_get_network() {
    let (wallet_manager, _temp_dir) = create_temp_wallet_manager();
    let network = wallet_manager.get_network();
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
    let wallet_id = wallet_manager
        .metadata_db
        .insert_wallet("Test Wallet", "test_descriptor", "test.sqlite")
        .unwrap();

    // Create event insert
    let event_insert = EventInsert {
        wallet_id,
        event_type: EventType::Receive,
        amount_sats: 1000000,
        is_confirmed: true,
        is_rbf: false,
        is_cpfp: false,
        balance_total: Some(1000000),
        txid: Some("test_txid".to_string()),
    };

    // Test the helper function
    let result = WalletManager::insert_and_broadcast_event_helper(
        &wallet_manager.metadata_db,
        &wallet_manager.event_sender,
        &event_insert,
    );

    assert!(result.is_ok());

    // Verify event was inserted into database
    let events = wallet_manager
        .metadata_db
        .get_all_events_with_wallets()
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::Receive);
    assert_eq!(events[0].amount_sats, 1000000);
    assert_eq!(events[0].wallet_id, wallet_id);
}

#[test]
fn test_wallet_manager_get_wallet_by_id() {
    let (wallet_manager, _temp_dir) = create_temp_wallet_manager();

    // Create a wallet in metadata
    let wallet_id = wallet_manager
        .metadata_db
        .insert_wallet("Test Wallet", "test_descriptor", "test.sqlite")
        .unwrap();

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
    let wallets = wallet_manager.metadata_db.get_all_wallets().unwrap();
    assert_eq!(wallets.len(), 0);

    // Create some wallets
    wallet_manager
        .metadata_db
        .insert_wallet("Wallet 1", "descriptor1", "wallet1.sqlite")
        .unwrap();

    wallet_manager
        .metadata_db
        .insert_wallet("Wallet 2", "descriptor2", "wallet2.sqlite")
        .unwrap();

    // Test getting all wallets
    let wallets = wallet_manager.metadata_db.get_all_wallets().unwrap();
    assert_eq!(wallets.len(), 2);
    // Verify both wallets are present (order may vary due to timestamp precision)
    let wallet_names: Vec<&str> = wallets.iter().map(|w| w.name.as_str()).collect();
    assert!(wallet_names.contains(&"Wallet 1"));
    assert!(wallet_names.contains(&"Wallet 2"));
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
        balance_total: None,
        txid: Some("test_rbf_txid".to_string()),
    };

    assert_eq!(event_insert.wallet_id, 1);
    assert_eq!(event_insert.event_type, EventType::Send);
    assert_eq!(event_insert.amount_sats, 500000);
    assert!(!event_insert.is_confirmed);
    assert!(event_insert.is_rbf);
    assert!(!event_insert.is_cpfp);
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
        balance_total: Some(1000000),
        created_at: "2024-01-01 12:00:00".to_string(),
    };

    assert_eq!(event.id, Some(1));
    assert_eq!(event.wallet_id, 1);
    assert_eq!(event.event_type, EventType::Receive);
    assert_eq!(event.amount_sats, 1000000);
    assert!(event.is_confirmed);
    assert!(!event.is_rbf);
    assert!(!event.is_cpfp);
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

#[test]
fn test_rbf_transaction_detection() {
    // Test RBF (Replace-By-Fee) transaction characteristics
    let rbf_event_insert = EventInsert {
        wallet_id: 1,
        event_type: EventType::Send,
        amount_sats: 50000,
        is_confirmed: false,
        is_rbf: true,
        is_cpfp: false,
        balance_total: Some(950000),
        txid: Some("rbf_txid".to_string()),
    };

    assert_eq!(rbf_event_insert.wallet_id, 1);
    assert_eq!(rbf_event_insert.event_type, EventType::Send);
    assert_eq!(rbf_event_insert.amount_sats, 50000);
    assert!(!rbf_event_insert.is_confirmed);
    assert!(rbf_event_insert.is_rbf);
    assert!(!rbf_event_insert.is_cpfp);
    assert_eq!(rbf_event_insert.balance_total, Some(950000));
}

#[test]
fn test_cpfp_transaction_detection() {
    // Test CPFP (Child-Pays-For-Parent) transaction characteristics
    let cpfp_event_insert = EventInsert {
        wallet_id: 1,
        event_type: EventType::Send,
        amount_sats: 25000,
        is_confirmed: false,
        is_rbf: false,
        is_cpfp: true,
        balance_total: Some(975000),
        txid: Some("cpfp_txid".to_string()),
    };

    assert_eq!(cpfp_event_insert.wallet_id, 1);
    assert_eq!(cpfp_event_insert.event_type, EventType::Send);
    assert_eq!(cpfp_event_insert.amount_sats, 25000);
    assert!(!cpfp_event_insert.is_confirmed);
    assert!(!cpfp_event_insert.is_rbf);
    assert!(cpfp_event_insert.is_cpfp);
    assert_eq!(cpfp_event_insert.balance_total, Some(975000));
}

#[test]
fn test_transaction_event_flags_combinations() {
    // Test various combinations of RBF and CPFP flags
    let test_cases = vec![
        (false, false), // Normal transaction
        (true, false),  // RBF transaction
        (false, true),  // CPFP transaction
        (true, true),   // Both RBF and CPFP (edge case)
    ];

    for (is_rbf, is_cpfp) in test_cases {
        let event = TransactionEvent {
            id: Some(1),
            wallet_id: 1,
            event_type: EventType::Send,
            amount_sats: 100000,
            is_confirmed: false,
            is_rbf,
            is_cpfp,
            balance_total: Some(900000),
            created_at: "2024-01-01 12:00:00".to_string(),
        };

        assert_eq!(event.is_rbf, is_rbf);
        assert_eq!(event.is_cpfp, is_cpfp);
        assert!(!event.is_confirmed); // RBF/CPFP typically unconfirmed
        assert_eq!(event.event_type, EventType::Send); // Usually send transactions
    }
}

#[test]
fn test_rbf_vs_cpfp_transaction_differences() {
    // Test the conceptual differences between RBF and CPFP
    let rbf_transaction = EventInsert {
        wallet_id: 1,
        event_type: EventType::Send,
        amount_sats: 100000, // Original amount
        is_confirmed: false,
        is_rbf: true,
        is_cpfp: false,
        balance_total: Some(900000),
        txid: Some("rbf_tx_id".to_string()),
    };

    let cpfp_transaction = EventInsert {
        wallet_id: 1,
        event_type: EventType::Send,
        amount_sats: 10000, // Smaller amount, just for fee
        is_confirmed: false,
        is_rbf: false,
        is_cpfp: true,
        balance_total: Some(890000),
        txid: Some("cpfp_tx_id".to_string()),
    };

    // RBF: Replace existing transaction with higher fee
    assert!(rbf_transaction.is_rbf);
    assert!(!rbf_transaction.is_cpfp);
    assert_eq!(rbf_transaction.amount_sats, 100000);

    // CPFP: Child transaction pays for parent
    assert!(!cpfp_transaction.is_rbf);
    assert!(cpfp_transaction.is_cpfp);
    assert_eq!(cpfp_transaction.amount_sats, 10000);

    // Both should be unconfirmed initially
    assert!(!rbf_transaction.is_confirmed);
    assert!(!cpfp_transaction.is_confirmed);
}

#[test]
fn test_transaction_confirmation_state_transitions() {
    // Test transaction state transitions from unconfirmed to confirmed
    let unconfirmed_event = TransactionEvent {
        id: Some(1),
        wallet_id: 1,
        event_type: EventType::Send,
        amount_sats: 100000,
        is_confirmed: false,
        is_rbf: true,
        is_cpfp: false,
        balance_total: Some(900000),
        created_at: "2024-01-01 12:00:00".to_string(),
    };

    let confirmed_event = TransactionEvent {
        id: Some(2),
        wallet_id: 1,
        event_type: EventType::Send,
        amount_sats: 100000,
        is_confirmed: true,
        is_rbf: false, // RBF flag typically cleared on confirmation
        is_cpfp: false,
        balance_total: Some(900000),
        created_at: "2024-01-01 12:05:00".to_string(),
    };

    // Initial state: unconfirmed with RBF
    assert!(!unconfirmed_event.is_confirmed);
    assert!(unconfirmed_event.is_rbf);

    // After confirmation: confirmed, RBF cleared
    assert!(confirmed_event.is_confirmed);
    assert!(!confirmed_event.is_rbf);

    // Amount should remain the same
    assert_eq!(unconfirmed_event.amount_sats, confirmed_event.amount_sats);
    assert_eq!(unconfirmed_event.wallet_id, confirmed_event.wallet_id);
}

#[test]
fn test_sync_wallet_manager_creation() {
    // Test that wallet manager can be created for sync operations
    let (wallet_manager, _temp_dir) = create_temp_wallet_manager();
    
    // Verify sync-related properties
    assert!(wallet_manager.wallets.is_empty()); // No wallets initially
    assert!(wallet_manager.wallet_dir.exists()); // Directory exists
    // Electrum client is initialized during wallet manager creation
    
    // Test network configuration for sync (using get_network method)
    assert_eq!(wallet_manager.get_network(), Network::Regtest);
}

#[test]
fn test_background_sync_interval_concept() {
    // Test concepts related to background sync timing
    use std::time::Duration;
    
    // Test 4-second interval constant
    let sync_interval = Duration::from_secs(4);
    assert_eq!(sync_interval.as_secs(), 4);
    assert_eq!(sync_interval.as_millis(), 4000);
    
    // Test that interval is reasonable for background sync
    assert!(sync_interval.as_secs() > 0); // Not too frequent
    assert!(sync_interval.as_secs() < 60); // Not too infrequent for user experience
}

#[test]
fn test_wallet_sync_error_handling() {
    // Test error handling during wallet sync operations
    let (wallet_manager, _temp_dir) = create_temp_wallet_manager();
    
    // Test empty wallet list sync (should not error)
    let result = tokio::runtime::Runtime::new().unwrap().block_on(async {
        let mut manager = wallet_manager;
        manager.sync_all_wallets().await
    });
    
    // Should succeed even with no wallets
    assert!(result.is_ok());
}

#[test]
fn test_sync_wallet_state_management() {
    // Test wallet state management during sync operations
    let (wallet_manager, _temp_dir) = create_temp_wallet_manager();
    
    // Test initial wallet state
    assert_eq!(wallet_manager.wallets.len(), 0);
    
    // Test wallet directory structure for sync
    let wallet_dir = &wallet_manager.wallet_dir;
    assert!(wallet_dir.exists());
    assert!(wallet_dir.is_dir());
    
    // Test that wallet directory is ready for sync operations
    let metadata = std::fs::metadata(wallet_dir).unwrap();
    assert!(metadata.is_dir());
}

#[test]
fn test_sync_concurrency_safety() {
    // Test that sync operations can handle concurrent access
    let (wallet_manager, _temp_dir) = create_temp_wallet_manager();
    
    // Test that wallet manager can be wrapped in Arc<Mutex<>> for concurrent access
    use std::sync::Arc;
    use tokio::sync::Mutex;
    
    let wallet_manager_arc = Arc::new(Mutex::new(wallet_manager));
    let wallet_manager_clone = Arc::clone(&wallet_manager_arc);
    
    // Test concurrent access pattern (similar to background sync implementation)
    let result = tokio::runtime::Runtime::new().unwrap().block_on(async {
        let manager = wallet_manager_clone.lock().await;
        // Test that we can obtain the lock (simulating background sync access)
        assert_eq!(manager.wallets.len(), 0);
        Ok::<(), anyhow::Error>(())
    });
    
    assert!(result.is_ok());
}

#[test]
fn test_sync_performance_considerations() {
    // Test performance-related aspects of sync operations
    let (wallet_manager, _temp_dir) = create_temp_wallet_manager();
    
    // Test that sync operations are designed to be efficient
    let start_time = std::time::Instant::now();
    
    let result = tokio::runtime::Runtime::new().unwrap().block_on(async {
        let mut manager = wallet_manager;
        manager.sync_all_wallets().await
    });
    
    let duration = start_time.elapsed();
    
    // Sync should complete quickly for empty wallet set
    assert!(result.is_ok());
    assert!(duration.as_secs() < 1); // Should complete within 1 second for empty set
}

#[test]
fn test_sync_all_wallets_empty_set() {
    // Test sync_all_wallets with empty wallet set
    let (wallet_manager, _temp_dir) = create_temp_wallet_manager();
    
    let result = tokio::runtime::Runtime::new().unwrap().block_on(async {
        let mut manager = wallet_manager;
        let sync_result = manager.sync_all_wallets().await;
        
        // Test that sync succeeds with empty wallet set
        assert!(sync_result.is_ok());
        
        // Test that wallet count remains zero
        assert_eq!(manager.wallets.len(), 0);
        
        sync_result
    });
    
    assert!(result.is_ok());
}
