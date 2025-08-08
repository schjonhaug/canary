use anyhow::{Result, anyhow};
use lettre::message::{header, MultiPart, SinglePart};
use lettre::{Message, SmtpTransport, Transport};
use lettre::transport::smtp::authentication::Credentials;

#[derive(Debug, Clone)]
pub struct EmailConfig {
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_username: String,
    pub smtp_password: String,
    pub from_email: String,
    pub from_name: String,
}

impl EmailConfig {
    pub fn from_env() -> Result<Self> {
        let smtp_host = std::env::var("SMTP_HOST")
            .map_err(|_| anyhow!("SMTP_HOST environment variable not set"))?;
        let smtp_port = std::env::var("SMTP_PORT")
            .unwrap_or_else(|_| "587".to_string())
            .parse::<u16>()
            .map_err(|_| anyhow!("Invalid SMTP_PORT value"))?;
        let smtp_username = std::env::var("SMTP_USERNAME")
            .map_err(|_| anyhow!("SMTP_USERNAME environment variable not set"))?;
        let smtp_password = std::env::var("SMTP_PASSWORD")
            .map_err(|_| anyhow!("SMTP_PASSWORD environment variable not set"))?;
        let from_email = std::env::var("FROM_EMAIL")
            .map_err(|_| anyhow!("FROM_EMAIL environment variable not set"))?;
        let from_name = std::env::var("FROM_NAME")
            .unwrap_or_else(|_| "Canary Bitcoin Wallet".to_string());

        Ok(Self {
            smtp_host,
            smtp_port,
            smtp_username,
            smtp_password,
            from_email,
            from_name,
        })
    }
}

pub struct EmailService {
    config: EmailConfig,
}

impl EmailService {
    pub fn new(config: EmailConfig) -> Self {
        Self { config }
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

        let html_body = format!(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta charset="utf-8">
                <title>Verify Your Email - Canary Wallet</title>
            </head>
            <body style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto; padding: 20px;">
                <div style="text-align: center; margin-bottom: 30px;">
                    <h1 style="color: #1f2937;">Welcome to Canary Wallet</h1>
                </div>
                
                <div style="background-color: #f9fafb; padding: 20px; border-radius: 8px; margin-bottom: 20px;">
                    <h2 style="color: #1f2937; margin-top: 0;">Verify Your Email Address</h2>
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

        let email = Message::builder()
            .from(format!("{} <{}>", self.config.from_name, self.config.from_email).parse()?)
            .to(format!("{} <{}>", to_name, to_email).parse()?)
            .subject("Verify Your Email - Canary Wallet")
            .multipart(
                MultiPart::alternative()
                    .singlepart(
                        SinglePart::builder()
                            .header(header::ContentType::TEXT_PLAIN)
                            .body(text_body),
                    )
                    .singlepart(
                        SinglePart::builder()
                            .header(header::ContentType::TEXT_HTML)
                            .body(html_body),
                    ),
            )?;

        self.send_email(email).await
    }

    pub async fn send_password_reset(&self, to_email: &str, to_name: &str, reset_token: &str) -> Result<()> {
        let reset_url = format!("{}/reset-password/{}", 
            std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:3001".to_string()),
            reset_token
        );

        let html_body = format!(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta charset="utf-8">
                <title>Reset Your Password - Canary Wallet</title>
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
                        <a href="{reset_url}" style="color: #dc2626; word-break: break-all;">{reset_url}</a>
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
            name = to_name,
            reset_url = reset_url
        );

        let text_body = format!(
            r#"
Canary Wallet - Reset Your Password

Hi {name},

We received a request to reset your password for your Canary Wallet account. Visit the link below to create a new password:

{reset_url}

This reset link will expire in 1 hour. If you didn't request a password reset, you can safely ignore this email.

© 2024 Canary Wallet. All rights reserved.
            "#,
            name = to_name,
            reset_url = reset_url
        );

        let email = Message::builder()
            .from(format!("{} <{}>", self.config.from_name, self.config.from_email).parse()?)
            .to(format!("{} <{}>", to_name, to_email).parse()?)
            .subject("Reset Your Password - Canary Wallet")
            .multipart(
                MultiPart::alternative()
                    .singlepart(
                        SinglePart::builder()
                            .header(header::ContentType::TEXT_PLAIN)
                            .body(text_body),
                    )
                    .singlepart(
                        SinglePart::builder()
                            .header(header::ContentType::TEXT_HTML)
                            .body(html_body),
                    ),
            )?;

        self.send_email(email).await
    }

    async fn send_email(&self, email: Message) -> Result<()> {
        let creds = Credentials::new(
            self.config.smtp_username.clone(),
            self.config.smtp_password.clone(),
        );

        let mailer = SmtpTransport::relay(&self.config.smtp_host)?
            .port(self.config.smtp_port)
            .credentials(creds)
            .build();

        println!("Attempting to send email to {} via {}:{}", 
            email.envelope().to().first().map(|a| a.as_ref()).unwrap_or("unknown"),
            self.config.smtp_host,
            self.config.smtp_port
        );
        
        match mailer.send(&email) {
            Ok(response) => {
                println!("Email sent successfully: {:?}", response);
                Ok(())
            },
            Err(e) => {
                eprintln!("Failed to send email: {}", e);
                Err(anyhow!("Failed to send email: {}", e))
            },
        }
    }
    
    pub async fn send_transaction_notification(
        &self,
        to_email: &str,
        to_name: &str,
        subject: &str,
        html_body: &str,
        text_body: &str,
    ) -> Result<()> {
        use lettre::{Message, message::{header, MultiPart, SinglePart}};
        
        let from_email = std::env::var("FROM_EMAIL")
            .unwrap_or_else(|_| "notifications@canarybitcoin.com".to_string());
        let from_name = std::env::var("FROM_NAME")
            .unwrap_or_else(|_| "Canary Wallet".to_string());
        
        let email = Message::builder()
            .from(format!("{} <{}>", from_name, from_email).parse()?)
            .to(format!("{} <{}>", to_name, to_email).parse()?)
            .subject(subject)
            .multipart(
                MultiPart::alternative()
                    .singlepart(
                        SinglePart::builder()
                            .header(header::ContentType::TEXT_PLAIN)
                            .body(text_body.to_string()),
                    )
                    .singlepart(
                        SinglePart::builder()
                            .header(header::ContentType::TEXT_HTML)
                            .body(html_body.to_string()),
                    ),
            )?;

        self.send_email(email).await
    }
}