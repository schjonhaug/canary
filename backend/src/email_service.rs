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
        language: &str,
    ) -> Result<()> {
        let verification_url = format!(
            "{}/verify-email/{}",
            std::env::var("FRONTEND_URL").expect("FRONTEND_URL must be set"),
            verification_token
        );

        // Detect Norwegian language
        let is_norwegian = language.to_lowercase().starts_with("no")
            || language.to_lowercase().starts_with("nb")
            || language.to_lowercase().starts_with("nn");

        let (subject, header, greeting, body_text, button_text, link_fallback, expiry_text, footer) = if is_norwegian {
            (
                "Bekreft e-postadressen din - Canary",
                "Velkommen til Canary!",
                format!("Hei {},", to_name),
                "Takk for at du opprettet en Canary-konto! For å komme i gang, bekreft e-postadressen din ved å klikke på knappen nedenfor.",
                "Bekreft e-postadresse",
                "Hvis knappen ikke fungerer, kan du kopiere og lime inn denne lenken i nettleseren:",
                "Denne bekreftelseslenken utløper om 24 timer. Hvis du ikke opprettet en konto, kan du trygt ignorere denne e-posten.",
                "Dette varselet ble sendt av Canary",
            )
        } else {
            (
                "Verify Your Email - Canary",
                "Welcome to Canary!",
                format!("Hi {},", to_name),
                "Thank you for creating your Canary account! To get started, please verify your email address by clicking the button below.",
                "Verify Email Address",
                "If the button doesn't work, you can copy and paste this link into your browser:",
                "This verification link will expire in 24 hours. If you didn't create an account, you can safely ignore this email.",
                "This notification was sent by Canary",
            )
        };

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

        self.send_email(to_email, to_name, subject, &html_body, &text_body)
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
            std::env::var("FRONTEND_URL").expect("FRONTEND_URL must be set"),
            reset_token
        );

        // Detect Norwegian language
        let is_norwegian = language.to_lowercase().starts_with("no")
            || language.to_lowercase().starts_with("nb")
            || language.to_lowercase().starts_with("nn");

        let (subject, header, greeting, body_text, button_text, link_fallback, expiry_text, footer) = if is_norwegian {
            (
                "Tilbakestill passordet ditt - Canary",
                "Tilbakestill passordet ditt",
                format!("Hei {},", to_name),
                "Vi mottok en forespørsel om å tilbakestille passordet for Canary-kontoen din. Klikk på knappen nedenfor for å opprette et nytt passord.",
                "Tilbakestill passord",
                "Hvis knappen ikke fungerer, kan du kopiere og lime inn denne lenken i nettleseren:",
                "Denne tilbakestillingslenken utløper om 1 time. Hvis du ikke ba om å tilbakestille passordet, kan du trygt ignorere denne e-posten.",
                "Dette varselet ble sendt av Canary",
            )
        } else {
            (
                "Reset Your Password - Canary",
                "Reset Your Password",
                format!("Hi {},", to_name),
                "We received a request to reset your password for your Canary account. Click the button below to create a new password.",
                "Reset Password",
                "If the button doesn't work, you can copy and paste this link into your browser:",
                "This reset link will expire in 1 hour. If you didn't request a password reset, you can safely ignore this email.",
                "This notification was sent by Canary",
            )
        };

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
        language: &str,
    ) -> Result<()> {
        // Detect Norwegian language
        let is_norwegian = language.to_lowercase().starts_with("no")
            || language.to_lowercase().starts_with("nb")
            || language.to_lowercase() == "norwegian";

        let (subject, header, greeting, body_text, expiry_text, footer) = if is_norwegian {
            (
                "Bekreft e-postadressen din - Canary",
                "E-postbekreftelse kreves",
                format!("Hei {},", to_name),
                "Bekreft e-postadressen din for å motta varsler fra Bitcoin-lommeboken. Skriv inn bekreftelseskoden nedenfor:",
                "Denne bekreftelseskoden utløper om 10 minutter. Hvis du ikke ba om denne bekreftelsen, kan du trygt ignorere denne e-posten.",
                "Dette varselet ble sendt av Canary",
            )
        } else {
            (
                "Verify Your Email - Canary",
                "Email Verification Required",
                format!("Hi {},", to_name),
                "Please verify your email address to receive Bitcoin wallet notifications. Enter the verification code below:",
                "This verification code will expire in 10 minutes. If you didn't request this verification, you can safely ignore this email.",
                "This notification was sent by Canary",
            )
        };

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

        self.send_email(to_email, to_name, subject, &html_body, &text_body)
            .await
    }

    pub async fn send_trial_ending_notification(
        &self,
        to_email: &str,
        to_name: &str,
        trial_ends_at: &str,
        language: &str,
    ) -> Result<()> {
        let billing_url = format!(
            "{}/billing",
            std::env::var("FRONTEND_URL").expect("FRONTEND_URL must be set")
        );

        // Detect Norwegian language
        let is_norwegian = language.to_lowercase().starts_with("no")
            || language.to_lowercase().starts_with("nb")
            || language.to_lowercase().starts_with("nn");

        let (
            subject,
            header,
            greeting,
            body_text,
            what_happens_header,
            sync_stops,
            notifications_stop,
            data_safe,
            button_text,
            choose_plan,
            personal_plan,
            team_plan,
            footer,
        ) = if is_norwegian {
            (
                "Canary-prøveperioden din utløper om 3 dager",
                "Prøveperioden din utløper snart",
                format!("Hei {},", to_name),
                format!("Din 30-dagers Canary Team-prøveperiode utløper om 3 dager, den <strong>{}</strong>.", trial_ends_at),
                "Hva skjer når prøveperioden utløper?",
                "Lommeboksynkronisering stopper",
                "Varsler stopper",
                "Lommebokdataene dine forblir trygge og tilgjengelige",
                "Se abonnementsplaner",
                "Velg planen som passer for deg:",
                "Personal: 1 lommebok, 1 kontakt, 10 minutters synkronisering",
                "Team: 5 lommebøker, 5 kontakter per lommebok, 2 minutters synkronisering",
                "Dette varselet ble sendt av Canary",
            )
        } else {
            (
                "Your Canary trial ends in 3 days",
                "Your Trial is Ending Soon",
                format!("Hi {},", to_name),
                format!("Your 30-day Canary Team trial will end in 3 days on <strong>{}</strong>.", trial_ends_at),
                "What happens when your trial ends?",
                "Wallet syncing will stop",
                "Notifications will stop",
                "Your wallet data remains safe and accessible",
                "View Subscription Plans",
                "Choose the plan that's right for you:",
                "Personal: 1 wallet, 1 contact, 10-minute sync",
                "Team: 5 wallets, 5 contacts per wallet, 2-minute sync",
                "This notification was sent by Canary",
            )
        };

        let body_text_plain = if is_norwegian {
            format!("Din 30-dagers Canary Team-prøveperiode utløper om 3 dager, den {}.", trial_ends_at)
        } else {
            format!("Your 30-day Canary Team trial will end in 3 days on {}.", trial_ends_at)
        };

        let continue_text = if is_norwegian {
            "For å fortsette å nyte uavbrutt overvåking av Bitcoin-lommeboken med automatiske varsler, vennligst abonner på en av planene våre."
        } else {
            "To continue enjoying uninterrupted Bitcoin wallet monitoring with automatic notifications, please subscribe to one of our plans."
        };

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
