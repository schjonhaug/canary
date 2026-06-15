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
use nostr_sdk::client::Error as NostrClientError;
use nostr_sdk::nips::{nip04, nip17};
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;
use std::str::FromStr;
use std::time::Duration;

const NOSTR_SENDER_SECRET_KEY: &str = "nostr_sender_secret_key";
const NOSTR_DM_MODE_SETTING_KEY: &str = "nostr_dm_mode";
const DEFAULT_DISCOVERY_RELAYS: [&str; 3] = [
    "wss://purplepag.es",
    "wss://relay.damus.io",
    "wss://relay.nostr.band",
];
const DEFAULT_NIP04_RELAYS: [&str; 5] = [
    "wss://relay.primal.net",
    "wss://relay.damus.io",
    "wss://nos.lol",
    "wss://relay.nostr.band",
    "wss://nostr.wine",
];
const NOSTR_SEND_CONCURRENCY: usize = 3;
const NOSTR_DISCOVERY_CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
const NOSTR_INBOX_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);
const NOSTR_INBOX_CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
const NOSTR_NIP04_CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
const NOSTR_PUBLISH_TIMEOUT: Duration = Duration::from_secs(10);
const NOSTR_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
pub const NOSTR_DISCOVERY_FAILED_ERROR_CODE: &str = "nostr_discovery_failed";
pub const NOSTR_INBOX_CONNECT_FAILED_ERROR_CODE: &str = "nostr_inbox_connect_failed";
pub const NOSTR_INBOX_DISCOVERY_TIMEOUT_ERROR_CODE: &str = "nostr_inbox_discovery_timeout";
pub const NOSTR_NO_DM_RELAYS_ERROR_CODE: &str = "nostr_no_dm_relays";
pub const NOSTR_PUBLISH_TIMEOUT_ERROR_CODE: &str = "nostr_publish_timeout";
pub const NOSTR_SEND_FAILED_ERROR_CODE: &str = "nostr_send_failed";
pub const NOSTR_NIP04_FAILED_ERROR_CODE: &str = "nostr_nip04_failed";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NostrDmMode {
    Auto,
    Nip17,
    Nip04,
}

impl NostrDmMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Nip17 => "nip17",
            Self::Nip04 => "nip04",
        }
    }
}

impl Default for NostrDmMode {
    fn default() -> Self {
        Self::Auto
    }
}

impl FromStr for NostrDmMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "nip17" => Ok(Self::Nip17),
            "nip04" => Ok(Self::Nip04),
            _ => Err("Unsupported Nostr DM mode".to_string()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NostrSendSuccess {
    pub event_id: EventId,
    pub dm_mode_used: NostrDmMode,
}

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
    nip04_relays: Vec<String>,
    metadata_db: Option<MetadataDb>,
}

impl NostrProvider {
    pub fn new(sender_keys: NostrSenderKeys) -> Self {
        Self::with_metadata_db(sender_keys, None)
    }

    pub fn with_metadata_db(sender_keys: NostrSenderKeys, metadata_db: Option<MetadataDb>) -> Self {
        Self {
            sender_keys: sender_keys.keys,
            discovery_relays: DEFAULT_DISCOVERY_RELAYS
                .iter()
                .map(|relay| relay.to_string())
                .collect(),
            nip04_relays: DEFAULT_NIP04_RELAYS
                .iter()
                .map(|relay| relay.to_string())
                .collect(),
            metadata_db,
        }
    }

    pub async fn send_test_message(
        &self,
        recipient: PublicKey,
        dm_mode: NostrDmMode,
    ) -> (NotificationResult, Option<NostrDmMode>) {
        let result = self.send_test_message_for_mode(recipient, dm_mode).await;
        match result {
            Ok(success) => (
                NotificationResult {
                    success: true,
                    provider_id: Some(success.event_id.to_hex()),
                    error_message: None,
                },
                Some(success.dm_mode_used),
            ),
            Err(error_message) => (self.nostr_error_result(error_message), None),
        }
    }

