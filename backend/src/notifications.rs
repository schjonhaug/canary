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
        wallet_balance_sats: Option<i64>,
    ) -> Vec<(NotificationMethod, NotificationResult, String)>;

    fn provider_info(&self) -> ProviderInfo;
    fn name(&self) -> &'static str;
}

pub fn notification_methods_for_provider<'a>(
    contacts: &'a [Contact],
    provider_type: &'a crate::metadata::ProviderType,
) -> impl Iterator<Item = (&'a Contact, &'a NotificationMethod)> + 'a {
    contacts.iter().flat_map(move |contact| {
        contact
            .notification_methods
            .iter()
            .filter(move |method| method.is_enabled && &method.provider_type == provider_type)
            .map(move |method| (contact, method))
    })
}

pub fn contact_allows_notification(
    contact: &Contact,
    notification: &TransactionNotification,
) -> bool {
    match notification {
        TransactionNotification::Pending(tx) => {
            if tx.transaction_status == "replaced" {
                contact.notify_rbf
            } else if tx.parent_txid.is_some() {
                contact.notify_cpfp
            } else {
                match tx.transaction_type {
                    crate::metadata::EventType::Send => contact.notify_sending,
                    crate::metadata::EventType::Receive => contact.notify_receiving,
                }
            }
        }
        TransactionNotification::Confirmed(tx) => match tx.transaction_type {
            crate::metadata::EventType::Send => contact.notify_sent,
            crate::metadata::EventType::Receive => contact.notify_received,
        },
        TransactionNotification::BalanceAlert(alert) => match alert.contact_id.as_ref() {
            Some(contact_id) => {
                // Stored contacts should have ids; if that invariant is broken,
                // never deliver a contact-specific alert to an ambiguous contact.
                if contact.id.is_none() {
                    return false;
                }
                contact.id.as_deref() == Some(contact_id.as_str())
            }
            None => true,
        },
    }
}

pub fn notification_log_type(notification: &TransactionNotification) -> &'static str {
    match notification {
        TransactionNotification::Pending(tx) => {
            if tx.transaction_status == "replaced" {
                "rbf"
            } else if tx.parent_txid.is_some() {
                "cpfp"
            } else {
                match tx.transaction_type {
                    crate::metadata::EventType::Send => "sending",
                    crate::metadata::EventType::Receive => "receiving",
                }
            }
        }
        TransactionNotification::Confirmed(tx) => match tx.transaction_type {
            crate::metadata::EventType::Send => "sent",
            crate::metadata::EventType::Receive => "received",
        },
        TransactionNotification::BalanceAlert(_) => "balance_alert",
    }
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
        wallet_balance_sats: Option<i64>,
    ) -> Result<Vec<(NotificationMethod, NotificationResult, String)>> {
        match self.providers.get(provider_name) {
            Some(provider) => {
                let results = provider
                    .send_notification(
                        notification,
                        wallet_name,
                        contacts,
                        user_language,
                        wallet_balance_sats,
                    )
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
