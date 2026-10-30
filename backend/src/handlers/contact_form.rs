//! The public contact form, delivered to the operator by email.

use super::auth::{client_ip, enforce_ip_rate_limit};
use super::error::{ApiError, ApiResult, OrInternal};
use crate::api::{AppServicesState, EmailServiceState};
use crate::config::AppConfig;
use crate::models::{ContactFormRequest, ContactFormResponse};
use axum::{
    extract::{ConnectInfo, Extension, State},
    http::HeaderMap,
    response::Json,
};
use std::{net::SocketAddr, sync::Arc};
use tracing::info;

const MAX_CONTACT_REQUESTS_PER_IP: i64 = 3;
const CONTACT_RATE_LIMIT_WINDOW_MINUTES: i64 = 60;

/// Contact form submission endpoint
pub async fn submit_contact_form(
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    State(email_service): State<EmailServiceState>,
    address: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(payload): Json<ContactFormRequest>,
) -> ApiResult<Json<ContactFormResponse>> {
    enforce_ip_rate_limit(
        &app_services,
        &config,
        "contact",
        client_ip(&headers, address, &config),
        MAX_CONTACT_REQUESTS_PER_IP,
        CONTACT_RATE_LIMIT_WINDOW_MINUTES,
        true,
    )
    .await?;
    let email = payload.email.trim();
    let message = payload.message.trim();
    validate_contact_form(email, message)?;

    let email_service = email_service
        .ok_or_else(|| anyhow::anyhow!("Email service not configured"))
        .or_internal("Contact form is temporarily unavailable.")?;
    email_service
        .send_contact_form_submission(email, message)
        .await
        .or_internal("Failed to send message. Please try again later.")?;

    info!("Contact form submitted");
    Ok(Json(ContactFormResponse {
        message: "Thank you for your message. We'll get back to you soon.".to_string(),
    }))
}

fn validate_contact_form(email: &str, message: &str) -> ApiResult<()> {
    if !valid_contact_email(email) {
        return Err(ApiError::bad_request(
            "invalid_email_format",
            "Please provide a valid email address",
        ));
    }
    if message.len() < 10 {
        return Err(ApiError::bad_request(
            "message_too_short",
            "Message must be at least 10 characters",
        ));
    }
    if message.len() > 5000 {
        return Err(ApiError::bad_request(
            "message_too_long",
            "Message must be less than 5000 characters",
        ));
    }
    Ok(())
}

fn valid_contact_email(email: &str) -> bool {
    if email.len() > 255 || email.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.contains('@')
        && domain.contains('.')
        && domain.split('.').all(|part| !part.is_empty())
}

#[cfg(test)]
mod tests {
    use super::valid_contact_email;

    #[test]
    fn contact_reply_address_rejects_invalid_header_values() {
        assert!(valid_contact_email("visitor@example.com"));
        for address in [
            "a@b@c.com",
            "a@b.com\r\nBcc: other@example.com",
            "@example.com",
            "visitor@",
            "visitor@example..com",
            "visitor@example",
        ] {
            assert!(!valid_contact_email(address), "accepted {address:?}");
        }
    }
}
