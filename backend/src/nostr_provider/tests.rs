//! End-to-end tests that drive the provider and client against a local WebSocket relay.

use super::client::nostr_client;
use super::{NostrDmMode, NostrProvider};
use nostr_sdk::prelude::*;
use std::time::Duration;

#[tokio::test]
async fn legacy_delivery_logs_do_not_identify_recipient() {
    use futures::{SinkExt, StreamExt};
    use std::sync::{Arc, Mutex};
    use tokio_tungstenite::tungstenite::Message;

    #[derive(Clone)]
    struct Capture(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let captured = Arc::new(Mutex::new(Vec::new()));
    let writer = Capture(captured.clone());
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_env_filter("off,canary::nostr_provider=trace")
        .with_writer(move || writer.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay = format!(
        "ws://{}/synthetic-private-relay",
        listener.local_addr().unwrap()
    );
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        while let Some(frame) = socket.next().await {
            let Message::Text(text) = frame.unwrap() else {
                continue;
            };
            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
            if value[0] == "EVENT" {
                let event: Event = serde_json::from_value(value[1].clone()).unwrap();
                socket
                    .send(Message::Text(
                        serde_json::json!(["OK", event.id.to_hex(), true, ""])
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
                // Keep the relay alive until the short-lived client shuts down.
                while socket.next().await.is_some() {}
                return event;
            }
        }
        panic!("No notification event received");
    });
    let sender = Keys::generate();
    let recipient = Keys::generate();
    let provider = NostrProvider {
        sender_keys: sender.clone(),
        discovery_relays: vec![],
        nip04_relays: vec![relay.clone()],
        metadata_db: None,
        onion_socks_proxy: None,
    };
    let message = "synthetic-private-wallet notification";
    let result = provider
        .send_test_message(recipient.public_key(), NostrDmMode::Nip04, message.into())
        .await;
    assert!(result.is_ok(), "Synthetic delivery must succeed");
    let event = tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
    event.verify().unwrap();
    let plaintext =
        nostr::nips::nip04::decrypt(recipient.secret_key(), &sender.public_key(), &event.content)
            .unwrap();
    assert_eq!(plaintext, message);
    let logs = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
    assert!(logs.contains("Publishing legacy NIP-04 Nostr DM"));
    for sensitive in [
        recipient.public_key().to_hex(),
        sender.public_key().to_hex(),
        relay,
        message.into(),
    ] {
        assert!(
            !logs.contains(&sensitive),
            "Sensitive fixture appeared in application logs"
        );
    }
}

#[tokio::test]
async fn signed_nostr_client_can_answer_relay_auth() {
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay = RelayUrl::parse(&format!("ws://{}/chat", listener.local_addr().unwrap())).unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        socket
            .send(Message::Text(r#"["AUTH","challenge-1"]"#.into()))
            .await
            .unwrap();
        while let Some(message) = socket.next().await {
            let message = message.unwrap();
            if let Message::Text(text) = message {
                let response: serde_json::Value = serde_json::from_str(&text).unwrap();
                if response[0] == "AUTH" {
                    let event: Event = serde_json::from_value(response[1].clone()).unwrap();
                    socket
                        .send(Message::Text(
                            serde_json::json!(["OK", event.id.to_hex(), true, ""])
                                .to_string()
                                .into(),
                        ))
                        .await
                        .unwrap();
                    return event;
                }
            }
        }
        panic!("client disconnected without authenticating");
    });
    let keys = Keys::generate();
    let client = nostr_client(keys.clone(), None);
    client
        .add_relay(relay.clone())
        .capabilities(RelayCapabilities::WRITE)
        .await
        .unwrap();
    client
        .try_connect_relay(relay.clone(), Duration::from_secs(5))
        .await
        .unwrap();
    let mut server = server;
    let result = tokio::time::timeout(Duration::from_secs(5), &mut server).await;
    server.abort();
    client.shutdown().await;
    let event = result
        .expect("client must answer the relay AUTH challenge")
        .unwrap();
    assert_eq!(event.pubkey, keys.public_key());
    assert_eq!(event.kind, Kind::Authentication);
    event.verify().unwrap();
    assert!(event
        .tags
        .iter()
        .any(|tag| tag.as_slice() == ["relay", relay.as_str()]));
    assert!(event
        .tags
        .iter()
        .any(|tag| tag.as_slice() == ["challenge", "challenge-1"]));
}
