use crate::email_queue;
use anyhow::{anyhow, Result};
use rand::Rng;
use resend_rs::types::{ContactData, CreateEmailBaseOptions};
use resend_rs::Resend;
use rust_i18n::t;

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
}

impl EmailConfig {
    pub fn from_env() -> Result<Self> {
        let resend_api_key = std::env::var("RESEND_API_KEY")
            .map_err(|_| anyhow!("RESEND_API_KEY environment variable not set"))?;
        let resend_from_email = std::env::var("RESEND_FROM_EMAIL")
            .map_err(|_| anyhow!("RESEND_FROM_EMAIL environment variable not set"))?;
        let resend_from_name = std::env::var("RESEND_FROM_NAME")
            .unwrap_or_else(|_| "Canary Bitcoin Wallet".to_string());
        let frontend_url = std::env::var("FRONTEND_URL")
            .map_err(|_| anyhow!("FRONTEND_URL environment variable not set"))?;

        Ok(Self {
            resend_api_key,
            resend_from_email,
            resend_from_name,
            frontend_url,
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

    pub fn from_env() -> Result<Self> {
        let config = EmailConfig::from_env()?;
        Ok(Self::new(config))
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

        // Get translations using rust-i18n
        let locale = language;
        let subject = t!("auth_email.verify_email.subject", locale = locale).to_string();
        let header = t!("auth_email.verify_email.header", locale = locale).to_string();
        let greeting = t!("common.greeting", locale = locale, to_name = to_name).to_string();
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
                    <h1 style="color: #1f2937;">Canary</h1>
                </div>

                <div style="background-color: #f9fafb; padding: 20px; border-radius: 8px; margin-bottom: 20px;">
                    <h2 style="color: #1f2937; margin-top: 0;">{header}</h2>
                    <p style="color: #4b5563; line-height: 1.6;">
                        {greeting}
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
            "#,
            subject = subject,
            header = header,
            greeting = greeting,
            body_text = body_text,
            verification_url = verification_url,
            button_text = button_text,
            link_fallback = link_fallback,
            expiry_text = expiry_text,
            footer = footer
        );

        let text_body = format!(
            r#"
{header}

{greeting}

{body_text}

{verification_url}

{expiry_text}

{footer}
            "#,
            header = header,
            greeting = greeting,
            body_text = body_text,
            verification_url = verification_url,
            expiry_text = expiry_text,
            footer = footer
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

        // Get translations using rust-i18n
        let locale = language;
        let subject = t!("auth_email.password_reset.subject", locale = locale).to_string();
        let header = t!("auth_email.password_reset.header", locale = locale).to_string();
        let greeting = t!("common.greeting", locale = locale, to_name = to_name).to_string();
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
                    <h1 style="color: #1f2937;">Canary</h1>
                </div>

                <div style="background-color: #f9fafb; padding: 20px; border-radius: 8px; margin-bottom: 20px;">
                    <h2 style="color: #1f2937; margin-top: 0;">{header}</h2>
                    <p style="color: #4b5563; line-height: 1.6;">
                        {greeting}
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
            "#,
            subject = subject,
            header = header,
            greeting = greeting,
            body_text = body_text,
            reset_url = reset_url,
            button_text = button_text,
            link_fallback = link_fallback,
            expiry_text = expiry_text,
            footer = footer
        );

        let text_body = format!(
            r#"
{header}

{greeting}

{body_text}

{reset_url}

{expiry_text}

{footer}
            "#,
            header = header,
            greeting = greeting,
            body_text = body_text,
            reset_url = reset_url,
            expiry_text = expiry_text,
            footer = footer
        );

        self.send_email(to_email, to_name, &subject, &html_body, &text_body)
            .await
    }

    /// Generate a 6-digit OTP code for email verification
    pub fn generate_otp_code() -> String {
        let mut rng = rand::thread_rng();
        format!("{:06}", rng.gen_range(100000..1000000))
    }

    /// Send OTP verification code via email for contact verification
    pub async fn send_contact_otp_verification(
        &self,
        to_email: &str,
        to_name: &str,
        otp_code: &str,
        language: &str,
    ) -> Result<()> {
        // Get translations using rust-i18n
        let locale = language;
        let subject = t!("auth_email.contact_otp.subject", locale = locale).to_string();
        let header = t!("auth_email.contact_otp.header", locale = locale).to_string();
        let greeting = t!("common.greeting", locale = locale, to_name = to_name).to_string();
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
                    <h1 style="color: #1f2937;">Canary</h1>
                </div>

                <div style="background-color: #f9fafb; padding: 20px; border-radius: 8px; margin-bottom: 20px;">
                    <h2 style="color: #1f2937; margin-top: 0;">{header}</h2>
                    <p style="color: #4b5563; line-height: 1.6;">
                        {greeting}
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
            "#,
            subject = subject,
            header = header,
            greeting = greeting,
            body_text = body_text,
            otp_code = otp_code,
            expiry_text = expiry_text,
            footer = footer
        );

        let text_body = format!(
            r#"
Canary - {header}

{greeting}

{body_text}

Your verification code is: {otp_code}

{expiry_text}

{footer}
            "#,
            header = header,
            greeting = greeting,
            body_text = body_text,
            otp_code = otp_code,
            expiry_text = expiry_text,
            footer = footer
        );

        self.send_email(to_email, to_name, &subject, &html_body, &text_body)
            .await
    }

    /// Send account locked notification email
    pub async fn send_account_locked(
        &self,
        to_email: &str,
        to_name: &str,
        lockout_minutes: i64,
        language: &str,
    ) -> Result<()> {
        let reset_url = format!("{}/forgot-password", self.config.frontend_url);

        // Escape name for HTML to prevent XSS
        let safe_name = html_escape(to_name);

        // Get translations using rust-i18n
        let locale = language;
        let subject = t!("auth_email.account_locked.subject", locale = locale).to_string();
        let header = t!("auth_email.account_locked.header", locale = locale).to_string();
        let greeting = t!("common.greeting", locale = locale, to_name = &safe_name).to_string();
        let body_text = t!("auth_email.account_locked.body", locale = locale).to_string();
        let unlock_text = t!(
            "auth_email.account_locked.unlock_text",
            locale = locale,
            minutes = lockout_minutes
        )
        .to_string();
        let security_warning = t!(
            "auth_email.account_locked.security_warning",
            locale = locale
        )
        .to_string();
        let button_text = t!("auth_email.account_locked.button", locale = locale).to_string();
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
                    <h1 style="color: #1f2937;">Canary</h1>
                </div>

                <div style="background-color: #fef2f2; padding: 20px; border-radius: 8px; margin-bottom: 20px; border-left: 4px solid #dc2626;">
                    <h2 style="color: #991b1b; margin-top: 0;">{header}</h2>
                    <p style="color: #4b5563; line-height: 1.6;">
                        {greeting}
                    </p>
                    <p style="color: #4b5563; line-height: 1.6;">
                        {body_text}
                    </p>
                    <p style="color: #4b5563; line-height: 1.6;">
                        {unlock_text}
                    </p>

                    <div style="background-color: #fff; border-left: 4px solid #f59e0b; padding: 15px; margin: 20px 0;">
                        <p style="color: #92400e; margin: 0;">
                            ⚠️ {security_warning}
                        </p>
                    </div>

                    <div style="text-align: center; margin: 30px 0;">
                        <a href="{reset_url}"
                           style="background-color: #dc2626; color: white; padding: 12px 24px; text-decoration: none; border-radius: 6px; display: inline-block; font-weight: 500;">
                            {button_text}
                        </a>
                    </div>
                </div>

                <div style="text-align: center; color: #6b7280; font-size: 12px; margin-top: 30px;">
                    <p>{footer}</p>
                </div>
            </body>
            </html>
            "#,
            subject = subject,
            header = header,
            greeting = greeting,
            body_text = body_text,
            unlock_text = unlock_text,
            security_warning = security_warning,
            reset_url = reset_url,
            button_text = button_text,
            footer = footer
        );

        let text_body = format!(
            r#"
{header}

{greeting}

{body_text}

{unlock_text}

⚠️ {security_warning}

{reset_url}

{footer}
            "#,
            header = header,
            greeting = greeting,
            body_text = body_text,
            unlock_text = unlock_text,
            security_warning = security_warning,
            reset_url = reset_url,
            footer = footer
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
    ) -> Result<()> {
        let billing_url = format!("{}/billing", self.config.frontend_url);

        // Get translations using rust-i18n
        let locale = language;
        let subject = t!("auth_email.trial_ending.subject", locale = locale).to_string();
        let header = t!("auth_email.trial_ending.header", locale = locale).to_string();
        let greeting = t!("common.greeting", locale = locale, to_name = to_name).to_string();
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
                    <h1 style="color: #1f2937;">Canary</h1>
                </div>

                <div style="background-color: #f9fafb; padding: 20px; border-radius: 8px; margin-bottom: 20px;">
                    <h2 style="color: #1f2937; margin-top: 0;">{header}</h2>
                    <p style="color: #4b5563; line-height: 1.6;">
                        {greeting}
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
            "#,
            subject = subject,
            header = header,
            greeting = greeting,
            body_text = body_text,
            continue_text = continue_text,
            what_happens_header = what_happens_header,
            sync_stops = sync_stops,
            notifications_stop = notifications_stop,
            data_safe = data_safe,
            billing_url = billing_url,
            button_text = button_text,
            choose_plan = choose_plan,
            personal_plan = personal_plan,
            team_plan = team_plan,
            footer = footer
        );

        let text_body = format!(
            r#"
{header}

{greeting}

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
            "#,
            header = header,
            greeting = greeting,
            body_text_plain = body_text_plain,
            continue_text = continue_text,
            what_happens_header = what_happens_header,
            sync_stops = sync_stops,
            notifications_stop = notifications_stop,
            data_safe = data_safe,
            choose_plan = choose_plan,
            personal_plan = personal_plan,
            team_plan = team_plan,
            billing_url = billing_url,
            footer = footer
        );

        self.send_email(to_email, to_name, &subject, &html_body, &text_body)
            .await
    }

