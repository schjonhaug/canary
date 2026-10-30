//! Relay selection and connection: default discovery and NIP-04 relays, recipient inbox relay
//! discovery (kind 10050), and clearnet/`.onion` filtering.

use super::errors::{NostrError, RelayFailures};
use super::provider::NostrProvider;
use super::NOSTR_SEND_CONCURRENCY;
use futures::{stream, StreamExt};
use nostr::nips::nip17;
use nostr_sdk::prelude::*;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::time::Duration;

pub(super) const DEFAULT_DISCOVERY_RELAYS: [&str; 3] = [
    "wss://purplepag.es",
    "wss://relay.damus.io",
    "wss://relay.nostr.band",
];
pub(super) const DEFAULT_NIP04_RELAYS: [&str; 5] = [
    "wss://relay.primal.net",
    "wss://relay.damus.io",
    "wss://nos.lol",
    "wss://relay.nostr.band",
    "wss://nostr.wine",
];
const NOSTR_MAX_INBOX_RELAYS: usize = 5;
const NOSTR_DISCOVERY_CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
const NOSTR_INBOX_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);
const NOSTR_INBOX_CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
const NOSTR_NIP04_CONNECT_TIMEOUT: Duration = Duration::from_secs(8);

impl NostrProvider {
    pub(super) async fn connect_discovery_relays(
        &self,
        client: &Client,
    ) -> Result<Vec<RelayUrl>, NostrError> {
        for relay in &self.discovery_relays {
            if client
                .add_relay(relay)
                .capabilities(RelayCapabilities::DISCOVERY)
                .await
                .is_err()
            {
                tracing::warn!("Failed to add Nostr discovery relay");
            }
        }

        let output = client
            .try_connect()
            .timeout(NOSTR_DISCOVERY_CONNECT_TIMEOUT)
            .await;
        tracing::info!(
            connected_relays = output.success.len(),
            failed_relays = output.failed.len(),
            "Nostr discovery relay connection completed"
        );

        if output.success.is_empty() {
            return Err(NostrError::DiscoveryFailed(RelayFailures::from_output(
                &output.failed,
            )));
        }

        Ok(output.success.into_keys().collect())
    }

    pub(super) async fn discover_recipient_inbox_relays(
        &self,
        client: &Client,
        discovery_relays: &[RelayUrl],
        recipient: PublicKey,
    ) -> Result<Vec<RelayUrl>, NostrError> {
        let filter = Filter::new()
            .author(recipient)
            .kind(Kind::InboxRelays)
            .limit(1);

        let events = client
            .fetch_events(
                discovery_relays
                    .iter()
                    .cloned()
                    .map(|relay| (relay, vec![filter.clone()]))
                    .collect::<std::collections::HashMap<_, _>>(),
            )
            .timeout(NOSTR_INBOX_DISCOVERY_TIMEOUT)
            .await
            .map_err(|e| {
                if is_timeout(&e) {
                    NostrError::InboxDiscoveryTimedOut
                } else {
                    NostrError::InboxDiscoveryFailed(e)
                }
            })?;

        tracing::info!(
            event_count = events.len(),
            "Nostr recipient inbox relay discovery completed"
        );

        let Some(inbox_event) = events.first() else {
            return Err(NostrError::NoInboxRelays);
        };

        let (inbox_relays, discovered_relay_count) = dedupe_limited_relays(
            nip17::extract_relay_list(inbox_event),
            NOSTR_MAX_INBOX_RELAYS,
        );

        tracing::info!(
            discovered_relay_count,
            attempted_relay_count = inbox_relays.len(),
            max_relay_count = NOSTR_MAX_INBOX_RELAYS,
            "Discovered Nostr recipient inbox relays"
        );

        if inbox_relays.is_empty() {
            return Err(NostrError::NoInboxRelays);
        }

        Ok(inbox_relays)
    }

    pub(super) async fn connect_inbox_relays(
        &self,
        client: &Client,
        inbox_relays: &[RelayUrl],
    ) -> Result<Vec<RelayUrl>, NostrError> {
        let inbox_relays = selectable_inbox_relays(inbox_relays, self.onion_socks_proxy)?;
        let mut connected_relays = Vec::new();
        let mut failed_relays = RelayFailures::default();

        let results = stream::iter(inbox_relays)
            .map(|relay| async move {
                match client
                    .add_relay(relay.clone())
                    .capabilities(RelayCapabilities::WRITE)
                    .await
                {
                    Ok(_) => match client
                        .try_connect_relay(relay.clone(), NOSTR_INBOX_CONNECT_TIMEOUT)
                        .await
                    {
                        Ok(()) => Ok(relay),
                        Err(e) => Err((relay, e)),
                    },
                    Err(e) => Err((relay, e)),
                }
            })
            .buffer_unordered(NOSTR_SEND_CONCURRENCY)
            .collect::<Vec<_>>()
            .await;

        for result in results {
            match result {
                Ok(relay) => connected_relays.push(relay),
                Err((relay, error)) => failed_relays.push(relay, error),
            }
        }

        tracing::info!(
            connected_relays = connected_relays.len(),
            failed_relays = failed_relays.len(),
            "Nostr inbox relay connection completed"
        );

        if connected_relays.is_empty() {
            return Err(NostrError::InboxConnectFailed(failed_relays));
        }

        Ok(connected_relays)
    }