    async fn send_test_message_for_mode(
        &self,
        recipient: PublicKey,
        dm_mode: NostrDmMode,
    ) -> Result<NostrSendSuccess, String> {
        install_default_rustls_crypto_provider();

        match dm_mode {
            NostrDmMode::Auto => {
                match self
                    .send_nip17_message(recipient, nostr_test_message(NostrDmMode::Nip17))
                    .await
                {
                    Ok(success) => Ok(success),
                    Err(error) if is_missing_nip17_inbox_error(&error) => {
                        tracing::info!(
                            recipient = %recipient.to_hex(),
                            "Falling back to legacy NIP-04 Nostr test DM"
                        );
                        self.send_nip04_message(recipient, nostr_test_message(NostrDmMode::Nip04))
                            .await
                    }
                    Err(error) => Err(error),
                }
            }
            NostrDmMode::Nip17 => {
                self.send_nip17_message(recipient, nostr_test_message(NostrDmMode::Nip17))
                    .await
            }
            NostrDmMode::Nip04 => {
                self.send_nip04_message(recipient, nostr_test_message(NostrDmMode::Nip04))
                    .await
            }
        }
    }

    async fn stored_dm_mode(&self) -> NostrDmMode {
        let Some(metadata_db) = &self.metadata_db else {
            return NostrDmMode::default();
        };

        metadata_db
            .get_instance_setting(NOSTR_DM_MODE_SETTING_KEY)
            .await
            .ok()
            .flatten()
            .and_then(|value| NostrDmMode::from_str(&value).ok())
            .unwrap_or_default()
    }

    async fn send_message(
        &self,
        recipient: PublicKey,
        message: String,
        dm_mode: NostrDmMode,
    ) -> Result<NostrSendSuccess, String> {
        install_default_rustls_crypto_provider();

        match dm_mode {
            NostrDmMode::Auto => match self.send_nip17_message(recipient, message.clone()).await {
                Ok(success) => Ok(success),
                Err(error) if is_missing_nip17_inbox_error(&error) => {
                    tracing::info!(
                        recipient = %recipient.to_hex(),
                        "Falling back to legacy NIP-04 Nostr DM"
                    );
                    self.send_nip04_message(recipient, message).await
                }
                Err(error) => Err(error),
            },
            NostrDmMode::Nip17 => self.send_nip17_message(recipient, message).await,
            NostrDmMode::Nip04 => self.send_nip04_message(recipient, message).await,
        }
    }

    async fn send_nip17_message(
        &self,
        recipient: PublicKey,
        message: String,
    ) -> Result<NostrSendSuccess, String> {
        let keys = self.sender_keys.clone();

        // Keep the client short-lived for v1 so relay state does not outlive a single send attempt.
        // The NIP-17 phases are explicit so each failure can produce an actionable user message.
        let client = Client::builder().signer(keys).build();

        let send = self
            .send_nip17_message_with_client(&client, recipient, message)
            .await;

        // Shutdown is best-effort cleanup for this short-lived client; send result is reported above.
        let _ = tokio::time::timeout(NOSTR_SHUTDOWN_TIMEOUT, client.shutdown()).await;

        match send {
            Ok(output) => Ok(NostrSendSuccess {
                event_id: output.val,
                dm_mode_used: NostrDmMode::Nip17,
            }),
            Err(error_message) => Err(error_message),
        }
    }

