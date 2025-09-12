use crate::email_service::EmailService;
use crate::metadata::TwilioConfig;
use anyhow::{anyhow, Result};
use argon2::password_hash::{rand_core::OsRng, SaltString};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use base64::{engine::general_purpose, Engine as _};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use utoipa::ToSchema;
use uuid::Uuid;

/// Load Twilio configuration from environment variables
pub fn load_twilio_config_from_env() -> Result<TwilioConfig> {
    let account_sid =
        std::env::var("TWILIO_ACCOUNT_SID").map_err(|_| anyhow!("TWILIO_ACCOUNT_SID not set"))?;

    let auth_token =
        std::env::var("TWILIO_AUTH_TOKEN").map_err(|_| anyhow!("TWILIO_AUTH_TOKEN not set"))?;

    let messaging_service_sid = std::env::var("TWILIO_MESSAGING_SERVICE_SID")
        .map_err(|_| anyhow!("TWILIO_MESSAGING_SERVICE_SID not set"))?;

    let verify_service_sid = std::env::var("TWILIO_VERIFY_SERVICE_SID").ok();

    Ok(TwilioConfig {
        id: None,
        account_sid,
        auth_token,
        messaging_service_sid,
        verify_service_sid,
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // UUIDv4
    pub email: String,
    pub is_admin: bool,
    pub exp: usize,
    pub iat: usize,
}

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String, // UUIDv4
    pub is_admin: bool,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub browser_locale: Option<String>,
    #[serde(default)]
    pub marketing_emails_opt_in: bool,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct ForgotPasswordRequest {
    pub email: String,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct ResetPasswordRequest {
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthResponse {
    pub token: String,
    pub user: AuthUserResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_name: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthUserResponse {
    pub id: String, // UUIDv4
    pub email: String,
    pub name: Option<String>,
    pub is_admin: bool,
    pub email_verified: bool,
    pub subscription_tier: crate::subscription::SubscriptionTier,
    pub created_at: String,
    pub preferred_fiat_currency: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateUserRequest {
    pub name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UpdateUserResponse {
    pub user: AuthUserResponse,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateUserPreferencesRequest {
    pub preferred_fiat_currency: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserPreferencesResponse {
    pub preferred_fiat_currency: String,
}

// Development mode configuration
const DEV_MODE: bool = cfg!(debug_assertions);

// Dev mode test email addresses (bypass email verification in dev mode)
pub const DEV_TEST_EMAILS: [&str; 3] = [
    "delivered+admin@resend.dev", // Team tier + Admin
    "delivered+alice@resend.dev", // Personal tier
    "delivered+bob@resend.dev",   // Team tier
];

// Dev mode password for all test accounts
pub const DEV_TEST_PASSWORD: &str = "password123";

pub struct AuthService {
    jwt_secret: String,
    client: Client,
    pub email_service: Option<EmailService>,
}

impl AuthService {
    pub fn new(jwt_secret: String, email_service: Option<EmailService>) -> Self {
        Self {
            jwt_secret,
            client: Client::new(),
            email_service,
        }
    }

    pub fn hash_password(&self, password: &str) -> Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let password_hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| anyhow!("Failed to hash password: {}", e))?
            .to_string();
        Ok(password_hash)
    }

    pub fn verify_password(&self, password: &str, password_hash: &str) -> Result<bool> {
        let parsed_hash = PasswordHash::new(password_hash)
            .map_err(|e| anyhow!("Invalid password hash: {}", e))?;

        match Argon2::default().verify_password(password.as_bytes(), &parsed_hash) {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    pub fn generate_verification_token(&self) -> String {
        Uuid::new_v4().to_string()
    }

    pub async fn send_email_verification(
        &self,
        email: &str,
        name: &str,
        token: &str,
    ) -> Result<()> {
        if let Some(email_service) = &self.email_service {
            email_service
                .send_email_verification(email, name, token)
                .await
        } else {
            // In development mode without email service, just log the token
            if DEV_MODE {
                println!(
                    "[DEV MODE] Email verification token for {}: {}",
                    email, token
                );
                Ok(())
            } else {
                Err(anyhow!("Email service not configured"))
            }
        }
    }

    pub async fn send_password_reset(&self, email: &str, name: &str, token: &str) -> Result<()> {
        if let Some(email_service) = &self.email_service {
            email_service.send_password_reset(email, name, token).await
        } else {
            // In development mode without email service, just log the token
            if DEV_MODE {
                println!("[DEV MODE] Password reset token for {}: {}", email, token);
                Ok(())
            } else {
                Err(anyhow!("Email service not configured"))
            }
        }
    }

    pub fn is_dev_test_email(&self, email: &str) -> bool {
        DEV_MODE && DEV_TEST_EMAILS.contains(&email)
    }

    pub fn get_dev_test_password(&self) -> &'static str {
        DEV_TEST_PASSWORD
    }

    // Keep this method for SMS contact verification (not auth login)
    pub async fn send_contact_otp(
        &self,
        twilio_config: &TwilioConfig,
        phone_number: &str,
    ) -> Result<()> {
        let verify_service_sid = twilio_config
            .verify_service_sid
            .as_ref()
            .ok_or_else(|| anyhow!("Twilio Verify service SID not configured"))?;

        let url = format!(
            "https://verify.twilio.com/v2/Services/{}/Verifications",
            verify_service_sid
        );

        let auth_string = format!("{}:{}", twilio_config.account_sid, twilio_config.auth_token);
        let auth_header = format!("Basic {}", general_purpose::STANDARD.encode(auth_string));

        let params = [("To", phone_number), ("Channel", "sms")];

        let response = self
            .client
            .post(&url)
            .header("Authorization", auth_header)
            .form(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("Failed to send OTP: {}", error_text));
        }

        Ok(())
    }

    // Keep this method for SMS contact verification (not auth login)
    pub async fn verify_contact_otp(
        &self,
        twilio_config: &TwilioConfig,
        phone_number: &str,
        code: &str,
    ) -> Result<bool> {
        let verify_service_sid = twilio_config
            .verify_service_sid
            .as_ref()
            .ok_or_else(|| anyhow!("Twilio Verify service SID not configured"))?;

        let url = format!(
            "https://verify.twilio.com/v2/Services/{}/VerificationCheck",
            verify_service_sid
        );

        let auth_string = format!("{}:{}", twilio_config.account_sid, twilio_config.auth_token);
        let auth_header = format!("Basic {}", general_purpose::STANDARD.encode(auth_string));

        let params = [("To", phone_number), ("Code", code)];

        let response = self
            .client
            .post(&url)
            .header("Authorization", auth_header)
            .form(&params)
            .send()
            .await?;

        if response.status().is_success() {
            let body: serde_json::Value = response.json().await?;
            if let Some(status) = body.get("status").and_then(|s| s.as_str()) {
                return Ok(status == "approved");
            }
        }

        Ok(false)
    }

    // Email contact verification methods
    pub async fn send_email_contact_otp(
        &self,
        to_email: &str,
        to_name: &str,
        otp_code: &str,
    ) -> Result<()> {
        if let Some(email_service) = &self.email_service {
            email_service
                .send_contact_otp_verification(to_email, to_name, otp_code)
                .await
        } else {
            Err(anyhow!("Email service not configured"))
        }
    }

    pub fn verify_email_contact_otp(&self, stored_code: &str, provided_code: &str) -> bool {
        stored_code == provided_code
    }

    // Check if email matches current user's account email (skip verification if same)
    pub fn should_skip_email_verification(&self, contact_email: &str, user_email: &str) -> bool {
        contact_email.to_lowercase() == user_email.to_lowercase()
    }

    pub fn generate_token(&self, user_id: &str, email: &str, is_admin: bool) -> Result<String> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as usize;

        let claims = Claims {
            sub: user_id.to_string(),
            email: email.to_string(),
            is_admin,
            exp: now + 7 * 24 * 60 * 60, // 7 days
            iat: now,
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.jwt_secret.as_ref()),
        )?;

        Ok(token)
    }

    pub fn validate_token(&self, token: &str) -> Result<Claims> {
        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.jwt_secret.as_ref()),
            &Validation::new(Algorithm::HS256),
        )?;

        Ok(token_data.claims)
    }

    pub fn hash_token(token: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

pub fn authenticate_user(auth_header: Option<&str>) -> Result<AuthUser> {
    // SAAS mode: validate JWT token
    let auth_header = auth_header.ok_or_else(|| anyhow!("Authorization header required"))?;

    if !auth_header.starts_with("Bearer ") {
        return Err(anyhow!("Invalid authorization header format"));
    }

    let token = &auth_header[7..];
    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "your-secret-key-change-in-production".to_string());

    let auth_service = AuthService::new(jwt_secret, None);
    let claims = auth_service.validate_token(token)?;

    Ok(AuthUser {
        user_id: claims.sub,
        is_admin: claims.is_admin,
    })
}
