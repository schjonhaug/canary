//! Time-bounded, audited customer-scoped administrator support access.

#![allow(clippy::result_large_err)]

use crate::admin_mfa::MAX_ADMIN_SESSION_AGE_SECONDS;
use crate::api::AppServicesState;
use crate::auth::AuthUser;
use crate::config::AppConfig;
use crate::extractors::AuthenticatedUser;
use crate::metadata::{AdminSupportGrant, UserRecord, WalletMetadata, WalletsListResponse};
use crate::models::ErrorResponse;
use crate::utils::current_unix_timestamp;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const MIN_REASON_CHARS: usize = 8;
const MAX_REASON_CHARS: usize = 280;
const SUPPORT_LOOKUP_MAX_ATTEMPTS: i64 = 10;
const SUPPORT_LOOKUP_WINDOW_MINUTES: i64 = 15;

#[derive(Debug, Deserialize)]
pub struct SupportAccessRequest {
    pub email: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct SupportGrantView {
    pub target_user_id: String,
    pub target_email: String,
    pub reason: String,
    pub expires_at: i64,
}

#[derive(Debug, Serialize)]
pub struct SupportAccessResponse {
    pub grant: Option<SupportGrantView>,
    pub timestamp: u64,
    pub wallets: Vec<WalletMetadata>,
}

fn error_response(status: StatusCode, code: &'static str, message: impl Into<String>) -> Response {
    (status, Json(ErrorResponse::coded(code, message))).into_response()
}

fn require_cloud_admin(config: &AppConfig, user: &AuthUser) -> Result<(), Response> {
    if !config.is_cloud_mode() || !user.is_admin || user.is_demo {
        return Err(error_response(
            StatusCode::FORBIDDEN,
            "access_denied",
            "Access denied",
        ));
    }
    Ok(())
}

fn normalize_reason(reason: &str) -> Result<String, Response> {
    let reason = reason.trim();
    if reason.chars().count() < MIN_REASON_CHARS {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "support_reason_required",
            "Describe why you need this access.",
        ));
    }
    if reason.chars().count() > MAX_REASON_CHARS {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "support_reason_too_long",
            format!("Reason must be at most {MAX_REASON_CHARS} characters."),
        ));
    }
    if reason
        .chars()
        .any(|ch| ch.is_control() && ch != '\n' && ch != '\r')
    {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "support_reason_required",
            "Describe why you need this access.",
        ));
    }
    Ok(reason.to_string())
}

async fn lookup_customer(
    app_services: &AppServicesState,
    email: &str,
) -> Result<UserRecord, Response> {
    let lowered = email.to_lowercase();
    let mut candidate = app_services
        .metadata_db
        .get_user_by_email(email)
        .await
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to look up customer: {error}"
                ))),
            )
                .into_response()
        })?;
    if candidate.is_none() && lowered != email {
        candidate = app_services
            .metadata_db
            .get_user_by_email(&lowered)
            .await
            .map_err(|error| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(format!(
                        "Failed to look up customer: {error}"
                    ))),
                )
                    .into_response()
            })?;
    }
    candidate.ok_or_else(|| {
        error_response(
            StatusCode::NOT_FOUND,
            "support_target_not_found",
            "No customer matches that email.",
        )
    })
}

async fn wallets_for_target(
    app_services: &AppServicesState,
    target_user_id: &str,
) -> Result<WalletsListResponse, Response> {
    let mut wallets_response = app_services
        .get_wallets_list_for_user(target_user_id)
        .await
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to get wallets list: {error}"
                ))),
            )
                .into_response()
        })?;
    if let Ok(Some(user_record)) = app_services
        .metadata_db
        .get_user_by_id(target_user_id)
        .await
    {
        if let Some(currency) = user_record.preferred_fiat_currency {
            if let Ok(rates) = app_services.metadata_db.get_exchange_rates().await {
                if let Some(rate) = rates.get(&currency) {
                    for wallet in &mut wallets_response.wallets {
                        if let Some(balance_sats) = wallet.balance_total {
                            let balance_btc = balance_sats as f64 / 100_000_000.0;
                            wallet.balance_fiat = Some(balance_btc * rate.rate_per_btc);
                            wallet.fiat_currency = Some(currency.clone());
                        }
                    }
                }
            }
        }
    }
    Ok(wallets_response)
}

