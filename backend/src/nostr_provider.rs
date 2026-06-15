use crate::message_formatter::MessageFormatter;
use crate::metadata::{
    Contact, Language, MetadataDb, NotificationMethod, ProviderType, TransactionNotification,
};
use crate::notifications::{
    notification_methods_for_provider, NotificationProvider, NotificationResult, ProviderInfo,
};
use crate::tls::install_default_rustls_crypto_provider;
use anyhow::Result;
use async_trait::async_trait;
use futures::{stream, StreamExt};
use nostr_gossip_memory::prelude::NostrGossipMemory;
use nostr_sdk::client::Error as NostrClientError;
use nostr_sdk::prelude::*;
use serde_json::json;
use std::time::Duration;

const NOSTR_SENDER_SECRET_KEY: &str = "nostr_sender_secret_key";
const DEFAULT_DISCOVERY_RELAYS: [&str; 3] = [
    "wss://purplepag.es",
    "wss://relay.damus.io",
    "wss://relay.nostr.band",
];
const NOSTR_SEND_CONCURRENCY: usize = 3;
const NOSTR_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
pub const NOSTR_NO_DM_RELAYS_ERROR_CODE: &str = "nostr_no_dm_relays";
pub const NOSTR_SEND_FAILED_ERROR_CODE: &str = "nostr_send_failed";
pub const NOSTR_SEND_TIMEOUT_ERROR_CODE: &str = "nostr_send_timeout";

#[derive(Clone)]
pub struct NostrSenderKeys {
    keys: Keys,
    pub sender_npub: String,
}

impl std::fmt::Debug for NostrSenderKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NostrSenderKeys")
            .field("sender_npub", &self.sender_npub)
            .field("keys", &"[redacted]")
            .finish()
    }
}

pub fn parse_nostr_recipient_or_error(input: &str) -> Result<PublicKey, String> {
    let value = input.trim();
    if value.is_empty() {
        return Err("Nostr recipient cannot be empty".to_string());
    }

    if value.to_lowercase().starts_with("nsec") {
        return Err("Enter a recipient npub, not a private nsec key".to_string());
    }

    PublicKey::parse(value)
        .map_err(|_| "Enter a valid Nostr public key as npub or 64-character hex".to_string())
}

pub fn canonicalize_nostr_public_key(input: &str) -> Result<(String, String), String> {
    let public_key = parse_nostr_recipient_or_error(input)?;
    let npub = public_key
        .to_bech32()
        .map_err(|_| "Failed to format Nostr public key".to_string())?;
    Ok((public_key.to_hex(), npub))
}

pub fn nostr_display_target(hex_public_key: &str) -> Option<String> {
    PublicKey::parse(hex_public_key)
        .ok()
        .and_then(|public_key| public_key.to_bech32().ok())
}

pub async fn ensure_nostr_sender_keys(metadata_db: &MetadataDb) -> Result<NostrSenderKeys> {
    if let Some(secret_hex) = metadata_db
        .get_instance_secret(NOSTR_SENDER_SECRET_KEY)
        .await?
    {
        return sender_keys_from_secret(secret_hex);
    }

    let keys = Keys::generate();
    let secret_hex = keys.secret_key().to_secret_hex();
    metadata_db
        .set_instance_secret_if_absent(NOSTR_SENDER_SECRET_KEY, &secret_hex)
        .await?;

    let secret_hex = metadata_db
        .get_instance_secret(NOSTR_SENDER_SECRET_KEY)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Nostr sender key missing after insert"))?;
    sender_keys_from_secret(secret_hex)
}

fn sender_keys_from_secret(secret_hex: String) -> Result<NostrSenderKeys> {
    let keys = Keys::parse(&secret_hex)?;
    let sender_npub = keys.public_key().to_bech32()?;
    Ok(NostrSenderKeys { keys, sender_npub })
}

pub struct NostrProvider {
    sender_keys: Keys,
    discovery_relays: Vec<String>,
    send_timeout: Duration,
}

impl NostrProvider {
    pub fn new(sender_keys: NostrSenderKeys) -> Self {
        Self {
            sender_keys: sender_keys.keys,
            discovery_relays: DEFAULT_DISCOVERY_RELAYS
                .iter()
                .map(|relay| relay.to_string())
                .collect(),
            send_timeout: Duration::from_secs(20),
        }
    }

    pub async fn send_test_message(
        &self,
        recipient: PublicKey,
        message: String,
    ) -> NotificationResult {
        self.send_nip17_message(recipient, message).await
    }

    async fn send_nip17_message(
        &self,
        recipient: PublicKey,
        message: String,
    ) -> NotificationResult {
        install_default_rustls_crypto_provider();

        let keys = self.sender_keys.clone();

        // Keep the client short-lived for v1 so relay/gossip state does not outlive a single
        // send attempt. A reused client can be introduced later if Nostr fan-out becomes common.
        let gossip = NostrGossipMemory::unbounded();
        let client = Client::builder().signer(keys).gossip(gossip).build();

        for relay in &self.discovery_relays {
            if let Err(e) = client.add_discovery_relay(relay).await {
                tracing::warn!("Failed to add Nostr discovery relay {}: {}", relay, e);
            }
        }

        let send = tokio::time::timeout(self.send_timeout, async {
            client.connect().await;
            client.send_private_msg(recipient, message, vec![]).await
        })
        .await;

        // Shutdown is best-effort cleanup for this short-lived client; send result is reported above.
        let _ = tokio::time::timeout(NOSTR_SHUTDOWN_TIMEOUT, client.shutdown()).await;

        match send {
            Ok(Ok(output)) => NotificationResult {
                success: true,
                provider_id: Some(output.val.to_hex()),
                error_message: None,
            },
            Ok(Err(NostrClientError::PrivateMsgRelaysNotFound)) => NotificationResult {
                success: false,
                provider_id: None,
                error_message: Some(
                    "Recipient has no kind 10050 Nostr DM inbox relay list".to_string(),
                ),
            },
            Ok(Err(e)) => NotificationResult {
                success: false,
                provider_id: None,
                error_message: Some(format!("Nostr send failed: {}", e)),
            },
            Err(_) => NotificationResult {
                success: false,
                provider_id: None,
                error_message: Some("Nostr send timed out".to_string()),
            },
        }
    }
}

