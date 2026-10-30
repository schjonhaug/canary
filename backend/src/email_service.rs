//! Transactional email through Resend: account verification, password reset,
//! contact codes, registration and trial-ending notices, contact form submissions,
//! and the marketing audience.

use crate::config::AppConfig;
use crate::email_queue;
use anyhow::{Result, anyhow};
use rand::RngExt;
use resend_rs::Resend;
use resend_rs::types::{CreateContactOptions, CreateEmailBaseOptions};
use rust_i18n::t;

// SDK errors may contain response bodies, recipient details or request URLs.
// Do not retain them as an anyhow source: callers also render debug/error chains.
fn safe_provider_error(error: resend_rs::Error) -> anyhow::Error {
    match error {
        resend_rs::Error::Resend(response) => {
            anyhow!(
                "Email provider rejected request (HTTP {})",
                response.status_code
            )
        }
        resend_rs::Error::Http(_) => anyhow!("Email provider transport failed"),
        resend_rs::Error::Parse(_) => anyhow!("Email provider returned an invalid response"),
        resend_rs::Error::RateLimit { .. } => anyhow!("Email provider request failed"),
    }
}

/// Escape HTML special characters to prevent XSS in email content
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

#[derive(Debug, Clone)]
pub struct EmailConfig {
    pub resend_api_key: String,
    pub resend_from_email: String,
    pub resend_from_name: String,
    pub frontend_url: String,
    /// Recipient of contact form submissions.
    pub contact_form_email: Option<String>,
    /// Resend audience that marketing opt-ins are added to.
    pub audience_id: Option<String>,
}

impl EmailConfig {
    /// Resolves the Resend settings, or explains which one is missing.
    pub fn from_config(config: &AppConfig) -> Result<Self> {
        if config.is_restore_drill() {
            return Err(anyhow!("Email is disabled during restore drills"));
        }
        let email = config.email();
        let resend_api_key = email
            .resend_api_key
            .as_ref()
            .map(|key| key.expose().to_string())
            .ok_or_else(|| anyhow!("RESEND_API_KEY environment variable not set"))?;
        let resend_from_email = email
            .resend_from_email
            .clone()
            .ok_or_else(|| anyhow!("RESEND_FROM_EMAIL environment variable not set"))?;
        let resend_from_name = email
            .resend_from_name
            .clone()
            .unwrap_or_else(|| "Canary Wallet".to_string());
        let frontend_url = config
            .frontend_url()
            .map(str::to_string)
            .ok_or_else(|| anyhow!("FRONTEND_URL environment variable not set"))?;

        Ok(Self {
            resend_api_key,
            resend_from_email,
            resend_from_name,
            frontend_url,
            contact_form_email: email.contact_form_email.clone(),
            audience_id: email.resend_audience_id.clone(),
        })
    }
}

#[derive(Clone)]
pub struct EmailService {
    config: EmailConfig,
    resend: Resend,
}

impl EmailService {
    pub fn new(config: EmailConfig) -> Self {
        let resend = Resend::new(&config.resend_api_key);
        Self { config, resend }
    }

    /// Builds the service when email delivery is configured.
    pub fn from_config(config: &AppConfig) -> Result<Self> {
        Ok(Self::new(EmailConfig::from_config(config)?))
    }

