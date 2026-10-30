//! Errors returned by Electrum client operations and how they are classified.

use bdk_electrum::electrum_client;
use bdk_wallet::chain::local_chain::CannotConnectError;
use std::fmt;
use std::io;
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

impl ElectrumError {
    /// Classifies the failure for reconnect decisions and log labels.
    pub fn kind(&self) -> ErrorKind {
        match self {
            // These leave the socket in an unknown state, so the connection must be replaced
            // whatever the underlying cause (including a wrapped cleanup error's own kind).
            Self::ConnectionPoisoned { .. }
            | Self::WorkerStuck { .. }
            | Self::SubscriptionCleanup { .. } => ErrorKind::Transport,
            Self::Timeout { .. } => ErrorKind::Timeout,
            Self::TaskFailed { .. } => ErrorKind::Task,
            // The update does not connect to the local chain; this is not a network failure.
            Self::ApplyUpdate { .. } => ErrorKind::Connection,
            Self::Connect { source } | Self::Client { source, .. } => client_error_kind(source),
            Self::UnsupportedScheme | Self::InvalidHeight(_) => ErrorKind::Unknown,
        }
    }

    /// True when the connection is unusable and has to be replaced before more work runs.
    pub fn is_transport(&self) -> bool {
        self.kind() == ErrorKind::Transport
    }
}

/// What kind of failure an [`ElectrumError`] is.
///
/// Variants are declared in precedence order. When the client retried and every attempt failed
/// differently, the attempt with the earliest kind decides, so a single transport failure among
/// the attempts still triggers a reconnect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorKind {
    /// The socket is broken or in an unknown state; the connection must be replaced.
    Transport,
    /// The server did not answer in time.
    Timeout,
    /// The fetched update does not connect to the wallet's local chain.
    Connection,
    /// The network or host is unreachable.
    Network,
    /// The server answered with an error or a response Canary could not use.
    Server,
    /// The blocking worker panicked or was cancelled.
    Task,
    /// Anything else.
    Unknown,
}

