use crate::metadata::{Contact, NotificationMethod, TransactionEvent};
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationResult {
    pub success: bool,
    pub provider_id: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProviderInfo {
    pub name: String,
    pub display_name: String,
    pub config_schema: serde_json::Value,
}

#[async_trait]
pub trait NotificationProvider: Send + Sync {
    async fn send_notification(
        &self,
        event: &TransactionEvent,
        wallet_name: &str,
        contacts: &[Contact],
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

    pub fn get_provider(&self, name: &str) -> Option<Arc<dyn NotificationProvider>> {
        self.providers.get(name).cloned()
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
        event: &TransactionEvent,
        wallet_name: &str,
        contacts: &[Contact],
    ) -> Result<Vec<(NotificationMethod, NotificationResult, String)>> {
        match self.providers.get(provider_name) {
            Some(provider) => {
                let results = provider
                    .send_notification(event, wallet_name, contacts)
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