    pub async fn send_email_verification(
        &self,
        to_email: &str,
        to_name: &str,
        verification_token: &str,
        language: &str,
    ) -> Result<()> {
        let verification_url = format!(
            "{}/verify-email/{}",
            self.config.frontend_url, verification_token
        );

        // Escape name for HTML to prevent XSS
        let safe_name = html_escape(to_name);

        // Get translations using rust-i18n
        let locale = language;
        let subject = t!("auth_email.verify_email.subject", locale = locale).to_string();
        let header = t!("auth_email.verify_email.header", locale = locale).to_string();
        let greeting_html =
            t!("common.greeting", locale = locale, to_name = &safe_name).to_string();
        let greeting_text = t!("common.greeting", locale = locale, to_name = to_name).to_string();
        let body_text = t!("auth_email.verify_email.body", locale = locale).to_string();
        let button_text = t!("auth_email.verify_email.button", locale = locale).to_string();
        let link_fallback =
            t!("auth_email.verify_email.link_fallback", locale = locale).to_string();
        let expiry_text = t!("auth_email.verify_email.expiry", locale = locale).to_string();
        let footer = t!("common.footer", locale = locale).to_string();

        let html_body = format!(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta charset="utf-8">
                <title>{subject}</title>
            </head>
            <body style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto; padding: 20px;">
                <div style="text-align: center; margin-bottom: 30px;">
                    <h1 style="color: #1f2937;">Canary Wallet</h1>
                </div>

                <div style="background-color: #f9fafb; padding: 20px; border-radius: 8px; margin-bottom: 20px;">
                    <h2 style="color: #1f2937; margin-top: 0;">{header}</h2>
                    <p style="color: #4b5563; line-height: 1.6;">
                        {greeting_html}
                    </p>
                    <p style="color: #4b5563; line-height: 1.6;">
                        {body_text}
                    </p>

                    <div style="text-align: center; margin: 30px 0;">
                        <a href="{verification_url}"
                           style="background-color: #3b82f6; color: white; padding: 12px 24px; text-decoration: none; border-radius: 6px; display: inline-block; font-weight: 500;">
                            {button_text}
                        </a>
                    </div>

                    <p style="color: #6b7280; font-size: 14px; line-height: 1.6;">
                        {link_fallback}
                        <br>
                        <a href="{verification_url}" style="color: #3b82f6; word-break: break-all;">{verification_url}</a>
                    </p>

                    <p style="color: #6b7280; font-size: 14px; line-height: 1.6;">
                        {expiry_text}
                    </p>
                </div>

                <div style="text-align: center; color: #6b7280; font-size: 12px; margin-top: 30px;">
                    <p>{footer}</p>
                </div>
            </body>
            </html>
            "#
        );

        let text_body = format!(
            r"
{header}

{greeting_text}

{body_text}

{verification_url}

{expiry_text}

{footer}
            "
        );

        self.send_email(to_email, to_name, &subject, &html_body, &text_body)
            .await
    }

    pub async fn send_password_reset(
        &self,
        to_email: &str,
        to_name: &str,
        reset_token: &str,
        language: &str,
    ) -> Result<()> {
        let reset_url = format!(
            "{}/reset-password/{}",
            self.config.frontend_url, reset_token
        );

        // Escape name for HTML to prevent XSS
        let safe_name = html_escape(to_name);

        // Get translations using rust-i18n
        let locale = language;
        let subject = t!("auth_email.password_reset.subject", locale = locale).to_string();
        let header = t!("auth_email.password_reset.header", locale = locale).to_string();
        let greeting_html =
            t!("common.greeting", locale = locale, to_name = &safe_name).to_string();
        let greeting_text = t!("common.greeting", locale = locale, to_name = to_name).to_string();
        let body_text = t!("auth_email.password_reset.body", locale = locale).to_string();
        let button_text = t!("auth_email.password_reset.button", locale = locale).to_string();
        let link_fallback =
            t!("auth_email.password_reset.link_fallback", locale = locale).to_string();
        let expiry_text = t!("auth_email.password_reset.expiry", locale = locale).to_string();
        let footer = t!("common.footer", locale = locale).to_string();

        let html_body = format!(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta charset="utf-8">
                <title>{subject}</title>
            </head>
            <body style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto; padding: 20px;">
                <div style="text-align: center; margin-bottom: 30px;">
                    <h1 style="color: #1f2937;">Canary Wallet</h1>
                </div>

                <div style="background-color: #f9fafb; padding: 20px; border-radius: 8px; margin-bottom: 20px;">
                    <h2 style="color: #1f2937; margin-top: 0;">{header}</h2>
                    <p style="color: #4b5563; line-height: 1.6;">
                        {greeting_html}
                    </p>
                    <p style="color: #4b5563; line-height: 1.6;">
                        {body_text}
                    </p>

                    <div style="text-align: center; margin: 30px 0;">
                        <a href="{reset_url}"
                           style="background-color: #dc2626; color: white; padding: 12px 24px; text-decoration: none; border-radius: 6px; display: inline-block; font-weight: 500;">
                            {button_text}
                        </a>
                    </div>

                    <p style="color: #6b7280; font-size: 14px; line-height: 1.6;">
                        {link_fallback}
                        <br>
                        <a href="{reset_url}" style="color: #3b82f6; word-break: break-all;">{reset_url}</a>
                    </p>

                    <p style="color: #6b7280; font-size: 14px; line-height: 1.6;">
                        {expiry_text}
                    </p>
                </div>

                <div style="text-align: center; color: #6b7280; font-size: 12px; margin-top: 30px;">
                    <p>{footer}</p>
                </div>
            </body>
            </html>
            "#
        );

        let text_body = format!(
            r"
{header}

{greeting_text}

{body_text}

{reset_url}

{expiry_text}

{footer}
            "
        );

        self.send_email(to_email, to_name, &subject, &html_body, &text_body)
            .await
    }

