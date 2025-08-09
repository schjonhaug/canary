use crate::email_service::EmailService;
use crate::message_formatter::MessageFormatter;
use crate::metadata::{Contact, NotificationMethod, ProviderType, TransactionEvent};
use crate::notifications::{NotificationProvider, NotificationResult, ProviderInfo};
use anyhow::Result;
use async_trait::async_trait;
use serde_json;

pub struct EmailProvider {
    email_service: Option<EmailService>,
}

impl EmailProvider {
    pub fn new() -> Self {
        // Try to create email service from environment
        let email_service = EmailService::from_env().ok();

        if email_service.is_none() {
            eprintln!("Warning: Email provider created but Resend not configured");
        }

        Self { email_service }
    }
}

#[async_trait]
impl NotificationProvider for EmailProvider {
    async fn send_notification(
        &self,
        event: &TransactionEvent,
        wallet_name: &str,
        contacts: &[Contact],
    ) -> Vec<(NotificationMethod, NotificationResult, String)> {
        let mut results = Vec::new();

        for contact in contacts {
            // Find email notification methods for this contact
            let email_methods: Vec<&NotificationMethod> = contact
                .notification_methods
                .iter()
                .filter(|method| matches!(method.provider_type, ProviderType::Email))
                .collect();

            for method in email_methods {
                let message = MessageFormatter::create_localized_message(
                    event,
                    wallet_name,
                    &contact.language,
                );
                let email_address = &method.notification_target;

                let result = if let Some(email_service) = &self.email_service {
                    // Clone data for background task
                    let email_service_clone = email_service.clone();
                    let email_address = email_address.to_string();
                    let contact_name = contact.name.clone();
                    let wallet_name = wallet_name.to_string();
                    let event_clone = event.clone();
                    let message_clone = message.clone();
                    let _method_id = method.id.clone();

                    // Spawn background task for email sending - don't wait for it
                    tokio::spawn(async move {
                        match Self::send_transaction_email_static(
                            &email_service_clone,
                            &email_address,
                            &contact_name,
                            &wallet_name,
                            &event_clone,
                            &message_clone,
                        )
                        .await
                        {
                            Ok(_) => {
                                println!("✅ Email sent successfully to {}", email_address);
                            }
                            Err(e) => {
                                eprintln!("❌ Failed to send email to {}: {}", email_address, e);
                            }
                        }
                    });

                    // Return success immediately - email will be sent in background
                    NotificationResult {
                        success: true,
                        provider_id: Some("email".to_string()),
                        error_message: None,
                    }
                } else {
                    NotificationResult {
                        success: false,
                        provider_id: Some("email".to_string()),
                        error_message: Some("Resend not configured".to_string()),
                    }
                };

                results.push((method.clone(), result, message));
            }
        }

        results
    }

    fn provider_info(&self) -> ProviderInfo {
        ProviderInfo {
            name: "email".to_string(),
            display_name: "Email".to_string(),
            config_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "resend_api_key": {"type": "string"},
                    "from_email": {"type": "string"},
                    "from_name": {"type": "string"}
                }
            }),
        }
    }

    fn name(&self) -> &'static str {
        "email"
    }
}

impl EmailProvider {
    // Static version for use in spawned tasks
    async fn send_transaction_email_static(
        email_service: &EmailService,
        to_email: &str,
        to_name: &str,
        wallet_name: &str,
        event: &TransactionEvent,
        message: &str,
    ) -> Result<()> {
        Self::send_transaction_email_impl(
            email_service,
            to_email,
            to_name,
            wallet_name,
            event,
            message,
        )
        .await
    }

    // Shared implementation
    async fn send_transaction_email_impl(
        email_service: &EmailService,
        to_email: &str,
        to_name: &str,
        wallet_name: &str,
        event: &TransactionEvent,
        message: &str,
    ) -> Result<()> {
        // Determine the type of transaction
        let (subject, emoji) = match event.event_type {
            crate::metadata::EventType::Receive => ("Bitcoin Received", "💰"),
            crate::metadata::EventType::Send => ("Bitcoin Sent", "📤"),
        };

        let subject = format!("{} {} - {}", emoji, subject, wallet_name);

        // Create HTML body with better formatting
        let html_body = format!(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta charset="utf-8">
                <title>{}</title>
            </head>
            <body style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto; padding: 20px;">
                <div style="background-color: #f9fafb; padding: 20px; border-radius: 8px; margin-bottom: 20px;">
                    <h2 style="color: #1f2937; margin-top: 0;">{} Bitcoin Transaction</h2>
                    <p style="color: #4b5563; line-height: 1.6;">
                        Hi {},
                    </p>
                    <div style="background-color: white; padding: 15px; border-radius: 6px; margin: 20px 0;">
                        <pre style="white-space: pre-wrap; word-wrap: break-word; font-family: 'Courier New', monospace; color: #374151; margin: 0;">{}</pre>
                    </div>
                    <p style="color: #6b7280; font-size: 14px; line-height: 1.6;">
                        Wallet: {}
                    </p>
                </div>
                
                <div style="text-align: center; color: #6b7280; font-size: 12px; margin-top: 30px;">
                    <p>This notification was sent by Canary Wallet</p>
                </div>
            </body>
            </html>
            "#,
            subject,
            emoji,
            to_name,
            message,
            &event.wallet_checksum[..8] // Show first 8 chars of wallet checksum
        );

        let text_body = format!(
            "{}\n\nHi {},\n\n{}\n\nWallet: {}\n\nThis notification was sent by Canary Wallet",
            subject,
            to_name,
            message,
            &event.wallet_checksum[..8]
        );

        // Send using the Resend email service
        email_service
            .send_transaction_notification(to_email, to_name, &subject, &html_body, &text_body)
            .await
    }
}
