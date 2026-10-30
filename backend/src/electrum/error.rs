//! Errors returned by Electrum client operations.

use bdk_electrum::electrum_client;
use bdk_wallet::chain::local_chain::CannotConnectError;
use tokio::task::JoinError;

/// A failed Electrum operation.
///
/// `operation` names the call that failed (for example `"Sync"`) so log lines say what was
/// being attempted without every caller adding its own context.
#[derive(Debug, thiserror::Error)]
pub enum ElectrumError {
    /// The configured URL is neither `tcp://` nor `ssl://`.
    #[error("Unsupported Electrum URL scheme. Use 'tcp://' or 'ssl://'")]
    UnsupportedScheme,

    /// Opening a connection to the server failed.
    #[error("Could not connect to the Electrum server: {source}")]
    Connect {
        #[source]
        source: electrum_client::Error,
    },

    /// An earlier worker timed out and never stopped, so the shared socket is in an unknown
    /// state and no new work may start on it.
    #[error(
        "{operation} cannot start: Electrum client requires connection replacement after an unresponsive worker"
    )]
    ConnectionPoisoned { operation: &'static str },

    /// The operation exceeded its deadline and its worker stopped after cancellation.
    #[error("{operation} timed out after {secs} seconds")]
    Timeout { operation: &'static str, secs: u64 },

    /// The operation exceeded its deadline and its worker ignored cancellation. The socket it
    /// holds must be replaced.
    #[error(
        "{operation} timed out after {secs} seconds and its worker did not stop within {} seconds; connection replacement required",
        super::CANCELLATION_DRAIN_TIMEOUT_SECS
    )]
    WorkerStuck { operation: &'static str, secs: u64 },

    /// The blocking worker panicked or was cancelled.
    #[error("{operation} task failed: {source}")]
    TaskFailed {
        operation: &'static str,
        #[source]
        source: JoinError,
    },

    /// The Electrum client returned an error.
    #[error("{operation} failed: {source}")]
    Client {
        operation: &'static str,
        #[source]
        source: electrum_client::Error,
    },

    /// Removing script subscriptions did not finish. The server may still hold some of them, so
    /// the connection must be replaced.
    #[error(
        "Electrum subscription cleanup did not complete; connection replacement required: {source}"
    )]
    SubscriptionCleanup {
        #[source]
        source: Box<ElectrumError>,
    },

    /// The fetched update does not connect to the wallet's local chain.
    #[error("Failed to apply update: {source}")]
    ApplyUpdate {
        #[source]
        source: CannotConnectError,
    },

    /// The server reported a block height that does not fit in `u32`.
    #[error("Block height {0} does not fit in u32")]
    InvalidHeight(usize),
}