    /// Generate a 6-digit OTP code for email verification
    pub fn generate_otp_code() -> String {
        let mut rng = rand::rng();
        format!("{:06}", rng.random_range(100_000..1_000_000))
    }

    /// Send OTP verification code via email for contact verification
    pub async fn send_contact_otp_verification(
        &self,
        to_email: &str,
        to_name: &str,
        otp_code: &str,
        language: &str,
    ) -> Result<()> {
        // Escape name for HTML to prevent XSS
        let safe_name = html_escape(to_name);

        // Get translations using rust-i18n
        let locale = language;
        let subject = t!("auth_email.contact_otp.subject", locale = locale).to_string();
        let header = t!("auth_email.contact_otp.header", locale = locale).to_string();
        let greeting_html =
            t!("common.greeting", locale = locale, to_name = &safe_name).to_string();
        let greeting_text = t!("common.greeting", locale = locale, to_name = to_name).to_string();
        let body_text = t!("auth_email.contact_otp.body", locale = locale).to_string();
        let expiry_text = t!("auth_email.contact_otp.expiry", locale = locale).to_string();
        let footer = t!("common.footer", locale = locale).to_string();

        let html_body = format!(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta charset="utf-8">
                <title>{subject}</title>
            </head>
            <body style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto; padding: 20px;">
                <div style="text-align: center; margin-bottom: 30px;">
                    <h1 style="color: #1f2937;">Canary Wallet</h1>
                </div>

                <div style="background-color: #f9fafb; padding: 20px; border-radius: 8px; margin-bottom: 20px;">
                    <h2 style="color: #1f2937; margin-top: 0;">{header}</h2>
                    <p style="color: #4b5563; line-height: 1.6;">
                        {greeting_html}
                    </p>
                    <p style="color: #4b5563; line-height: 1.6;">
                        {body_text}
                    </p>

                    <div style="text-align: center; margin: 30px 0;">
                        <div style="background-color: #e5e7eb; border: 2px dashed #9ca3af; padding: 20px; border-radius: 8px; display: inline-block;">
                            <span style="font-size: 32px; font-weight: bold; font-family: 'Courier New', monospace; color: #1f2937; letter-spacing: 8px;">
                                {otp_code}
                            </span>
                        </div>
                    </div>

                    <p style="color: #6b7280; font-size: 14px; line-height: 1.6;">
                        {expiry_text}
                    </p>
                </div>

                <div style="text-align: center; color: #6b7280; font-size: 12px; margin-top: 30px;">
                    <p>{footer}</p>
                </div>
            </body>
            </html>
            "#
        );

        let text_body = format!(
            r"
Canary Wallet - {header}

{greeting_text}

{body_text}

Your verification code is: {otp_code}

{expiry_text}

{footer}
            "
        );

        self.send_email(to_email, to_name, &subject, &html_body, &text_body)
            .await
    }

