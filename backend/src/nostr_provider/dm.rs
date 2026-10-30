//! Building and publishing direct messages: NIP-17 gift wraps to the recipient's inbox relays
//! and legacy NIP-04 events to the default relays.

use super::client::nostr_client;
use super::errors::{NostrError, RelayFailures};
use super::mode::NostrDmMode;
use super::provider::NostrProvider;
use nostr::nips::{nip04, nip17};
use nostr_sdk::prelude::*;
use std::time::Duration;

const NOSTR_SEND_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(30);
const NOSTR_PUBLISH_TIMEOUT: Duration = Duration::from_secs(10);
const NOSTR_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// A delivered Nostr DM.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NostrSendSuccess {
    /// Id of the published event (the gift wrap for NIP-17).
    pub event_id: EventId,
    /// The DM format that was sent.
    pub dm_mode_used: NostrDmMode,
}

impl NostrProvider {
    pub(super) async fn send_nip17_message(
        &self,
        recipient: PublicKey,
        message: String,
    ) -> Result<NostrSendSuccess, NostrError> {
        // Keep the client short-lived for v1 so relay state does not outlive a single send attempt.
        // The NIP-17 phases are explicit so each failure can produce an actionable user message.
        let client = nostr_client(self.sender_keys.clone(), self.onion_socks_proxy);

        let send = tokio::time::timeout(
            NOSTR_SEND_ATTEMPT_TIMEOUT,
            self.send_nip17_message_with_client(&client, recipient, message),
        )
        .await
        .map_err(|_| NostrError::SendTimedOut)
        .and_then(|result| result);

        // Shutdown is best-effort cleanup for this short-lived client; send result is reported above.
        let _ = tokio::time::timeout(NOSTR_SHUTDOWN_TIMEOUT, client.shutdown()).await;

        match send {
            Ok(output) => Ok(NostrSendSuccess {
                event_id: output.value,
                dm_mode_used: NostrDmMode::Nip17,
            }),
            Err(error) => Err(error),
        }
    }

    pub(super) async fn send_nip04_message(
        &self,
        recipient: PublicKey,
        message: String,
    ) -> Result<NostrSendSuccess, NostrError> {
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
                .map_err(|_| NostrError::LegacyPublishTimedOut)?
                .map_err(NostrError::LegacyPublishFailed)
        })
        .await
        .map_err(|_| NostrError::LegacySendTimedOut)??;

        let _ = tokio::time::timeout(NOSTR_SHUTDOWN_TIMEOUT, client.shutdown()).await;

        if output.success.is_empty() {
            return Err(NostrError::LegacyPublishRejected(
                RelayFailures::from_output(&output.failed),
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
    ) -> Result<Output<EventId, EventSendStatus>, NostrError> {
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
        .map_err(|_| NostrError::PublishTimedOut)?
        .map_err(NostrError::PublishFailed)?;

        require_complete_inbox_publish(output)
    }
}

fn build_nip17_dm_event(
    keys: &Keys,
    recipient: PublicKey,
    message: String,
) -> Result<Event, NostrError> {
    nip17::PrivateDirectMessageBuilder::new(recipient, message)
        .finalize(keys)
        .map_err(NostrError::Encryption)
}

fn build_nip04_dm_event(
    keys: &Keys,
    recipient: PublicKey,
    message: String,
) -> Result<Event, NostrError> {
    let encrypted = nip04::encrypt(keys.secret_key(), &recipient, message)
        .map_err(NostrError::LegacyEncryption)?;
    EventBuilder::new(Kind::EncryptedDirectMessage, encrypted)
        .tag(Tag::public_key(recipient))
        .finalize(keys)
        .map_err(NostrError::LegacySigning)
}

fn require_complete_inbox_publish<S>(
    output: Output<EventId, S>,
) -> Result<Output<EventId, S>, NostrError> {
    // Kind 10050 lists the recipient's chosen inboxes. Partial delivery (public
    // relays OK, AUTH-gated self-hosted relay rejected) is not success for a
    // monitoring product, even if some copy of the gift wrap landed elsewhere.
    if output.success.is_empty() || !output.failed.is_empty() {
        return Err(NostrError::PublishRejected(RelayFailures::from_output(
            &output.failed,
        )));
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(
            error.to_string(),
            "Nostr send failed: Nostr publish failed: ws://haven.local/chat: authentication failed"
        );
        assert_eq!(error.code(), "nostr_auth_failed");

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
            require_complete_inbox_publish(empty)
                .unwrap_err()
                .to_string(),
            "Nostr send failed: Nostr publish failed: no relay accepted the message"
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