async fn view_for_grant(
    app_services: &AppServicesState,
    grant: AdminSupportGrant,
) -> Result<SupportAccessResponse, Response> {
    let target = app_services
        .metadata_db
        .get_user_by_id(&grant.target_user_id)
        .await
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to load support target: {error}"
                ))),
            )
                .into_response()
        })?
        .ok_or_else(|| {
            error_response(
                StatusCode::NOT_FOUND,
                "support_target_not_found",
                "No customer matches that email.",
            )
        })?;
    let wallets_response = wallets_for_target(app_services, &grant.target_user_id).await?;
    Ok(SupportAccessResponse {
        grant: Some(SupportGrantView {
            target_user_id: grant.target_user_id,
            target_email: target.email,
            reason: grant.reason,
            expires_at: grant.expires_at,
        }),
        timestamp: wallets_response.timestamp,
        wallets: wallets_response.wallets,
    })
}

pub async fn get_support_access(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
) -> Response {
    if let Err(response) = require_cloud_admin(&config, &user) {
        return response;
    }
    match app_services
        .metadata_db
        .active_admin_support_grant(&user.user_id)
        .await
    {
        Ok(Some(grant)) => match view_for_grant(&app_services, grant).await {
            Ok(body) => (StatusCode::OK, Json(body)).into_response(),
            Err(response) => response,
        },
        Ok(None) => {
            let timestamp = match current_unix_timestamp() {
                Ok(timestamp) => timestamp,
                Err(error) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse::new(format!(
                            "system clock is before UNIX_EPOCH: {error}"
                        ))),
                    )
                        .into_response();
                }
            };
            (
                StatusCode::OK,
                Json(SupportAccessResponse {
                    grant: None,
                    timestamp,
                    wallets: Vec::new(),
                }),
            )
                .into_response()
        }
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(format!(
                "Failed to load support access: {error}"
            ))),
        )
            .into_response(),
    }
}

pub async fn create_support_access(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    Json(payload): Json<SupportAccessRequest>,
) -> Response {
    if let Err(response) = require_cloud_admin(&config, &user) {
        return response;
    }
    let reason = match normalize_reason(&payload.reason) {
        Ok(reason) => reason,
        Err(response) => return response,
    };
    let email = payload.email.trim().to_string();
    if email.is_empty() || email.len() > 254 || !email.contains('@') {
        return error_response(
            StatusCode::BAD_REQUEST,
            "invalid_email_format",
            "Invalid email format.",
        );
    }
    match app_services
        .metadata_db
        .check_auth_rate_limit(
            "admin_support",
            &user.user_id,
            SUPPORT_LOOKUP_MAX_ATTEMPTS,
            SUPPORT_LOOKUP_WINDOW_MINUTES,
        )
        .await
    {
        Ok(true) => {}
        Ok(false) => {
            return error_response(
                StatusCode::TOO_MANY_REQUESTS,
                "support_access_rate_limited",
                "Too many support lookups. Try again in a few minutes.",
            );
        }
        Err(_) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse::new("Support access unavailable")),
            )
                .into_response();
        }
    }
    let target = match lookup_customer(&app_services, &email).await {
        Ok(target) => target,
        Err(response) => return response,
    };
    if target.is_admin || target.is_demo || target.id == user.user_id {
        return error_response(
            StatusCode::BAD_REQUEST,
            "support_target_invalid",
            "Support access is only for customer accounts.",
        );
    }
    match app_services
        .metadata_db
        .remaining_admin_mfa_seconds(&user.user_id, MAX_ADMIN_SESSION_AGE_SECONDS)
        .await
    {
        Ok(Some(ttl)) if ttl > 0 => match app_services
            .metadata_db
            .grant_admin_support_access(&user.user_id, &target.id, &reason, ttl)
            .await
        {
            Ok(grant) => match view_for_grant(&app_services, grant).await {
                Ok(body) => (StatusCode::OK, Json(body)).into_response(),
                Err(response) => response,
            },
            Err(error) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to grant support access: {error}"
                ))),
            )
                .into_response(),
        },
        Ok(_) => error_response(
            StatusCode::UNAUTHORIZED,
            "admin_reauthentication_required",
            "Sign in again with your password and authenticator code.",
        ),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("Support access unavailable")),
        )
            .into_response(),
    }
}

pub async fn revoke_support_access(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
) -> Response {
    if let Err(response) = require_cloud_admin(&config, &user) {
        return response;
    }
    match app_services
        .metadata_db
        .revoke_admin_support_grants(&user.user_id)
        .await
    {
        Ok(_) => {
            let timestamp = current_unix_timestamp().unwrap_or(0);
            (
                StatusCode::OK,
                Json(SupportAccessResponse {
                    grant: None,
                    timestamp,
                    wallets: Vec::new(),
                }),
            )
                .into_response()
        }
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(format!(
                "Failed to end support access: {error}"
            ))),
        )
            .into_response(),
    }
}