    /// Send registration attempt notification to existing user
    pub async fn send_registration_attempt_notification(
        &self,
        to_email: &str,
        to_name: &str,
        language: &str,
    ) -> Result<()> {
        let reset_url = format!("{}/forgot-password", self.config.frontend_url);

        // Escape name for HTML to prevent XSS
        let safe_name = html_escape(to_name);

        // Get translations using rust-i18n
        let locale = language;
        let subject = t!("auth_email.registration_attempt.subject", locale = locale).to_string();
        let header = t!("auth_email.registration_attempt.header", locale = locale).to_string();
        let greeting_html =
            t!("common.greeting", locale = locale, to_name = &safe_name).to_string();
        let greeting_text = t!("common.greeting", locale = locale, to_name = to_name).to_string();
        let body_text = t!("auth_email.registration_attempt.body", locale = locale).to_string();
        let button_text = t!("auth_email.registration_attempt.button", locale = locale).to_string();
        let link_fallback = t!(
            "auth_email.registration_attempt.link_fallback",
            locale = locale
        )
        .to_string();
        let security_note = t!(
            "auth_email.registration_attempt.security_note",
            locale = locale
        )
        .to_string();
        let footer = t!("common.footer", locale = locale).to_string();

        let html_body = format!(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta charset="utf-8">
                <title>{subject}</title>
            </head>
            <body style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto; padding: 20px;">
                <div style="text-align: center; margin-bottom: 30px;">
                    <h1 style="color: #1f2937;">Canary Wallet</h1>
                </div>

                <div style="background-color: #f9fafb; padding: 20px; border-radius: 8px; margin-bottom: 20px;">
                    <h2 style="color: #1f2937; margin-top: 0;">{header}</h2>
                    <p style="color: #4b5563; line-height: 1.6;">
                        {greeting_html}
                    </p>
                    <p style="color: #4b5563; line-height: 1.6;">
                        {body_text}
                    </p>

                    <div style="background-color: #fff; border-left: 4px solid #f59e0b; padding: 15px; margin: 20px 0;">
                        <p style="color: #92400e; margin: 0;">
                            {security_note}
                        </p>
                    </div>

                    <div style="text-align: center; margin: 30px 0;">
                        <a href="{reset_url}"
                           style="background-color: #dc2626; color: white; padding: 12px 24px; text-decoration: none; border-radius: 6px; display: inline-block; font-weight: 500;">
                            {button_text}
                        </a>
                    </div>

                    <p style="color: #6b7280; font-size: 14px; line-height: 1.6;">
                        {link_fallback}
                        <br>
                        <a href="{reset_url}" style="color: #3b82f6; word-break: break-all;">{reset_url}</a>
                    </p>
                </div>

                <div style="text-align: center; color: #6b7280; font-size: 12px; margin-top: 30px;">
                    <p>{footer}</p>
                </div>
            </body>
            </html>
            "#
        );

        let text_body = format!(
            r"
{header}

{greeting_text}

{body_text}

{security_note}

{reset_url}

{footer}
            "
        );

        self.send_email(to_email, to_name, &subject, &html_body, &text_body)
            .await
    }

