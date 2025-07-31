use crate::metadata::{Contact, NotificationMethod, ProviderType, TransactionEvent, EventType, Language};
use crate::message_formatter::MessageFormatter;
use crate::notifications::{NotificationProvider, NotificationResult, ProviderInfo};
use async_trait::async_trait;
use serde_json::json;

pub struct NtfyProvider {
    client: reqwest::Client,
}

impl NtfyProvider {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    fn generate_topic_from_name(&self, name: &str, language: &Language) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        // Create a simple topic from name and language
        let mut hasher = DefaultHasher::new();
        format!("{}-{}", name, language.as_str()).hash(&mut hasher);
        let hash = hasher.finish();
        
        // Convert to a readable format
        let clean_name = name.to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .take(10)
            .collect::<String>();
            
        format!("{}-{}-{:x}", clean_name, language.as_str(), hash % 0xffffff)
    }
}

#[async_trait]
impl NotificationProvider for NtfyProvider {
    async fn send_notification(
        &self,
        event: &TransactionEvent,
        wallet_name: &str,
        contacts: &[Contact],
    ) -> Vec<(NotificationMethod, NotificationResult, String)> {
        let mut results = Vec::new();
        
        for contact in contacts {
            // Find ntfy notification methods for this contact, or auto-generate if none exists
            let mut ntfy_methods: Vec<&NotificationMethod> = contact.notification_methods
                .iter()
                .filter(|method| matches!(method.provider_type, ProviderType::Ntfy))
                .collect();

            // If no ntfy methods, auto-generate one from contact name (legacy behavior)
            let auto_generated_method;
            if ntfy_methods.is_empty() {
                let topic = self.generate_topic_from_name(&contact.name, &contact.language);
                auto_generated_method = NotificationMethod {
                    id: None,
                    contact_id: contact.id.unwrap_or(0),
                    provider_type: ProviderType::Ntfy,
                    notification_target: topic,
                    created_at: String::new(),
                };
                ntfy_methods.push(&auto_generated_method);
            }

            for method in ntfy_methods {
                let message = MessageFormatter::create_localized_message(event, wallet_name, &contact.language);
                
                let topic = &method.notification_target;
                let ntfy_url = format!("https://ntfy.sh/{}", topic);
                
                println!("📱 Sending ntfy notification to topic '{}' for {}", topic, contact.name);
                println!("   Message: {}", message);
                
                let result = match self.client
                    .post(&ntfy_url)
                    .header("Content-Type", "text/plain; charset=utf-8")
                    .header("Title", format!("Canary - {}", wallet_name))
                    .header("Priority", if event.is_confirmed { "default" } else { "high" })
                    .header("Tags", if event.event_type == EventType::Receive { "money_with_wings" } else { "arrow_right" })
                    .body(message.clone())
                    .send()
                    .await
                {
                    Ok(response) => {
                        if response.status().is_success() {
                            println!("✅ Successfully sent ntfy notification to {}", contact.name);
                            NotificationResult {
                                success: true,
                                provider_id: Some(format!("ntfy_{}", chrono::Utc::now().timestamp())),
                                error_message: None,
                            }
                        } else {
                            let error = format!("HTTP {}: {}", response.status(), response.status().canonical_reason().unwrap_or("Unknown"));
                            println!("❌ Failed to send ntfy notification to {}: {}", contact.name, error);
                            NotificationResult {
                                success: false,
                                provider_id: None,
                                error_message: Some(error),
                            }
                        }
                    }
                    Err(e) => {
                        let error = format!("Request failed: {}", e);
                        println!("❌ Failed to send ntfy notification to {}: {}", contact.name, error);
                        NotificationResult {
                            success: false,
                            provider_id: None,
                            error_message: Some(error),
                        }
                    }
                };
                
                results.push((method.clone(), result, message));
            }
        }
        
        results
    }
    
    fn provider_info(&self) -> ProviderInfo {
        ProviderInfo {
            name: "ntfy".to_string(),
            display_name: "ntfy.sh Notifications".to_string(),
            config_schema: json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "title": "ntfy Topic",
                        "description": "The ntfy.sh topic name to send notifications to (e.g., 'my-bitcoin-wallet')"
                    }
                },
                "required": ["topic"]
            }),
        }
    }
    
    fn name(&self) -> &'static str {
        "ntfy"
    }
}

impl Default for NtfyProvider {
    fn default() -> Self {
        Self::new()
    }
}