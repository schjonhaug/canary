//! Short-lived signing Nostr clients and the optional SOCKS proxy for `.onion` relays.

use super::errors::NostrError;
use nostr_sdk::prelude::*;
use std::net::{Ipv4Addr, SocketAddr};

pub(super) const NOSTR_SOCKS_PROXY_ENV: &str = "CANARY_NOSTR_SOCKS_PROXY";

pub(super) fn nostr_client(keys: Keys, onion_socks_proxy: Option<SocketAddr>) -> Client {
    let mut builder =
        Client::builder().authenticator(nostr_sdk::authenticator::SignerAuthenticator::new(keys));
    if let Some(addr) = onion_socks_proxy {
        tracing::info!(
            socks_proxy = %addr,
            "Routing .onion Nostr inbox relays through SOCKS"
        );
        builder = builder.proxy(Proxy::onion(addr));
    }
    builder.build()
}

/// Parses `CANARY_NOSTR_SOCKS_PROXY`: `host:port` with an optional `socks5h://`, `socks5://` or
/// `socks://` prefix, where the host is an IP address or `localhost`. An empty value means no
/// proxy.
pub fn parse_nostr_onion_socks_proxy(value: &str) -> Result<Option<SocketAddr>, NostrError> {
    let trimmed = value.trim();
    let invalid = || NostrError::InvalidSocksProxy {
        value: trimmed.to_string(),
    };
    if trimmed.is_empty() {
        return Ok(None);
    }

    let without_scheme = trimmed
        .strip_prefix("socks5h://")
        .or_else(|| trimmed.strip_prefix("socks5://"))
        .or_else(|| trimmed.strip_prefix("socks://"))
        .unwrap_or(trimmed);

    if let Ok(addr) = without_scheme.parse::<SocketAddr>() {
        return Ok(Some(addr));
    }

    let Some((host, port_str)) = without_scheme.rsplit_once(':') else {
        return Err(invalid());
    };

    let port: u16 = port_str.parse().map_err(|_| invalid())?;
    let host = host.trim_matches(|c| c == '[' || c == ']');
    if host.eq_ignore_ascii_case("localhost") {
        return Ok(Some(SocketAddr::from((Ipv4Addr::LOCALHOST, port))));
    }

    Err(invalid())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nostr_onion_socks_proxy_addresses() {
        assert_eq!(parse_nostr_onion_socks_proxy("").unwrap(), None);
        assert_eq!(parse_nostr_onion_socks_proxy("  ").unwrap(), None);
        assert_eq!(
            parse_nostr_onion_socks_proxy("127.0.0.1:9050").unwrap(),
            Some(SocketAddr::from((Ipv4Addr::LOCALHOST, 9050)))
        );
        assert_eq!(
            parse_nostr_onion_socks_proxy("localhost:9050").unwrap(),
            Some(SocketAddr::from((Ipv4Addr::LOCALHOST, 9050)))
        );
        assert_eq!(
            parse_nostr_onion_socks_proxy("socks5://127.0.0.1:9050").unwrap(),
            Some(SocketAddr::from((Ipv4Addr::LOCALHOST, 9050)))
        );
        assert_eq!(
            parse_nostr_onion_socks_proxy("[::1]:9050").unwrap(),
            Some("[::1]:9050".parse().unwrap())
        );
        assert!(parse_nostr_onion_socks_proxy("not-a-proxy").is_err());
        assert!(parse_nostr_onion_socks_proxy("example.com:9050").is_err());
    }

    #[test]
    fn nip42_auth_event_uses_the_connected_relay_url_including_path() {
        let relay = RelayUrl::parse("ws://haven.local/chat").unwrap();
        let event = nostr::nips::nip42::ClientAuthentication::new("challenge-1", relay.clone())
            .finalize(&Keys::generate())
            .unwrap();

        assert_eq!(event.kind, Kind::Authentication);
        assert!(event
            .tags
            .iter()
            .any(|tag| { tag.as_slice() == ["relay", relay.as_str()] }));
        assert!(event
            .tags
            .iter()
            .any(|tag| { tag.as_slice() == ["challenge", "challenge-1"] }));
    }

    #[tokio::test]
    async fn onion_inbox_relays_use_configured_socks_proxy() {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 9050));
        let client = nostr_client(Keys::generate(), Some(addr));
        let clearnet = "wss://relay.damus.io";
        let onion = "ws://oxtrdevav64z64yb7x6rjg4ntzqjhedm5b5zjqulugknhzr46ny2qbad.onion";

        client.add_relay(clearnet).await.unwrap();
        client.add_relay(onion).await.unwrap();

        let clearnet_relay = client.relay(clearnet).await.unwrap().unwrap();
        assert!(clearnet_relay.proxy().is_none());

        let onion_relay = client.relay(onion).await.unwrap().unwrap();
        assert_eq!(onion_relay.proxy(), Some(addr));
    }
}
