use crate::message_formatter::MessageFormatter;
use crate::metadata::{
    Contact, Language, NotificationMethod, ProviderType, TransactionNotification,
};
use crate::notifications::{
    notification_methods_for_provider, NotificationProvider, NotificationResult, ProviderInfo,
};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

const TELEGRAM_BOT_TOKEN_ENV: &str = "TELEGRAM_BOT_TOKEN";
const DEFAULT_TELEGRAM_API_BASE: &str = "https://api.telegram.org";
const TELEGRAM_SEND_CONCURRENCY: usize = 4;

pub struct TelegramProvider {
    bot_token: String,
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
    pub fn from_env() -> Option<Self> {
        if crate::config::AppConfig::restore_drill_enabled() {
            return None;
        }
        let bot_token = std::env::var(TELEGRAM_BOT_TOKEN_ENV)
            .ok()?
            .trim()
            .to_string();
        if bot_token.is_empty() {
            return None;
        }
        Some(Self::new(bot_token, DEFAULT_TELEGRAM_API_BASE.to_string()))
    }

    pub fn new(bot_token: String, api_base: String) -> Self {
        Self {
            bot_token,
            api_base: api_base.trim_end_matches('/').to_string(),
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("failed to build Telegram HTTP client"),
        }
    }

    pub async fn send_message(&self, chat_id: &str, text: &str) -> NotificationResult {
        let url = format!("{}/bot{}/sendMessage", self.api_base, self.bot_token);
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
        && (1..=20).contains(&chat_id.strip_prefix('-').unwrap_or(chat_id).len())
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
    }
}
