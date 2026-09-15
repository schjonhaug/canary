use crate::message_formatter::MessageFormatter;
use crate::metadata::{
    Contact, Language, MetadataDb, NotificationMethod, ProviderType, TransactionNotification,
};
use crate::notifications::{
    notification_methods_for_provider, NotificationProvider, NotificationResult, ProviderInfo,
};
use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

const TELEGRAM_BOT_TOKEN_ENV: &str = "TELEGRAM_BOT_TOKEN";
pub const TELEGRAM_BOT_TOKEN_SETTING_KEY: &str = "telegram_bot_token";
const DEFAULT_TELEGRAM_API_BASE: &str = "https://api.telegram.org";
const TELEGRAM_SEND_CONCURRENCY: usize = 4;

pub struct TelegramProvider {
    metadata_db: MetadataDb,
    api_base: String,
    client: Client,
}

impl std::fmt::Debug for TelegramProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelegramProvider")
            .field("bot_token", &"[redacted]")
            .field("api_base", &self.api_base)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Deserialize)]
struct TelegramApiResponse {
    ok: bool,
    description: Option<String>,
}

impl TelegramProvider {
    /// Always register on self-hosted so a token saved in Settings works without a restart.
    pub fn for_self_hosted(metadata_db: MetadataDb) -> Option<Self> {
        if crate::config::AppConfig::restore_drill_enabled() {
            return None;
        }
        Some(Self::with_metadata_db(
            metadata_db,
            DEFAULT_TELEGRAM_API_BASE.to_string(),
        ))
    }

    pub fn with_metadata_db(metadata_db: MetadataDb, api_base: String) -> Self {
        Self {
            metadata_db,
            api_base: api_base.trim_end_matches('/').to_string(),
            client: telegram_http_client(),
        }
    }

    pub async fn send_message(&self, chat_id: &str, text: &str) -> NotificationResult {
        let Some(bot_token) = resolve_bot_token(&self.metadata_db).await else {
            return NotificationResult {
                success: false,
                provider_id: None,
                error_message: Some("Telegram notifications are not configured".to_string()),
            };
        };
        let url = format!("{}/bot{}/sendMessage", self.api_base, bot_token);
        match self
            .client
            .post(&url)
            .json(&json!({
                "chat_id": chat_id,
                "text": text,
                "disable_web_page_preview": true,
            }))
            .send()
            .await
        {
            Ok(response) => {
                let status = response.status();
                let parsed = response.json::<TelegramApiResponse>().await.ok();
                if status.is_success() && parsed.as_ref().is_some_and(|body| body.ok) {
                    NotificationResult {
                        success: true,
                        provider_id: Some(format!("telegram_{}", uuid::Uuid::new_v4())),
                        error_message: None,
                    }
                } else {
                    NotificationResult {
                        success: false,
                        provider_id: None,
                        error_message: Some(
                            parsed
                                .and_then(|body| body.description)
                                .unwrap_or_else(|| format!("HTTP {}", status.as_u16())),
                        ),
                    }
                }
            }
            Err(_) => NotificationResult {
                success: false,
                provider_id: None,
                error_message: Some("Request to Telegram failed".to_string()),
            },
        }
    }
}

fn telegram_http_client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("failed to build Telegram HTTP client")
}

fn env_bot_token() -> Option<String> {
    let bot_token = std::env::var(TELEGRAM_BOT_TOKEN_ENV)
        .ok()?
        .trim()
        .to_string();
    if bot_token.is_empty() {
        None
    } else {
        Some(bot_token)
    }
}

/// A Settings token wins when set; otherwise use the optional compose/VPS env fallback.
pub async fn resolve_bot_token(metadata_db: &MetadataDb) -> Option<String> {
    if crate::config::AppConfig::restore_drill_enabled() {
        return None;
    }

    if let Ok(Some(token)) = metadata_db
        .get_instance_secret(TELEGRAM_BOT_TOKEN_SETTING_KEY)
        .await
    {
        let token = token.trim().to_string();
        if !token.is_empty() {
            return Some(token);
        }
    }

    env_bot_token()
}

pub async fn is_configured(metadata_db: &MetadataDb) -> bool {
    resolve_bot_token(metadata_db).await.is_some()
}

pub async fn set_bot_token(metadata_db: &MetadataDb, token: &str) -> Result<()> {
    let token = token.trim();
    if token.is_empty() {
        metadata_db
            .delete_instance_secret(TELEGRAM_BOT_TOKEN_SETTING_KEY)
            .await
    } else {
        metadata_db
            .set_instance_secret(TELEGRAM_BOT_TOKEN_SETTING_KEY, token)
            .await
    }
}

pub fn validate_telegram_chat_id(input: &str) -> Result<String, String> {
    let chat_id = input.trim();
    if chat_id.is_empty() {
        return Err("Telegram chat ID cannot be empty".to_string());
    }
    if let Some(username) = chat_id.strip_prefix('@') {
        if !(5..=32).contains(&username.len())
            || !username
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
            || !username
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
        {
            return Err(
                "Enter a Telegram @username starting with a letter, 5-32 characters".to_string(),
            );
        }
        return Ok(format!("@{username}"));
    }
    if chat_id
        .strip_prefix('-')
        .unwrap_or(chat_id)
        .chars()
        .all(|c| c.is_ascii_digit())
        && (1..=19).contains(&chat_id.strip_prefix('-').unwrap_or(chat_id).len())
    {
        return Ok(chat_id.to_string());
    }
    Err("Enter a numeric Telegram chat ID or an @username".to_string())
}

