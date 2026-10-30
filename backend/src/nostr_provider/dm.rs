//! Building and publishing direct messages: NIP-17 gift wraps to the recipient's inbox relays
//! and legacy NIP-04 events to the default relays.

use super::client::nostr_client;
use super::mode::NostrDmMode;
use super::provider::NostrProvider;
use super::relays::format_relay_failures;
use nostr::nips::{nip04, nip17};
use nostr_sdk::prelude::*;
use std::time::Duration;

const NOSTR_SEND_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(30);
const NOSTR_PUBLISH_TIMEOUT: Duration = Duration::from_secs(10);
const NOSTR_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct NostrSendSuccess {
    pub(super) event_id: EventId,
    pub(super) dm_mode_used: NostrDmMode,
}

impl NostrProvider {
    pub(super) async fn send_nip17_message(
        &self,
        recipient: PublicKey,
        message: String,
    ) -> Result<NostrSendSuccess, String> {
        // Keep the client short-lived for v1 so relay state does not outlive a single send attempt.
        // The NIP-17 phases are explicit so each failure can produce an actionable user message.
        let client = nostr_client(self.sender_keys.clone(), self.onion_socks_proxy);

        let send = tokio::time::timeout(
            NOSTR_SEND_ATTEMPT_TIMEOUT,
            self.send_nip17_message_with_client(&client, recipient, message),
        )
        .await
        .map_err(|_| "Nostr send attempt timed out".to_string())
        .and_then(|result| result);

        // Shutdown is best-effort cleanup for this short-lived client; send result is reported above.
        let _ = tokio::time::timeout(NOSTR_SHUTDOWN_TIMEOUT, client.shutdown()).await;

        match send {
            Ok(output) => Ok(NostrSendSuccess {
                event_id: output.value,
                dm_mode_used: NostrDmMode::Nip17,
            }),
            Err(error_message) => Err(error_message),
        }
    }

    pub(super) async fn send_nip04_message(
        &self,
        recipient: PublicKey,
        message: String,
    ) -> Result<NostrSendSuccess, String> {
        let keys = self.sender_keys.clone();
        let client = nostr_client(keys.clone(), self.onion_socks_proxy);
        let output = tokio::time::timeout(NOSTR_SEND_ATTEMPT_TIMEOUT, async {
            let relays = self.connect_nip04_relays(&client).await?;

            let event = build_nip04_dm_event(&keys, recipient, message)?;

            tracing::info!(
                relay_count = relays.len(),
                "Publishing legacy NIP-04 Nostr DM"
            );

            tokio::time::timeout(NOSTR_PUBLISH_TIMEOUT, client.send_event(&event).to(relays))
                .await
                .map_err(|_| "Nostr legacy DM publish timed out".to_string())?
                .map_err(|e| format!("Nostr legacy DM publish failed: {e}"))
        })
        .await
        .map_err(|_| "Nostr legacy DM send attempt timed out".to_string())??;

        let _ = tokio::time::timeout(NOSTR_SHUTDOWN_TIMEOUT, client.shutdown()).await;

        if output.success.is_empty() {
            let failed_relays = output
                .failed
                .iter()
                .map(|(url, error)| format!("{url}: {error}"))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(format!(
                "Nostr legacy DM publish failed: {}",
                if failed_relays.is_empty() {
                    "no relay accepted the message".to_string()
                } else {
                    failed_relays
                }
            ));
        }

        Ok(NostrSendSuccess {
            event_id: output.value,
            dm_mode_used: NostrDmMode::Nip04,
        })
    }

    async fn send_nip17_message_with_client(
        &self,
        client: &Client,
        recipient: PublicKey,
        message: String,
    ) -> Result<Output<EventId, EventSendStatus>, String> {
        let discovery_relays = self.connect_discovery_relays(client).await?;
        let inbox_relays = self
            .discover_recipient_inbox_relays(client, &discovery_relays, recipient)
            .await?;
        let connected_inbox_relays = self.connect_inbox_relays(client, &inbox_relays).await?;

        tracing::info!(
            relay_count = connected_inbox_relays.len(),
            "Publishing Nostr DM to recipient inbox relays"
        );

        let event = build_nip17_dm_event(&self.sender_keys, recipient, message)?;
        let output = tokio::time::timeout(
            NOSTR_PUBLISH_TIMEOUT,
            client.send_event(&event).to(connected_inbox_relays),
        )
        .await
        .map_err(|_| "Nostr publish timed out".to_string())?
        .map_err(|e| format!("Nostr publish failed: {e}"))?;

        require_complete_inbox_publish(output)
    }
}