#[async_trait]
impl NotificationProvider for NostrProvider {
    async fn send_notification(
        &self,
        notification: &TransactionNotification,
        wallet_name: &str,
        contacts: &[Contact],
        user_language: &Language,
        wallet_balance_sats: Option<i64>,
    ) -> Vec<(NotificationMethod, NotificationResult, String)> {
        let send_jobs: Vec<(NotificationMethod, String)> =
            notification_methods_for_provider(contacts, &ProviderType::Nostr)
                .map(|(contact, method)| {
                    let message = MessageFormatter::create_localized_message(
                        notification,
                        wallet_name,
                        user_language,
                        contact.include_wallet_balance_in_tx_notifications,
                        wallet_balance_sats,
                    );
                    (method.clone(), message)
                })
                .collect();

        let send_tasks = send_jobs.into_iter().map(|(method, message)| async move {
            let result = match PublicKey::parse(&method.notification_target) {
                Ok(public_key) => self.send_nip17_message(public_key, message.clone()).await,
                Err(_) => NotificationResult {
                    success: false,
                    provider_id: None,
                    error_message: Some("Invalid Nostr recipient public key".to_string()),
                },
            };

            (method, result, message)
        });

        stream::iter(send_tasks)
            .buffer_unordered(NOSTR_SEND_CONCURRENCY)
            .collect()
            .await
    }

    fn provider_info(&self) -> ProviderInfo {
        ProviderInfo {
            name: "nostr".to_string(),
            display_name: "Nostr DM".to_string(),
            config_schema: json!({
                "type": "object",
                "properties": {
                    "recipient": {
                        "type": "string",
                        "title": "Nostr recipient",
                        "description": "Recipient npub or hex public key"
                    }
                },
                "required": ["recipient"]
            }),
        }
    }

    fn name(&self) -> &'static str {
        "nostr"
    }
}

pub fn normalize_nostr_recipient_or_error(input: &str) -> Result<String, String> {
    canonicalize_nostr_public_key(input).map(|(hex, _)| hex)
}

pub fn nostr_test_message() -> String {
    "This is a test Nostr DM from Canary Wallet.".to_string()
}

pub fn nostr_test_error_code(error_message: Option<&str>) -> Option<&'static str> {
    match error_message {
        Some("Recipient has no kind 10050 Nostr DM inbox relay list") => {
            Some(NOSTR_NO_DM_RELAYS_ERROR_CODE)
        }
        Some("Nostr send timed out") => Some(NOSTR_SEND_TIMEOUT_ERROR_CODE),
        Some(message) if message.starts_with("Nostr send failed:") => {
            Some(NOSTR_SEND_FAILED_ERROR_CODE)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_npub_and_hex_public_keys() {
        let keys = Keys::generate();
        let public_key = keys.public_key();
        let hex = public_key.to_hex();
        let npub = public_key.to_bech32().unwrap();

        assert_eq!(
            canonicalize_nostr_public_key(&format!(" {} ", npub)).unwrap(),
            (hex.clone(), npub.clone())
        );
        assert_eq!(
            canonicalize_nostr_public_key(&hex.to_uppercase()).unwrap(),
            (hex, npub)
        );
    }

    #[test]
    fn rejects_empty_invalid_and_private_keys() {
        assert!(canonicalize_nostr_public_key("   ")
            .unwrap_err()
            .contains("cannot be empty"));
        assert!(canonicalize_nostr_public_key("nsec1not-a-recipient")
            .unwrap_err()
            .contains("not a private nsec key"));
        assert!(canonicalize_nostr_public_key("not-a-public-key")
            .unwrap_err()
            .contains("valid Nostr public key"));
    }

    #[test]
    fn formats_display_target_from_stored_hex_public_key() {
        let public_key = Keys::generate().public_key();
        let npub = public_key.to_bech32().unwrap();

        assert_eq!(nostr_display_target(&public_key.to_hex()), Some(npub));
        assert_eq!(nostr_display_target("not-a-public-key"), None);
    }

    #[test]
    fn maps_known_test_send_errors_to_codes() {
        assert_eq!(
            nostr_test_error_code(Some("Nostr send timed out")),
            Some(NOSTR_SEND_TIMEOUT_ERROR_CODE)
        );
        assert_eq!(
            nostr_test_error_code(Some(
                "Recipient has no kind 10050 Nostr DM inbox relay list"
            )),
            Some(NOSTR_NO_DM_RELAYS_ERROR_CODE)
        );
        assert_eq!(
            nostr_test_error_code(Some("Nostr send failed: relay disconnected")),
            Some(NOSTR_SEND_FAILED_ERROR_CODE)
        );
        assert_eq!(nostr_test_error_code(Some("different error")), None);
    }
}
