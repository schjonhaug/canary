//! Tests for the shared wallet helpers and BDK persistence round trips, plus
//! the `queue_wallet` fixture used by the sibling modules' tests.

use super::WalletEntry;
use super::address_watch_sync_targets;
use super::manager::WalletManager;
use super::persistence::{create_wallet_file, persist_wallet};
use crate::metadata::{WalletMetadata, WalletStatus, WalletType};
use crate::utils::parse_multipath_descriptor;
use bdk_wallet::rusqlite::{Connection, OptionalExtension};
use bdk_wallet::{KeychainKind, Wallet, bitcoin::Network};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::spawn_blocking;

const TWO_PATH_DESCRIPTOR: &str = "wpkh(tpubDDnGNapGEY6AZAdQbfRJgMg9fvz8pUBrLwvyvUqEgcUfgzM6zc2eVK4vY9x9L5FJWdX8WumXuLEDV5zDZnTfbn87vLe9XceCFwTu9so9Kks/<0;1>/*)";
const THREE_PATH_DESCRIPTOR: &str = "wpkh(tpubDDnGNapGEY6AZAdQbfRJgMg9fvz8pUBrLwvyvUqEgcUfgzM6zc2eVK4vY9x9L5FJWdX8WumXuLEDV5zDZnTfbn87vLe9XceCFwTu9so9Kks/<0;1;2>/*)";

pub(super) fn queue_wallet(checksum: &str, last_synced_at: Option<&str>) -> WalletMetadata {
    WalletMetadata {
        checksum: checksum.to_string(),
        name: checksum.to_string(),
        descriptor: format!("descriptor-{checksum}"),
        hex_color: "#000000".to_string(),
        created_at: "2026-08-10 00:00:00".to_string(),
        balance_total: Some(0),
        last_activity: None,
        status: WalletStatus::Ready,
        contact_count: None,
        user_id: "foss-user".to_string(),
        is_active: true,
        balance_fiat: None,
        fiat_currency: None,
        wallet_type: WalletType::Descriptor,
        last_synced_at: last_synced_at.map(str::to_string),
    }
}

#[test]
fn mixed_address_watch_group_suppresses_only_pending_watcher() {
    let mut ready = queue_wallet("ready", None);
    ready.wallet_type = WalletType::Address;
    ready.descriptor = "addr(shared-address)".to_string();

    let mut pending = ready.clone();
    pending.checksum = "pending".to_string();
    pending.status = WalletStatus::Pending;

    assert_eq!(
        address_watch_sync_targets(&[ready, pending]),
        vec![("ready".to_string(), false), ("pending".to_string(), true)]
    );
}

fn create_persisted_test_wallet(path: &Path) {
    let mut conn = Connection::open(path).unwrap();
    let mut wallet = Wallet::create_from_two_path_descriptor(TWO_PATH_DESCRIPTOR)
        .network(Network::Regtest)
        .create_wallet(&mut conn)
        .unwrap();
    let _: Vec<_> = wallet
        .reveal_addresses_to(KeychainKind::External, 7)
        .collect();
    let _: Vec<_> = wallet
        .reveal_addresses_to(KeychainKind::Internal, 4)
        .collect();
    assert!(wallet.persist(&mut conn).unwrap());
}

#[test]
fn bdk_two_path_creation_matches_canarys_former_split() {
    let (receive_descriptor, change_descriptor) =
        parse_multipath_descriptor(TWO_PATH_DESCRIPTOR).unwrap();
    let legacy_wallet = Wallet::create(receive_descriptor, change_descriptor)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .unwrap();
    let native_wallet = Wallet::create_from_two_path_descriptor(TWO_PATH_DESCRIPTOR)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .unwrap();

    for keychain in [KeychainKind::External, KeychainKind::Internal] {
        assert_eq!(
            legacy_wallet.public_descriptor(keychain),
            native_wallet.public_descriptor(keychain)
        );
        for index in [0, 1, 20] {
            assert_eq!(
                legacy_wallet.peek_address(keychain, index).address,
                native_wallet.peek_address(keychain, index).address
            );
        }
    }
}

#[test]
fn bdk_two_path_creation_rejects_wrong_path_shapes() {
    let non_multipath = TWO_PATH_DESCRIPTOR.replace("<0;1>", "0");
    assert!(
        Wallet::create_from_two_path_descriptor(non_multipath)
            .network(Network::Regtest)
            .create_wallet_no_persist()
            .is_err()
    );
    assert!(
        Wallet::create_from_two_path_descriptor(THREE_PATH_DESCRIPTOR)
            .network(Network::Regtest)
            .create_wallet_no_persist()
            .is_err()
    );

    let hardened_after_xpub = TWO_PATH_DESCRIPTOR.replace("/<0;1>/*", "/84h/<0;1>/*");
    assert!(parse_multipath_descriptor(&hardened_after_xpub).is_err());
}

