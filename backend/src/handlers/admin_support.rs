//! Time-bounded, audited customer-scoped administrator support access.

use crate::admin_mfa::MAX_ADMIN_SESSION_AGE_SECONDS;
use crate::api::AppServicesState;
use crate::auth::AuthUser;
use crate::config::AppConfig;
use crate::extractors::AuthenticatedUser;
use crate::handlers::{ApiError, ApiResult, OrInternal};
use crate::metadata::{AdminSupportGrant, UserRecord, WalletMetadata, WalletsListResponse};
use crate::utils::amount::sats_to_btc;
use crate::utils::current_unix_timestamp;
use axum::{extract::State, http::StatusCode, response::Json};
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

fn require_cloud_admin(config: &AppConfig, user: &AuthUser) -> ApiResult<()> {
    if !config.is_cloud_mode() || !user.is_admin || user.is_demo {
        return Err(ApiError::forbidden("access_denied", "Access denied"));
    }
    Ok(())
}

fn reason_required() -> ApiError {
    ApiError::bad_request(
        "support_reason_required",
        "Describe why you need this access.",
    )
}

fn support_target_not_found() -> ApiError {
    ApiError::not_found(
        "support_target_not_found",
        "No customer matches that email.",
    )
}

fn support_access_unavailable() -> ApiError {
    ApiError::uncoded(
        StatusCode::SERVICE_UNAVAILABLE,
        "Support access unavailable",
    )
}

fn normalize_reason(reason: &str) -> ApiResult<String> {
    let reason = reason.trim();
    if reason.chars().count() < MIN_REASON_CHARS {
        return Err(reason_required());
    }
    if reason.chars().count() > MAX_REASON_CHARS {
        return Err(ApiError::bad_request(
            "support_reason_too_long",
            format!("Reason must be at most {MAX_REASON_CHARS} characters."),
        ));
    }
    if reason
        .chars()
        .any(|ch| ch.is_control() && ch != '\n' && ch != '\r')
    {
        return Err(reason_required());
    }
    Ok(reason.to_string())
}

async fn lookup_customer(app_services: &AppServicesState, email: &str) -> ApiResult<UserRecord> {
    const LOOKUP_FAILED: &str = "Failed to look up customer";
    let lowered = email.to_lowercase();
    let mut candidate = app_services
        .metadata_db
        .get_user_by_email(email)
        .await
        .or_internal(LOOKUP_FAILED)?;
    if candidate.is_none() && lowered != email {
        candidate = app_services
            .metadata_db
            .get_user_by_email(&lowered)
            .await
            .or_internal(LOOKUP_FAILED)?;
    }
    candidate.ok_or_else(support_target_not_found)
}

async fn wallets_for_target(
    app_services: &AppServicesState,
    target_user_id: &str,
) -> ApiResult<WalletsListResponse> {
    let mut wallets_response = app_services
        .get_wallets_list_for_user(target_user_id)
        .await
        .or_internal("Failed to get wallets list")?;
    if let Ok(Some(user_record)) = app_services
        .metadata_db
        .get_user_by_id(target_user_id)
        .await
        && let Some(currency) = user_record.preferred_fiat_currency
        && let Ok(rates) = app_services.metadata_db.get_exchange_rates().await
        && let Some(rate) = rates.get(&currency)
    {
        for wallet in &mut wallets_response.wallets {
            if let Some(balance_sats) = wallet.balance_total {
                let balance_btc = sats_to_btc(balance_sats);
                wallet.balance_fiat = Some(balance_btc * rate.rate_per_btc);
                wallet.fiat_currency = Some(currency.clone());
            }
        }
    }
    Ok(wallets_response)
}

async fn view_for_grant(
    app_services: &AppServicesState,
    grant: AdminSupportGrant,
) -> ApiResult<SupportAccessResponse> {
    let target = app_services
        .metadata_db
        .get_user_by_id(&grant.target_user_id)
        .await
        .or_internal("Failed to load support target")?
        .ok_or_else(support_target_not_found)?;
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

fn empty_support_access(timestamp: u64) -> Json<SupportAccessResponse> {
    Json(SupportAccessResponse {
        grant: None,
        timestamp,
        wallets: Vec::new(),
    })
}

pub async fn get_support_access(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
) -> ApiResult<Json<SupportAccessResponse>> {
    require_cloud_admin(&config, &user)?;
    let grant = app_services
        .metadata_db
        .active_admin_support_grant(&user.user_id)
        .await
        .or_internal("Failed to load support access")?;
    if let Some(grant) = grant {
        return Ok(Json(view_for_grant(&app_services, grant).await?));
    }
    let timestamp = current_unix_timestamp().or_internal("Failed to read the system clock")?;
    Ok(empty_support_access(timestamp))
}

pub async fn create_support_access(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
    Json(payload): Json<SupportAccessRequest>,
) -> ApiResult<Json<SupportAccessResponse>> {
    require_cloud_admin(&config, &user)?;
    let reason = normalize_reason(&payload.reason)?;
    let email = payload.email.trim().to_string();
    if email.is_empty() || email.len() > 254 || !email.contains('@') {
        return Err(ApiError::bad_request(
            "invalid_email_format",
            "Invalid email format.",
        ));
    }
    let within_limit = app_services
        .metadata_db
        .check_auth_rate_limit(
            "admin_support",
            &user.user_id,
            SUPPORT_LOOKUP_MAX_ATTEMPTS,
            SUPPORT_LOOKUP_WINDOW_MINUTES,
        )
        .await
        .map_err(|_| support_access_unavailable())?;
    if !within_limit {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "support_access_rate_limited",
            "Too many support lookups. Try again in a few minutes.",
        ));
    }
    let target = lookup_customer(&app_services, &email).await?;
    if target.is_admin || target.is_demo || target.id == user.user_id {
        return Err(ApiError::bad_request(
            "support_target_invalid",
            "Support access is only for customer accounts.",
        ));
    }
    let remaining_mfa_seconds = app_services
        .metadata_db
        .remaining_admin_mfa_seconds(&user.user_id, MAX_ADMIN_SESSION_AGE_SECONDS)
        .await
        .map_err(|_| support_access_unavailable())?;
    let Some(ttl) = remaining_mfa_seconds.filter(|ttl| *ttl > 0) else {
        return Err(ApiError::unauthorized(
            "admin_reauthentication_required",
            "Sign in again with your password and authenticator code.",
        ));
    };
    let grant = app_services
        .metadata_db
        .grant_admin_support_access(&user.user_id, &target.id, &reason, ttl)
        .await
        .or_internal("Failed to grant support access")?;
    Ok(Json(view_for_grant(&app_services, grant).await?))
}

pub async fn revoke_support_access(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
) -> ApiResult<Json<SupportAccessResponse>> {
    require_cloud_admin(&config, &user)?;
    app_services
        .metadata_db
        .revoke_admin_support_grants(&user.user_id)
        .await
        .or_internal("Failed to end support access")?;
    Ok(empty_support_access(current_unix_timestamp().unwrap_or(0)))
}