    /// Send contact form submission to admin
    pub async fn send_contact_form_submission(
        &self,
        from_email: &str,
        message: &str,
    ) -> Result<()> {
        // Get the contact form recipient email from env
        // In cloud mode, this must be explicitly configured (no fallback)
        let to_email = std::env::var("CONTACT_FORM_EMAIL")
            .map_err(|_| anyhow!("CONTACT_FORM_EMAIL environment variable not set - required for contact form submissions"))?;

        let subject = format!("Contact Form Submission from {}", from_email);

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
                    <h1 style="color: #1f2937;">Canary Contact Form</h1>
                </div>

                <div style="background-color: #f9fafb; padding: 20px; border-radius: 8px; margin-bottom: 20px;">
                    <h2 style="color: #1f2937; margin-top: 0;">New Message Received</h2>

                    <p style="color: #4b5563; line-height: 1.6;">
                        <strong>From:</strong> {from_email}
                    </p>

                    <div style="background-color: #fff; border-left: 4px solid #3b82f6; padding: 15px; margin: 20px 0;">
                        <p style="color: #1f2937; margin: 0; white-space: pre-wrap;">{message}</p>
                    </div>
                </div>

                <div style="text-align: center; color: #6b7280; font-size: 12px; margin-top: 30px;">
                    <p>This message was sent via the Canary contact form</p>
                </div>
            </body>
            </html>
            "#,
            from_email = from_email,
            message = message
        );