    pub async fn send_trial_ending_notification(
        &self,
        to_email: &str,
        to_name: &str,
        trial_ends_at: &str,
        language: &str,
        idempotency_key: &str,
    ) -> Result<()> {
        let billing_url = format!("{}/billing", self.config.frontend_url);

        // Escape name for HTML to prevent XSS
        let safe_name = html_escape(to_name);

        // Get translations using rust-i18n
        let locale = language;
        let subject = t!("auth_email.trial_ending.subject", locale = locale).to_string();
        let header = t!("auth_email.trial_ending.header", locale = locale).to_string();
        let greeting_html =
            t!("common.greeting", locale = locale, to_name = &safe_name).to_string();
        let greeting_text = t!("common.greeting", locale = locale, to_name = to_name).to_string();
        // Body has a placeholder for trial_ends_at - wrap in <strong> for HTML
        let body_text = t!(
            "auth_email.trial_ending.body",
            locale = locale,
            trial_ends_at = format!("<strong>{}</strong>", trial_ends_at)
        )
        .to_string();
        let body_text_plain = t!(
            "auth_email.trial_ending.body",
            locale = locale,
            trial_ends_at = trial_ends_at
        )
        .to_string();
        let continue_text =
            t!("auth_email.trial_ending.continue_text", locale = locale).to_string();
        let what_happens_header = t!(
            "auth_email.trial_ending.what_happens_header",
            locale = locale
        )
        .to_string();
        let sync_stops = t!("auth_email.trial_ending.sync_stops", locale = locale).to_string();
        let notifications_stop = t!(
            "auth_email.trial_ending.notifications_stop",
            locale = locale
        )
        .to_string();
        let data_safe = t!("auth_email.trial_ending.data_safe", locale = locale).to_string();
        let button_text = t!("auth_email.trial_ending.button", locale = locale).to_string();
        let choose_plan = t!("auth_email.trial_ending.choose_plan", locale = locale).to_string();
        let personal_plan =
            t!("auth_email.trial_ending.personal_plan", locale = locale).to_string();
        let team_plan = t!("auth_email.trial_ending.team_plan", locale = locale).to_string();
        let footer = t!("common.footer", locale = locale).to_string();

        let html_body = format!(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta charset="utf-8">
                <title>{subject}</title>
            </head>
            <body style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto; padding: 20px;">
                <div style="text-align: center; margin-bottom: 30px;">
                    <h1 style="color: #1f2937;">Canary Wallet</h1>
                </div>

                <div style="background-color: #f9fafb; padding: 20px; border-radius: 8px; margin-bottom: 20px;">
                    <h2 style="color: #1f2937; margin-top: 0;">{header}</h2>
                    <p style="color: #4b5563; line-height: 1.6;">
                        {greeting_html}
                    </p>
                    <p style="color: #4b5563; line-height: 1.6;">
                        {body_text}
                    </p>
                    <p style="color: #4b5563; line-height: 1.6;">
                        {continue_text}
                    </p>

                    <div style="background-color: #fff; border-left: 4px solid #3b82f6; padding: 15px; margin: 20px 0;">
                        <p style="color: #1f2937; margin: 0; font-weight: 500;">{what_happens_header}</p>
                        <ul style="color: #4b5563; margin: 10px 0; padding-left: 20px;">
                            <li>{sync_stops}</li>
                            <li>{notifications_stop}</li>
                            <li>{data_safe}</li>
                        </ul>
                    </div>

                    <div style="text-align: center; margin: 30px 0;">
                        <a href="{billing_url}"
                           style="background-color: #3b82f6; color: white; padding: 12px 24px; text-decoration: none; border-radius: 6px; display: inline-block; font-weight: 500;">
                            {button_text}
                        </a>
                    </div>

                    <p style="color: #6b7280; font-size: 14px; line-height: 1.6;">
                        {choose_plan}
                    </p>
                    <ul style="color: #6b7280; font-size: 14px; line-height: 1.6;">
                        <li><strong>{personal_plan}</strong></li>
                        <li><strong>{team_plan}</strong></li>
                    </ul>
                </div>

                <div style="text-align: center; color: #6b7280; font-size: 12px; margin-top: 30px;">
                    <p>{footer}</p>
                </div>
            </body>
            </html>
            "#
        );

        let text_body = format!(
            r"
{header}

{greeting_text}

{body_text_plain}

{continue_text}

{what_happens_header}
- {sync_stops}
- {notifications_stop}
- {data_safe}

{choose_plan}
- {personal_plan}
- {team_plan}

{billing_url}

{footer}
            "
        );

        let from = format!(
            "{} <{}>",
            self.config.resend_from_name, self.config.resend_from_email
        );
        let to = vec![format!("{} <{}>", to_name, to_email)];
        let email = CreateEmailBaseOptions::new(from, to, subject)
            .with_html(&html_body)
            .with_text(&text_body)
            .with_idempotency_key(idempotency_key);

        self.resend
            .emails
            .send(email)
            .await
            .map(|_| ())
            .map_err(safe_provider_error)
    }

