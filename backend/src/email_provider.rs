use crate::email_service::{BatchEmailRequest, EmailService};
use crate::message_formatter::MessageFormatter;
use crate::metadata::{
    Contact, ContentPrivacyLevel, Language, NotificationMethod, ProviderType,
    TransactionNotification,
};
use crate::notifications::{
    notification_methods_for_provider, NotificationProvider, NotificationResult, ProviderInfo,
};
use async_trait::async_trait;
use rust_i18n::t;

pub struct EmailProvider {
    email_service: Option<EmailService>,
}

impl Default for EmailProvider {
    fn default() -> Self {
        Self::new()
    }
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
        user_language: &Language,
        wallet_balance_sats: Option<i64>,
    ) -> Vec<(NotificationMethod, NotificationResult, String)> {
        let mut results = Vec::new();

        // If no email service configured, return early
        let Some(email_service) = &self.email_service else {
            // Return error for all email methods
            for (contact, method) in
                notification_methods_for_provider(contacts, &ProviderType::Email)
            {
                let message = MessageFormatter::create_localized_message_for_level(
                    notification,
                    wallet_name,
                    user_language,
                    contact.include_wallet_balance_in_tx_notifications,
                    wallet_balance_sats,
                    method.content_privacy_level,
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
            return results;
        };

        // Collect all email data for batch sending
        let mut batch_data = Vec::new();

        for (contact, method) in notification_methods_for_provider(contacts, &ProviderType::Email) {
            let message = MessageFormatter::create_localized_message_for_level(
                notification,
                wallet_name,
                user_language,
                contact.include_wallet_balance_in_tx_notifications,
                wallet_balance_sats,
                method.content_privacy_level,
            );

            // Build email subject and body based on notification type
            let subject = MessageFormatter::create_localized_email_subject_for_level(
                notification,
                wallet_name,
                user_language,
                method.content_privacy_level,
            );

            let (html_body, text_body) = match notification {
                TransactionNotification::Pending(tx) | TransactionNotification::Confirmed(tx) => {
                    let emoji = match method.content_privacy_level {
                        ContentPrivacyLevel::Minimal => "🔔",
                        _ => match tx.transaction_type {
                            crate::metadata::EventType::Receive => "💸",
                            crate::metadata::EventType::Send => "📤",
                        },
                    };

                    let html_body = Self::build_transaction_html(
                        &subject,
                        emoji,
                        &contact.name,
                        &message,
                        user_language,
                    );

                    let text_body = Self::build_transaction_text(
                        &subject,
                        &contact.name,
                        &message,
                        user_language,
                    );

                    (html_body, text_body)
                }
                TransactionNotification::BalanceAlert(_)
                    if method.content_privacy_level == ContentPrivacyLevel::Detailed =>
                {
                    let html_body = Self::build_balance_alert_html(
                        wallet_name,
                        &contact.name,
                        &message,
                        user_language,
                    );
                    let text_body = Self::build_balance_alert_text(
                        wallet_name,
                        &contact.name,
                        &message,
                        user_language,
                    );
                    (html_body, text_body)
                }
                TransactionNotification::BalanceAlert(_) => (
                    Self::build_transaction_html(
                        &subject,
                        "🔔",
                        &contact.name,
                        &message,
                        user_language,
                    ),
                    Self::build_transaction_text(&subject, &contact.name, &message, user_language),
                ),
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

        // If no emails to send, return empty
        if batch_data.is_empty() {
            return results;
        }

        // Extract batch requests for queuing
        let batch_requests: Vec<BatchEmailRequest> =
            batch_data.iter().map(|(_, _, req)| req.clone()).collect();

        // Queue emails for background sending (with rate limiting and retries)
        let batch_results = email_service.send_batch_emails(batch_requests).await;

        // Process results and return
        for ((method, message, _), result) in batch_data.into_iter().zip(batch_results) {
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
        language: &Language,
    ) -> String {
        let locale = language.as_str();
        let header = t!("email.transaction.header", locale = locale).to_string();
        let greeting = t!("common.greeting", locale = locale, to_name = to_name).to_string();
        let footer = t!("common.footer", locale = locale).to_string();

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
                    <h2 style="color: #1f2937; margin-top: 0;">{} {}</h2>
                    <p style="color: #4b5563; line-height: 1.6;">
                        {}
                    </p>
                    <div style="background-color: white; padding: 15px; border-radius: 6px; margin: 20px 0;">
                        <pre style="white-space: pre-wrap; word-wrap: break-word; font-family: 'Courier New', monospace; color: #374151; margin: 0;">{}</pre>
                    </div>
                </div>

                <div style="text-align: center; color: #6b7280; font-size: 12px; margin-top: 30px;">
                    <p>{}</p>
                </div>
            </body>
            </html>
            "#,
            subject, emoji, header, greeting, message, footer
        )
    }

    // Helper method to build transaction email text
    fn build_transaction_text(
        subject: &str,
        to_name: &str,
        message: &str,
        language: &Language,
    ) -> String {
        let locale = language.as_str();
        let greeting = t!("common.greeting", locale = locale, to_name = to_name).to_string();
        let footer = t!("common.footer", locale = locale).to_string();

        format!("{}\n\n{}\n\n{}\n\n{}", subject, greeting, message, footer)
    }

    // Helper method to build balance alert email HTML
    fn build_balance_alert_html(
        wallet_name: &str,
        to_name: &str,
        message: &str,
        language: &Language,
    ) -> String {
        let locale = language.as_str();
        let header = t!("email.balance_alert.header", locale = locale).to_string();
        let wallet_label = t!("common.wallet", locale = locale).to_string();
        let greeting = t!("common.greeting", locale = locale, to_name = to_name).to_string();
        let footer = t!("common.footer", locale = locale).to_string();

        let subject = format!("📊 {} - {}", header, wallet_name);
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
                    <h1 style="margin: 0; font-size: 24px;">📊 {}</h1>
                    <p style="margin: 5px 0 0 0; opacity: 0.9;">{}: {}</p>
                </div>

                <div style="background: #f8f9fa; padding: 20px; border-radius: 8px; margin-bottom: 20px;">
                    <p style="margin: 0; font-size: 16px; color: #333;">{}</p>
                    <p style="margin: 15px 0; font-size: 16px; color: #333;">{}</p>
                </div>

                <div style="text-align: center; color: #666; font-size: 12px; margin-top: 30px;">
                    <p>{}</p>
                </div>
            </body>
            </html>
            "#,
            subject, header, wallet_label, wallet_name, greeting, message, footer
        )
    }

    // Helper method to build balance alert email text
    fn build_balance_alert_text(
        wallet_name: &str,
        to_name: &str,
        message: &str,
        language: &Language,
    ) -> String {
        let locale = language.as_str();
        let header = t!("email.balance_alert.header", locale = locale).to_string();
        let wallet_label = t!("common.wallet", locale = locale).to_string();
        let greeting = t!("common.greeting", locale = locale, to_name = to_name).to_string();
        let footer = t!("common.footer", locale = locale).to_string();

        let subject = format!("📊 {} - {}", header, wallet_name);
        format!(
            "{}\n\n{}\n\n{}\n\n{}: {}\n\n{}",
            subject, greeting, message, wallet_label, wallet_name, footer
        )
    }
}