impl ErrorKind {
    /// True when the sync loop should replace the connection before retrying. Timeouts count:
    /// they usually mean the connection has gone stale.
    pub fn requires_reconnect(self) -> bool {
        matches!(self, Self::Transport | Self::Timeout)
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Transport => "TRANSPORT",
            Self::Timeout => "TIMEOUT",
            Self::Connection => "CONNECTION",
            Self::Network => "NETWORK",
            Self::Server => "SERVER",
            Self::Task => "TASK",
            Self::Unknown => "UNKNOWN",
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

fn client_error_kind(error: &electrum_client::Error) -> ErrorKind {
    use electrum_client::Error;

    match error {
        Error::IOError(error) => io_error_kind(error),
        Error::SharedIOError(error) => io_error_kind(error),
        Error::AllAttemptsErrored(errors) => errors
            .iter()
            .map(client_error_kind)
            .min()
            .unwrap_or(ErrorKind::Unknown),
        // A response cut off mid-line means the server closed the socket while sending it.
        Error::JSON(error) if error.is_eof() => ErrorKind::Transport,
        // The socket reader thread is gone, so no response can arrive on this connection.
        Error::Mpsc => ErrorKind::Transport,
        Error::Protocol(_) | Error::InvalidResponse(_) | Error::Message(_) => ErrorKind::Server,
        _ => ErrorKind::Unknown,
    }
}

fn io_error_kind(error: &io::Error) -> ErrorKind {
    use io::ErrorKind as Io;

    match error.kind() {
        Io::BrokenPipe
        | Io::ConnectionReset
        | Io::ConnectionAborted
        | Io::ConnectionRefused
        | Io::NotConnected
        | Io::UnexpectedEof => ErrorKind::Transport,
        // A blocking socket's read/write timeout surfaces as `WouldBlock` on Linux.
        Io::TimedOut | Io::WouldBlock => ErrorKind::Timeout,
        Io::HostUnreachable | Io::NetworkUnreachable | Io::NetworkDown => ErrorKind::Network,
        _ => ErrorKind::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bdk_electrum::electrum_client::Error;
    use std::sync::Arc;

    fn client(source: Error) -> ElectrumError {
        ElectrumError::Client {
            operation: "Sync",
            source,
        }
    }

    fn io(kind: io::ErrorKind) -> Error {
        Error::IOError(io::Error::from(kind))
    }

    #[test]
    fn broken_connection_io_kinds_are_transport() {
        for kind in [
            io::ErrorKind::BrokenPipe,
            io::ErrorKind::ConnectionReset,
            io::ErrorKind::ConnectionAborted,
            io::ErrorKind::ConnectionRefused,
            io::ErrorKind::NotConnected,
            io::ErrorKind::UnexpectedEof,
        ] {
            assert!(client(io(kind)).is_transport(), "{kind:?}");
            let shared = Error::SharedIOError(Arc::new(io::Error::from(kind)));
            assert!(client(shared).is_transport(), "shared {kind:?}");
        }
    }

    #[test]
    fn os_error_codes_map_to_transport() {
        // EPIPE and ECONNRESET, as the kernel reports them on a dropped socket.
        #[cfg(target_os = "linux")]
        let codes = [32, 104];
        #[cfg(target_os = "macos")]
        let codes = [32, 54];
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        for code in codes {
            let error = Error::IOError(io::Error::from_raw_os_error(code));
            assert!(client(error).is_transport(), "os error {code}");
        }
    }

    #[test]
    fn other_io_kinds_are_not_transport() {
        let timed_out = client(io(io::ErrorKind::TimedOut));
        assert!(!timed_out.is_transport());
        assert_eq!(timed_out.kind(), ErrorKind::Timeout);

        let unreachable = client(io(io::ErrorKind::NetworkUnreachable));
        assert_eq!(unreachable.kind(), ErrorKind::Network);

        let would_block = client(io(io::ErrorKind::WouldBlock));
        assert_eq!(would_block.kind(), ErrorKind::Timeout);
        assert!(would_block.kind().requires_reconnect());
        assert!(!client(io(io::ErrorKind::InvalidData)).is_transport());
    }

    #[test]
    fn all_attempts_errored_takes_the_most_severe_attempt() {
        let nested = Error::AllAttemptsErrored(vec![
            Error::Protocol(serde_json::json!("busy")),
            Error::AllAttemptsErrored(vec![io(io::ErrorKind::BrokenPipe)]),
        ]);
        assert!(client(nested).is_transport());

        let timed_out = Error::AllAttemptsErrored(vec![
            Error::Protocol(serde_json::json!("busy")),
            io(io::ErrorKind::TimedOut),
        ]);
        assert_eq!(client(timed_out).kind(), ErrorKind::Timeout);

        let protocol = Error::AllAttemptsErrored(vec![Error::Protocol(serde_json::json!("x"))]);
        assert!(!client(protocol).is_transport());

        // electrum-client leaves the list empty when it gives up after a single attempt.
        let empty = client(Error::AllAttemptsErrored(Vec::new()));
        assert!(!empty.is_transport());
        assert_eq!(empty.kind(), ErrorKind::Unknown);
    }

    #[test]
    fn server_errors_are_not_transport() {
        let protocol = client(Error::Protocol(serde_json::json!({
            "code": -32600,
            "message": "connection closed by server policy"
        })));
        assert!(!protocol.is_transport());
        assert_eq!(protocol.kind(), ErrorKind::Server);

        let invalid = client(Error::InvalidResponse(serde_json::json!(null)));
        assert_eq!(invalid.kind(), ErrorKind::Server);

        let message = client(Error::Message("cannot find agreement block".to_string()));
        assert_eq!(message.kind(), ErrorKind::Server);

        assert!(!client(Error::CouldntLockReader).is_transport());
    }

    #[test]
    fn truncated_json_is_transport_but_malformed_json_is_not() {
        let truncated = serde_json::from_str::<serde_json::Value>("{\"id\":").unwrap_err();
        assert!(client(Error::JSON(truncated)).is_transport());

        let malformed = serde_json::from_str::<serde_json::Value>("nope").unwrap_err();
        assert!(!client(Error::JSON(malformed)).is_transport());
    }

    #[test]
    fn mpsc_is_transport() {
        assert!(client(Error::Mpsc).is_transport());
    }

    #[test]
    fn timeouts_are_not_transport_unless_the_worker_is_stuck() {
        let timeout = ElectrumError::Timeout {
            operation: "Sync",
            secs: 60,
        };
        assert!(!timeout.is_transport());
        assert_eq!(timeout.kind(), ErrorKind::Timeout);
        assert!(timeout.kind().requires_reconnect());

        let stuck = ElectrumError::WorkerStuck {
            operation: "Sync",
            secs: 60,
        };
        assert!(stuck.is_transport());
    }

    #[test]
    fn connection_replacement_cases_are_transport() {
        assert!(ElectrumError::ConnectionPoisoned { operation: "Sync" }.is_transport());

        let cleanup = ElectrumError::SubscriptionCleanup {
            source: Box::new(client(Error::Protocol(serde_json::json!("x")))),
        };
        assert!(cleanup.is_transport());
    }

    #[tokio::test]
    async fn task_failures_are_not_transport() {
        let task = tokio::spawn(std::future::pending::<()>());
        task.abort();
        let source = task.await.unwrap_err();
        let error = ElectrumError::TaskFailed {
            operation: "Sync",
            source,
        };
        assert!(!error.is_transport());
        assert_eq!(error.kind(), ErrorKind::Task);
        assert!(!error.kind().requires_reconnect());
    }

    #[test]
    fn apply_update_failures_keep_the_connection_label() {
        let error = ElectrumError::ApplyUpdate {
            source: CannotConnectError {
                try_include_height: 1,
            },
        };
        assert_eq!(error.kind(), ErrorKind::Connection);
        assert!(!error.kind().requires_reconnect());
    }

    #[test]
    fn labels_match_the_log_vocabulary() {
        let labels: Vec<_> = [
            ErrorKind::Transport,
            ErrorKind::Timeout,
            ErrorKind::Connection,
            ErrorKind::Network,
            ErrorKind::Server,
            ErrorKind::Task,
            ErrorKind::Unknown,
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        assert_eq!(
            labels,
            [
                "TRANSPORT",
                "TIMEOUT",
                "CONNECTION",
                "NETWORK",
                "SERVER",
                "TASK",
                "UNKNOWN"
            ]
        );
    }
}