    async fn send_nip04_message(
        &self,
        recipient: PublicKey,
        message: String,
    ) -> Result<NostrSendSuccess, String> {
        let keys = self.sender_keys.clone();
        let client = Client::builder().signer(keys.clone()).build();
        let relays = self.connect_nip04_relays(&client).await?;

        let event = build_nip04_dm_event(&keys, recipient, message).await?;

        tracing::info!(
            recipient = %recipient.to_hex(),
            relay_count = relays.len(),
            "Publishing legacy NIP-04 Nostr DM"
        );

        let output =
            tokio::time::timeout(NOSTR_PUBLISH_TIMEOUT, client.send_event_to(relays, &event))
                .await
                .map_err(|_| "Nostr legacy DM publish timed out".to_string())?
                .map_err(|e| format!("Nostr legacy DM publish failed: {}", e))?;

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
            event_id: output.val,
            dm_mode_used: NostrDmMode::Nip04,
        })
    }

    async fn send_nip17_message_with_client(
        &self,
        client: &Client,
        recipient: PublicKey,
        message: String,
    ) -> Result<Output<EventId>, String> {
        let discovery_relays = self.connect_discovery_relays(client).await?;
        let inbox_relays = self
            .discover_recipient_inbox_relays(client, &discovery_relays, recipient)
            .await?;
        let connected_inbox_relays = self.connect_inbox_relays(client, &inbox_relays).await?;

        tracing::info!(
            recipient = %recipient.to_hex(),
            relay_count = connected_inbox_relays.len(),
            "Publishing Nostr DM to recipient inbox relays"
        );

        let output = tokio::time::timeout(
            NOSTR_PUBLISH_TIMEOUT,
            client.send_private_msg_to(connected_inbox_relays, recipient, message, vec![]),
        )
        .await
        .map_err(|_| "Nostr publish timed out".to_string())?
        .map_err(|e| format!("Nostr publish failed: {}", e))?;

        if output.success.is_empty() {
            let failed_relays = output
                .failed
                .iter()
                .map(|(url, error)| format!("{url}: {error}"))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(format!(
                "Nostr publish failed: {}",
                if failed_relays.is_empty() {
                    "no relay accepted the message".to_string()
                } else {
                    failed_relays
                }
            ));
        }

        Ok(output)
    }

    async fn connect_discovery_relays(&self, client: &Client) -> Result<Vec<RelayUrl>, String> {
        for relay in &self.discovery_relays {
            if let Err(e) = client.add_discovery_relay(relay).await {
                tracing::warn!("Failed to add Nostr discovery relay {}: {}", relay, e);
            }
        }

        let output = client.try_connect(NOSTR_DISCOVERY_CONNECT_TIMEOUT).await;
        tracing::info!(
            connected_relays = output.success.len(),
            failed_relays = output.failed.len(),
            "Nostr discovery relay connection completed"
        );

        if output.success.is_empty() {
            return Err(format!(
                "Nostr discovery relays failed: {}",
                format_relay_failures(&output.failed, "no relays connected")
            ));
        }

        Ok(output.success.into_iter().collect())
    }

    async fn discover_recipient_inbox_relays(
        &self,
        client: &Client,
        discovery_relays: &[RelayUrl],
        recipient: PublicKey,
    ) -> Result<Vec<RelayUrl>, String> {
        let filter = Filter::new()
            .author(recipient)
            .kind(Kind::InboxRelays)
            .limit(1);

        let events = client
            .fetch_events_from(
                discovery_relays.iter().cloned(),
                filter,
                NOSTR_INBOX_DISCOVERY_TIMEOUT,
            )
            .await
            .map_err(|e| {
                if e.to_string().to_lowercase().contains("timeout") {
                    "Nostr inbox relay discovery timed out".to_string()
                } else {
                    format!("Nostr inbox relay discovery failed: {}", e)
                }
            })?;

        tracing::info!(
            recipient = %recipient.to_hex(),
            event_count = events.len(),
            "Nostr recipient inbox relay discovery completed"
        );

        let Some(inbox_event) = events.first() else {
            return Err("Recipient has no kind 10050 Nostr DM inbox relay list".to_string());
        };

        let mut seen = HashSet::new();
        let inbox_relays: Vec<RelayUrl> = nip17::extract_relay_list(inbox_event)
            .filter_map(|relay| {
                if seen.insert(relay.clone()) {
                    Some(relay.clone())
                } else {
                    None
                }
            })
            .collect();

        tracing::info!(
            recipient = %recipient.to_hex(),
            relay_count = inbox_relays.len(),
            "Discovered Nostr recipient inbox relays"
        );

        if inbox_relays.is_empty() {
            return Err("Recipient has no kind 10050 Nostr DM inbox relay list".to_string());
        }

        Ok(inbox_relays)
    }

    async fn connect_inbox_relays(
        &self,
        client: &Client,
        inbox_relays: &[RelayUrl],
    ) -> Result<Vec<RelayUrl>, String> {
        let mut connected_relays = Vec::new();
        let mut failed_relays = Vec::new();

        for relay in inbox_relays {
            match client.add_write_relay(relay.clone()).await {
                Ok(_) => match client
                    .try_connect_relay(relay.clone(), NOSTR_INBOX_CONNECT_TIMEOUT)
                    .await
                {
                    Ok(_) => connected_relays.push(relay.clone()),
                    Err(e) => failed_relays.push(format!("{relay}: {e}")),
                },
                Err(e) => failed_relays.push(format!("{relay}: {e}")),
            }
        }

        tracing::info!(
            connected_relays = connected_relays.len(),
            failed_relays = failed_relays.len(),
            "Nostr inbox relay connection completed"
        );

        if connected_relays.is_empty() {
            return Err(format!(
                "Nostr inbox relay connection failed: {}",
                if failed_relays.is_empty() {
                    "no recipient inbox relays connected".to_string()
                } else {
                    failed_relays.join("; ")
                }
            ));
        }

        Ok(connected_relays)
    }

    async fn connect_nip04_relays(&self, client: &Client) -> Result<Vec<RelayUrl>, String> {
        let mut connected_relays = Vec::new();
        let mut failed_relays = Vec::new();

        for relay in &self.nip04_relays {
            match client.add_write_relay(relay).await {
                Ok(_) => match client
                    .try_connect_relay(relay, NOSTR_NIP04_CONNECT_TIMEOUT)
                    .await
                {
                    Ok(_) => match RelayUrl::parse(relay) {
                        Ok(url) => connected_relays.push(url),
                        Err(e) => failed_relays.push(format!("{relay}: {e}")),
                    },
                    Err(e) => failed_relays.push(format!("{relay}: {e}")),
                },
                Err(e) => failed_relays.push(format!("{relay}: {e}")),
            }
        }

        tracing::info!(
            connected_relays = connected_relays.len(),
            failed_relays = failed_relays.len(),
            "Nostr legacy NIP-04 relay connection completed"
        );

        if connected_relays.is_empty() {
            return Err(format!(
                "Nostr legacy DM relay connection failed: {}",
                if failed_relays.is_empty() {
                    "no legacy DM relays connected".to_string()
                } else {
                    failed_relays.join("; ")
                }
            ));
        }

        Ok(connected_relays)
    }

    fn nostr_error_result(&self, error_message: String) -> NotificationResult {
        let error_message =
            if error_message == NostrClientError::PrivateMsgRelaysNotFound.to_string() {
                "Recipient has no kind 10050 Nostr DM inbox relay list".to_string()
            } else if error_message.starts_with("Nostr discovery relays failed:")
                || error_message.starts_with("Nostr inbox relay connection failed:")
                || error_message == "Nostr inbox relay discovery timed out"
                || error_message == "Recipient has no kind 10050 Nostr DM inbox relay list"
                || error_message == "Nostr publish timed out"
                || error_message.starts_with("Nostr legacy DM")
            {
                error_message
            } else {
                format!("Nostr send failed: {}", error_message)
            };

        NotificationResult {
            success: false,
            provider_id: None,
            error_message: Some(error_message),
        }
    }
}

