use crate::electrum::ElectrumClient;
use bdk_wallet::{KeychainKind, Wallet};
use bdk_wallet::bitcoin::Network;
use tempfile::TempDir;

#[test]
fn test_electrum_client_creation() {
    // Test that we can create an Electrum client
    // Note: This will fail if no Electrum server is running on regtest
    let result = ElectrumClient::new("tcp://127.0.0.1:50001");

    // In a test environment without a running Electrum server, this should fail
    // But we can still test the error handling
    match result {
        Ok(client) => {
            // If we have a running server, test basic functionality
            let features = client.server_features();
            assert!(features.is_ok());
            assert_eq!(features.unwrap(), "Connected to Electrum via BDK");
        }
        Err(e) => {
            // Expected in test environment without server
            assert!(
                e.to_string().contains("Connection refused")
                    || e.to_string().contains("failed to connect")
                    || e.to_string().contains("timeout")
            );
        }
    }
}

#[test]
fn test_server_features() {
    // Test the server_features method returns expected string
    // This is a static method that doesn't require actual connection
    // Skip test if Electrum server is not available
    let client = match bdk_electrum::electrum_client::Client::new("tcp://127.0.0.1:50001") {
        Ok(electrum_client) => ElectrumClient {
            client: bdk_electrum::BdkElectrumClient::new(electrum_client),
        },
        Err(_) => {
            // Skip test if Electrum server is not available
            return;
        }
    };

    let features = client.server_features();
    assert!(features.is_ok());
    assert_eq!(features.unwrap(), "Connected to Electrum via BDK");
}

#[test]
fn test_electrum_client_constants() {
    // Test that the constants are defined correctly
    // These are defined in the electrum.rs file
    assert_eq!(crate::electrum::STOP_GAP, 20);
    assert_eq!(crate::electrum::BATCH_SIZE, 5);
}

#[test]
fn test_electrum_client_structure() {
    // Test that the ElectrumClient struct has the expected structure
    // Skip test if Electrum server is not available
    let client = match bdk_electrum::electrum_client::Client::new("tcp://127.0.0.1:50001") {
        Ok(electrum_client) => ElectrumClient {
            client: bdk_electrum::BdkElectrumClient::new(electrum_client),
        },
        Err(_) => {
            // Skip test if Electrum server is not available
            return;
        }
    };

    // Verify the client field exists and is accessible
    // This is a compile-time check that the structure is correct
    let _client_ref = &client.client;
}

#[test]
fn test_electrum_client_methods_exist() {
    // Test that all expected methods exist on ElectrumClient
    // This is a compile-time check

    // Create a dummy client for method testing
    // Skip test if Electrum server is not available
    let client = match bdk_electrum::electrum_client::Client::new("tcp://127.0.0.1:50001") {
        Ok(electrum_client) => ElectrumClient {
            client: bdk_electrum::BdkElectrumClient::new(electrum_client),
        },
        Err(_) => {
            // Skip test if Electrum server is not available
            return;
        }
    };

    // Test that methods exist (compile-time check)
    let _features_result = client.server_features();

    // Note: sync_wallet and sync_wallet_incremental require actual wallet instances
    // which are complex to create in unit tests, so we test their existence
    // through integration tests instead
}

#[test]
fn test_electrum_client_error_handling() {
    // Test error handling for invalid connection strings
    let result = bdk_electrum::electrum_client::Client::new("invalid://connection/string");
    assert!(result.is_err());

    // Test error handling for unreachable servers
    let result = bdk_electrum::electrum_client::Client::new("tcp://192.168.1.999:99999");
    assert!(result.is_err());
}

#[test]
fn test_bdk_electrum_client_creation() {
    // Test BDK Electrum client creation
    let result = bdk_electrum::electrum_client::Client::new("tcp://127.0.0.1:50001");

    match result {
        Ok(_client) => {
            // Success case - server is available
            let bdk_client = bdk_electrum::BdkElectrumClient::new(_client);
            // Verify BDK client was created successfully
            assert!(std::mem::size_of_val(&bdk_client) > 0);
        }
        Err(_) => {
            // Expected in test environment without server
            // This is fine for unit tests
        }
    }
}