#[async_trait]
impl NotificationProvider for TelegramProvider {
    async fn send_notification(
        &self,
        notification: &TransactionNotification,
        wallet_name: &str,
        contacts: &[Contact],
        user_language: &Language,
        wallet_balance_sats: Option<i64>,
    ) -> Vec<(NotificationMethod, NotificationResult, String)> {
        use futures::{stream, StreamExt};

        let send_jobs: Vec<(NotificationMethod, String)> =
            notification_methods_for_provider(contacts, &ProviderType::Telegram)
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

        stream::iter(send_jobs)
            .map(|(method, message)| async move {
                let result = self
                    .send_message(&method.notification_target, &message)
                    .await;
                (method, result, message)
            })
            .buffer_unordered(TELEGRAM_SEND_CONCURRENCY)
            .collect()
            .await
    }

    fn provider_info(&self) -> ProviderInfo {
        ProviderInfo {
            name: "telegram".to_string(),
            display_name: "Telegram".to_string(),
            config_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": {
                        "type": "string",
                        "title": "Telegram chat ID",
                        "description": "Numeric chat ID or public @username"
                    }
                },
                "required": ["chat_id"]
            }),
        }
    }

    fn name(&self) -> &'static str {
        "telegram"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, NetworkConfig, OperatingMode};
    use tempfile::tempdir;
    use tokio::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::const_new(());

    struct EnvGuard {
        restore_drill: Option<String>,
        telegram_token: Option<String>,
    }

    impl EnvGuard {
        fn capture() -> Self {
            Self {
                restore_drill: std::env::var("CANARY_RESTORE_DRILL").ok(),
                telegram_token: std::env::var(TELEGRAM_BOT_TOKEN_ENV).ok(),
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            restore_env_var("CANARY_RESTORE_DRILL", self.restore_drill.clone());
            restore_env_var(TELEGRAM_BOT_TOKEN_ENV, self.telegram_token.clone());
        }
    }

    fn restore_env_var(name: &str, value: Option<String>) {
        if let Some(value) = value {
            std::env::set_var(name, value);
        } else {
            std::env::remove_var(name);
        }
    }

    async fn create_test_db() -> (MetadataDb, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let test_config = AppConfig::new_for_test(
            NetworkConfig::Regtest,
            Some("tcp://127.0.0.1:50001".to_string()),
            "127.0.0.1:3000".to_string(),
            temp_dir.path().to_string_lossy().to_string(),
            OperatingMode::SelfHosted,
            None,
            None,
        );
        let db = MetadataDb::new(db_path.to_str().unwrap(), &test_config)
            .await
            .unwrap();
        (db, temp_dir)
    }

    #[test]
    fn accepts_numeric_and_username_chat_ids() {
        assert_eq!(validate_telegram_chat_id(" 12345 ").unwrap(), "12345");
        assert_eq!(
            validate_telegram_chat_id("-1001234567890").unwrap(),
            "-1001234567890"
        );
        assert_eq!(
            validate_telegram_chat_id("@CanaryAlerts").unwrap(),
            "@CanaryAlerts"
        );
    }

    #[test]
    fn rejects_empty_and_malformed_chat_ids() {
        assert!(validate_telegram_chat_id("").is_err());
        assert!(validate_telegram_chat_id("@ab").is_err());
        assert!(validate_telegram_chat_id("@1channel").is_err());
        assert!(validate_telegram_chat_id("https://t.me/canary").is_err());
        assert!(validate_telegram_chat_id("chat id").is_err());
        assert!(validate_telegram_chat_id("12345678901234567890").is_err());
    }

    #[tokio::test]
    async fn settings_token_wins_over_env() {
        let _lock = ENV_LOCK.lock().await;
        let _env = EnvGuard::capture();
        std::env::remove_var("CANARY_RESTORE_DRILL");
        std::env::set_var(TELEGRAM_BOT_TOKEN_ENV, "env-token");
        let (db, _temp) = create_test_db().await;

        set_bot_token(&db, "settings-token").await.unwrap();

        assert_eq!(
            resolve_bot_token(&db).await.as_deref(),
            Some("settings-token")
        );
    }

    #[tokio::test]
    async fn clearing_settings_falls_back_to_env() {
        let _lock = ENV_LOCK.lock().await;
        let _env = EnvGuard::capture();
        std::env::remove_var("CANARY_RESTORE_DRILL");
        std::env::set_var(TELEGRAM_BOT_TOKEN_ENV, "env-token");
        let (db, _temp) = create_test_db().await;

        set_bot_token(&db, "settings-token").await.unwrap();
        set_bot_token(&db, "  ").await.unwrap();

        assert_eq!(resolve_bot_token(&db).await.as_deref(), Some("env-token"));
    }

    #[tokio::test]
    async fn restore_drill_disables_telegram() {
        let _lock = ENV_LOCK.lock().await;
        let _env = EnvGuard::capture();
        std::env::set_var("CANARY_RESTORE_DRILL", "1");
        std::env::set_var(TELEGRAM_BOT_TOKEN_ENV, "env-token");
        let (db, _temp) = create_test_db().await;
        set_bot_token(&db, "settings-token").await.unwrap();

        assert!(resolve_bot_token(&db).await.is_none());
        assert!(TelegramProvider::for_self_hosted(db).is_none());
    }
}