fn is_missing_nip17_inbox_error(error_message: &str) -> bool {
    error_message == "Recipient has no kind 10050 Nostr DM inbox relay list"
        || error_message == NostrClientError::PrivateMsgRelaysNotFound.to_string()
}

async fn build_nip04_dm_event(
    keys: &Keys,
    recipient: PublicKey,
    message: String,
) -> Result<Event, String> {
    let encrypted = nip04::encrypt(keys.secret_key(), &recipient, message)
        .map_err(|e| format!("Nostr legacy DM encryption failed: {}", e))?;
    EventBuilder::new(Kind::EncryptedDirectMessage, encrypted)
        .tag(Tag::public_key(recipient))
        .sign(keys)
        .await
        .map_err(|e| format!("Nostr legacy DM signing failed: {}", e))
}

pub async fn get_nostr_dm_mode(metadata_db: &MetadataDb) -> Result<NostrDmMode> {
    Ok(metadata_db
        .get_instance_setting(NOSTR_DM_MODE_SETTING_KEY)
        .await?
        .and_then(|value| NostrDmMode::from_str(&value).ok())
        .unwrap_or_default())
}

pub async fn set_nostr_dm_mode(metadata_db: &MetadataDb, dm_mode: NostrDmMode) -> Result<()> {
    metadata_db
        .set_instance_setting(NOSTR_DM_MODE_SETTING_KEY, dm_mode.as_str())
        .await
}