    /// Send contact form submission to admin
    pub async fn send_contact_form_submission(
        &self,
        from_email: &str,
        message: &str,
    ) -> Result<()> {
        // In cloud mode, this must be explicitly configured (no fallback)
        let to_email = self.config.contact_form_email.clone().ok_or_else(|| {
            anyhow!("CONTACT_FORM_EMAIL environment variable not set - required for contact form submissions")
        })?;

        let subject = format!("Contact Form Submission from {from_email}");

        // Escape user-supplied content for HTML to prevent XSS
        let safe_from_email = html_escape(from_email);
        let safe_message = html_escape(message);

        let html_body = format!(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta charset="utf-8">
                <title>Contact Form Submission</title>
            </head>
            <body style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto; padding: 20px;">
                <div style="text-align: center; margin-bottom: 30px;">
                    <h1 style="color: #1f2937;">Canary Wallet Contact Form</h1>
                </div>

                <div style="background-color: #f9fafb; padding: 20px; border-radius: 8px; margin-bottom: 20px;">
                    <h2 style="color: #1f2937; margin-top: 0;">New Message Received</h2>

                    <p style="color: #4b5563; line-height: 1.6;">
                        <strong>From:</strong> {safe_from_email}
                    </p>

                    <div style="background-color: #fff; border-left: 4px solid #3b82f6; padding: 15px; margin: 20px 0;">
                        <p style="color: #1f2937; margin: 0; white-space: pre-wrap;">{safe_message}</p>
                    </div>
                </div>

                <div style="text-align: center; color: #6b7280; font-size: 12px; margin-top: 30px;">
                    <p>This message was sent via the Canary Wallet contact form</p>
                </div>
            </body>
            </html>
            "#
        );

        let text_body = format!(
            r"
Canary Wallet Contact Form - New Message

From: {from_email}

Message:
{message}

---
This message was sent via the Canary Wallet contact form
            "
        );

        self.send_email_with_reply_to(
            &to_email,
            "Canary Wallet Admin",
            &subject,
            &html_body,
            &text_body,
            Some(from_email),
        )
        .await
    }

    pub async fn add_to_marketing_audience(&self, email: &str, name: &str) -> Result<()> {
        let audience_id = self
            .config
            .audience_id
            .clone()
            .ok_or_else(|| anyhow!("RESEND_AUDIENCE_ID environment variable not set"))?;

        let (first_name, last_name) = if name.contains(' ') {
            let parts: Vec<&str> = name.splitn(2, ' ').collect();
            (parts[0], *parts.get(1).unwrap_or(&""))
        } else {
            (name, "")
        };

        let contact = CreateContactOptions::new(email)
            .with_audience_id(&audience_id)
            .with_first_name(first_name)
            .with_last_name(last_name)
            .with_unsubscribed(false);

        if self.resend.contacts.create(contact).await.is_ok() {
            tracing::info!("Marketing audience enrollment completed");
            Ok(())
        } else {
            tracing::warn!("Marketing audience enrollment failed");
            Ok(())
        }
    }

    async fn send_email(
        &self,
        to_email: &str,
        to_name: &str,
        subject: &str,
        html_body: &str,
        text_body: &str,
    ) -> Result<()> {
        self.send_email_with_reply_to(to_email, to_name, subject, html_body, text_body, None)
            .await
    }

    async fn send_email_with_reply_to(
        &self,
        to_email: &str,
        to_name: &str,
        subject: &str,
        html_body: &str,
        text_body: &str,
        reply_to: Option<&str>,
    ) -> Result<()> {
        let from = format!(
            "{} <{}>",
            self.config.resend_from_name, self.config.resend_from_email
        );
        let to = vec![format!("{} <{}>", to_name, to_email)];

        let email = create_email_options(from, to, subject, html_body, text_body, reply_to);

        // Send email
        match self.resend.emails.send(email).await {
            Ok(_) => Ok(()),
            Err(e) => Err(safe_provider_error(e)),
        }
    }

    /// Queue multiple emails for background sending via the global email queue
    /// Returns Vec of Results - all success since emails are queued, not sent immediately
    /// Actual sending happens asynchronously with rate limiting and retries
    pub fn send_batch_emails(emails: Vec<BatchEmailRequest>) -> Vec<Result<String>> {
        let count = emails.len();
        if count == 0 {
            return Vec::new();
        }

        // Queue all emails for background processing
        match email_queue::queue_emails(emails) {
            // Report success for every email: they are queued and sent in the background
            Ok(()) => (0..count).map(|idx| Ok(format!("queued-{idx}"))).collect(),
            Err(e) => {
                eprintln!("❌ Failed to queue emails: {e}");
                (0..count)
                    .map(|_| Err(anyhow!("Failed to queue email: {e}")))
                    .collect()
            }
        }
    }
}

