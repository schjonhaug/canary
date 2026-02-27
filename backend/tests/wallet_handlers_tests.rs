//! Integration tests for wallet HTTP handlers
//!
//! Tests use self-hosted mode which provides a hardcoded admin user without JWT authentication.
//! Run with: cargo test --test wallet_handlers_tests -- --test-threads=1

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use canary::{
    api::{create_router_with_services, AppServices},
    config::{AppConfig, NetworkConfig, OperatingMode},
    electrum::ElectrumClientManager,
    notifications::NotificationManager,
    wallet::{WalletCreationService, WalletManager},
    WalletDetailResponse, WalletMetadata, WalletsListResponse,
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::sync::Arc;
use tempfile::{tempdir, TempDir};
use tokio::sync::{broadcast, Mutex};
use tower::ServiceExt;

// Test data - valid testnet descriptor from system tests
const VALID_TESTNET_DESCRIPTOR: &str = "wpkh(tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5/<0;1>/*)";

// Valid testnet XPUB (same key, no descriptor wrapper)
const VALID_TESTNET_XPUB: &str = "tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5";

// Mainnet XPUB for network mismatch test
const MAINNET_XPUB: &str =
    "zpub6rFR7y4Q2AijBEqTUquhVz398htDFrtymD9xYYfG1m4wAcvPhXNfE3EfH1r1ADqtfSdVCToUG868RvUUkgDKf31mGDtKsAYz2oz2AGutZYs";

/// Test helper to create test application with self-hosted mode
/// Returns (router, temp_dir) - temp_dir must be kept alive for test duration
async fn create_test_app() -> (axum::Router, TempDir) {
    let temp_dir = tempdir().unwrap();
    let temp_path = temp_dir.path().to_str().unwrap();
    let test_db_path = format!("{}/test_metadata.sqlite", temp_path);

    // Create test config with self-hosted mode (no auth required)
    let test_config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        temp_path.to_string(),
        OperatingMode::SelfHosted, // Self-hosted = hardcoded admin user
        None,
        None, // No JWT secret needed for self-hosted mode
    );

    let (event_tx, _event_rx) =
        broadcast::channel::<canary::metadata::TransactionNotification>(100);
    let _current_block_header = Arc::new(Mutex::new(None::<canary::electrum::BlockHeader>));

    let wallet_manager = Arc::new(
        WalletManager::new(
            event_tx.clone(),
            temp_path.into(),
            &test_db_path,
            bdk_wallet::bitcoin::Network::Regtest,
            "tcp://127.0.0.1:50001", // Test electrum (not connected)
            &test_config,
        )
        .await,
    );

    // Create AppServices for non-blocking architecture
    let app_services = {
        let electrum_client = wallet_manager.get_electrum_client().await;
        let wallet_creation_service = WalletCreationService::new(
            wallet_manager.wallet_dir.clone(),
            wallet_manager.metadata_db.clone(),
            electrum_client,
            wallet_manager.get_network(),
            wallet_manager.clone(),
        );
        Arc::new(AppServices {
            metadata_db: wallet_manager.metadata_db.clone(),
            wallet_creation_service,
        })
    };

    let notification_manager = Arc::new(Mutex::new(NotificationManager::new()));
    let stripe_billing = None;
    // Use mock electrum manager that reports as connected (for testing without real server)
    let electrum_manager = Some(Arc::new(ElectrumClientManager::new_mock_connected()));

    let router = create_router_with_services(
        app_services,
        notification_manager,
        stripe_billing,
        test_config,
        electrum_manager,
    );

    (router, temp_dir)
}

/// Helper to parse response body as JSON
async fn body_to_json(body: Body) -> Value {
    let bytes = body.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

// =============================================================================
// POST /api/wallets - Create Wallet Tests
// =============================================================================

#[tokio::test]
async fn test_create_wallet_success() {
    let (app, _temp_dir) = create_test_app().await;

    let request = Request::builder()
        .uri("/api/wallets")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "Test Wallet",
                "descriptor": VALID_TESTNET_DESCRIPTOR
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "Expected 201 CREATED for valid wallet creation"
    );

    let body = body_to_json(response.into_body()).await;
    assert!(
        body.get("message").is_some(),
        "Response should have message"
    );
    assert!(body.get("wallet").is_some(), "Response should have wallet");

    let wallet = &body["wallet"];
    assert_eq!(wallet["name"], "Test Wallet");
    assert_eq!(wallet["status"], "pending", "New wallet should be pending");
}

