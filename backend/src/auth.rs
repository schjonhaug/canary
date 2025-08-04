use anyhow::{Result, anyhow};
use jsonwebtoken::{encode, decode, Header, Validation, EncodingKey, DecodingKey, Algorithm};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use base64::{Engine as _, engine::general_purpose};
use reqwest::Client;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::metadata::TwilioConfig;
use utoipa::ToSchema;

/// Load Twilio configuration from environment variables
pub fn load_twilio_config_from_env() -> Result<TwilioConfig> {
    let account_sid = std::env::var("TWILIO_ACCOUNT_SID")
        .map_err(|_| anyhow!("TWILIO_ACCOUNT_SID not set"))?;
    
    let auth_token = std::env::var("TWILIO_AUTH_TOKEN")
        .map_err(|_| anyhow!("TWILIO_AUTH_TOKEN not set"))?;
    
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
    pub sub: i64,
    pub phone: String,
    pub is_admin: bool,
    pub exp: usize,
    pub iat: usize,
}

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: i64,
    pub is_admin: bool,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct SendOtpRequest {
    pub phone_number: String,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct VerifyOtpRequest {
    pub phone_number: String,
    pub code: String,
    pub name: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthResponse {
    pub token: String,
    pub user: AuthUserResponse,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthUserResponse {
    pub id: i64,
    pub phone_number: String,
    pub name: Option<String>,
    pub created_at: String,
}

// Development mode configuration
const DEV_MODE: bool = cfg!(debug_assertions);
pub const DEV_ADMIN_PHONE: &str = "+4799999900";

// Dev mode test phone numbers
const DEV_TEST_PHONES: [&str; 3] = [
    "+4799999901",
    "+4699999902", 
    "+3399999903"
];

pub struct AuthService {
    jwt_secret: String,
    client: Client,
}

impl AuthService {
    pub fn new(jwt_secret: String) -> Self {
        Self {
            jwt_secret,
            client: Client::new(),
        }
    }

    pub async fn send_otp(&self, twilio_config: &TwilioConfig, phone_number: &str) -> Result<()> {
        // Development mode: bypass Twilio for dev test phones
        if DEV_MODE && (DEV_TEST_PHONES.contains(&phone_number) || phone_number == DEV_ADMIN_PHONE) {
            return Ok(());
        }

        let verify_service_sid = twilio_config.verify_service_sid
            .as_ref()
            .ok_or_else(|| anyhow!("Twilio Verify service SID not configured"))?;

        let url = format!(
            "https://verify.twilio.com/v2/Services/{}/Verifications",
            verify_service_sid
        );

        let auth_string = format!("{}:{}", twilio_config.account_sid, twilio_config.auth_token);
        let auth_header = format!("Basic {}", general_purpose::STANDARD.encode(auth_string));

        let params = [
            ("To", phone_number),
            ("Channel", "sms"),
        ];

        let response = self.client
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

    pub async fn verify_otp(&self, twilio_config: &TwilioConfig, phone_number: &str, code: &str) -> Result<bool> {
        // Development mode: accept any code for dev test phones
        if DEV_MODE && (DEV_TEST_PHONES.contains(&phone_number) || phone_number == DEV_ADMIN_PHONE) {
            return Ok(true);
        }

        let verify_service_sid = twilio_config.verify_service_sid
            .as_ref()
            .ok_or_else(|| anyhow!("Twilio Verify service SID not configured"))?;

        let url = format!(
            "https://verify.twilio.com/v2/Services/{}/VerificationCheck",
            verify_service_sid
        );

        let auth_string = format!("{}:{}", twilio_config.account_sid, twilio_config.auth_token);
        let auth_header = format!("Basic {}", general_purpose::STANDARD.encode(auth_string));

        let params = [
            ("To", phone_number),
            ("Code", code),
        ];

        let response = self.client
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

    pub fn generate_token(&self, user_id: i64, phone_number: &str, is_admin: bool) -> Result<String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs() as usize;
        
        let claims = Claims {
            sub: user_id,
            phone: phone_number.to_string(),
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
    // Check if auth is enabled via environment variable
    let auth_enabled = std::env::var("CANARY_ENABLE_AUTH")
        .map(|v| v.to_lowercase() == "true" || v == "1")
        .unwrap_or(false);
    
    if !auth_enabled {
        // Self-hosted mode: always return admin user
        return Ok(AuthUser {
            user_id: 1,
            is_admin: true,
        });
    }

    // SAAS mode: validate JWT token
    let auth_header = auth_header.ok_or_else(|| anyhow!("Authorization header required"))?;
    
    if !auth_header.starts_with("Bearer ") {
        return Err(anyhow!("Invalid authorization header format"));
    }

    let token = &auth_header[7..];
    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "your-secret-key-change-in-production".to_string());
    
    let auth_service = AuthService::new(jwt_secret);
    let claims = auth_service.validate_token(token)?;

    // Check if user is admin
    let admin_phone = std::env::var("ADMIN_PHONE_NUMBER").ok();
    let is_admin = admin_phone.map_or(false, |phone| phone == claims.phone) 
        || claims.is_admin
        || (DEV_MODE && claims.phone == DEV_ADMIN_PHONE);

    Ok(AuthUser {
        user_id: claims.sub,
        is_admin,
    })
}