    pub(super) async fn connect_nip04_relays(
        &self,
        client: &Client,
    ) -> Result<Vec<RelayUrl>, NostrError> {
        let mut connected_relays = Vec::new();
        let mut failed_relays = RelayFailures::default();

        for relay in &self.nip04_relays {
            match client
                .add_relay(relay)
                .capabilities(RelayCapabilities::WRITE)
                .await
            {
                Ok(_) => match client
                    .try_connect_relay(relay, NOSTR_NIP04_CONNECT_TIMEOUT)
                    .await
                {
                    Ok(()) => match RelayUrl::parse(relay) {
                        Ok(url) => connected_relays.push(url),
                        Err(e) => failed_relays.push(relay, e),
                    },
                    Err(e) => failed_relays.push(relay, e),
                },
                Err(e) => failed_relays.push(relay, e),
            }
        }

        tracing::info!(
            connected_relays = connected_relays.len(),
            failed_relays = failed_relays.len(),
            "Nostr legacy NIP-04 relay connection completed"
        );

        if connected_relays.is_empty() {
            return Err(NostrError::LegacyRelayConnectFailed(failed_relays));
        }

        Ok(connected_relays)
    }
}

fn selectable_inbox_relays(
    relays: &[RelayUrl],
    onion_socks_proxy: Option<SocketAddr>,
) -> Result<Vec<RelayUrl>, NostrError> {
    let (onion, clearnet): (Vec<_>, Vec<_>) = relays
        .iter()
        .cloned()
        .partition(nostr::types::RelayUrl::is_onion);

    if onion_socks_proxy.is_some() {
        return Ok(relays.to_vec());
    }

    if clearnet.is_empty() {
        if onion.is_empty() {
            return Err(NostrError::NoInboxRelays);
        }
        return Err(NostrError::OnionSocksRequired);
    }

    if !onion.is_empty() {
        tracing::info!(
            skipped_onion_relays = onion.len(),
            "Skipping .onion Nostr inbox relays because CANARY_NOSTR_SOCKS_PROXY is not set"
        );
    }

    Ok(clearnet)
}

/// nostr-sdk's fetch error does not distinguish its own timeout from transport errors that
/// mention one, so inbox discovery keeps classifying timeouts by the error text.
fn is_timeout(error: &nostr_sdk::error::Error) -> bool {
    error.to_string().to_lowercase().contains("timeout")
}

fn dedupe_limited_relays<I>(relays: I, limit: usize) -> (Vec<RelayUrl>, usize)
where
    I: IntoIterator<Item = RelayUrl>,
{
    let mut seen = HashSet::new();
    let mut limited_relays = Vec::new();
    let mut discovered_relay_count = 0;

    for relay in relays {
        if !seen.insert(relay.clone()) {
            continue;
        }

        discovered_relay_count += 1;
        if limited_relays.len() < limit {
            limited_relays.push(relay);
        }
    }

    (limited_relays, discovered_relay_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn limits_recipient_inbox_relay_attempts() {
        let relays = [
            "wss://relay-1.example.com",
            "wss://relay-2.example.com",
            "wss://relay-1.example.com",
            "wss://relay-3.example.com",
            "wss://relay-4.example.com",
            "wss://relay-5.example.com",
            "wss://relay-6.example.com",
        ]
        .into_iter()
        .map(|relay| RelayUrl::parse(relay).unwrap());

        let (limited_relays, discovered_relay_count) = dedupe_limited_relays(relays, 3);

        assert_eq!(discovered_relay_count, 6);
        assert_eq!(limited_relays.len(), 3);
        assert_eq!(limited_relays[0].to_string(), "wss://relay-1.example.com");
        assert_eq!(limited_relays[1].to_string(), "wss://relay-2.example.com");
        assert_eq!(limited_relays[2].to_string(), "wss://relay-3.example.com");
    }

    #[test]
    fn selects_clearnet_inbox_relays_without_socks_and_keeps_onion_with_socks() {
        let clearnet = RelayUrl::parse("wss://relay.damus.io").unwrap();
        let onion =
            RelayUrl::parse("ws://oxtrdevav64z64yb7x6rjg4ntzqjhedm5b5zjqulugknhzr46ny2qbad.onion")
                .unwrap();
        let socks = SocketAddr::from((Ipv4Addr::LOCALHOST, 9050));

        let clearnet_only = [clearnet.clone()];
        let mixed = [clearnet.clone(), onion.clone()];
        let onion_first = [onion.clone(), clearnet.clone()];
        let onion_only = [onion.clone()];

        assert_eq!(
            selectable_inbox_relays(&clearnet_only, None).unwrap(),
            vec![clearnet.clone()]
        );
        assert_eq!(
            selectable_inbox_relays(&mixed, None).unwrap(),
            vec![clearnet.clone()]
        );
        assert_eq!(
            selectable_inbox_relays(&onion_first, Some(socks)).unwrap(),
            vec![onion.clone(), clearnet.clone()]
        );
        assert!(matches!(
            selectable_inbox_relays(&onion_only, None),
            Err(NostrError::OnionSocksRequired)
        ));
        assert_eq!(
            selectable_inbox_relays(&onion_only, Some(socks)).unwrap(),
            vec![onion]
        );
    }
}