#[tokio::test]
async fn test_create_wallet_duplicate_descriptor() {
    let (app, _temp_dir) = create_test_app().await;

    // Create first wallet
    let create_request = json!({
        "name": "First Wallet",
        "descriptor": VALID_TESTNET_DESCRIPTOR
    });

    let request = Request::builder()
        .uri("/api/wallets")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(create_request.to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // Try to create duplicate wallet
    let request = Request::builder()
        .uri("/api/wallets")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "Duplicate Wallet",
                "descriptor": VALID_TESTNET_DESCRIPTOR
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "Expected 409 CONFLICT for duplicate descriptor"
    );

    let body = body_to_json(response.into_body()).await;
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("already been added"),
        "Error should mention wallet already exists"
    );
    assert_eq!(
        body["error_code"].as_str().unwrap(),
        "wallet_already_exists",
        "Error should include wallet_already_exists error code"
    );
}

#[tokio::test]
async fn test_create_wallet_network_mismatch() {
    let (app, _temp_dir) = create_test_app().await;

    // Try to create wallet with mainnet key on regtest server
    let request = Request::builder()
        .uri("/api/wallets")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "Mainnet Wallet",
                "descriptor": MAINNET_XPUB,
                "script_type": "p2wpkh"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected 400 BAD_REQUEST for network mismatch"
    );

    let body = body_to_json(response.into_body()).await;
    let error = body["error"].as_str().unwrap();
    assert!(
        error.contains("mainnet") || error.contains("network"),
        "Error should mention network mismatch: {}",
        error
    );
}

#[tokio::test]
async fn test_create_wallet_invalid_descriptor() {
    let (app, _temp_dir) = create_test_app().await;

    let request = Request::builder()
        .uri("/api/wallets")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "Invalid Wallet",
                "descriptor": "not_a_valid_descriptor"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected 400 BAD_REQUEST for invalid descriptor"
    );
}

#[tokio::test]
async fn test_create_wallet_xpub_fresh_no_script_type() {
    let (app, _temp_dir) = create_test_app().await;

    // Fresh XPUB wallet without script_type should fail
    let request = Request::builder()
        .uri("/api/wallets")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "Fresh XPUB Wallet",
                "descriptor": VALID_TESTNET_XPUB,
                "is_fresh_wallet": true
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected 400 BAD_REQUEST for fresh XPUB without script_type"
    );

    let body = body_to_json(response.into_body()).await;
    let error = body["error"].as_str().unwrap();
    assert!(
        error.contains("script_type") || error.contains("Script type"),
        "Error should mention script_type requirement: {}",
        error
    );
}

#[tokio::test]
async fn test_create_wallet_custom_stop_gap_without_script_type() {
    let (app, _temp_dir) = create_test_app().await;

    // Custom stop_gap with XPUB requires explicit script_type
    let request = Request::builder()
        .uri("/api/wallets")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "Stop Gap Wallet",
                "descriptor": VALID_TESTNET_XPUB,
                "stop_gap": "500"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected 400 BAD_REQUEST for custom stop_gap without script_type"
    );

    let body = body_to_json(response.into_body()).await;
    let error = body["error"].as_str().unwrap();
    assert!(
        error.to_lowercase().contains("script")
            || error.to_lowercase().contains("stop_gap")
            || error.to_lowercase().contains("stop gap"),
        "Error should mention script_type or stop_gap requirement: {}",
        error
    );
}

// =============================================================================
// GET /api/wallets - List Wallets Tests
// =============================================================================

