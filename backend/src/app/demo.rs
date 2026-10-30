//! Seeds the public demo account with example wallets.
//!
//! The demo user exists only on hosted instances. Seeding is idempotent: each
//! wallet is created once, through the normal wallet creation flow, and later
//! startups leave it alone.

use crate::api::AppServices;
use crate::metadata::{BalanceAlertType, CreateBalanceAlertInput, ProviderType, UserRecord};
use bdk_wallet::bitcoin::Network;
use tracing::{error, info, warn};

const DEMO_USER_EMAIL: &str = "demo@canarybitcoin.com";

const BACON_WALLET_NAME: &str = "Bacon Wallet";
/// Funded by `docker-utils.sh` in the regtest environment.
const BACON_REGTEST_DESCRIPTOR: &str = "wpkh([9a6a2580/84h/1h/0h]tpubDCMRAYcH71Gagskm7E5peNMYB5sKaLLwtn2c4Rb3CMUTRVUk5dkpsskhspa5MEcVZ11LwTcM7R5mzndUCG9WabYcT5hfQHbYVoaLFBZHPCi/<0;1>/*)#4laqdwct";
/// A real mainnet wallet with transaction history.
const BACON_MAINNET_DESCRIPTOR: &str = "wpkh([00000000/84h/0h/0h]xpub6DEzNop46vmxR49zYWFnMwmEfawSNmAMf6dLH5YKDY463twtvw1XD7ihwJRLPRGZJz799VPFzXHpZu6WdhT29WnaeuChS6aZHZPFmqczR5K/<0;1>/*)#4jhrljfg";

const GENESIS_WALLET_NAME: &str = "Satoshi (Genesis)";
/// The genesis block coinbase public key. Watching the P2PK output instead of
/// the P2PKH address avoids the tens of thousands of donation transactions
/// sent to that address, which overwhelm Electrum sync.
const GENESIS_PUBKEY: &str = "04678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5f";
const RETIRED_DEMO_WALLETS: [&str; 3] = [
    "Satoshi (First Reward)",
    "Satoshi (Hal Finney Tx)",
    "Satoshi (Patoshi)",
];

/// Creates the demo wallets that are missing for `network`.
pub async fn seed_demo_wallets(app_services: &AppServices, network: Network) {
    let Ok(Some(demo_user)) = app_services
        .metadata_db
        .get_user_by_email(DEMO_USER_EMAIL)
        .await
    else {
        return;
    };

    let bacon_descriptor = match network {
        Network::Regtest => Some(BACON_REGTEST_DESCRIPTOR),
        Network::Bitcoin => Some(BACON_MAINNET_DESCRIPTOR),
        _ => None,
    };
    if let Some(descriptor) = bacon_descriptor {
        seed_bacon_wallet(app_services, &demo_user, descriptor).await;
    }

    if network == Network::Bitcoin {
        seed_genesis_wallet(app_services, &demo_user).await;
    }
}

/// Creates the Bacon wallet with a demo contact and two balance alerts.
async fn seed_bacon_wallet(app_services: &AppServices, demo_user: &UserRecord, descriptor: &str) {
    // Match by name rather than descriptor to avoid BDK normalization differences.
    let existing_wallets = app_services
        .metadata_db
        .get_wallets_for_user(Some(&demo_user.id))
        .await
        .unwrap_or_default();
    if existing_wallets.iter().any(|w| w.name == BACON_WALLET_NAME) {
        return;
    }

    let wallet = match app_services
        .wallet_creation_service
        .create_wallet_non_blocking(
            BACON_WALLET_NAME,
            descriptor,
            &demo_user.id,
            false,
            None,
            None,
        )
        .await
    {
        Ok(wallet) => wallet,
        Err(e) => {
            error!("Failed to create demo Bacon wallet: {e}");
            return;
        }
    };
    info!("Created demo Bacon wallet");

    let contact_id = match app_services
        .metadata_db
        .insert_contact_with_notification_methods(
            &wallet.checksum,
            "Canary Wallet",
            vec![(ProviderType::Email, "contact@canarybitcoin.com".to_string())],
        )
        .await
    {
        Ok(contact_id) => contact_id,
        Err(e) => {
            warn!("Skipping demo balance alerts: failed to add demo contact: {e}");
            return;
        }
    };

    let alerts = [
        (0, BalanceAlertType::Equals, "balance equals 0 BTC"),
        (
            21_000_000,
            BalanceAlertType::Above,
            "balance above 0.21 BTC",
        ),
    ];
    for (threshold_sats, alert_type, description) in alerts {
        let result = app_services
            .metadata_db
            .create_balance_alert_with_contact(CreateBalanceAlertInput {
                wallet_checksum: &wallet.checksum,
                contact_id: Some(&contact_id),
                threshold_sats,
                alert_type,
                threshold_currency: None,
                threshold_fiat_amount: None,
                current_balance_sats: None,
            })
            .await;
        if let Err(e) = result {
            error!("Failed to add demo alert ({description}): {e}");
        }
    }
}

/// Watches the mainnet genesis coinbase and removes retired demo wallets.
async fn seed_genesis_wallet(app_services: &AppServices, demo_user: &UserRecord) {
    let existing_wallets = app_services
        .metadata_db
        .get_wallets_for_user(Some(&demo_user.id))
        .await
        .unwrap_or_default();

    let mut genesis_cleanup_failed = false;
    for wallet in &existing_wallets {
        // Earlier versions watched the genesis P2PKH address with an addr() descriptor.
        let is_old_genesis =
            wallet.name == GENESIS_WALLET_NAME && wallet.descriptor.starts_with("addr(");
        if !is_old_genesis && !RETIRED_DEMO_WALLETS.contains(&wallet.name.as_str()) {
            continue;
        }
        info!("Removing retired demo wallet '{}'", wallet.name);
        if let Err(e) = app_services
            .metadata_db
            .hard_delete_wallet_by_checksum(&wallet.checksum)
            .await
        {
            error!(
                "Failed to delete retired demo wallet '{}': {e}",
                wallet.name
            );
            genesis_cleanup_failed |= is_old_genesis;
        }
    }

    let already_exists = existing_wallets
        .iter()
        .any(|w| w.name == GENESIS_WALLET_NAME && w.descriptor.starts_with("pk("));
    if already_exists || genesis_cleanup_failed {
        return;
    }

    match app_services
        .wallet_creation_service
        .create_wallet_non_blocking(
            GENESIS_WALLET_NAME,
            GENESIS_PUBKEY,
            &demo_user.id,
            false,
            None,
            None,
        )
        .await
    {
        Ok(_) => info!("Created demo wallet {GENESIS_WALLET_NAME}"),
        Err(e) => error!("Failed to create demo wallet {GENESIS_WALLET_NAME}: {e}"),
    }
}
