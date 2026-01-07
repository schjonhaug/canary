use crate::metadata::{Contact, Language, NotificationMethod, TransactionNotification};
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationResult {
    pub success: bool,
    pub provider_id: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub name: String,
    pub display_name: String,
    pub config_schema: serde_json::Value,
}

#[async_trait]
pub trait NotificationProvider: Send + Sync {
    async fn send_notification(
        &self,
        notification: &TransactionNotification,
        wallet_name: &str,
        contacts: &[Contact],
        user_language: &Language,
    ) -> Vec<(NotificationMethod, NotificationResult, String)>;

    fn provider_info(&self) -> ProviderInfo;
    fn name(&self) -> &'static str;
}

pub struct NotificationManager {
    providers: HashMap<String, Arc<dyn NotificationProvider>>,
}

impl NotificationManager {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    pub fn register_provider(&mut self, provider: Arc<dyn NotificationProvider>) {
        let name = provider.name().to_string();
        self.providers.insert(name, provider);
    }

    pub fn list_providers(&self) -> Vec<ProviderInfo> {
        self.providers
            .values()
            .map(|provider| provider.provider_info())
            .collect()
    }

    pub async fn send_notifications(
        &self,
        provider_name: &str,
        notification: &TransactionNotification,
        wallet_name: &str,
        contacts: &[Contact],
        user_language: &Language,
    ) -> Result<Vec<(NotificationMethod, NotificationResult, String)>> {
        match self.providers.get(provider_name) {
            Some(provider) => {
                let results = provider
                    .send_notification(notification, wallet_name, contacts, user_language)
                    .await;
                Ok(results)
            }
            None => Err(anyhow::anyhow!(
                "Notification provider '{}' not found",
                provider_name
            )),
        }
    }
}

impl Default for NotificationManager {
    fn default() -> Self {
        Self::new()
    }
}
