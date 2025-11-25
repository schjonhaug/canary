use reqwest::Client;

pub struct AdminNotifications {
    client: Client,
    topic: Option<String>,
}

impl AdminNotifications {
    pub fn new() -> Self {
        let topic = std::env::var("ADMIN_NOTIFICATION_TOPIC").ok();

        Self {
            client: Client::new(),
            topic,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.topic.is_some()
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

    async fn send_notification(&self, topic: &str, title: &str, message: &str, tag: &str) {
        let ntfy_url = format!("https://ntfy.sh/{}", topic);

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