        let text_body = format!(
            r#"
Canary Contact Form - New Message

From: {from_email}

Message:
{message}

---
This message was sent via the Canary contact form
            "#,
            from_email = from_email,
            message = message
        );

        self.send_email(&to_email, "Canary Admin", &subject, &html_body, &text_body)
            .await
    }

    pub async fn add_to_marketing_audience(&self, email: &str, name: &str) -> Result<()> {
        let audience_id = std::env::var("RESEND_AUDIENCE_ID")
            .map_err(|_| anyhow!("RESEND_AUDIENCE_ID environment variable not set"))?;

        let (first_name, last_name) = if name.contains(' ') {
            let parts: Vec<&str> = name.splitn(2, ' ').collect();
            (parts[0], *parts.get(1).unwrap_or(&""))
        } else {
            (name, "")
        };

        let contact = ContactData::new(email)
            .with_first_name(first_name)
            .with_last_name(last_name)
            .with_unsubscribed(false);

        match self.resend.contacts.create(&audience_id, contact).await {
            Ok(_) => {
                println!("Added {} to marketing audience", email);
                Ok(())
            }
            Err(e) => {
                println!(
                    "Warning: Failed to add {} to marketing audience: {}",
                    email, e
                );
                Ok(())
            }
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
        let from = format!(
            "{} <{}>",
            self.config.resend_from_name, self.config.resend_from_email
        );
        let to = vec![format!("{} <{}>", to_name, to_email)];

        // Create email with Resend SDK
        let email = CreateEmailBaseOptions::new(from, to, subject)
            .with_html(html_body)
            .with_text(text_body);

        // Send email
        match self.resend.emails.send(email).await {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow!("Resend API error: {}", e)),
        }
    }

    /// Queue multiple emails for background sending via the global email queue
    /// Returns Vec of Results - all success since emails are queued, not sent immediately
    /// Actual sending happens asynchronously with rate limiting and retries
    pub async fn send_batch_emails(&self, emails: Vec<BatchEmailRequest>) -> Vec<Result<String>> {
        if emails.is_empty() {
            return Vec::new();
        }

        // Queue all emails for background processing
        match email_queue::queue_emails(emails.clone()) {
            Ok(_) => {
                // Return success for all emails - they're queued and will be sent in background
                emails
                    .iter()
                    .enumerate()
                    .map(|(idx, _)| Ok(format!("queued-{}", idx)))
                    .collect()
            }
            Err(e) => {
                // Failed to queue - return error for all
                eprintln!("❌ Failed to queue emails: {}", e);
                emails
                    .iter()
                    .map(|_| Err(anyhow!("Failed to queue email: {}", e)))
                    .collect()
            }
        }
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
