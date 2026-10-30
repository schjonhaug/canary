//! `NostrError`: every way a Nostr recipient, proxy setting or DM send can fail, with the
//! user-facing message (`Display`) and the stable error code the API returns for it.

use std::collections::HashMap;
use std::fmt;

use nostr_sdk::prelude::RelayUrl;

const INVALID_RECIPIENT_ERROR_CODE: &str = "invalid_nostr_recipient";
const INVALID_SOCKS_PROXY_ERROR_CODE: &str = "nostr_invalid_socks_proxy";
const DISCOVERY_FAILED_ERROR_CODE: &str = "nostr_discovery_failed";
const INBOX_CONNECT_FAILED_ERROR_CODE: &str = "nostr_inbox_connect_failed";
const INBOX_DISCOVERY_TIMEOUT_ERROR_CODE: &str = "nostr_inbox_discovery_timeout";
const NO_DM_RELAYS_ERROR_CODE: &str = "nostr_no_dm_relays";
const PUBLISH_TIMEOUT_ERROR_CODE: &str = "nostr_publish_timeout";
const SEND_FAILED_ERROR_CODE: &str = "nostr_send_failed";
const AUTH_FAILED_ERROR_CODE: &str = "nostr_auth_failed";
const NIP04_FAILED_ERROR_CODE: &str = "nostr_nip04_failed";
const ONION_SOCKS_REQUIRED_ERROR_CODE: &str = "nostr_onion_socks_required";

