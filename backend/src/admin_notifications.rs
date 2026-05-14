use std::future::Future;
use std::sync::OnceLock;
use std::time::Duration;

use reqwest::Client;

const ADMIN_NOTIFICATION_TIMEOUT: Duration = Duration::from_secs(10);
static ADMIN_NOTIFICATION_CLIENT: OnceLock<Client> = OnceLock::new();

pub struct AdminNotifications {
    client: Client,
    topic: Option<String>,
    server_url: String,
}

impl AdminNotifications {
    pub fn is_enabled_for_env() -> bool {
        let is_cloud_mode = std::env::var("CANARY_MODE")
            .map(|m| m.to_lowercase() == "cloud")
            .unwrap_or(false);

        is_cloud_mode && std::env::var("ADMIN_NOTIFICATION_TOPIC").is_ok()
    }

    pub fn new(server_url: impl Into<String>) -> Self {
        let topic = if Self::is_enabled_for_env() {
            std::env::var("ADMIN_NOTIFICATION_TOPIC").ok()
        } else {
            None
        };

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

    pub fn spawn_if_enabled<F, Fut>(server_url: impl Into<String>, notification_fn: F) -> bool
    where
        F: FnOnce(AdminNotifications) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let admin_notifications = Self::new(server_url);
        if !admin_notifications.is_enabled() {
            return false;
        }

        tokio::spawn(async move {
            notification_fn(admin_notifications).await;
        });
        true
    }

    pub async fn notify_user_signup(&self, email: &str, name: Option<&str>) {
        if let Some(topic) = &self.topic {
            let display_name = name.unwrap_or("Unknown");
            let message = format!(
                "🆕 New user registered\n📧 Email: {}\n👤 Name: {}",
                email, display_name
            );

            self.send_notification(
                topic,
                "New User Registration",
                &message,
                "bust_in_silhouette",
            )
            .await;
        }
    }

    pub async fn notify_wallet_creation(
        &self,
        wallet_name: &str,
        user_email: &str,
        checksum: &str,
    ) {
        if let Some(topic) = &self.topic {
            let message = format!(
                "💼 New wallet created\n📝 Name: {}\n👤 User: {}\n🔑 ID: {}",
                wallet_name, user_email, checksum
            );

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
                "🚨 Electrum connection failed!\n\n🔌 Server: {}\n❌ Consecutive failures: {}\n📝 Last error: {}\n\n⚠️ Wallet syncing is degraded until connection is restored.",
                electrum_url, consecutive_failures, error_info
            );

            self.send_notification(topic, "Electrum Connection Failed", &message, "warning")
                .await;
        }
    }

    pub async fn notify_electrum_reconnected(&self, electrum_url: &str) {
        if let Some(topic) = &self.topic {
            let message = format!(
                "✅ Electrum connection restored!\n\n🔌 Server: {}\n📡 Wallet syncing has resumed.",
                electrum_url
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
                "🚨 Provider: {}\n📊 Consecutive failures: {}\n🏷️ Error type: {}\n📝 Last error: {}\n\n💡 Suggested action: {}\n\n⚠️ {} notifications are not being delivered until this is resolved.",
                provider_name, consecutive_failures, error_category, error_info, suggested_action, provider_name
            );

            self.send_notification(
                topic,
                &format!("{} Delivery Failed", provider_name),
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
                "✅ {} is working again.\n📡 Notifications will be delivered normally.",
                provider_name
            );

            self.send_notification(
                topic,
                &format!("{} Delivery Restored", provider_name),
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
            .header("Title", format!("Canary Admin - {}", title))
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
            Err(e) => {
                tracing::error!("❌ Admin notification error: {} - {}", title, e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AdminNotifications;
    use std::time::Duration;
    use tokio::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::const_new(());

    struct EnvGuard {
        canary_mode: Option<String>,
        admin_topic: Option<String>,
        ntfy_server_url: Option<String>,
    }

    impl EnvGuard {
        fn capture() -> Self {
            Self {
                canary_mode: std::env::var("CANARY_MODE").ok(),
                admin_topic: std::env::var("ADMIN_NOTIFICATION_TOPIC").ok(),
                ntfy_server_url: std::env::var("NTFY_SERVER_URL").ok(),
            }
        }

        fn restore(&self) {
            restore_env_var("CANARY_MODE", self.canary_mode.clone());
            restore_env_var("ADMIN_NOTIFICATION_TOPIC", self.admin_topic.clone());
            restore_env_var("NTFY_SERVER_URL", self.ntfy_server_url.clone());
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            self.restore();
        }
    }

    fn restore_env_var(key: &str, value: Option<String>) {
        if let Some(value) = value {
            std::env::set_var(key, value);
        } else {
            std::env::remove_var(key);
        }
    }

    #[tokio::test]
    async fn is_enabled_for_env_requires_cloud_mode_and_topic() {
        let _lock = ENV_LOCK.lock().await;
        let env_guard = EnvGuard::capture();

        std::env::remove_var("CANARY_MODE");
        std::env::remove_var("ADMIN_NOTIFICATION_TOPIC");
        assert!(!AdminNotifications::is_enabled_for_env());

        std::env::set_var("CANARY_MODE", "cloud");
        assert!(!AdminNotifications::is_enabled_for_env());

        std::env::set_var("CANARY_MODE", "self-hosted");
        std::env::set_var("ADMIN_NOTIFICATION_TOPIC", "admin-topic");
        assert!(!AdminNotifications::is_enabled_for_env());

        std::env::set_var("CANARY_MODE", "cloud");
        assert!(AdminNotifications::is_enabled_for_env());

        drop(env_guard);
    }

    #[tokio::test]
    async fn spawn_if_enabled_does_not_run_when_disabled() {
        let _lock = ENV_LOCK.lock().await;
        let env_guard = EnvGuard::capture();

        std::env::remove_var("CANARY_MODE");
        std::env::remove_var("ADMIN_NOTIFICATION_TOPIC");

        let (sender, receiver) = tokio::sync::oneshot::channel();
        let spawned =
            AdminNotifications::spawn_if_enabled("https://ntfy.sh", move |_| async move {
                let _ = sender.send(());
            });

        assert!(!spawned);
        assert!(matches!(
            tokio::time::timeout(Duration::from_millis(25), receiver).await,
            Ok(Err(_))
        ));

        drop(env_guard);
    }

    #[tokio::test]
    async fn spawn_if_enabled_runs_when_enabled() {
        let _lock = ENV_LOCK.lock().await;
        let env_guard = EnvGuard::capture();

        std::env::set_var("CANARY_MODE", "cloud");
        std::env::set_var("ADMIN_NOTIFICATION_TOPIC", "admin-topic");

        let (sender, receiver) = tokio::sync::oneshot::channel();
        let spawned =
            AdminNotifications::spawn_if_enabled("https://ntfy.sh", move |_| async move {
                let _ = sender.send(());
            });

        assert!(spawned);
        assert!(tokio::time::timeout(Duration::from_secs(1), receiver)
            .await
            .is_ok());

        drop(env_guard);
    }
}