#[tokio::test]
async fn test_get_wallets_list_empty() {
    let (app, _temp_dir) = create_test_app().await;

    let request = Request::builder()
        .uri("/api/wallets")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let list: WalletsListResponse = serde_json::from_slice(&bytes).unwrap();

    assert!(list.wallets.is_empty(), "Should return empty wallet list");
    assert!(list.timestamp > 0, "Should have valid timestamp");
}

#[tokio::test]
async fn test_get_wallets_list_with_wallets() {
    let (app, _temp_dir) = create_test_app().await;

    // Create a wallet first
    let request = Request::builder()
        .uri("/api/wallets")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "My Wallet",
                "descriptor": VALID_TESTNET_DESCRIPTOR
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // Get wallets list
    let request = Request::builder()
        .uri("/api/wallets")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let list: WalletsListResponse = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(list.wallets.len(), 1, "Should have one wallet");
    assert_eq!(list.wallets[0].name, "My Wallet");
}

// =============================================================================
// GET /api/wallets/{checksum} - Get Wallet Tests
// =============================================================================

#[tokio::test]
async fn test_get_wallet_success() {
    let (app, _temp_dir) = create_test_app().await;

    // Create a wallet
    let request = Request::builder()
        .uri("/api/wallets")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "Test Wallet",
                "descriptor": VALID_TESTNET_DESCRIPTOR
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = body_to_json(response.into_body()).await;
    let checksum = body["wallet"]["checksum"].as_str().unwrap();

    // Get wallet by checksum
    let request = Request::builder()
        .uri(format!("/api/wallets/{}", checksum))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let wallet: WalletMetadata = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(wallet.checksum, checksum);
    assert_eq!(wallet.name, "Test Wallet");
}

#[tokio::test]
async fn test_get_wallet_not_found() {
    let (app, _temp_dir) = create_test_app().await;

    let request = Request::builder()
        .uri("/api/wallets/nonexistent_checksum")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Expected 404 NOT_FOUND for nonexistent wallet"
    );
}

// =============================================================================
// GET /api/wallets/{checksum}/detail - Get Wallet Detail Tests
// =============================================================================

#[tokio::test]
async fn test_get_wallet_detail_success() {
    let (app, _temp_dir) = create_test_app().await;

    // Create a wallet
    let request = Request::builder()
        .uri("/api/wallets")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "Detail Test Wallet",
                "descriptor": VALID_TESTNET_DESCRIPTOR
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = body_to_json(response.into_body()).await;
    let checksum = body["wallet"]["checksum"].as_str().unwrap();

    // Get wallet detail
    let request = Request::builder()
        .uri(format!("/api/wallets/{}/detail", checksum))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let detail: WalletDetailResponse = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(detail.wallet.checksum, checksum);
    assert_eq!(detail.wallet.name, "Detail Test Wallet");
    assert!(detail.timestamp > 0);
    // Pending wallet should have empty transactions
    assert!(
        detail.transactions.is_empty(),
        "Pending wallet should have no transactions"
    );
}

#[tokio::test]
async fn test_get_wallet_detail_not_found() {
    let (app, _temp_dir) = create_test_app().await;

    let request = Request::builder()
        .uri("/api/wallets/nonexistent_checksum/detail")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Expected 404 NOT_FOUND for nonexistent wallet detail"
    );
}

// =============================================================================
// PUT /api/wallets/{checksum} - Update Wallet Tests
// =============================================================================

#[tokio::test]
async fn test_update_wallet_success() {
    let (app, _temp_dir) = create_test_app().await;

    // Create a wallet
    let request = Request::builder()
        .uri("/api/wallets")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "Original Name",
                "descriptor": VALID_TESTNET_DESCRIPTOR
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = body_to_json(response.into_body()).await;
    let checksum = body["wallet"]["checksum"].as_str().unwrap();

    // Update wallet name
    let request = Request::builder()
        .uri(format!("/api/wallets/{}", checksum))
        .method("PUT")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "Updated Name"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Verify the update
    let request = Request::builder()
        .uri(format!("/api/wallets/{}", checksum))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let wallet: WalletMetadata = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(wallet.name, "Updated Name");
}