fn format_relay_failures(
    failures: &std::collections::HashMap<RelayUrl, String>,
    empty_message: &str,
) -> String {
    if failures.is_empty() {
        return empty_message.to_string();
    }

    failures
        .iter()
        .map(|(url, error)| format!("{url}: {error}"))
        .collect::<Vec<_>>()
        .join("; ")
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
                Ok(public_key) => {
                    match self
                        .send_message(public_key, message.clone(), self.stored_dm_mode().await)
                        .await
                    {
                        Ok(success) => NotificationResult {
                            success: true,
                            provider_id: Some(success.event_id.to_hex()),
                            error_message: None,
                        },
                        Err(error_message) => self.nostr_error_result(error_message),
                    }
                }
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

pub fn nostr_test_message(dm_mode_used: NostrDmMode) -> String {
    format!(
        "This is a test Nostr DM from Canary Wallet.\n\nDM format: {}.",
        match dm_mode_used {
            NostrDmMode::Auto => "Auto",
            NostrDmMode::Nip17 => "Modern NIP-17",
            NostrDmMode::Nip04 => "Legacy NIP-04",
        }
    )
}

pub fn nostr_test_error_code(error_message: Option<&str>) -> Option<&'static str> {
    match error_message {
        Some(message) if message.starts_with("Nostr discovery relays failed:") => {
            Some(NOSTR_DISCOVERY_FAILED_ERROR_CODE)
        }
        Some(message) if message.starts_with("Nostr inbox relay connection failed:") => {
            Some(NOSTR_INBOX_CONNECT_FAILED_ERROR_CODE)
        }
        Some("Nostr inbox relay discovery timed out") => {
            Some(NOSTR_INBOX_DISCOVERY_TIMEOUT_ERROR_CODE)
        }
        Some("Recipient has no kind 10050 Nostr DM inbox relay list") => {
            Some(NOSTR_NO_DM_RELAYS_ERROR_CODE)
        }
        Some("Nostr publish timed out") => Some(NOSTR_PUBLISH_TIMEOUT_ERROR_CODE),
        Some(message) if message.starts_with("Nostr send failed:") => {
            Some(NOSTR_SEND_FAILED_ERROR_CODE)
        }
        Some(message) if message.starts_with("Nostr publish failed:") => {
            Some(NOSTR_SEND_FAILED_ERROR_CODE)
        }
        Some(message) if message.starts_with("Nostr inbox relay discovery failed:") => {
            Some(NOSTR_SEND_FAILED_ERROR_CODE)
        }
        Some(message) if message.starts_with("Nostr legacy DM") => {
            Some(NOSTR_NIP04_FAILED_ERROR_CODE)
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
    fn parses_nostr_dm_modes() {
        assert_eq!("auto".parse::<NostrDmMode>().unwrap(), NostrDmMode::Auto);
        assert_eq!("nip17".parse::<NostrDmMode>().unwrap(), NostrDmMode::Nip17);
        assert_eq!("NIP04".parse::<NostrDmMode>().unwrap(), NostrDmMode::Nip04);
        assert!("kind4".parse::<NostrDmMode>().is_err());
    }

    #[test]
    fn formats_test_message_with_delivery_mode() {
        assert!(nostr_test_message(NostrDmMode::Nip17).contains("DM format: Modern NIP-17."));
        assert!(nostr_test_message(NostrDmMode::Nip04).contains("DM format: Legacy NIP-04."));
    }

    #[tokio::test]
    async fn builds_legacy_nip04_dm_events() {
        let sender_keys = Keys::generate();
        let recipient = Keys::generate().public_key();
        let event = build_nip04_dm_event(&sender_keys, recipient, "hello".to_string())
            .await
            .unwrap();

        assert_eq!(event.kind, Kind::EncryptedDirectMessage);
        assert_eq!(event.pubkey, sender_keys.public_key());
        assert!(event.content.contains("?iv="));
        assert!(event.tags.iter().any(|tag| {
            matches!(
                tag.as_standardized(),
                Some(TagStandard::PublicKey { public_key, .. }) if *public_key == recipient
            )
        }));
    }

    #[test]
    fn maps_known_test_send_errors_to_codes() {
        assert_eq!(
            nostr_test_error_code(Some("Nostr inbox relay discovery timed out")),
            Some(NOSTR_INBOX_DISCOVERY_TIMEOUT_ERROR_CODE)
        );
        assert_eq!(
            nostr_test_error_code(Some(
                "Nostr inbox relay connection failed: wss://example.com"
            )),
            Some(NOSTR_INBOX_CONNECT_FAILED_ERROR_CODE)
        );
        assert_eq!(
            nostr_test_error_code(Some("Nostr publish timed out")),
            Some(NOSTR_PUBLISH_TIMEOUT_ERROR_CODE)
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
        assert_eq!(
            nostr_test_error_code(Some("Nostr discovery relays failed: wss://example.com")),
            Some(NOSTR_DISCOVERY_FAILED_ERROR_CODE)
        );
        assert_eq!(
            nostr_test_error_code(Some(
                "Nostr legacy DM publish failed: no relay accepted the message"
            )),
            Some(NOSTR_NIP04_FAILED_ERROR_CODE)
        );
        assert_eq!(nostr_test_error_code(Some("different error")), None);
    }
}
