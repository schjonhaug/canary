use canary::config::{AppConfig, NetworkConfig, OperatingMode};
use canary::metadata::{EventType, MetadataDb, TransactionNotification};
use canary::subscription::SubscriptionTier;
use canary::wallet::WalletManager;
use std::process::Command;
use std::time::Duration;
use tempfile::tempdir;
use tokio::sync::broadcast;
use tokio::time::sleep;

/// Debug test to understand why wallet drain events aren't being created
#[tokio::test]
#[ignore]
async fn debug_wallet_drain_detection() {
    println!("🔍 Starting wallet drain debug test...");

    // Create temporary directory for test data
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_string_lossy().to_string();

    // Create test database
    let db_path = temp_dir.path().join("test.db");
    let test_config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        temp_path.clone(),
        OperatingMode::SelfHosted,
        None,
        None, // No JWT secret needed for self-hosted mode
    );

    let metadata_db = MetadataDb::new(db_path.to_str().unwrap(), &test_config)
        .await
        .expect("Failed to create metadata db");

    // Create test user
    let test_user_id = metadata_db
        .create_user(
            "debug@example.com",
            "hashedpassword",
            Some("Debug User"),
            true,
            None, // preferred_currency
            None, // preferred_language
        )
        .await
        .expect("Failed to create user");

    // Create wallet manager
    let wallet_dir = temp_dir.path().join("wallets");
    std::fs::create_dir_all(&wallet_dir).expect("Failed to create wallet dir");

    let (event_sender, _event_receiver) = broadcast::channel::<TransactionNotification>(100);

    let wallet_manager = WalletManager::new(
        event_sender,
        wallet_dir,
        &db_path.to_string_lossy(),
        bdk_wallet::bitcoin::Network::Regtest,
        "tcp://127.0.0.1:50001",
        &test_config,
    )
    .await;

    // Use a real test descriptor (simplified)
    let test_descriptor =
        "wpkh(tprv8ZgxMBicQKsPeQXeTomURYacnjTRfqFN1IsMyHQHBkDSm/84'/1'/0'/0/*)#test123";

    let bob_checksum = metadata_db
        .insert_wallet("DebugBob", test_descriptor, &test_user_id)
        .await
        .expect("Failed to insert wallet");

    println!("✅ Created test wallet with checksum: {}", bob_checksum);

    // Fund Bob with 0.1 BTC from miner
    println!("💰 Funding Bob's test wallet...");
    let output = Command::new("../regtest-env/docker-utils.sh")
        .args(["miner", "sent", "bob", "0.1"])
        .output()
        .expect("Failed to execute miner fund command");

    println!(
        "Fund command output: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    // Wait and sync to ensure funding is detected
    sleep(Duration::from_millis(3000)).await;
    let _ = wallet_manager
        .sync_tier_parallel(SubscriptionTier::Team)
        .await;
    sleep(Duration::from_millis(2000)).await;

    // Check initial events
    let initial_events = metadata_db
        .get_transactions_by_wallet_checksum(&bob_checksum, None, false)
        .await
        .expect("Failed to get initial events");

    println!("📊 Initial events for Bob: {} events", initial_events.len());
    for event in &initial_events {
        println!(
            "   - {:?}: {} sats, confirmed: {}",
            event.transaction_type,
            event.amount_sats,
            event.confirmed_at.is_some()
        );
    }

    // Now drain the wallet
    println!("🔥 Draining Bob's wallet...");
    let drain_output = Command::new("../regtest-env/docker-utils.sh")
        .args(["bob", "sending", "alice", "max"])
        .output()
        .expect("Failed to execute drain command");

    println!(
        "Drain command output: {}",
        String::from_utf8_lossy(&drain_output.stdout)
    );

    // Wait and sync to detect the drain
    sleep(Duration::from_millis(3000)).await;
    let _ = wallet_manager
        .sync_tier_parallel(SubscriptionTier::Team)
        .await;
    sleep(Duration::from_millis(2000)).await;

    // Check events after drain
    let post_drain_events = metadata_db
        .get_transactions_by_wallet_checksum(&bob_checksum, None, false)
        .await
        .expect("Failed to get post-drain events");

    println!(
        "📊 Post-drain events for Bob: {} events",
        post_drain_events.len()
    );
    for event in &post_drain_events {
        println!(
            "   - {:?}: {} sats, confirmed: {}",
            event.transaction_type,
            event.amount_sats,
            event.confirmed_at.is_some()
        );
    }

    let new_events = post_drain_events.len() - initial_events.len();
    println!("📈 New events created: {}", new_events);

    // Look specifically for send events
    let send_events: Vec<_> = post_drain_events
        .iter()
        .filter(|e| e.transaction_type == EventType::Send)
        .collect();

    println!("💸 Send events found: {}", send_events.len());
    for event in &send_events {
        println!(
            "   - Send: {} sats, confirmed: {}",
            event.amount_sats,
            event.confirmed_at.is_some()
        );
    }

    if send_events.is_empty() {
        println!("❌ NO SEND EVENTS FOUND - This confirms the wallet drain detection bug!");
        println!("   The transaction was sent but no events were created in the database.");
        println!("   This means none of the transaction detection cases (1-4) are matching.");
    } else {
        println!("✅ Send events were created - wallet drain detection is working!");
    }

    // Mine the transaction to confirm it
    println!("⛏️ Mining block to confirm transaction...");
    let mine_output = Command::new("../regtest-env/docker-utils.sh")
        .args(["mine", "1"])
        .output()
        .expect("Failed to execute mine command");

    println!(
        "Mine command output: {}",
        String::from_utf8_lossy(&mine_output.stdout)
    );

    // Final sync and check
    sleep(Duration::from_millis(3000)).await;
    let _ = wallet_manager
        .sync_tier_parallel(SubscriptionTier::Team)
        .await;
    sleep(Duration::from_millis(2000)).await;

    let final_events = metadata_db
        .get_transactions_by_wallet_checksum(&bob_checksum, None, false)
        .await
        .expect("Failed to get final events");

    println!("📊 Final events for Bob: {} events", final_events.len());
    for event in &final_events {
        println!(
            "   - {:?}: {} sats, confirmed: {}",
            event.transaction_type,
            event.amount_sats,
            event.confirmed_at.is_some()
        );
    }

    println!("🎯 Debug test completed!");
    println!("   Initial events: {}", initial_events.len());
    println!("   Final events: {}", final_events.len());
    println!(
        "   Events created: {}",
        final_events.len() - initial_events.len()
    );
}
