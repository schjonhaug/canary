use anyhow::{Result, anyhow};
use resend_rs::Resend;
use resend_rs::types::{CreateEmailBaseOptions, ContactData};

#[derive(Debug, Clone)]
pub struct EmailConfig {
    pub resend_api_key: String,
    pub resend_from_email: String,
    pub resend_from_name: String,
}

impl EmailConfig {
    pub fn from_env() -> Result<Self> {
        let resend_api_key = std::env::var("RESEND_API_KEY")
            .map_err(|_| anyhow!("RESEND_API_KEY environment variable not set"))?;
        let resend_from_email = std::env::var("RESEND_FROM_EMAIL")
            .map_err(|_| anyhow!("RESEND_FROM_EMAIL environment variable not set"))?;
        let resend_from_name = std::env::var("RESEND_FROM_NAME")
            .unwrap_or_else(|_| "Canary Bitcoin Wallet".to_string());

        Ok(Self {
            resend_api_key,
            resend_from_email,
            resend_from_name,
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
        Self { 
            config,
            resend,
        }
    }

    pub fn from_env() -> Result<Self> {
        let config = EmailConfig::from_env()?;
        Ok(Self::new(config))
    }

    pub async fn send_email_verification(&self, to_email: &str, to_name: &str, verification_token: &str) -> Result<()> {
        let verification_url = format!("{}/verify-email/{}", 
            std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:3001".to_string()),
            verification_token
        );

        let subject = "Verify Your Email - Canary Wallet";
        let html_body = format!(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta charset="utf-8">
                <title>{}</title>
            </head>
            <body style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto; padding: 20px;">
                <div style="text-align: center; margin-bottom: 30px;">
                    <h1 style="color: #1f2937;">Canary Wallet</h1>
                </div>
                
                <div style="background-color: #f9fafb; padding: 20px; border-radius: 8px; margin-bottom: 20px;">
                    <h2 style="color: #1f2937; margin-top: 0;">Welcome to Canary Wallet!</h2>
                    <p style="color: #4b5563; line-height: 1.6;">
                        Hi {name},
                    </p>
                    <p style="color: #4b5563; line-height: 1.6;">
                        Thank you for creating your Canary Wallet account! To get started, please verify your email address by clicking the button below.
                    </p>
                    
                    <div style="text-align: center; margin: 30px 0;">
                        <a href="{verification_url}" 
                           style="background-color: #3b82f6; color: white; padding: 12px 24px; text-decoration: none; border-radius: 6px; display: inline-block; font-weight: 500;">
                            Verify Email Address
                        </a>
                    </div>
                    
                    <p style="color: #6b7280; font-size: 14px; line-height: 1.6;">
                        If the button doesn't work, you can copy and paste this link into your browser:
                        <br>
                        <a href="{verification_url}" style="color: #3b82f6; word-break: break-all;">{verification_url}</a>
                    </p>
                    
                    <p style="color: #6b7280; font-size: 14px; line-height: 1.6;">
                        This verification link will expire in 24 hours. If you didn't create an account, you can safely ignore this email.
                    </p>
                </div>
                
                <div style="text-align: center; color: #6b7280; font-size: 12px; margin-top: 30px;">
                    <p>© 2024 Canary Wallet. All rights reserved.</p>
                </div>
            </body>
            </html>
            "#,
            subject, name = to_name, verification_url = verification_url
        );

        let text_body = format!(
            r#"
Welcome to Canary Wallet

Hi {name},

Thank you for creating your Canary Wallet account! To get started, please verify your email address by visiting the link below:

{verification_url}

This verification link will expire in 24 hours. If you didn't create an account, you can safely ignore this email.

© 2024 Canary Wallet. All rights reserved.
            "#,
            name = to_name, verification_url = verification_url
        );

        self.send_email(to_email, to_name, subject, &html_body, &text_body).await
    }

    pub async fn send_password_reset(&self, to_email: &str, to_name: &str, reset_token: &str) -> Result<()> {
        let reset_url = format!("{}/reset-password/{}", 
            std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:3001".to_string()),
            reset_token
        );

        let subject = "Reset Your Password - Canary Wallet";
        let html_body = format!(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta charset="utf-8">
                <title>{}</title>
            </head>
            <body style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto; padding: 20px;">
                <div style="text-align: center; margin-bottom: 30px;">
                    <h1 style="color: #1f2937;">Canary Wallet</h1>
                </div>
                
                <div style="background-color: #f9fafb; padding: 20px; border-radius: 8px; margin-bottom: 20px;">
                    <h2 style="color: #1f2937; margin-top: 0;">Reset Your Password</h2>
                    <p style="color: #4b5563; line-height: 1.6;">
                        Hi {name},
                    </p>
                    <p style="color: #4b5563; line-height: 1.6;">
                        We received a request to reset your password for your Canary Wallet account. Click the button below to create a new password.
                    </p>
                    
                    <div style="text-align: center; margin: 30px 0;">
                        <a href="{reset_url}" 
                           style="background-color: #dc2626; color: white; padding: 12px 24px; text-decoration: none; border-radius: 6px; display: inline-block; font-weight: 500;">
                            Reset Password
                        </a>
                    </div>
                    
                    <p style="color: #6b7280; font-size: 14px; line-height: 1.6;">
                        If the button doesn't work, you can copy and paste this link into your browser:
                        <br>
                        <a href="{reset_url}" style="color: #3b82f6; word-break: break-all;">{reset_url}</a>
                    </p>
                    
                    <p style="color: #6b7280; font-size: 14px; line-height: 1.6;">
                        This reset link will expire in 1 hour. If you didn't request a password reset, you can safely ignore this email.
                    </p>
                </div>
                
                <div style="text-align: center; color: #6b7280; font-size: 12px; margin-top: 30px;">
                    <p>© 2024 Canary Wallet. All rights reserved.</p>
                </div>
            </body>
            </html>
            "#,
            subject, name = to_name, reset_url = reset_url
        );

        let text_body = format!(
            r#"
Reset Your Password - Canary Wallet

Hi {name},

We received a request to reset your password for your Canary Wallet account. Visit the link below to create a new password:

{reset_url}

This reset link will expire in 1 hour. If you didn't request a password reset, you can safely ignore this email.

© 2024 Canary Wallet. All rights reserved.
            "#,
            name = to_name, reset_url = reset_url
        );

        self.send_email(to_email, to_name, subject, &html_body, &text_body).await
    }
    
    pub async fn send_transaction_notification(
        &self,
        to_email: &str,
        to_name: &str,
        subject: &str,
        html_body: &str,
        text_body: &str,
    ) -> Result<()> {
        self.send_email(to_email, to_name, subject, html_body, text_body).await
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
            },
            Err(e) => {
                println!("Warning: Failed to add {} to marketing audience: {}", email, e);
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
        let from = format!("{} <{}>", self.config.resend_from_name, self.config.resend_from_email);
        let to = vec![format!("{} <{}>", to_name, to_email)];
        
        // Create email with Resend SDK
        let email = CreateEmailBaseOptions::new(from, to, subject)
            .with_html(html_body)
            .with_text(text_body);

        // Send email
        match self.resend.emails.send(email).await {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow!("Resend API error: {}", e))
        }
    }
}