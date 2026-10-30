//! Delivers transaction and balance alert events to each wallet's contacts.

use crate::admin_notifications::AdminNotifications;
use crate::api::NotificationManagerState;
use crate::config::AppConfig;
use crate::metadata::{
    Contact, Language, NotificationLogParams, NotificationMethod, TransactionNotification,
    WalletMetadata,
};
use crate::notification_failure_tracker::NotificationFailureTracker;
use crate::notifications::{
    contact_allows_notification, notification_log_type, NotificationProvider, NotificationResult,
};
use crate::ntfy_provider::{NtfyAuth, NtfyProvider};
use crate::wallet::WalletManager;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::broadcast::{self, error::RecvError};
use tracing::{error, info, warn};

/// Consumes notification events and fans them out to every registered
/// provider, logging each delivery and alerting admins when a paid provider
/// (SMS or email) keeps failing.
pub struct NotificationDispatcher {
    manager: NotificationManagerState,
    wallet_manager: Arc<WalletManager>,
    config: AppConfig,
    sms_failures: NotificationFailureTracker,
    email_failures: NotificationFailureTracker,
}

/// Tally of one event's deliveries, for the summary log line.
#[derive(Default)]
struct DeliverySummary {
    sent_by_provider: BTreeMap<String, usize>,
    failed: usize,
}

impl NotificationDispatcher {
    pub fn new(
        manager: NotificationManagerState,
        wallet_manager: Arc<WalletManager>,
        config: AppConfig,
    ) -> Self {
        Self {
            manager,
            wallet_manager,
            config,
            sms_failures: NotificationFailureTracker::new("twilio"),
            email_failures: NotificationFailureTracker::new("email"),
        }
    }

