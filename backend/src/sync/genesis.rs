//! Electrum height conventions and the Bitcoin mainnet genesis coinbase special
//! cases.
//!
//! Electrum reports height 0 for both mempool transactions and the genesis block,
//! and may leave the genesis coinbase out of a script's balance, so mainnet needs
//! typed special cases that other networks do not.

use super::address_watch::AddressWatchTxState;
use crate::metadata::EventType;
use crate::utils::amount::sats_to_i64;
use bdk_wallet::bitcoin::{
    Amount, Network, OutPoint, PublicKey, ScriptBuf, Sequence, Transaction as BitcoinTransaction,
    TxIn, TxOut, Txid, Witness, absolute, transaction,
};
use std::str::FromStr;
use std::sync::LazyLock;
use tracing::{debug, warn};

/// The Bitcoin mainnet genesis coinbase txid — the only confirmed transaction at block height 0.
/// Electrum returns height=0 for both mempool transactions and genesis block transactions,
/// so mainnet needs a typed txid special-case while non-mainnet networks treat height=0 as
/// unconfirmed.
const MAINNET_GENESIS_COINBASE_TXID_HEX: &str =
    "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b";
static MAINNET_GENESIS_COINBASE_TXID: LazyLock<Txid> = LazyLock::new(|| {
    Txid::from_str(MAINNET_GENESIS_COINBASE_TXID_HEX)
        .expect("mainnet genesis coinbase txid must be valid")
});

/// Bitcoin mainnet genesis block timestamp (2009-01-03T18:15:05Z) as a fallback when the
/// Electrum server cannot serve the block 0 header.
pub(super) const MAINNET_GENESIS_BLOCK_TIMESTAMP: u64 = 1_231_006_505;
const MAINNET_GENESIS_COINBASE_AMOUNT_SATS: u64 = 50 * 100_000_000;
const MAINNET_GENESIS_PUBKEY_HEX: &str = "04678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5f";
static MAINNET_GENESIS_P2PK_SCRIPT: LazyLock<ScriptBuf> = LazyLock::new(|| {
    let pubkey = PublicKey::from_str(MAINNET_GENESIS_PUBKEY_HEX)
        .expect("mainnet genesis pubkey must be valid");
    ScriptBuf::new_p2pk(&pubkey)
});

/// Check if a transaction is confirmed based on Electrum's height convention.
/// height > 0: confirmed at that block height
/// height == 0: unconfirmed (mempool), EXCEPT for Bitcoin mainnet genesis coinbase
/// height < 0: unconfirmed with unconfirmed parents
pub(super) fn is_tx_confirmed(network: Network, height: i32, txid: &Txid) -> bool {
    height > 0 || is_mainnet_genesis_coinbase(network, height, txid)
}

/// Convert a confirmed Electrum height into the persisted unsigned block height.
/// Callers must only use this after `is_tx_confirmed(...)` returns true.
pub(super) fn confirmed_block_height(height: i32) -> u32 {
    debug_assert!(
        height >= 0,
        "confirmed transactions must have a non-negative block height"
    );
    u32::try_from(height).expect("confirmed transactions must have a non-negative block height")
}

pub(super) fn genesis_block_timestamp(network: Network, height: u32) -> Option<u64> {
    if matches!(network, Network::Bitcoin) && height == 0 {
        Some(MAINNET_GENESIS_BLOCK_TIMESTAMP)
    } else {
        None
    }
}

fn is_mainnet_genesis_coinbase(network: Network, height: i32, txid: &Txid) -> bool {
    height == 0 && matches!(network, Network::Bitcoin) && txid == &*MAINNET_GENESIS_COINBASE_TXID
}

pub(super) fn balance_with_mainnet_genesis_coinbase(
    network: Network,
    script: &ScriptBuf,
    electrum_balance_sats: i64,
    history: &[bdk_electrum::electrum_client::GetHistoryRes],
) -> i64 {
    if !matches!(network, Network::Bitcoin) || script != &*MAINNET_GENESIS_P2PK_SCRIPT {
        return electrum_balance_sats;
    }

    let has_genesis_coinbase = history
        .iter()
        .any(|entry| is_mainnet_genesis_coinbase(network, entry.height, &entry.tx_hash));
    let genesis_amount = sats_to_i64(MAINNET_GENESIS_COINBASE_AMOUNT_SATS);

    if !has_genesis_coinbase {
        warn!(
            "Genesis P2PK script detected but Electrum history does not include the genesis coinbase; balance may exclude the known coinbase amount"
        );
        return electrum_balance_sats;
    }

    // The genesis coinbase has never moved. For this exact script, a reported balance at or above
    // 50 BTC means the Electrum server already included the coinbase amount.
    if electrum_balance_sats >= genesis_amount {
        debug!(
            "Genesis P2PK script balance already includes at least the coinbase amount; leaving Electrum balance unchanged to avoid double-counting"
        );
        electrum_balance_sats
    } else {
        electrum_balance_sats + genesis_amount
    }
}