#[tokio::test]
async fn test_update_wallet_empty_name() {
    let (app, _temp_dir) = create_test_app().await;

    // Create a wallet
    let request = Request::builder()
        .uri("/api/wallets")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "Test Wallet",
                "descriptor": VALID_TESTNET_DESCRIPTOR
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = body_to_json(response.into_body()).await;
    let checksum = body["wallet"]["checksum"].as_str().unwrap();

    // Try to update with empty name
    let request = Request::builder()
        .uri(format!("/api/wallets/{}", checksum))
        .method("PUT")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": ""
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected 400 BAD_REQUEST for empty name"
    );
}

#[tokio::test]
async fn test_update_wallet_not_found() {
    let (app, _temp_dir) = create_test_app().await;

    let request = Request::builder()
        .uri("/api/wallets/nonexistent_checksum")
        .method("PUT")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "New Name"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Expected 404 NOT_FOUND for nonexistent wallet update"
    );
}

// =============================================================================
// DELETE /api/wallets/{checksum} - Delete Wallet Tests
// =============================================================================

#[tokio::test]
async fn test_delete_wallet_success() {
    let (app, _temp_dir) = create_test_app().await;

    // Create a wallet
    let request = Request::builder()
        .uri("/api/wallets")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "Wallet to Delete",
                "descriptor": VALID_TESTNET_DESCRIPTOR
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = body_to_json(response.into_body()).await;
    let checksum = body["wallet"]["checksum"].as_str().unwrap().to_string();

    // Delete the wallet (soft delete)
    let request = Request::builder()
        .uri(format!("/api/wallets/{}", checksum))
        .method("DELETE")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::NO_CONTENT,
        "Expected 204 NO_CONTENT for successful delete"
    );

    // Soft deleted wallet is still visible until background sync removes it
    // It should be marked with status: 'deleted'
    // Initial delay to allow async database operations to complete in CI environments
    // CI runners under load may need more time for SQLite writes to propagate
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Use retry loop to handle potential async timing issues in CI environments
    const MAX_ATTEMPTS: usize = 20;
    const RETRY_DELAY: tokio::time::Duration = tokio::time::Duration::from_millis(100);
    let mut wallet_status = String::new();

    for attempt in 1..=MAX_ATTEMPTS {
        let request = Request::builder()
            .uri(format!("/api/wallets/{}", checksum))
            .body(Body::empty())
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "Soft deleted wallet should still be accessible"
        );

        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let wallet: WalletMetadata = serde_json::from_slice(&bytes).unwrap();
        wallet_status = wallet.status.clone();

        if wallet_status == "deleted" {
            break;
        }

        if attempt < MAX_ATTEMPTS {
            // Small delay before retrying to allow database state to propagate
            tokio::time::sleep(RETRY_DELAY).await;
        }
    }

    assert_eq!(
        wallet_status, "deleted",
        "Soft deleted wallet should have status: 'deleted'"
    );
}

#[tokio::test]
async fn test_delete_wallet_not_found() {
    let (app, _temp_dir) = create_test_app().await;

    let request = Request::builder()
        .uri("/api/wallets/nonexistent_checksum")
        .method("DELETE")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Expected 404 NOT_FOUND for nonexistent wallet delete"
    );
}

// =============================================================================
// POST /api/wallets - Address Wallet Tests
// =============================================================================

/// Derive a regtest address of the given script type from the test tpub.
fn derive_regtest_address(script_type: &str, index: u32) -> String {
    use bdk_wallet::keys::DescriptorPublicKey;
    use miniscript::descriptor::Descriptor;

    let tpub = VALID_TESTNET_XPUB;

    let desc_str = match script_type {
        "p2pkh" => format!("pkh({}/0/*)", tpub),
        "p2sh" => format!("sh(wpkh({}/0/*))", tpub),
        "p2wpkh" => format!("wpkh({}/0/*)", tpub),
        "p2tr" => format!("tr({}/0/*)", tpub),
        _ => panic!("unsupported script type: {}", script_type),
    };

    let desc: Descriptor<DescriptorPublicKey> = desc_str.parse().unwrap();
    let derived = desc
        .at_derivation_index(index)
        .expect("derivation should succeed");
    let addr = derived
        .address(bdk_wallet::bitcoin::Network::Regtest)
        .expect("address derivation should succeed");
    addr.to_string()
}