fn create_email_options(
    from: String,
    to: Vec<String>,
    subject: &str,
    html_body: &str,
    text_body: &str,
    reply_to: Option<&str>,
) -> CreateEmailBaseOptions {
    let email = CreateEmailBaseOptions::new(from, to, subject)
        .with_html(html_body)
        .with_text(text_body);
    match reply_to {
        Some(address) => email.with_reply(address),
        None => email,
    }
}

/// Request for a single email in a batch
#[derive(Debug, Clone)]
pub struct BatchEmailRequest {
    pub to_email: String,
    pub to_name: String,
    pub subject: String,
    pub html_body: String,
    pub text_body: String,
}

#[cfg(test)]
mod privacy_tests {
    use super::*;
    use crate::config::{EmailSettings, NetworkConfig, OperatingMode, SecretString};

    #[test]
    fn contact_email_replies_to_submitter_without_changing_sender() {
        let email = create_email_options(
            "Canary <no-reply@canarybitcoin.com>".to_owned(),
            vec!["Admin <admin@example.com>".to_owned()],
            "Contact form",
            "<p>Hello</p>",
            "Hello",
            Some("visitor@example.com"),
        );
        let payload = serde_json::to_value(email).unwrap();
        assert_eq!(payload["from"], "Canary <no-reply@canarybitcoin.com>");
        assert_eq!(
            payload["reply_to"],
            serde_json::json!(["visitor@example.com"])
        );

        let transactional = create_email_options(
            "Canary <no-reply@canarybitcoin.com>".to_owned(),
            vec!["User <user@example.com>".to_owned()],
            "Notification",
            "<p>Hello</p>",
            "Hello",
            None,
        );
        assert!(
            serde_json::to_value(transactional)
                .unwrap()
                .get("reply_to")
                .is_none()
        );
    }

    #[test]
    fn provider_errors_discard_untrusted_fields_and_error_chains() {
        let sensitive = "private@example.invalid https://example.invalid/reset?token=secret";
        let errors = [
            resend_rs::Error::Parse(sensitive.to_string()),
            resend_rs::Error::Resend(resend_rs::types::ErrorResponse {
                status_code: 422,
                name: sensitive.to_string(),
                message: sensitive.to_string(),
            }),
        ];
        for error in errors {
            let safe = safe_provider_error(error);
            assert_eq!(safe.chain().count(), 1);
            let rendered = format!("{safe:#} {safe:?}");
            assert!(!rendered.contains("private@example.invalid"));
            assert!(!rendered.contains("token=secret"));
        }
    }

    fn email_config(restore_drill: bool, email: EmailSettings) -> AppConfig {
        AppConfig::new_for_test(
            NetworkConfig::Regtest,
            None,
            "127.0.0.1:3000".to_string(),
            "./database".to_string(),
            OperatingMode::Cloud,
            Some("http://127.0.0.1".to_string()),
            None,
        )
        .with_restore_drill(restore_drill)
        .with_email(email)
    }

    fn configured_email() -> EmailSettings {
        EmailSettings {
            resend_api_key: Some(SecretString::new("re_present")),
            resend_from_email: Some("alerts@example.invalid".to_string()),
            resend_from_name: Some("Canary".to_string()),
            ..EmailSettings::default()
        }
    }

    #[test]
    fn from_config_is_disabled_during_restore_drill() {
        let error = EmailConfig::from_config(&email_config(true, configured_email())).unwrap_err();
        assert!(error.to_string().contains("restore drills"));
    }

    #[test]
    fn from_config_reports_missing_values_and_defaults_sender_name() {
        let error =
            EmailConfig::from_config(&email_config(false, EmailSettings::default())).unwrap_err();
        assert_eq!(
            error.to_string(),
            "RESEND_API_KEY environment variable not set"
        );

        let email = EmailSettings {
            resend_from_name: None,
            ..configured_email()
        };
        let config = EmailConfig::from_config(&email_config(false, email)).unwrap();
        assert_eq!(config.resend_from_name, "Canary Wallet");
        assert_eq!(config.frontend_url, "http://127.0.0.1");
        assert_eq!(config.contact_form_email, None);
    }
}
