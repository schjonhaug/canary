use crate::email_queue;
use anyhow::{anyhow, Result};
use rand::Rng;
use resend_rs::types::{ContactData, CreateEmailBaseOptions};
use resend_rs::Resend;

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
    ) -> Result<()> {
        let verification_url = format!(
            "{}/verify-email/{}",
            std::env::var("FRONTEND_URL").expect("FRONTEND_URL must be set"),
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
            subject,
            name = to_name,
            verification_url = verification_url
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
            name = to_name,
            verification_url = verification_url
        );

        self.send_email(to_email, to_name, subject, &html_body, &text_body)
            .await
    }

    pub async fn send_password_reset(
        &self,
        to_email: &str,
        to_name: &str,
        reset_token: &str,
    ) -> Result<()> {
        let reset_url = format!(
            "{}/reset-password/{}",
            std::env::var("FRONTEND_URL").expect("FRONTEND_URL must be set"),
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
            subject,
            name = to_name,
            reset_url = reset_url
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
            name = to_name,
            reset_url = reset_url
        );

        self.send_email(to_email, to_name, subject, &html_body, &text_body)
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
    ) -> Result<()> {
        let subject = "Verify Your Email - Canary Wallet Contact";
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
                    <h2 style="color: #1f2937; margin-top: 0;">Email Verification Required</h2>
                    <p style="color: #4b5563; line-height: 1.6;">
                        Hi {name},
                    </p>
                    <p style="color: #4b5563; line-height: 1.6;">
                        Please verify your email address to receive Bitcoin transaction notifications. Enter the verification code below:
                    </p>
                    
                    <div style="text-align: center; margin: 30px 0;">
                        <div style="background-color: #e5e7eb; border: 2px dashed #9ca3af; padding: 20px; border-radius: 8px; display: inline-block;">
                            <span style="font-size: 32px; font-weight: bold; font-family: 'Courier New', monospace; color: #1f2937; letter-spacing: 8px;">
                                {otp_code}
                            </span>
                        </div>
                    </div>
                    
                    <p style="color: #6b7280; font-size: 14px; line-height: 1.6;">
                        This verification code will expire in 10 minutes. If you didn't request this verification, you can safely ignore this email.
                    </p>
                </div>
                
                <div style="text-align: center; color: #6b7280; font-size: 12px; margin-top: 30px;">
                    <p>© 2024 Canary Wallet. All rights reserved.</p>
                </div>
            </body>
            </html>
            "#,
            subject,
            name = to_name,
            otp_code = otp_code
        );

        let text_body = format!(
            r#"
Canary Wallet - Email Verification

Hi {name},

Please verify your email address to receive Bitcoin transaction notifications. 

Your verification code is: {otp_code}

This verification code will expire in 10 minutes. If you didn't request this verification, you can safely ignore this email.

© 2024 Canary Wallet. All rights reserved.
            "#,
            name = to_name,
            otp_code = otp_code
        );

        self.send_email(to_email, to_name, subject, &html_body, &text_body)
            .await
    }

    pub async fn send_trial_ending_notification(
        &self,
        to_email: &str,
        to_name: &str,
        trial_ends_at: &str,
    ) -> Result<()> {
        let billing_url = format!(
            "{}/billing",
            std::env::var("FRONTEND_URL").expect("FRONTEND_URL must be set")
        );

        let subject = "Your Canary trial ends in 3 days";
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
                    <h2 style="color: #1f2937; margin-top: 0;">Your Trial is Ending Soon</h2>
                    <p style="color: #4b5563; line-height: 1.6;">
                        Hi {name},
                    </p>
                    <p style="color: #4b5563; line-height: 1.6;">
                        Your 30-day Canary Wallet Team trial will end in 3 days on <strong>{trial_end_date}</strong>.
                    </p>
                    <p style="color: #4b5563; line-height: 1.6;">
                        To continue enjoying uninterrupted Bitcoin wallet monitoring with automatic transaction notifications, please subscribe to one of our plans.
                    </p>

                    <div style="background-color: #fff; border-left: 4px solid #3b82f6; padding: 15px; margin: 20px 0;">
                        <p style="color: #1f2937; margin: 0; font-weight: 500;">What happens when your trial ends?</p>
                        <ul style="color: #4b5563; margin: 10px 0; padding-left: 20px;">
                            <li>Wallet syncing will stop</li>
                            <li>Transaction notifications will pause</li>
                            <li>Your wallet data remains safe and accessible</li>
                        </ul>
                    </div>

                    <div style="text-align: center; margin: 30px 0;">
                        <a href="{billing_url}"
                           style="background-color: #3b82f6; color: white; padding: 12px 24px; text-decoration: none; border-radius: 6px; display: inline-block; font-weight: 500;">
                            View Subscription Plans
                        </a>
                    </div>

                    <p style="color: #6b7280; font-size: 14px; line-height: 1.6;">
                        Choose the plan that's right for you:
                    </p>
                    <ul style="color: #6b7280; font-size: 14px; line-height: 1.6;">
                        <li><strong>Personal ($9/month):</strong> 1 wallet, 1 contact, 10-minute sync</li>
                        <li><strong>Team ($29/month):</strong> 5 wallets, 5 contacts per wallet, 2-minute sync</li>
                    </ul>
                </div>

                <div style="text-align: center; color: #6b7280; font-size: 12px; margin-top: 30px;">
                    <p>© 2024 Canary Wallet. All rights reserved.</p>
                </div>
            </body>
            </html>
            "#,
            subject,
            name = to_name,
            trial_end_date = trial_ends_at,
            billing_url = billing_url
        );

        let text_body = format!(
            r#"
Your Trial is Ending Soon - Canary Wallet

Hi {name},

Your 30-day Canary Wallet Team trial will end in 3 days on {trial_end_date}.

To continue enjoying uninterrupted Bitcoin wallet monitoring with automatic transaction notifications, please subscribe to one of our plans.

What happens when your trial ends?
- Wallet syncing will stop
- Transaction notifications will pause
- Your wallet data remains safe and accessible

Choose the plan that's right for you:
- Personal ($9/month): 1 wallet, 1 contact, 10-minute sync
- Team ($29/month): 5 wallets, 5 contacts per wallet, 2-minute sync

View subscription plans: {billing_url}

© 2024 Canary Wallet. All rights reserved.
            "#,
            name = to_name,
            trial_end_date = trial_ends_at,
            billing_url = billing_url
        );

        self.send_email(to_email, to_name, subject, &html_body, &text_body)
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
