//! Operator notifications (signups, wallet creation, provider health) sent
//! to a private ntfy topic in cloud mode.

use std::future::Future;
use std::sync::OnceLock;
use std::time::Duration;

use reqwest::Client;

use crate::config::AppConfig;

const ADMIN_NOTIFICATION_TIMEOUT: Duration = Duration::from_secs(10);
static ADMIN_NOTIFICATION_CLIENT: OnceLock<Client> = OnceLock::new();

fn user_signup_message() -> &'static str {
    "🆕 New user registered"
}

fn wallet_creation_message(checksum: &str) -> String {
    format!("💼 New wallet created\n🔑 ID: {checksum}")
}

pub struct AdminNotifications {
    client: Client,
    topic: Option<String>,
    server_url: String,
}

/// The topic to notify, if admin notifications are enabled: only in cloud
/// mode, outside restore drills, with a topic configured.
fn enabled_topic(is_cloud_mode: bool, restore_drill: bool, topic: Option<&str>) -> Option<String> {
    if is_cloud_mode && !restore_drill {
        topic.map(str::to_string)
    } else {
        None
    }
}

impl AdminNotifications {
    /// Builds the notifier for `config`, posting to its default ntfy server.
    pub fn from_config(config: &AppConfig) -> Self {
        Self::with_topic(config.ntfy_server_url(), Self::topic_for(config))
    }

    /// Whether `config` enables admin notifications.
    pub fn is_enabled_for(config: &AppConfig) -> bool {
        Self::topic_for(config).is_some()
    }

    fn topic_for(config: &AppConfig) -> Option<String> {
        enabled_topic(
            config.is_cloud_mode(),
            config.is_restore_drill(),
            config.admin_notification_topic(),
        )
    }

    fn with_topic(server_url: impl Into<String>, topic: Option<String>) -> Self {
        Self {
            client: Self::default_client(),
            topic,
            server_url: server_url.into().trim_end_matches('/').to_string(),
        }
    }

    fn default_client() -> Client {
        ADMIN_NOTIFICATION_CLIENT
            .get_or_init(|| {
                Client::builder()
                    .timeout(ADMIN_NOTIFICATION_TIMEOUT)
                    .build()
                    .expect("failed to build admin notification HTTP client")
            })
            .clone()
    }

    pub fn is_enabled(&self) -> bool {
        self.topic.is_some()
    }

    /// Runs `notification_fn` in the background when `config` enables admin
    /// notifications. Returns whether it was spawned.
    pub fn spawn_if_enabled<F, Fut>(config: &AppConfig, notification_fn: F) -> bool
    where
        F: FnOnce(AdminNotifications) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let admin_notifications = Self::from_config(config);
        if !admin_notifications.is_enabled() {
            return false;
        }