#[tokio::test]
async fn locked_wallet_persists_on_the_blocking_pool_and_reloads() {
    let temp_dir = tempfile::tempdir().unwrap();
    let wallet_path = temp_dir.path().join("wallet.sqlite");
    let creation_path = wallet_path.clone();
    let state = spawn_blocking(move || {
        create_wallet_file(
            &creation_path,
            TWO_PATH_DESCRIPTOR.to_string(),
            Network::Regtest,
        )
    })
    .await
    .unwrap()
    .unwrap();
    let entry: WalletEntry = Arc::new(Mutex::new(state));

    let mut guard = entry.clone().lock_owned().await;
    let _: Vec<_> = guard
        .0
        .reveal_addresses_to(KeychainKind::External, 5)
        .collect();
    let guard = persist_wallet(guard).await.unwrap();
    assert!(guard.0.staged().is_none());
    drop(guard);
    drop(entry);

    let (wallet, _conn) =
        WalletManager::load_wallet_from_disk(&wallet_path, TWO_PATH_DESCRIPTOR, Network::Regtest)
            .await
            .unwrap();
    assert_eq!(wallet.derivation_index(KeychainKind::External), Some(5));
}

#[tokio::test]
async fn persisted_wallet_load_checks_descriptor_and_network() {
    let temp_dir = tempfile::tempdir().unwrap();
    let wallet_path = temp_dir.path().join("wallet.sqlite");
    create_persisted_test_wallet(&wallet_path);

    let (wallet, conn) =
        WalletManager::load_wallet_from_disk(&wallet_path, TWO_PATH_DESCRIPTOR, Network::Regtest)
            .await
            .unwrap();
    assert_eq!(wallet.network(), Network::Regtest);
    drop(wallet);
    drop(conn);

    let mismatching_descriptor = TWO_PATH_DESCRIPTOR.replace("<0;1>", "<1;0>");
    assert!(
        WalletManager::load_wallet_from_disk(
            &wallet_path,
            &mismatching_descriptor,
            Network::Regtest,
        )
        .await
        .is_err()
    );
    assert!(
        WalletManager::load_wallet_from_disk(&wallet_path, TWO_PATH_DESCRIPTOR, Network::Bitcoin,)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn bdk_v2_sqlite_wallet_migrates_and_reloads() {
    let temp_dir = tempfile::tempdir().unwrap();
    let wallet_path = temp_dir.path().join("wallet.sqlite");
    create_persisted_test_wallet(&wallet_path);

    // bdk_wallet 2.3 used wallet schema v0. Recreate that state from the otherwise
    // identical persisted data so this test remains self-contained.
    {
        let conn = Connection::open(&wallet_path).unwrap();
        conn.execute_batch(
            "DROP TABLE bdk_wallet_locked_outpoints;
                 UPDATE bdk_schemas SET version = 0 WHERE name = 'bdk_wallet';",
        )
        .unwrap();
        let lock_table: Option<String> = conn
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'bdk_wallet_locked_outpoints'",
                    [],
                    |row| row.get(0),
                )
                .optional()
                .unwrap();
        assert!(lock_table.is_none());
    }

    let (wallet, conn) =
        WalletManager::load_wallet_from_disk(&wallet_path, TWO_PATH_DESCRIPTOR, Network::Regtest)
            .await
            .unwrap();

    assert_eq!(wallet.network(), Network::Regtest);
    assert_eq!(wallet.derivation_index(KeychainKind::External), Some(7));
    assert_eq!(wallet.derivation_index(KeychainKind::Internal), Some(4));
    let (receive_descriptor, change_descriptor) =
        parse_multipath_descriptor(TWO_PATH_DESCRIPTOR).unwrap();
    assert_eq!(
        wallet.public_descriptor(KeychainKind::External).to_string(),
        receive_descriptor
    );
    assert_eq!(
        wallet.public_descriptor(KeychainKind::Internal).to_string(),
        change_descriptor
    );
    assert_eq!(
        conn.query_row(
            "SELECT version FROM bdk_schemas WHERE name = 'bdk_wallet'",
            [],
            |row| row.get::<_, u32>(0),
        )
        .unwrap(),
        1
    );
    assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'bdk_wallet_locked_outpoints'",
                [],
                |row| row.get::<_, u32>(0),
            )
            .unwrap(),
            1
        );
    drop(wallet);
    drop(conn);

    let (reloaded, _conn) =
        WalletManager::load_wallet_from_disk(&wallet_path, TWO_PATH_DESCRIPTOR, Network::Regtest)
            .await
            .unwrap();
    assert_eq!(reloaded.derivation_index(KeychainKind::External), Some(7));
    assert_eq!(reloaded.derivation_index(KeychainKind::Internal), Some(4));
}