pub(super) fn mainnet_genesis_coinbase_tx_state(
    network: Network,
    height: i32,
    txid: &Txid,
    script: &ScriptBuf,
) -> Option<AddressWatchTxState> {
    if !is_mainnet_genesis_coinbase(network, height, txid) {
        return None;
    }

    Some(AddressWatchTxState {
        tx: BitcoinTransaction {
            version: transaction::Version(1),
            lock_time: absolute::LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint::null(),
                script_sig: ScriptBuf::new(),
                sequence: Sequence::MAX,
                witness: Witness::default(),
            }],
            output: vec![TxOut {
                value: Amount::from_sat(MAINNET_GENESIS_COINBASE_AMOUNT_SATS),
                script_pubkey: script.clone(),
            }],
        },
        transaction_type: EventType::Receive,
        amount_sats: sats_to_i64(MAINNET_GENESIS_COINBASE_AMOUNT_SATS),
        is_confirmed: true,
        block_height: Some(0),
        first_seen_at: MAINNET_GENESIS_BLOCK_TIMESTAMP,
        confirmed_at: Some(MAINNET_GENESIS_BLOCK_TIMESTAMP),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_tx_confirmed_positive_height() {
        let txid =
            Txid::from_str("1111111111111111111111111111111111111111111111111111111111111111")
                .unwrap();

        assert!(is_tx_confirmed(Network::Bitcoin, 1, &txid));
        assert!(is_tx_confirmed(Network::Testnet, 100, &txid));
        assert!(is_tx_confirmed(Network::Regtest, 800_000, &txid));
    }

    #[test]
    fn test_is_tx_confirmed_mempool() {
        let non_genesis_txid =
            Txid::from_str("1111111111111111111111111111111111111111111111111111111111111111")
                .unwrap();
        let genesis_txid = Txid::from_str(MAINNET_GENESIS_COINBASE_TXID_HEX).unwrap();

        assert!(!is_tx_confirmed(Network::Bitcoin, 0, &non_genesis_txid));
        assert!(!is_tx_confirmed(Network::Testnet, 0, &genesis_txid));
        assert!(!is_tx_confirmed(Network::Regtest, 0, &genesis_txid));
    }

    #[test]
    fn test_is_tx_confirmed_unconfirmed_parents() {
        let txid =
            Txid::from_str("1111111111111111111111111111111111111111111111111111111111111111")
                .unwrap();

        assert!(!is_tx_confirmed(Network::Bitcoin, -1, &txid));
    }

    #[test]
    fn test_is_tx_confirmed_genesis_coinbase() {
        assert!(is_tx_confirmed(
            Network::Bitcoin,
            0,
            &MAINNET_GENESIS_COINBASE_TXID
        ));
    }

    #[test]
    fn test_is_tx_confirmed_genesis_coinbase_requires_mainnet() {
        assert!(!is_tx_confirmed(
            Network::Testnet,
            0,
            &MAINNET_GENESIS_COINBASE_TXID
        ));
        assert!(!is_tx_confirmed(
            Network::Regtest,
            0,
            &MAINNET_GENESIS_COINBASE_TXID
        ));
    }

    #[test]
    fn test_confirmed_block_height_accepts_genesis_height() {
        assert_eq!(confirmed_block_height(0), 0);
    }

    #[test]
    fn test_genesis_block_timestamp_is_mainnet_only() {
        assert_eq!(
            genesis_block_timestamp(Network::Bitcoin, 0),
            Some(MAINNET_GENESIS_BLOCK_TIMESTAMP)
        );
        assert_eq!(genesis_block_timestamp(Network::Testnet, 0), None);
        assert_eq!(genesis_block_timestamp(Network::Regtest, 0), None);
        assert_eq!(genesis_block_timestamp(Network::Bitcoin, 1), None);
    }

    #[test]
    fn test_mainnet_genesis_coinbase_tx_state() {
        let script = ScriptBuf::from_bytes(vec![0x51, 0x21, 0x02]);
        let state = mainnet_genesis_coinbase_tx_state(
            Network::Bitcoin,
            0,
            &MAINNET_GENESIS_COINBASE_TXID,
            &script,
        )
        .expect("mainnet genesis coinbase should produce a synthetic watch state");

        assert_eq!(state.transaction_type, EventType::Receive);
        assert_eq!(
            state.amount_sats,
            sats_to_i64(MAINNET_GENESIS_COINBASE_AMOUNT_SATS)
        );
        assert!(state.is_confirmed);
        assert_eq!(state.block_height, Some(0));
        assert_eq!(state.first_seen_at, MAINNET_GENESIS_BLOCK_TIMESTAMP);
        assert_eq!(state.confirmed_at, Some(MAINNET_GENESIS_BLOCK_TIMESTAMP));
        assert_eq!(state.tx.output.len(), 1);
        assert_eq!(state.tx.output[0].script_pubkey, script);
    }

    #[test]
    fn test_mainnet_genesis_coinbase_tx_state_requires_exact_match() {
        let script = ScriptBuf::new();
        let non_genesis_txid =
            Txid::from_str("1111111111111111111111111111111111111111111111111111111111111111")
                .unwrap();

        assert!(
            mainnet_genesis_coinbase_tx_state(
                Network::Testnet,
                0,
                &MAINNET_GENESIS_COINBASE_TXID,
                &script
            )
            .is_none()
        );
        assert!(
            mainnet_genesis_coinbase_tx_state(
                Network::Bitcoin,
                1,
                &MAINNET_GENESIS_COINBASE_TXID,
                &script
            )
            .is_none()
        );
        assert!(
            mainnet_genesis_coinbase_tx_state(Network::Bitcoin, 0, &non_genesis_txid, &script)
                .is_none()
        );
    }

    #[test]
    fn test_mainnet_genesis_p2pk_script_includes_checksig() {
        let script_bytes = MAINNET_GENESIS_P2PK_SCRIPT.as_bytes();

        assert_eq!(script_bytes.first(), Some(&0x41)); // push 65-byte uncompressed pubkey
        assert_eq!(script_bytes.last(), Some(&0xac)); // OP_CHECKSIG
        assert_eq!(script_bytes.len(), 67);
    }

    #[test]
    fn test_balance_with_mainnet_genesis_coinbase_adds_coinbase_when_history_present() {
        let history = vec![bdk_electrum::electrum_client::GetHistoryRes {
            tx_hash: *MAINNET_GENESIS_COINBASE_TXID,
            height: 0,
            fee: None,
        }];

        assert_eq!(
            balance_with_mainnet_genesis_coinbase(
                Network::Bitcoin,
                &MAINNET_GENESIS_P2PK_SCRIPT,
                1_032_180,
                &history,
            ),
            5_001_032_180
        );
    }

    #[test]
    fn test_balance_with_mainnet_genesis_coinbase_requires_exact_script() {
        let history = vec![bdk_electrum::electrum_client::GetHistoryRes {
            tx_hash: *MAINNET_GENESIS_COINBASE_TXID,
            height: 0,
            fee: None,
        }];

        assert_eq!(
            balance_with_mainnet_genesis_coinbase(
                Network::Bitcoin,
                &ScriptBuf::new(),
                1_032_180,
                &history,
            ),
            1_032_180
        );
    }

    #[test]
    fn test_balance_with_mainnet_genesis_coinbase_requires_mainnet() {
        let history = vec![bdk_electrum::electrum_client::GetHistoryRes {
            tx_hash: *MAINNET_GENESIS_COINBASE_TXID,
            height: 0,
            fee: None,
        }];

        assert_eq!(
            balance_with_mainnet_genesis_coinbase(
                Network::Testnet,
                &MAINNET_GENESIS_P2PK_SCRIPT,
                1_032_180,
                &history,
            ),
            1_032_180
        );
    }

    #[test]
    fn test_balance_with_mainnet_genesis_coinbase_avoids_double_counting() {
        let history = vec![bdk_electrum::electrum_client::GetHistoryRes {
            tx_hash: *MAINNET_GENESIS_COINBASE_TXID,
            height: 0,
            fee: None,
        }];

        assert_eq!(
            balance_with_mainnet_genesis_coinbase(
                Network::Bitcoin,
                &MAINNET_GENESIS_P2PK_SCRIPT,
                5_001_032_180,
                &history,
            ),
            5_001_032_180
        );
    }

    #[test]
    fn test_balance_with_mainnet_genesis_coinbase_does_not_adjust_without_history_entry() {
        assert_eq!(
            balance_with_mainnet_genesis_coinbase(
                Network::Bitcoin,
                &MAINNET_GENESIS_P2PK_SCRIPT,
                1_032_180,
                &[],
            ),
            1_032_180
        );
    }
}