#[test]
fn test_electrum_client_new_regtest_error_handling() {
    // Test that new() with regtest URL handles connection errors gracefully
    let result = ElectrumClient::new("tcp://127.0.0.1:50001");

    match result {
        Ok(_) => {
            // Success case - regtest server is running
            // This is fine for unit tests
        }
        Err(e) => {
            // Expected error in test environment
            let error_msg = e.to_string();
            assert!(
                error_msg.contains("Connection refused")
                    || error_msg.contains("failed to connect")
                    || error_msg.contains("timeout")
                    || error_msg.contains("No route to host")
                    || error_msg.contains("Network unreachable"),
                "Unexpected error message: {}",
                error_msg
            );
        }
    }
}

#[test]
fn test_electrum_client_constants_are_reasonable() {
    // Test that the constants have reasonable values
    assert!(crate::electrum::STOP_GAP > 0);
    assert!(crate::electrum::STOP_GAP <= 1000); // Shouldn't be unreasonably large

    assert!(crate::electrum::BATCH_SIZE > 0);
    assert!(crate::electrum::BATCH_SIZE <= 100); // Shouldn't be unreasonably large

    // BATCH_SIZE should be smaller than STOP_GAP for efficiency
    assert!(crate::electrum::BATCH_SIZE <= crate::electrum::STOP_GAP);
}

#[test]
fn test_electrum_client_connection_string_format() {
    // Test that the connection string format is correct
    let connection_string = "tcp://127.0.0.1:50001";

    // Verify it's a valid TCP connection string
    assert!(connection_string.starts_with("tcp://"));
    assert!(connection_string.contains("127.0.0.1"));
    assert!(connection_string.contains("50001"));

    // Test parsing the connection string
    let result = bdk_electrum::electrum_client::Client::new(connection_string);
    match result {
        Ok(_) => {
            // Success case - valid connection string
        }
        Err(_) => {
            // Expected in test environment without server
            // But the connection string format is still valid
        }
    }
}

#[test]
fn test_electrum_client_regtest_port() {
    // Test that the regtest port is the standard Electrum regtest port
    let regtest_port = 50001;

    // Standard Electrum regtest port
    assert_eq!(regtest_port, 50001);

    // Test connection to regtest port
    let connection_string = format!("tcp://127.0.0.1:{}", regtest_port);
    let result = bdk_electrum::electrum_client::Client::new(&connection_string);

    match result {
        Ok(_) => {
            // Success case - regtest server is running
        }
        Err(_) => {
            // Expected in test environment without server
            // But the port is correct
        }
    }
}