        tokio::spawn(async move {
            notification_fn(admin_notifications).await;
        });
        true
    }

    pub async fn notify_user_signup(&self) {
        if let Some(topic) = &self.topic {
            let message = user_signup_message();

            self.send_notification(
                topic,
                "New User Registration",
                message,
                "bust_in_silhouette",
            )
            .await;
        }
    }

    pub async fn notify_wallet_creation(&self, checksum: &str) {
        if let Some(topic) = &self.topic {
            let message = wallet_creation_message(checksum);

            self.send_notification(topic, "New Wallet Created", &message, "wallet")
                .await;
        }
    }

    pub async fn notify_electrum_disconnect(
        &self,
        electrum_url: &str,
        consecutive_failures: u32,
        last_error: Option<&str>,
    ) {
        if let Some(topic) = &self.topic {
            let error_info = last_error.unwrap_or("Unknown error");
            let message = format!(
                "🚨 Electrum connection failed!\n\n🔌 Server: {electrum_url}\n❌ Consecutive failures: {consecutive_failures}\n📝 Last error: {error_info}\n\n⚠️ Wallet syncing is degraded until connection is restored."
            );

            self.send_notification(topic, "Electrum Connection Failed", &message, "warning")
                .await;
        }
    }

    pub async fn notify_electrum_reconnected(&self, electrum_url: &str) {
        if let Some(topic) = &self.topic {
            let message = format!(
                "✅ Electrum connection restored!\n\n🔌 Server: {electrum_url}\n📡 Wallet syncing has resumed."
            );

            self.send_notification(topic, "Electrum Reconnected", &message, "white_check_mark")
                .await;
        }
    }

    /// Notify admin when a notification provider (SMS/Email) has consecutive delivery failures.
    pub async fn notify_provider_failure(
        &self,
        provider_name: &str,
        consecutive_failures: u32,
        error_category: &str,
        last_error: Option<&str>,
        suggested_action: &str,
    ) {
        if let Some(topic) = &self.topic {
            let error_info = last_error.unwrap_or("Unknown error");
            let message = format!(
                "🚨 Provider: {provider_name}\n📊 Consecutive failures: {consecutive_failures}\n🏷️ Error type: {error_category}\n📝 Last error: {error_info}\n\n💡 Suggested action: {suggested_action}\n\n⚠️ {provider_name} notifications are not being delivered until this is resolved."
            );

            self.send_notification(
                topic,
                &format!("{provider_name} Delivery Failed"),
                &message,
                "warning",
            )
            .await;
        }
    }

    /// Notify admin when a notification provider recovers after failures.
    pub async fn notify_provider_recovery(&self, provider_name: &str) {
        if let Some(topic) = &self.topic {
            let message = format!(
                "✅ {provider_name} is working again.\n📡 Notifications will be delivered normally."
            );

            self.send_notification(
                topic,
                &format!("{provider_name} Delivery Restored"),
                &message,
                "white_check_mark",
            )
            .await;
        }
    }

    async fn send_notification(&self, topic: &str, title: &str, message: &str, tag: &str) {
        let ntfy_url = format!("{}/{}", self.server_url, topic);

        let result = self
            .client
            .post(&ntfy_url)
            .header("Content-Type", "text/plain; charset=utf-8")
            .header("Title", format!("Canary Wallet Admin - {title}"))
            .header("Priority", "default")
            .header("Tags", tag)
            .body(message.to_string())
            .send()
            .await;

        match result {
            Ok(response) => {
                if response.status().is_success() {
                    tracing::info!("✅ Admin notification sent: {}", title);
                } else {
                    tracing::warn!(
                        "⚠️ Admin notification failed: {} - HTTP {}",
                        title,
                        response.status()
                    );
                }
            }
            Err(_) => {
                // reqwest errors can include the complete private ntfy topic URL.
                tracing::error!("Admin notification request failed");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{user_signup_message, wallet_creation_message, AdminNotifications};
    use crate::config::{AppConfig, NetworkConfig, OperatingMode};
    use std::time::Duration;

    fn config(mode: OperatingMode, topic: Option<&str>) -> AppConfig {
        let config = AppConfig::new_for_test(
            NetworkConfig::Regtest,
            None,
            "127.0.0.1:3000".to_string(),
            "./database".to_string(),
            mode,
            None,
            None,
        );
        match topic {
            Some(topic) => config.with_admin_notification_topic(topic),
            None => config,
        }
    }

    #[test]
    fn admin_notification_messages_exclude_user_pii() {
        assert_eq!(user_signup_message(), "🆕 New user registered");
        assert_eq!(
            wallet_creation_message("wallet-id"),
            "💼 New wallet created\n🔑 ID: wallet-id"
        );
    }

    #[test]
    fn is_enabled_for_requires_cloud_mode_and_topic() {
        assert!(!AdminNotifications::is_enabled_for(&config(
            OperatingMode::SelfHosted,
            None
        )));
        assert!(!AdminNotifications::is_enabled_for(&config(
            OperatingMode::Cloud,
            None
        )));
        assert!(!AdminNotifications::is_enabled_for(&config(
            OperatingMode::SelfHosted,
            Some("admin-topic")
        )));
        assert!(AdminNotifications::is_enabled_for(&config(
            OperatingMode::Cloud,
            Some("admin-topic")
        )));
        assert!(!AdminNotifications::is_enabled_for(
            &config(OperatingMode::Cloud, Some("admin-topic")).with_restore_drill(true)
        ));

        let notifier =
            AdminNotifications::from_config(&config(OperatingMode::Cloud, Some("admin-topic")));
        assert!(notifier.is_enabled());
        assert_eq!(notifier.server_url, "https://ntfy.sh");
    }

    #[tokio::test]
    async fn spawn_if_enabled_does_not_run_when_disabled() {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let spawned = AdminNotifications::spawn_if_enabled(
            &config(OperatingMode::Cloud, None),
            move |_| async move {
                let _ = sender.send(());
            },
        );

        assert!(!spawned);
        assert!(matches!(
            tokio::time::timeout(Duration::from_millis(25), receiver).await,
            Ok(Err(_))
        ));
    }

    #[tokio::test]
    async fn spawn_if_enabled_runs_when_enabled() {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let spawned = AdminNotifications::spawn_if_enabled(
            &config(OperatingMode::Cloud, Some("admin-topic")),
            move |_| async move {
                let _ = sender.send(());
            },
        );

        assert!(spawned);
        assert!(tokio::time::timeout(Duration::from_secs(1), receiver)
            .await
            .is_ok());
    }
}

#[cfg(test)]
mod log_privacy_tests {
    use super::*;
    use std::sync::{Arc, Mutex};
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
    #[tokio::test]
    async fn failed_admin_request_does_not_log_private_topic() {
        let logs = Arc::new(Mutex::new(Vec::new()));
        let writer = Capture(logs.clone());
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_max_level(tracing::Level::TRACE)
            .with_writer(move || writer.clone())
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);
        // A non-HTTP URL produces a real reqwest builder failure without network traffic.
        let admin = AdminNotifications {
            client: reqwest::Client::new(),
            topic: None,
            server_url: "file:///synthetic-private-server".into(),
        };
        admin
            .send_notification(
                "synthetic-private-topic",
                "Test",
                "synthetic-private-body",
                "test",
            )
            .await;
        let output = String::from_utf8(logs.lock().unwrap().clone()).unwrap();
        assert!(output.contains("Admin notification request failed"));
        for secret in [
            "synthetic-private-server",
            "synthetic-private-topic",
            "synthetic-private-body",
        ] {
            assert!(!output.contains(secret), "Private fixture appeared in logs");
        }
    }
}
