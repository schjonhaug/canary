use crate::email_service::{BatchEmailRequest, EmailService};
use crate::message_formatter::MessageFormatter;
use crate::metadata::{Contact, NotificationMethod, ProviderType, TransactionNotification};
use crate::notifications::{NotificationProvider, NotificationResult, ProviderInfo};
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
        notification: &TransactionNotification,
        wallet_name: &str,
        contacts: &[Contact],
    ) -> Vec<(NotificationMethod, NotificationResult, String)> {
        let mut results = Vec::new();

        // If no email service configured, return early
        let Some(email_service) = &self.email_service else {
            // Return error for all email methods
            for contact in contacts {
                for method in contact
                    .notification_methods
                    .iter()
                    .filter(|m| matches!(m.provider_type, ProviderType::Email))
                {
                    let message = MessageFormatter::create_localized_message(
                        notification,
                        wallet_name,
                        &contact.language,
                    );
                    results.push((
                        method.clone(),
                        NotificationResult {
                            success: false,
                            provider_id: Some("email".to_string()),
                            error_message: Some("Resend not configured".to_string()),
                        },
                        message,
                    ));
                }
            }
            return results;
        };

        // Collect all email data for batch sending
        let mut batch_data = Vec::new();

        for contact in contacts {
            let email_methods: Vec<&NotificationMethod> = contact
                .notification_methods
                .iter()
                .filter(|method| matches!(method.provider_type, ProviderType::Email))
                .collect();

            for method in email_methods {
                let message = MessageFormatter::create_localized_message(
                    notification,
                    wallet_name,
                    &contact.language,
                );

                // Build email subject and body based on notification type
                let subject = MessageFormatter::create_localized_email_subject(
                    notification,
                    wallet_name,
                    &contact.language,
                );

                let (html_body, text_body) = match notification {
                    TransactionNotification::Pending(tx)
                    | TransactionNotification::Confirmed(tx) => {
                        let emoji = if matches!(notification, TransactionNotification::Confirmed(_)) {
                            "✅"
                        } else {
                            match tx.transaction_type {
                                crate::metadata::EventType::Receive => "💸",
                                crate::metadata::EventType::Send => "📤",
                            }
                        };

                        let html_body = Self::build_transaction_html(
                            &subject,
                            emoji,
                            &contact.name,
                            &message,
                            &tx.wallet_checksum,
                        );

                        let text_body = Self::build_transaction_text(
                            &subject,
                            &contact.name,
                            &message,
                            &tx.wallet_checksum,
                        );

                        (html_body, text_body)
                    }
                    TransactionNotification::BalanceAlert(_) => {
                        let html_body = Self::build_balance_alert_html(wallet_name, &contact.name, &message);
                        let text_body = Self::build_balance_alert_text(wallet_name, &contact.name, &message);
                        (html_body, text_body)
                    }
                };

                batch_data.push((
                    method.clone(),
                    message.clone(),
                    BatchEmailRequest {
                        to_email: method.notification_target.clone(),
                        to_name: contact.name.clone(),
                        subject,
                        html_body,
                        text_body,
                    },
                ));
            }
        }

        // If no emails to send, return empty
        if batch_data.is_empty() {
            return results;
        }

        // Extract batch requests for queuing
        let batch_requests: Vec<BatchEmailRequest> = batch_data.iter().map(|(_, _, req)| req.clone()).collect();

        // Queue emails for background sending (with rate limiting and retries)
        let batch_results = email_service.send_batch_emails(batch_requests).await;

        // Process results and return
        for ((method, message, _), result) in batch_data.into_iter().zip(batch_results.into_iter()) {
            match result {
                Ok(_) => {
                    // Email queued successfully
                    results.push((
                        method,
                        NotificationResult {
                            success: true,
                            provider_id: Some("email".to_string()),
                            error_message: None,
                        },
                        message,
                    ));
                }
                Err(e) => {
                    // Failed to queue email
                    eprintln!("❌ Failed to queue email: {}", e);
                    results.push((
                        method,
                        NotificationResult {
                            success: false,
                            provider_id: Some("email".to_string()),
                            error_message: Some(format!("Failed to queue: {}", e)),
                        },
                        message,
                    ));
                }
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
    // Helper method to build transaction email HTML
    fn build_transaction_html(
        subject: &str,
        emoji: &str,
        to_name: &str,
        message: &str,
        wallet_checksum: &str,
    ) -> String {
        format!(
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
            &wallet_checksum[..8] // Show first 8 chars of wallet checksum
        )
    }

    // Helper method to build transaction email text
    fn build_transaction_text(
        subject: &str,
        to_name: &str,
        message: &str,
        wallet_checksum: &str,
    ) -> String {
        format!(
            "{}\n\nHi {},\n\n{}\n\nWallet: {}\n\nThis notification was sent by Canary Wallet",
            subject,
            to_name,
            message,
            &wallet_checksum[..8]
        )
    }

    // Helper method to build balance alert email HTML
    fn build_balance_alert_html(wallet_name: &str, to_name: &str, message: &str) -> String {
        let subject = format!("📊 Balance Alert - {}", wallet_name);
        format!(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta charset="utf-8">
                <title>{}</title>
            </head>
            <body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif; line-height: 1.6; max-width: 600px; margin: 0 auto; padding: 20px;">
                <div style="background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); color: white; padding: 20px; border-radius: 8px; margin-bottom: 20px;">
                    <h1 style="margin: 0; font-size: 24px;">📊 Balance Alert</h1>
                    <p style="margin: 5px 0 0 0; opacity: 0.9;">Wallet: {}</p>
                </div>

                <div style="background: #f8f9fa; padding: 20px; border-radius: 8px; margin-bottom: 20px;">
                    <p style="margin: 0; font-size: 16px; color: #333;">Hi {},</p>
                    <p style="margin: 15px 0; font-size: 16px; color: #333;">{}</p>
                </div>

                <div style="text-align: center; color: #666; font-size: 12px; margin-top: 30px;">
                    <p>This notification was sent by Canary Wallet</p>
                </div>
            </body>
            </html>
            "#,
            subject, wallet_name, to_name, message
        )
    }

    // Helper method to build balance alert email text
    fn build_balance_alert_text(wallet_name: &str, to_name: &str, message: &str) -> String {
        let subject = format!("📊 Balance Alert - {}", wallet_name);
        format!(
            "{}\n\nHi {},\n\n{}\n\nWallet: {}\n\nThis notification was sent by Canary Wallet",
            subject, to_name, message, wallet_name
        )
    }
}
