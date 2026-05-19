mod common;
use common::docker_environment::IsolatedTestEnvironment;

use canary::subscription::SubscriptionTier;
use canary::wallet::WalletCreationService;
use std::time::Duration;
use tokio::time::sleep;

/// System tests for single-address watch scenarios.
#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_unused_bech32_address_watch_becomes_ready_after_empty_sync() {
    let env = IsolatedTestEnvironment::new()
        .await
        .expect("Failed to create test environment");

    let bitcoin_container = format!("test-bitcoin-{}", env.test_id);
    let unused_address = IsolatedTestEnvironment::bitcoin_cli(
        &bitcoin_container,
        &["-rpcwallet=bob", "getnewaddress", "", "bech32"],
    )
    .expect("Failed to generate unused bech32 address")
    .trim()
    .to_string();

    let wallet_creation_service = WalletCreationService::new(
        env.wallet_manager.wallet_dir.clone(),
        env.metadata_db.clone(),
        env.wallet_manager.get_electrum_client().await,
        env.wallet_manager.get_network(),
        env.wallet_manager.clone(),
    );

    let created_wallet = wallet_creation_service
        .create_wallet_non_blocking(
            "Unused address watch",
            &unused_address,
            "foss-user",
            true,
            Some("auto"),
            Some("20"),
        )
        .await
        .expect("Failed to create unused address watch");

    assert_eq!(created_wallet.status, "pending");
    assert_eq!(created_wallet.wallet_type, "address");

    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(30);
    loop {
        let wallet = env
            .metadata_db
            .get_wallet_by_checksum(&created_wallet.checksum)
            .await
            .expect("Failed to fetch address watch")
            .expect("Address watch missing from database");

        if wallet.status == "ready" {
            assert_eq!(wallet.wallet_type, "address");
            assert_eq!(wallet.balance_total, Some(0));
            assert!(
                wallet.last_synced_at.is_some(),
                "empty successful address sync should set last_synced_at"
            );

            let transactions = env
                .get_wallet_transactions(&created_wallet.checksum)
                .await
                .expect("Failed to fetch address watch transactions");
            assert!(
                transactions.is_empty(),
                "unused address watch should not create transactions"
            );
            return;
        }

        if start.elapsed() > timeout {
            panic!(
                "Address watch {} stayed {} after {:?}",
                created_wallet.checksum,
                wallet.status,
                start.elapsed()
            );
        }

        env.wallet_manager
            .sync_tier_parallel(SubscriptionTier::Team)
            .await
            .expect("Failed to sync team wallets while waiting for address watch readiness");
        sleep(Duration::from_millis(500)).await;
    }
}