#[tokio::test]
async fn test_create_address_wallet_p2wpkh() {
    let (app, _temp_dir) = create_test_app().await;
    let addr = derive_regtest_address("p2wpkh", 0);

    let request = Request::builder()
        .uri("/api/wallets")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "SegWit Address",
                "descriptor": addr
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "Expected 201 CREATED for P2WPKH address wallet"
    );

    let body = body_to_json(response.into_body()).await;
    let wallet = &body["wallet"];
    assert_eq!(wallet["wallet_type"], "address");
    assert_eq!(wallet["status"], "ready", "Address wallet should be ready immediately");
}

#[tokio::test]
async fn test_create_address_wallet_p2tr() {
    let (app, _temp_dir) = create_test_app().await;
    let addr = derive_regtest_address("p2tr", 0);

    let request = Request::builder()
        .uri("/api/wallets")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "Taproot Address",
                "descriptor": addr
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "Expected 201 CREATED for P2TR address wallet"
    );

    let body = body_to_json(response.into_body()).await;
    let wallet = &body["wallet"];
    assert_eq!(wallet["wallet_type"], "address");
    assert_eq!(wallet["status"], "ready", "Address wallet should be ready immediately");
}

#[tokio::test]
async fn test_create_address_wallet_p2pkh() {
    let (app, _temp_dir) = create_test_app().await;
    let addr = derive_regtest_address("p2pkh", 0);

    let request = Request::builder()
        .uri("/api/wallets")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "Legacy Address",
                "descriptor": addr
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "Expected 201 CREATED for P2PKH address wallet"
    );

    let body = body_to_json(response.into_body()).await;
    let wallet = &body["wallet"];
    assert_eq!(wallet["wallet_type"], "address");
    assert_eq!(wallet["status"], "ready", "Address wallet should be ready immediately");
}

#[tokio::test]
async fn test_create_address_wallet_p2sh() {
    let (app, _temp_dir) = create_test_app().await;
    let addr = derive_regtest_address("p2sh", 0);

    let request = Request::builder()
        .uri("/api/wallets")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "P2SH Address",
                "descriptor": addr
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "Expected 201 CREATED for P2SH address wallet"
    );

    let body = body_to_json(response.into_body()).await;
    let wallet = &body["wallet"];
    assert_eq!(wallet["wallet_type"], "address");
    assert_eq!(wallet["status"], "ready", "Address wallet should be ready immediately");
}

#[tokio::test]
async fn test_create_address_wallet_duplicate_address() {
    let (app, _temp_dir) = create_test_app().await;
    let addr = derive_regtest_address("p2wpkh", 5);

    // Create first address wallet
    let request = Request::builder()
        .uri("/api/wallets")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "First Address Watch",
                "descriptor": addr
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // Try to create duplicate
    let request = Request::builder()
        .uri("/api/wallets")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "Duplicate Address Watch",
                "descriptor": addr
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "Expected 409 CONFLICT for duplicate address"
    );

    let body = body_to_json(response.into_body()).await;
    let error = body["error"].as_str().unwrap();
    assert!(
        error.contains("already being watched"),
        "Error should mention address is already being watched: {}",
        error
    );
}

#[tokio::test]
async fn test_create_address_wallet_network_mismatch() {
    let (app, _temp_dir) = create_test_app().await;

    // Use a mainnet P2WPKH address on regtest app
    let mainnet_addr = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4";

    let request = Request::builder()
        .uri("/api/wallets")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "Mainnet Address",
                "descriptor": mainnet_addr
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected 400 BAD_REQUEST for mainnet address on regtest"
    );

    let body = body_to_json(response.into_body()).await;
    let error = body["error"].as_str().unwrap();
    assert!(
        error.contains("regtest") || error.contains("network"),
        "Error should mention network mismatch: {}",
        error
    );
}
