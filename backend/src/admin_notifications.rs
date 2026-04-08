use std::future::Future;

use reqwest::Client;

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

    pub fn new() -> Self {
        let topic = if Self::is_enabled_for_env() {
            std::env::var("ADMIN_NOTIFICATION_TOPIC").ok()
        } else {
            None
        };

        let server_url = std::env::var("NTFY_SERVER_URL")
            .unwrap_or_else(|_| "https://ntfy.sh".to_string())
            .trim_end_matches('/')
            .to_string();

        Self {
            client: Client::new(),
            topic,
            server_url,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.topic.is_some()
    }

    pub fn spawn_if_enabled<F, Fut>(notification_fn: F) -> bool
    where
        F: FnOnce(AdminNotifications) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let admin_notifications = Self::new();
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

impl Default for AdminNotifications {
    fn default() -> Self {
        Self::new()
    }
}
