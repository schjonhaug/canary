//! Registration of the notification providers available in each operating mode.

use crate::config::AppConfig;
use crate::email_provider::EmailProvider;
use crate::email_queue::{self, EmailQueueConfig};
use crate::metadata::MetadataDb;
use crate::nostr_provider::{self, ensure_nostr_sender_keys, NostrProvider};
use crate::notifications::NotificationManager;
use crate::ntfy_provider::{NtfyAuth, NtfyProvider};
use crate::telegram_provider::TelegramProvider;
use crate::twilio_provider::TwilioProvider;
use crate::webhook_provider::WebhookProvider;
use std::sync::Arc;
use tracing::{error, info, warn};

/// Builds a [`NotificationManager`] with every provider that is enabled and
/// configured for the current operating mode.
///
/// Self-hosted installs use local channels (ntfy, webhooks, Telegram, Nostr).
/// Cloud installs use email and SMS, plus ntfy when enabled.
pub async fn build_notification_manager(
    config: &AppConfig,
    metadata_db: &MetadataDb,
) -> NotificationManager {
    let mut manager = NotificationManager::new();
    if config.is_self_hosted_mode() {
        register_self_hosted_providers(&mut manager, config, metadata_db).await;
    } else {
        register_cloud_providers(&mut manager, config);
    }
    manager
}

async fn register_self_hosted_providers(
    manager: &mut NotificationManager,
    config: &AppConfig,
    metadata_db: &MetadataDb,
) {
    info!("Registering ntfy notification provider");
    manager.register_provider(Arc::new(NtfyProvider::with_trusted_auth(
        &config.ntfy_server_url(),
        NtfyAuth::None,
    )));

    info!("Registering webhook notification provider");
    manager.register_provider(Arc::new(WebhookProvider::new()));

    if let Some(telegram) = TelegramProvider::for_self_hosted(metadata_db.clone()) {
        info!("Registering Telegram notification provider");
        manager.register_provider(Arc::new(telegram));
    }

    match ensure_nostr_sender_keys(metadata_db).await {
        Ok(nostr_keys) => {
            info!("Registering Nostr DM notification provider");
            if let Ok(Some(addr)) = nostr_provider::nostr_onion_socks_proxy_from_env() {
                info!(".onion inbox relays via SOCKS {addr} (CANARY_NOSTR_SOCKS_PROXY)");
            }
            manager.register_provider(Arc::new(NostrProvider::with_metadata_db(
                nostr_keys,
                Some(metadata_db.clone()),
            )));
        }
        Err(e) => error!(
            "Failed to initialize Nostr notification provider; Nostr contacts will not receive \
             notifications until key initialization succeeds: {e}"
        ),
    }
}

fn register_cloud_providers(manager: &mut NotificationManager, config: &AppConfig) {
    if config.is_ntfy_enabled() {
        info!("Registering ntfy notification provider");
        manager.register_provider(Arc::new(NtfyProvider::with_trusted_auth(
            &config.ntfy_server_url(),
            NtfyAuth::None,
        )));
    }

    if config.is_twilio_enabled() {
        if let Some(twilio) = TwilioProvider::from_env() {
            info!("Registering Twilio SMS notification provider");
            manager.register_provider(Arc::new(twilio));
        } else {
            warn!(
                "Twilio is enabled but TWILIO_ACCOUNT_SID, TWILIO_AUTH_TOKEN or \
                 TWILIO_SENDER_ID is missing; SMS notifications are disabled"
            );
        }
    }

    if config.is_email_enabled() {
        // The queue worker must be running before the provider can enqueue mail.
        match EmailQueueConfig::from_env().and_then(email_queue::start_email_queue_worker) {
            Ok(()) => {
                info!("Registering email notification provider (rate-limited queue)");
                manager.register_provider(Arc::new(EmailProvider::new()));
            }
            Err(e) => warn!("Email notifications are disabled: {e}"),
        }
    }
}
