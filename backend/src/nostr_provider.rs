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
use nostr::nips::{nip04, nip17};
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;
use std::net::{Ipv4Addr, SocketAddr};
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
const NOSTR_MAX_INBOX_RELAYS: usize = 5;
const NOSTR_SEND_CONCURRENCY: usize = 3;
const NOSTR_SEND_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(30);
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
pub const NOSTR_AUTH_FAILED_ERROR_CODE: &str = "nostr_auth_failed";
pub const NOSTR_NIP04_FAILED_ERROR_CODE: &str = "nostr_nip04_failed";
pub const NOSTR_ONION_SOCKS_REQUIRED_ERROR_CODE: &str = "nostr_onion_socks_required";
const NOSTR_ONION_SOCKS_REQUIRED_ERROR: &str = "Recipient inbox relays are .onion; configure CANARY_NOSTR_SOCKS_PROXY (system Tor SOCKS, typically 127.0.0.1:9050)";
const NOSTR_SOCKS_PROXY_ENV: &str = "CANARY_NOSTR_SOCKS_PROXY";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NostrDmMode {
    #[default]
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
    onion_socks_proxy: Option<SocketAddr>,
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
            onion_socks_proxy: match nostr_onion_socks_proxy_from_env() {
                Ok(proxy) => proxy,
                Err(error) => {
                    tracing::error!("{error}");
                    None
                }
            },
        }
    }

    pub async fn send_test_message(
        &self,
        recipient: PublicKey,
        dm_mode: NostrDmMode,
        message: String,
    ) -> (NotificationResult, Option<NostrDmMode>) {
        let result = self
            .send_test_message_for_mode(recipient, dm_mode, message)
            .await;
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
        message: String,
    ) -> Result<NostrSendSuccess, String> {
        install_default_rustls_crypto_provider();

        match dm_mode {
            NostrDmMode::Auto => self.send_nip17_message(recipient, message).await,
            NostrDmMode::Nip17 => self.send_nip17_message(recipient, message).await,
            NostrDmMode::Nip04 => self.send_nip04_message(recipient, message).await,
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
            NostrDmMode::Auto => self.send_nip17_message(recipient, message).await,
            NostrDmMode::Nip17 => self.send_nip17_message(recipient, message).await,
            NostrDmMode::Nip04 => self.send_nip04_message(recipient, message).await,
        }
    }

    async fn send_nip17_message(
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

    async fn send_nip04_message(
        &self,
        recipient: PublicKey,
        message: String,
    ) -> Result<NostrSendSuccess, String> {
        let keys = self.sender_keys.clone();
        let client = nostr_client(keys.clone(), self.onion_socks_proxy);
        let output = tokio::time::timeout(NOSTR_SEND_ATTEMPT_TIMEOUT, async {
            let relays = self.connect_nip04_relays(&client).await?;

            let event = build_nip04_dm_event(&keys, recipient, message).await?;

            tracing::info!(
                relay_count = relays.len(),
                "Publishing legacy NIP-04 Nostr DM"
            );

            tokio::time::timeout(NOSTR_PUBLISH_TIMEOUT, client.send_event(&event).to(relays))
                .await
                .map_err(|_| "Nostr legacy DM publish timed out".to_string())?
                .map_err(|e| format!("Nostr legacy DM publish failed: {}", e))
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
        .map_err(|e| format!("Nostr publish failed: {}", e))?;

        require_complete_inbox_publish(output)
    }

    async fn connect_discovery_relays(&self, client: &Client) -> Result<Vec<RelayUrl>, String> {
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
            return Err(format!(
                "Nostr discovery relays failed: {}",
                format_relay_failures(&output.failed, "no relays connected")
            ));
        }

        Ok(output.success.into_keys().collect())
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
                if e.to_string().to_lowercase().contains("timeout") {
                    "Nostr inbox relay discovery timed out".to_string()
                } else {
                    format!("Nostr inbox relay discovery failed: {}", e)
                }
            })?;

        tracing::info!(
            event_count = events.len(),
            "Nostr recipient inbox relay discovery completed"
        );

        let Some(inbox_event) = events.first() else {
            return Err("Recipient has no kind 10050 Nostr DM inbox relay list".to_string());
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
            return Err("Recipient has no kind 10050 Nostr DM inbox relay list".to_string());
        }

        Ok(inbox_relays)
    }

    async fn connect_inbox_relays(
        &self,
        client: &Client,
        inbox_relays: &[RelayUrl],
    ) -> Result<Vec<RelayUrl>, String> {
        let inbox_relays = selectable_inbox_relays(inbox_relays, self.onion_socks_proxy)?;
        let mut connected_relays = Vec::new();
        let mut failed_relays = Vec::new();

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
                        Ok(_) => Ok(relay),
                        Err(e) => Err(format!("{relay}: {e}")),
                    },
                    Err(e) => Err(format!("{relay}: {e}")),
                }
            })
            .buffer_unordered(NOSTR_SEND_CONCURRENCY)
            .collect::<Vec<_>>()
            .await;

        for result in results {
            match result {
                Ok(relay) => connected_relays.push(relay),
                Err(error) => failed_relays.push(error),
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
            match client
                .add_relay(relay)
                .capabilities(RelayCapabilities::WRITE)
                .await
            {
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
        NotificationResult {
            success: false,
            provider_id: None,
            error_message: Some(format_nostr_provider_error(error_message)),
        }
    }
}

fn format_nostr_provider_error(error_message: String) -> String {
    if error_message.starts_with("Nostr discovery relays failed:")
        || error_message.starts_with("Nostr inbox relay connection failed:")
        || error_message == "Nostr inbox relay discovery timed out"
        || error_message == "Recipient has no kind 10050 Nostr DM inbox relay list"
        || error_message == "Nostr publish timed out"
        || error_message == NOSTR_ONION_SOCKS_REQUIRED_ERROR
        || error_message.starts_with("Nostr legacy DM")
    {
        error_message
    } else {
        format!("Nostr send failed: {}", error_message)
    }
}

fn build_nip17_dm_event(
    keys: &Keys,
    recipient: PublicKey,
    message: String,
) -> Result<Event, String> {
    nip17::PrivateDirectMessageBuilder::new(recipient, message)
        .finalize(keys)
        .map_err(|e| format!("Nostr encryption failed: {}", e))
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
        .finalize(keys)
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

fn nostr_client(keys: Keys, onion_socks_proxy: Option<SocketAddr>) -> Client {
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

pub fn nostr_onion_socks_proxy_from_env() -> Result<Option<SocketAddr>, String> {
    match std::env::var(NOSTR_SOCKS_PROXY_ENV) {
        Ok(value) => parse_nostr_onion_socks_proxy(&value),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(_) => Err(format!(
            "Invalid {NOSTR_SOCKS_PROXY_ENV}: value is not valid unicode"
        )),
    }
}

pub fn parse_nostr_onion_socks_proxy(value: &str) -> Result<Option<SocketAddr>, String> {
    let trimmed = value.trim();
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
        return Err(format!(
            "Invalid {NOSTR_SOCKS_PROXY_ENV}: expected host:port, got '{trimmed}'"
        ));
    };

    let port: u16 = port_str.parse().map_err(|_| {
        format!("Invalid {NOSTR_SOCKS_PROXY_ENV}: expected host:port, got '{trimmed}'")
    })?;
    let host = host.trim_matches(|c| c == '[' || c == ']');
    if host.eq_ignore_ascii_case("localhost") {
        return Ok(Some(SocketAddr::from((Ipv4Addr::LOCALHOST, port))));
    }

    Err(format!(
        "Invalid {NOSTR_SOCKS_PROXY_ENV}: expected host:port, got '{trimmed}'"
    ))
}

fn selectable_inbox_relays(
    relays: &[RelayUrl],
    onion_socks_proxy: Option<SocketAddr>,
) -> Result<Vec<RelayUrl>, String> {
    let (onion, clearnet): (Vec<_>, Vec<_>) =
        relays.iter().cloned().partition(|relay| relay.is_onion());

    if onion_socks_proxy.is_some() {
        return Ok(relays.to_vec());
    }

    if clearnet.is_empty() {
        if onion.is_empty() {
            return Err("Recipient has no kind 10050 Nostr DM inbox relay list".to_string());
        }
        return Err(NOSTR_ONION_SOCKS_REQUIRED_ERROR.to_string());
    }

    if !onion.is_empty() {
        tracing::info!(
            skipped_onion_relays = onion.len(),
            "Skipping .onion Nostr inbox relays because CANARY_NOSTR_SOCKS_PROXY is not set"
        );
    }

    Ok(clearnet)
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
                .map(|(_contact, method)| {
                    let content = MessageFormatter::create_filtered_content(
                        notification,
                        wallet_name,
                        wallet_balance_sats,
                        method.content_fields,
                    );
                    let message = MessageFormatter::create_localized_filtered_message(
                        &content,
                        user_language,
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
        Some(NOSTR_ONION_SOCKS_REQUIRED_ERROR) => Some(NOSTR_ONION_SOCKS_REQUIRED_ERROR_CODE),
        Some("Nostr publish timed out") => Some(NOSTR_PUBLISH_TIMEOUT_ERROR_CODE),
        Some(message)
            if message.contains("authentication failed")
                || message.contains("failed to authenticate") =>
        {
            Some(NOSTR_AUTH_FAILED_ERROR_CODE)
        }
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
        use crate::metadata::Language;
        use crate::test_notification::format_generic_nostr_test_message;

        assert!(
            format_generic_nostr_test_message(&Language::English, NostrDmMode::Auto)
                .contains("DM format: Modern NIP-17.")
        );
        assert!(
            format_generic_nostr_test_message(&Language::English, NostrDmMode::Nip17)
                .contains("DM format: Modern NIP-17.")
        );
        assert!(
            format_generic_nostr_test_message(&Language::English, NostrDmMode::Nip04)
                .contains("DM format: Legacy NIP-04.")
        );
    }

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
        assert!(event
            .tags
            .iter()
            .any(|tag| { tag.as_slice() == ["p", recipient.to_hex().as_str()] }));
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
        assert_eq!(
            nostr_test_error_code(Some(
                "Nostr publish failed: ws://haven.local/chat: authentication failed"
            )),
            Some(NOSTR_AUTH_FAILED_ERROR_CODE)
        );
        assert_eq!(
            nostr_test_error_code(Some(
                "Nostr publish failed: ws://haven.local/chat: failed to authenticate"
            )),
            Some(NOSTR_AUTH_FAILED_ERROR_CODE)
        );
        assert_eq!(
            nostr_test_error_code(Some(NOSTR_ONION_SOCKS_REQUIRED_ERROR)),
            Some(NOSTR_ONION_SOCKS_REQUIRED_ERROR_CODE)
        );
        assert_eq!(
            nostr_test_error_code(Some(&format_nostr_provider_error(
                NOSTR_ONION_SOCKS_REQUIRED_ERROR.to_string()
            ))),
            Some(NOSTR_ONION_SOCKS_REQUIRED_ERROR_CODE)
        );
        assert_eq!(nostr_test_error_code(Some("different error")), None);
    }

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
        assert_eq!(
            selectable_inbox_relays(&onion_only, None).unwrap_err(),
            NOSTR_ONION_SOCKS_REQUIRED_ERROR
        );
        assert_eq!(
            selectable_inbox_relays(&onion_only, Some(socks)).unwrap(),
            vec![onion]
        );
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

        let empty: Output<EventId> = Output {
            value: event_id,
            success: Default::default(),
            failed: Default::default(),
        };
        assert_eq!(
            require_complete_inbox_publish(empty).unwrap_err(),
            "Nostr publish failed: no relay accepted the message"
        );

        let ok = Output {
            value: event_id,
            success: [(accepted, ())].into_iter().collect(),
            failed: Default::default(),
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
        assert!(result.0.success, "Synthetic delivery must succeed");
        let event = tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap();
        event.verify().unwrap();
        let plaintext = nostr::nips::nip04::decrypt(
            recipient.secret_key(),
            &sender.public_key(),
            &event.content,
        )
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
        let relay =
            RelayUrl::parse(&format!("ws://{}/chat", listener.local_addr().unwrap())).unwrap();
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