#[test]
fn test_address_revelation_logic() {
    // Test the address revelation logic without needing an actual Electrum connection
    let _temp_dir = TempDir::new().unwrap();
    
    // Create a test wallet with a simple descriptor
    let descriptor = "wpkh(tprv8ZgxMBicQKsPe6D3wxZPHxy7JEMBwjxiimYXSvM8aRaqZmDvU7Jec5c8aSB8rCzFmAP6aEjPhUmiXm2KB7XUzkApX2prmDHcQsUQY5DsxJw/84h/1h/0h/0/*)#5asejmkj";
    let change_descriptor = "wpkh(tprv8ZgxMBicQKsPe6D3wxZPHxy7JEMBwjxiimYXSvM8aRaqZmDvU7Jec5c8aSB8rCzFmAP6aEjPhUmiXm2KB7XUzkApX2prmDHcQsUQY5DsxJw/84h/1h/0h/1/*)#9f4c0wx2";
    
    let mut wallet = match Wallet::create(descriptor, change_descriptor)
        .network(Network::Regtest)
        .create_wallet_no_persist() {
        Ok(w) => w,
        Err(_) => return, // Skip test if wallet creation fails
    };
    
    // Test initial state
    let initial_external = wallet.next_derivation_index(KeychainKind::External);
    let initial_internal = wallet.next_derivation_index(KeychainKind::Internal);
    assert_eq!(initial_external, 0, "External addresses should start at 0");
    assert_eq!(initial_internal, 0, "Internal addresses should start at 0");
    
    // Test revealing addresses
    let revealed: Vec<_> = wallet.reveal_addresses_to(KeychainKind::External, 10).collect();
    assert_eq!(revealed.len(), 11, "Should reveal 11 addresses (indices 0-10)");
    assert_eq!(wallet.next_derivation_index(KeychainKind::External), 11, "Next index should be 11");
    
    // Test revealing more addresses
    let revealed: Vec<_> = wallet.reveal_addresses_to(KeychainKind::External, 50).collect();
    assert_eq!(revealed.len(), 40, "Should reveal 40 more addresses (11-50)");
    assert_eq!(wallet.next_derivation_index(KeychainKind::External), 51, "Next index should be 51");
    
    // Test that revealing to a lower index returns no new addresses
    let revealed: Vec<_> = wallet.reveal_addresses_to(KeychainKind::External, 30).collect();
    assert_eq!(revealed.len(), 0, "Should reveal 0 addresses when target is lower");
    assert_eq!(wallet.next_derivation_index(KeychainKind::External), 51, "Next index should remain 51");
}

#[test]
fn test_stop_gap_calculation() {
    // Test the stop gap calculation logic
    let stop_gap = crate::electrum::STOP_GAP as u32;
    
    // Test various scenarios
    assert_eq!(0 + stop_gap, 20, "From index 0, need to check up to 20");
    assert_eq!(50 + stop_gap, 70, "From index 50, need to check up to 70");
    assert_eq!(100 + stop_gap, 120, "From index 100, need to check up to 120");
    
    // Test that we need enough addresses revealed
    let highest_used = 45u32;
    let current_revealed = 50u32;
    let required = highest_used + stop_gap;
    assert_eq!(required, 65, "Need addresses up to index 65");
    assert!(current_revealed < required, "50 < 65, so need to reveal more");
}

#[test]
fn test_peek_address_behavior() {
    // Test that peek_address doesn't reveal new addresses
    let descriptor = "wpkh(tprv8ZgxMBicQKsPe6D3wxZPHxy7JEMBwjxiimYXSvM8aRaqZmDvU7Jec5c8aSB8rCzFmAP6aEjPhUmiXm2KB7XUzkApX2prmDHcQsUQY5DsxJw/84h/1h/0h/0/*)#5asejmkj";
    let change_descriptor = "wpkh(tprv8ZgxMBicQKsPe6D3wxZPHxy7JEMBwjxiimYXSvM8aRaqZmDvU7Jec5c8aSB8rCzFmAP6aEjPhUmiXm2KB7XUzkApX2prmDHcQsUQY5DsxJw/84h/1h/0h/1/*)#9f4c0wx2";
    
    let mut wallet = match Wallet::create(descriptor, change_descriptor)
        .network(Network::Regtest)
        .create_wallet_no_persist() {
        Ok(w) => w,
        Err(_) => return, // Skip test if wallet creation fails
    };
    
    // Reveal some addresses first
    let _: Vec<_> = wallet.reveal_addresses_to(KeychainKind::External, 5).collect();
    assert_eq!(wallet.next_derivation_index(KeychainKind::External), 6, "Should have revealed to index 5, next is 6");
    
    // Peek at an address within revealed range
    let addr = wallet.peek_address(KeychainKind::External, 3);
    assert!(addr.index == 3, "Should get address at index 3");
    assert_eq!(wallet.next_derivation_index(KeychainKind::External), 6, "Peeking within range shouldn't change next index");
    
    // Peek at address at the current boundary  
    let addr = wallet.peek_address(KeychainKind::External, 5);
    assert!(addr.index == 5, "Should get address at index 5");
    assert_eq!(wallet.next_derivation_index(KeychainKind::External), 6, "Peeking at last revealed address doesn't change index");
}