    /// Runs the dispatcher on its own task until the event channel closes.
    pub fn spawn(self, mut events: broadcast::Receiver<TransactionNotification>) {
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(notification) => self.dispatch(&notification).await,
                    Err(RecvError::Lagged(skipped)) => {
                        error!(
                            "Notification dispatcher fell behind; {skipped} events were dropped"
                        );
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        });
    }

    async fn dispatch(&self, notification: &TransactionNotification) {
        let wallet_checksum = match notification {
            TransactionNotification::Pending(tx) | TransactionNotification::Confirmed(tx) => {
                &tx.wallet_checksum
            }
            TransactionNotification::BalanceAlert(alert) => &alert.wallet_checksum,
        };
        let metadata_db = &self.wallet_manager.metadata_db;

        let Ok(Some(wallet)) = self
            .wallet_manager
            .get_wallet_by_checksum(wallet_checksum)
            .await
        else {
            return;
        };
        let Ok(contacts) = metadata_db
            .get_contacts_with_notification_methods(wallet_checksum)
            .await
        else {
            return;
        };
        let contacts: Vec<Contact> = contacts
            .into_iter()
            .filter(|contact| contact_allows_notification(contact, notification))
            .collect();
        if contacts.is_empty() {
            return;
        }

        let user_language = metadata_db
            .get_user_preferred_language(&wallet.user_id)
            .await
            .unwrap_or(Language::English);

        let mut summary = DeliverySummary::default();
        let manager = self.manager.lock().await;
        for provider in manager.list_providers() {
            let provider_name = provider.name.as_str();
            let deliveries = if provider_name == "ntfy" {
                // ntfy is resolved per user: each user may point at their own server.
                self.ntfy_provider_for(&wallet.user_id)
                    .await
                    .send_notification(
                        notification,
                        &wallet.name,
                        &contacts,
                        &user_language,
                        wallet.balance_total,
                    )
                    .await
            } else {
                match manager
                    .send_notifications(
                        provider_name,
                        notification,
                        &wallet.name,
                        &contacts,
                        &user_language,
                        wallet.balance_total,
                    )
                    .await
                {
                    Ok(deliveries) => deliveries,
                    Err(_) => continue,
                }
            };

            for (method, result, message) in deliveries {
                self.record_delivery(notification, provider_name, &method, &result, &message)
                    .await;
                if result.success {
                    *summary
                        .sent_by_provider
                        .entry(provider_name.to_string())
                        .or_default() += 1;
                } else {
                    summary.failed += 1;
                    // Targets and provider errors can contain private contact
                    // details, URLs, tokens, or untrusted response bodies.
                    warn!(provider = %provider_name, "Notification delivery failed");
                }
                self.track_provider_health(provider_name, &result).await;
            }
        }
        drop(manager);

        log_summary(notification, &wallet, &summary);
    }

    /// Builds an ntfy provider for the user's server and credentials.
    ///
    /// Cloud users cannot choose outbound ntfy hosts; this also keeps legacy
    /// stored preferences from becoming SSRF targets.
    async fn ntfy_provider_for(&self, user_id: &str) -> NtfyProvider {
        let metadata_db = &self.wallet_manager.metadata_db;
        let user_server_url = metadata_db
            .get_user_ntfy_server_url(user_id)
            .await
            .ok()
            .flatten()
            .filter(|url| !url.is_empty());

        let server = if self.config.is_cloud_mode() {
            self.config.ntfy_server_url()
        } else {
            user_server_url
                .clone()
                .unwrap_or_else(|| self.config.ntfy_server_url())
        };
        let server = server.trim().trim_end_matches('/').to_string();

        let auth = if self
            .config
            .should_use_ntfy_auth_for_url(&server, user_server_url.as_deref())
        {
            match metadata_db.get_user_ntfy_auth(user_id).await {
                Ok((Some(token), _, _)) => NtfyAuth::AccessToken(token),
                Ok((None, Some(username), Some(password))) => {
                    NtfyAuth::BasicAuth { username, password }
                }
                _ => NtfyAuth::None,
            }
        } else {
            NtfyAuth::None
        };
        let auth = self
            .config
            .with_managed_ntfy_auth(auth, &server, user_server_url.as_deref());

        self.config
            .ntfy_provider(&server, auth, user_server_url.as_deref())
    }

    /// Writes the delivery attempt to the notification log (audit trail).
    async fn record_delivery(
        &self,
        notification: &TransactionNotification,
        provider_name: &str,
        method: &NotificationMethod,
        result: &NotificationResult,
        message: &str,
    ) {
        let Some(method_id) = method.id.as_deref() else {
            return;
        };
        let log_params = NotificationLogParams {
            notification_method_id: method_id,
            provider_name,
            provider_message_id: result.provider_id.as_deref(),
            status: if result.success { "sent" } else { "failed" },
            error_message: result.error_message.as_deref(),
            message_content: message,
        };
        let metadata_db = &self.wallet_manager.metadata_db;
        let logged = match notification {
            TransactionNotification::Pending(tx) | TransactionNotification::Confirmed(tx) => {
                metadata_db
                    .insert_notification_log_for_transaction(
                        &tx.txid,
                        &tx.wallet_checksum,
                        &log_params,
                        notification_log_type(notification),
                    )
                    .await
            }
            TransactionNotification::BalanceAlert(alert) => {
                metadata_db
                    .insert_notification_log_for_balance_alert(
                        &alert.balance_alert_id,
                        &alert.wallet_checksum,
                        &log_params,
                    )
                    .await
            }
        };
        if let Err(e) = logged {
            error!("Failed to log notification to database: {e}");
        }
    }

    /// Alerts admins when SMS or email delivery keeps failing, and again when
    /// it recovers.
    async fn track_provider_health(&self, provider_name: &str, result: &NotificationResult) {
        let (tracker, display_name) = match provider_name {
            "twilio" => (&self.sms_failures, "SMS (Twilio)"),
            "email" => (&self.email_failures, "Email (Resend)"),
            _ => return,
        };
        let admin = || AdminNotifications::new(self.config.ntfy_server_url());

        if result.success {
            if tracker.record_success() {
                let admin = admin();
                if admin.is_enabled() {
                    admin.notify_provider_recovery(display_name).await;
                }
            }
            return;
        }

        let (should_alert, failures, category) = tracker
            .record_failure(result.error_message.as_deref())
            .await;
        if should_alert {
            let admin = admin();
            if admin.is_enabled() {
                admin
                    .notify_provider_failure(
                        display_name,
                        failures,
                        &category.to_string(),
                        result.error_message.as_deref(),
                        category.suggested_action(),
                    )
                    .await;
            }
            tracker.mark_alert_sent();
        }
    }
}

fn log_summary(
    notification: &TransactionNotification,
    wallet: &WalletMetadata,
    summary: &DeliverySummary,
) {
    let sent: usize = summary.sent_by_provider.values().sum();
    if sent == 0 && summary.failed == 0 {
        return;
    }
    let mut providers: Vec<String> = summary
        .sent_by_provider
        .iter()
        .map(|(provider, count)| format!("{count}×{provider}"))
        .collect();
    if summary.failed > 0 {
        providers.push(format!("{}×failed", summary.failed));
    }
    info!(
        "Sent {sent}/{} notifications for {}: {} ({})",
        sent + summary.failed,
        wallet.name,
        notification_log_type(notification),
        providers.join(", ")
    );
}
