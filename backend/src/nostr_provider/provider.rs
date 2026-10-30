//! `NostrProvider`: the configured Nostr DM sender and its `NotificationProvider` impl.

use super::dm::NostrSendSuccess;
use super::errors::NostrError;
use super::keys::NostrSenderKeys;
use super::mode::{NostrDmMode, NOSTR_DM_MODE_SETTING_KEY};
use super::relays::{DEFAULT_DISCOVERY_RELAYS, DEFAULT_NIP04_RELAYS};
use super::NOSTR_SEND_CONCURRENCY;
use crate::message_formatter::MessageFormatter;
use crate::metadata::{
    Contact, Language, MetadataDb, NotificationMethod, ProviderType, TransactionNotification,
};
use crate::notifications::{
    notification_methods_for_provider, NotificationProvider, NotificationResult, ProviderInfo,
};
use crate::tls::install_default_rustls_crypto_provider;
use async_trait::async_trait;
use futures::{stream, StreamExt};
use nostr_sdk::prelude::*;
use serde_json::json;
use std::net::SocketAddr;
use std::str::FromStr;

/// Sends notifications as Nostr direct messages from the instance's sender key, using a
/// short-lived client for each send.
pub struct NostrProvider {
    pub(super) sender_keys: Keys,
    pub(super) discovery_relays: Vec<String>,
    pub(super) nip04_relays: Vec<String>,
    pub(super) metadata_db: Option<MetadataDb>,
    pub(super) onion_socks_proxy: Option<SocketAddr>,
}

impl NostrProvider {
    /// Creates a provider without database access; notifications use the default DM mode.
    pub fn new(sender_keys: NostrSenderKeys) -> Self {
        Self::with_metadata_db(sender_keys, None)
    }

    /// Creates a provider that, when `metadata_db` is set, reads the instance's DM mode setting
    /// on each notification.
    pub fn with_metadata_db(sender_keys: NostrSenderKeys, metadata_db: Option<MetadataDb>) -> Self {
        Self {
            sender_keys: sender_keys.keys,
            discovery_relays: DEFAULT_DISCOVERY_RELAYS
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
            nip04_relays: DEFAULT_NIP04_RELAYS
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
            metadata_db,
            onion_socks_proxy: None,
        }
    }

    /// Route `.onion` inbox relays through `proxy` (`CANARY_NOSTR_SOCKS_PROXY`).
    #[must_use]
    pub fn with_onion_socks_proxy(mut self, proxy: Option<SocketAddr>) -> Self {
        self.onion_socks_proxy = proxy;
        self
    }

    /// Sends `message` to `recipient` in `dm_mode`, ignoring the stored DM mode setting.
    pub async fn send_test_message(
        &self,
        recipient: PublicKey,
        dm_mode: NostrDmMode,
        message: String,
    ) -> Result<NostrSendSuccess, NostrError> {
        self.send_message(recipient, message, dm_mode).await
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
    ) -> Result<NostrSendSuccess, NostrError> {
        install_default_rustls_crypto_provider();

        match dm_mode {
            NostrDmMode::Auto | NostrDmMode::Nip17 => {
                self.send_nip17_message(recipient, message).await
            }
            NostrDmMode::Nip04 => self.send_nip04_message(recipient, message).await,
        }
    }

    fn nostr_error_result(error: &NostrError) -> NotificationResult {
        NotificationResult {
            success: false,
            provider_id: None,
            error_message: Some(error.to_string()),
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
                .map(|(_contact, method)| {
                    let content = MessageFormatter::create_filtered_content(
                        notification,
                        wallet_name,
                        wallet_balance_sats,
                        method.content_fields,
                    );
                    let message = MessageFormatter::create_localized_filtered_message(
                        &content,
                        *user_language,
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
                        Err(error) => Self::nostr_error_result(&error),
                    }
                }
                Err(error) => Self::nostr_error_result(&NostrError::InvalidStoredRecipient(error)),
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