fn build_nip17_dm_event(
    keys: &Keys,
    recipient: PublicKey,
    message: String,
) -> Result<Event, String> {
    nip17::PrivateDirectMessageBuilder::new(recipient, message)
        .finalize(keys)
        .map_err(|e| format!("Nostr encryption failed: {e}"))
}

fn build_nip04_dm_event(
    keys: &Keys,
    recipient: PublicKey,
    message: String,
) -> Result<Event, String> {
    let encrypted = nip04::encrypt(keys.secret_key(), &recipient, message)
        .map_err(|e| format!("Nostr legacy DM encryption failed: {e}"))?;
    EventBuilder::new(Kind::EncryptedDirectMessage, encrypted)
        .tag(Tag::public_key(recipient))
        .finalize(keys)
        .map_err(|e| format!("Nostr legacy DM signing failed: {e}"))
}

fn require_complete_inbox_publish<S>(
    output: Output<EventId, S>,
) -> Result<Output<EventId, S>, String> {
    // Kind 10050 lists the recipient's chosen inboxes. Partial delivery (public
    // relays OK, AUTH-gated self-hosted relay rejected) is not success for a
    // monitoring product, even if some copy of the gift wrap landed elsewhere.
    if output.success.is_empty() || !output.failed.is_empty() {
        return Err(format!(
            "Nostr publish failed: {}",
            format_relay_failures(&output.failed, "no relay accepted the message")
        ));
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nostr_provider::errors::{nostr_test_error_code, NOSTR_AUTH_FAILED_ERROR_CODE};
    use std::collections::HashMap;

    #[tokio::test]
    async fn builds_legacy_nip04_dm_events() {
        let sender_keys = Keys::generate();
        let recipient = Keys::generate().public_key();
        let event = build_nip04_dm_event(&sender_keys, recipient, "hello".to_string()).unwrap();

        assert_eq!(event.kind, Kind::EncryptedDirectMessage);
        assert_eq!(event.pubkey, sender_keys.public_key());
        assert!(event.content.contains("?iv="));
        assert!(event
            .tags
            .iter()
            .any(|tag| { tag.as_slice() == ["p", recipient.to_hex().as_str()] }));
    }

    #[test]
    fn inbox_publish_fails_when_any_relay_rejects() {
        let accepted = RelayUrl::parse("wss://relay.example.com").unwrap();
        let rejected = RelayUrl::parse("ws://haven.local/chat").unwrap();
        let event_id = EventId::from_byte_array([0; 32]);

        let mixed = Output {
            value: event_id,
            success: [(accepted.clone(), ())].into_iter().collect(),
            failed: [(rejected.clone(), "authentication failed".to_string())]
                .into_iter()
                .collect(),
        };
        let error = require_complete_inbox_publish(mixed).unwrap_err();
        assert!(error.contains("ws://haven.local/chat: authentication failed"));
        assert_eq!(
            nostr_test_error_code(Some(&error)),
            Some(NOSTR_AUTH_FAILED_ERROR_CODE)
        );

        #[expect(
            clippy::zero_sized_map_values,
            reason = "nostr-sdk's Output type maps relay URLs to ()"
        )]
        let empty: Output<EventId> = Output {
            value: event_id,
            success: HashMap::default(),
            failed: HashMap::default(),
        };
        assert_eq!(
            require_complete_inbox_publish(empty).unwrap_err(),
            "Nostr publish failed: no relay accepted the message"
        );

        let ok = Output {
            value: event_id,
            success: [(accepted, ())].into_iter().collect(),
            failed: HashMap::default(),
        };
        assert!(require_complete_inbox_publish(ok).is_ok());
    }

    #[test]
    fn nip17_gift_wrap_is_readable_only_by_the_recipient() {
        let sender = Keys::generate();
        let recipient = Keys::generate();
        let event =
            build_nip17_dm_event(&sender, recipient.public_key(), "synthetic notice".into())
                .unwrap();
        assert_eq!(event.kind, Kind::GiftWrap);
        assert_ne!(event.pubkey, sender.public_key());
        event.verify().unwrap();
        let gift = nostr::nips::nip59::extract_rumor(&recipient, &event).unwrap();
        assert_eq!(gift.rumor.content, "synthetic notice");
        assert_eq!(gift.rumor.pubkey, sender.public_key());
        assert!(nostr::nips::nip59::extract_rumor(&Keys::generate(), &event).is_err());
    }
}