/// A Nostr recipient, proxy or send failure.
///
/// `Display` is the message shown to users. Failures without a specific explanation are
/// prefixed with "Nostr send failed:". [`NostrError::code`] gives the matching API error code.
#[derive(Debug, thiserror::Error)]
pub enum NostrError {
    /// The recipient field was empty.
    #[error("Nostr recipient cannot be empty")]
    EmptyRecipient,
    /// The user entered a private `nsec` key instead of a public key.
    #[error("Enter a recipient npub, not a private nsec key")]
    PrivateKeyAsRecipient,
    /// The recipient is neither a valid `npub` nor a 64-character hex public key.
    #[error("Enter a valid Nostr public key as npub or 64-character hex")]
    InvalidRecipient(#[source] nostr::error::Error),
    /// A stored notification target no longer parses as a public key.
    #[error("Invalid Nostr recipient public key")]
    InvalidStoredRecipient(#[source] nostr::error::Error),
    /// `CANARY_NOSTR_SOCKS_PROXY` is not a usable `host:port`.
    #[error("Invalid {env}: expected host:port, got '{value}'", env = super::client::NOSTR_SOCKS_PROXY_ENV)]
    InvalidSocksProxy {
        /// The trimmed setting value.
        value: String,
    },
    /// None of the discovery relays connected.
    #[error("Nostr discovery relays failed: {}", .0.or("no relays connected"))]
    DiscoveryFailed(RelayFailures),
    /// The recipient's inbox relay list lookup timed out.
    #[error("Nostr inbox relay discovery timed out")]
    InboxDiscoveryTimedOut,
    /// The recipient's inbox relay list lookup failed.
    #[error("Nostr send failed: Nostr inbox relay discovery failed: {0}")]
    InboxDiscoveryFailed(#[source] nostr_sdk::error::Error),
    /// The recipient has not published a usable kind 10050 inbox relay list.
    #[error("Recipient has no kind 10050 Nostr DM inbox relay list")]
    NoInboxRelays,
    /// Every inbox relay is `.onion` and no SOCKS proxy is configured.
    #[error(
        "Recipient inbox relays are .onion; configure CANARY_NOSTR_SOCKS_PROXY (system Tor SOCKS, typically 127.0.0.1:9050)"
    )]
    OnionSocksRequired,
    /// None of the recipient's inbox relays connected.
    #[error("Nostr inbox relay connection failed: {}", .0.or("no recipient inbox relays connected"))]
    InboxConnectFailed(RelayFailures),
    /// Building the NIP-17 gift wrap failed.
    #[error("Nostr send failed: Nostr encryption failed: {0}")]
    Encryption(#[source] nostr::error::Error),
    /// Publishing to the inbox relays timed out.
    #[error("Nostr publish timed out")]
    PublishTimedOut,
    /// The client could not publish to the inbox relays.
    #[error("Nostr send failed: Nostr publish failed: {0}")]
    PublishFailed(#[source] nostr_sdk::error::Error),
    /// At least one inbox relay rejected the message, or none accepted it.
    #[error("Nostr send failed: Nostr publish failed: {}", .0.or("no relay accepted the message"))]
    PublishRejected(RelayFailures),
    /// The whole NIP-17 send attempt timed out.
    #[error("Nostr send failed: Nostr send attempt timed out")]
    SendTimedOut,
    /// None of the legacy NIP-04 relays connected.
    #[error("Nostr legacy DM relay connection failed: {}", .0.or("no legacy DM relays connected"))]
    LegacyRelayConnectFailed(RelayFailures),
    /// NIP-04 encryption failed.
    #[error("Nostr legacy DM encryption failed: {0}")]
    LegacyEncryption(#[source] nostr::error::Error),
    /// Signing the NIP-04 event failed.
    #[error("Nostr legacy DM signing failed: {0}")]
    LegacySigning(#[source] nostr::error::Error),
    /// Publishing the NIP-04 event timed out.
    #[error("Nostr legacy DM publish timed out")]
    LegacyPublishTimedOut,
    /// The client could not publish the NIP-04 event.
    #[error("Nostr legacy DM publish failed: {0}")]
    LegacyPublishFailed(#[source] nostr_sdk::error::Error),
    /// No legacy relay accepted the NIP-04 event.
    #[error("Nostr legacy DM publish failed: {}", .0.or("no relay accepted the message"))]
    LegacyPublishRejected(RelayFailures),
    /// The whole NIP-04 send attempt timed out.
    #[error("Nostr legacy DM send attempt timed out")]
    LegacySendTimedOut,
}

impl NostrError {
    /// Stable error code for API responses. The frontend maps these codes to help text.
    pub fn code(&self) -> &'static str {
        match self {
            Self::EmptyRecipient
            | Self::PrivateKeyAsRecipient
            | Self::InvalidRecipient(_)
            | Self::InvalidStoredRecipient(_) => INVALID_RECIPIENT_ERROR_CODE,
            Self::InvalidSocksProxy { .. } => INVALID_SOCKS_PROXY_ERROR_CODE,
            Self::DiscoveryFailed(_) => DISCOVERY_FAILED_ERROR_CODE,
            Self::InboxConnectFailed(_) => INBOX_CONNECT_FAILED_ERROR_CODE,
            Self::InboxDiscoveryTimedOut => INBOX_DISCOVERY_TIMEOUT_ERROR_CODE,
            Self::NoInboxRelays => NO_DM_RELAYS_ERROR_CODE,
            Self::OnionSocksRequired => ONION_SOCKS_REQUIRED_ERROR_CODE,
            Self::PublishTimedOut => PUBLISH_TIMEOUT_ERROR_CODE,
            _ if self.relay_rejected_auth() => AUTH_FAILED_ERROR_CODE,
            Self::LegacyRelayConnectFailed(_)
            | Self::LegacyEncryption(_)
            | Self::LegacySigning(_)
            | Self::LegacyPublishTimedOut
            | Self::LegacyPublishFailed(_)
            | Self::LegacyPublishRejected(_)
            | Self::LegacySendTimedOut => NIP04_FAILED_ERROR_CODE,
            Self::InboxDiscoveryFailed(_)
            | Self::Encryption(_)
            | Self::PublishFailed(_)
            | Self::PublishRejected(_)
            | Self::SendTimedOut => SEND_FAILED_ERROR_CODE,
        }
    }

    /// True when the relay or SDK detail carried by this error reports a NIP-42 AUTH failure.
    ///
    /// Relays and nostr-sdk only report the reason as text, so this is the one place that
    /// inspects message content.
    fn relay_rejected_auth(&self) -> bool {
        match self {
            Self::InboxDiscoveryFailed(error)
            | Self::PublishFailed(error)
            | Self::LegacyPublishFailed(error) => is_auth_failure(&error.to_string()),
            Self::Encryption(error)
            | Self::LegacyEncryption(error)
            | Self::LegacySigning(error) => is_auth_failure(&error.to_string()),
            Self::DiscoveryFailed(failures)
            | Self::InboxConnectFailed(failures)
            | Self::PublishRejected(failures)
            | Self::LegacyRelayConnectFailed(failures)
            | Self::LegacyPublishRejected(failures) => failures.mention_auth_failure(),
            _ => false,
        }
    }
}

fn is_auth_failure(detail: &str) -> bool {
    detail.contains("authentication failed") || detail.contains("failed to authenticate")
}

/// Per-relay failure details, each formatted as `url: reason`.
///
/// Relays report rejections and nostr-sdk reports connection failures as text, so the details
/// are kept as the strings shown to users.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RelayFailures(Vec<String>);

impl RelayFailures {
    pub(super) fn from_output(failures: &HashMap<RelayUrl, String>) -> Self {
        Self(
            failures
                .iter()
                .map(|(url, error)| format!("{url}: {error}"))
                .collect(),
        )
    }

    pub(super) fn push(&mut self, relay: impl fmt::Display, error: impl fmt::Display) {
        self.0.push(format!("{relay}: {error}"));
    }

    pub(super) fn len(&self) -> usize {
        self.0.len()
    }

    /// The joined details, or `empty_message` when there are none.
    fn or(&self, empty_message: &str) -> String {
        if self.0.is_empty() {
            empty_message.to_string()
        } else {
            self.0.join("; ")
        }
    }

    fn mention_auth_failure(&self) -> bool {
        self.0.iter().any(|detail| is_auth_failure(detail))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failures(details: &[&str]) -> RelayFailures {
        RelayFailures(details.iter().map(ToString::to_string).collect())
    }

    fn protocol_error(message: &'static str) -> nostr::error::Error {
        nostr::error::Error::new(nostr::error::ErrorKind::Other, message)
    }

    #[test]
    fn messages_and_codes_match_the_api_contract() {
        let auth = "ws://haven.local/chat: authentication failed";
        let cases: Vec<(NostrError, &str, &str)> = vec![
            (
                NostrError::EmptyRecipient,
                "Nostr recipient cannot be empty",
                "invalid_nostr_recipient",
            ),
            (
                NostrError::PrivateKeyAsRecipient,
                "Enter a recipient npub, not a private nsec key",
                "invalid_nostr_recipient",
            ),
            (
                NostrError::InvalidSocksProxy {
                    value: "not-a-proxy".to_string(),
                },
                "Invalid CANARY_NOSTR_SOCKS_PROXY: expected host:port, got 'not-a-proxy'",
                "nostr_invalid_socks_proxy",
            ),
            (
                NostrError::DiscoveryFailed(RelayFailures::default()),
                "Nostr discovery relays failed: no relays connected",
                "nostr_discovery_failed",
            ),
            (
                NostrError::DiscoveryFailed(failures(&[auth])),
                "Nostr discovery relays failed: ws://haven.local/chat: authentication failed",
                "nostr_discovery_failed",
            ),
            (
                NostrError::InboxDiscoveryTimedOut,
                "Nostr inbox relay discovery timed out",
                "nostr_inbox_discovery_timeout",
            ),
            (
                NostrError::NoInboxRelays,
                "Recipient has no kind 10050 Nostr DM inbox relay list",
                "nostr_no_dm_relays",
            ),
            (
                NostrError::OnionSocksRequired,
                "Recipient inbox relays are .onion; configure CANARY_NOSTR_SOCKS_PROXY (system Tor SOCKS, typically 127.0.0.1:9050)",
                "nostr_onion_socks_required",
            ),
            (
                NostrError::InboxConnectFailed(RelayFailures::default()),
                "Nostr inbox relay connection failed: no recipient inbox relays connected",
                "nostr_inbox_connect_failed",
            ),
            (
                NostrError::InboxConnectFailed(failures(&["wss://a.example: refused", auth])),
                "Nostr inbox relay connection failed: wss://a.example: refused; ws://haven.local/chat: authentication failed",
                "nostr_inbox_connect_failed",
            ),
            (
                NostrError::Encryption(protocol_error("bad key")),
                "Nostr send failed: Nostr encryption failed: bad key",
                "nostr_send_failed",
            ),
            (
                NostrError::PublishTimedOut,
                "Nostr publish timed out",
                "nostr_publish_timeout",
            ),
            (
                NostrError::PublishRejected(RelayFailures::default()),
                "Nostr send failed: Nostr publish failed: no relay accepted the message",
                "nostr_send_failed",
            ),
            (
                NostrError::PublishRejected(failures(&[auth])),
                "Nostr send failed: Nostr publish failed: ws://haven.local/chat: authentication failed",
                "nostr_auth_failed",
            ),
            (
                NostrError::PublishRejected(failures(&[
                    "ws://haven.local/chat: failed to authenticate",
                ])),
                "Nostr send failed: Nostr publish failed: ws://haven.local/chat: failed to authenticate",
                "nostr_auth_failed",
            ),
            (
                NostrError::SendTimedOut,
                "Nostr send failed: Nostr send attempt timed out",
                "nostr_send_failed",
            ),
            (
                NostrError::LegacyRelayConnectFailed(RelayFailures::default()),
                "Nostr legacy DM relay connection failed: no legacy DM relays connected",
                "nostr_nip04_failed",
            ),
            (
                NostrError::LegacyRelayConnectFailed(failures(&[auth])),
                "Nostr legacy DM relay connection failed: ws://haven.local/chat: authentication failed",
                "nostr_auth_failed",
            ),
            (
                NostrError::LegacyEncryption(protocol_error("bad key")),
                "Nostr legacy DM encryption failed: bad key",
                "nostr_nip04_failed",
            ),
            (
                NostrError::LegacySigning(protocol_error("bad key")),
                "Nostr legacy DM signing failed: bad key",
                "nostr_nip04_failed",
            ),
            (
                NostrError::LegacyPublishTimedOut,
                "Nostr legacy DM publish timed out",
                "nostr_nip04_failed",
            ),
            (
                NostrError::LegacyPublishRejected(RelayFailures::default()),
                "Nostr legacy DM publish failed: no relay accepted the message",
                "nostr_nip04_failed",
            ),
            (
                NostrError::LegacySendTimedOut,
                "Nostr legacy DM send attempt timed out",
                "nostr_nip04_failed",
            ),
        ];

        for (error, message, code) in cases {
            assert_eq!(error.to_string(), message);
            assert_eq!(error.code(), code, "code for {message}");
        }
    }

    #[test]
    fn relay_failures_join_in_insertion_order() {
        let mut collected = RelayFailures::default();
        collected.push("wss://a.example", "refused");
        collected.push("wss://b.example", "timeout");
        assert_eq!(collected.len(), 2);
        assert_eq!(
            collected.or("unused"),
            "wss://a.example: refused; wss://b.example: timeout"
        );
    }
}
