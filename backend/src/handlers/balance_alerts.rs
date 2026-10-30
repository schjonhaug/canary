//! Balance alert handlers

use super::error::{ApiError, ApiResult, OrInternal};
use crate::api::AppServicesState;
use crate::exchange_rates;
use crate::extractors::{AuthenticatedUser, require_non_demo};
use crate::handlers::helpers::{verify_wallet_access, verify_wallet_read_access};
use crate::metadata::{BalanceAlert, BalanceAlertType, CreateBalanceAlertInput};
use crate::models::{BalanceAlertsResponse, CreateBalanceAlertRequest};
use crate::utils::amount::{btc_to_sats, sats_to_btc};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};

struct ValidatedBalanceAlert {
    threshold_sats: i64,
    threshold_currency: Option<String>,
    threshold_fiat_amount: Option<f64>,
}

/// A fiat threshold together with the exchange rate used to convert it.
struct FiatThreshold {
    currency: String,
    amount: f64,
    rate_per_btc: f64,
}

/// Get all balance alerts for a wallet
pub async fn get_wallet_balance_alerts(
    AuthenticatedUser(user): AuthenticatedUser,
    Path(checksum): Path<String>,
    State(app_services): State<AppServicesState>,
) -> ApiResult<Json<BalanceAlertsResponse>> {
    verify_wallet_read_access(&app_services, &user, &checksum, "Database error").await?;

    let alerts = app_services
        .metadata_db
        .get_all_balance_alerts_for_wallet(&checksum)
        .await
        .or_internal("Failed to get balance alerts")?;
    Ok(Json(BalanceAlertsResponse { alerts }))
}

/// Validate a balance alert for a wallet without creating it.
pub async fn validate_wallet_balance_alert(
    AuthenticatedUser(user): AuthenticatedUser,
    Path(checksum): Path<String>,
    State(app_services): State<AppServicesState>,
    Json(request): Json<CreateBalanceAlertRequest>,
) -> ApiResult<StatusCode> {
    require_non_demo(&user)?;
    let wallet = verify_wallet_access(&app_services, &user, &checksum, "Database error").await?;

    validate_balance_alert_request(
        &app_services,
        &checksum,
        wallet.balance_total,
        &request,
        false,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn validate_balance_alert_request(
    app_services: &AppServicesState,
    checksum: &str,
    current_balance_sats: Option<i64>,
    request: &CreateBalanceAlertRequest,
    check_duplicate: bool,
) -> ApiResult<ValidatedBalanceAlert> {
    let (threshold_sats, fiat) = resolve_threshold(app_services, request).await?;

    if check_duplicate {
        let duplicate = app_services
            .metadata_db
            .check_duplicate_balance_alert_for_contact(
                checksum,
                request.contact_id.as_deref(),
                threshold_sats,
                request.alert_type,
            )
            .await
            .or_internal("Failed to check for duplicate alert")?;
        if duplicate.is_some() {
            return Err(ApiError::conflict(
                "duplicate_alert",
                "An alert with this type and threshold already exists",
            ));
        }
    }

    if let Some(current_balance_sats) = current_balance_sats
        && let Some(message) = immediate_trigger_message(
            request.alert_type,
            current_balance_sats,
            threshold_sats,
            fiat.as_ref(),
        )
    {
        return Err(ApiError::bad_request(
            "alert_would_trigger_immediately",
            message,
        ));
    }

    let (threshold_currency, threshold_fiat_amount) = match fiat {
        Some(fiat) => (Some(fiat.currency), Some(fiat.amount)),
        None => (None, None),
    };
    Ok(ValidatedBalanceAlert {
        threshold_sats,
        threshold_currency,
        threshold_fiat_amount,
    })
}

/// Validates the requested threshold and converts it to satoshis.
///
/// Exactly one threshold type must be given: `threshold_sats` (BTC), or
/// `threshold_currency` together with `threshold_fiat_amount` (fiat).
async fn resolve_threshold(
    app_services: &AppServicesState,
    request: &CreateBalanceAlertRequest,
) -> ApiResult<(i64, Option<FiatThreshold>)> {
    match (
        request.threshold_sats,
        request.threshold_currency.as_deref(),
        request.threshold_fiat_amount,
    ) {
        (Some(sats), None, _) | (Some(sats), _, None) => {
            validate_btc_threshold(sats, request.alert_type)?;
            Ok((sats, None))
        }
        (None, Some(currency), Some(amount)) => {
            let fiat = resolve_fiat_threshold(app_services, currency, amount).await?;
            let threshold_sats = btc_to_sats(fiat.amount / fiat.rate_per_btc);
            if threshold_sats <= 0 {
                return Err(ApiError::bad_request(
                    "fiat_amount_too_small",
                    format!("Fiat amount {amount} {currency} is too small to convert to satoshis"),
                ));
            }
            Ok((threshold_sats, Some(fiat)))
        }
        _ => Err(ApiError::bad_request(
            "invalid_threshold_config",
            "Exactly one threshold type must be provided: either threshold_sats (BTC) OR threshold_currency + threshold_fiat_amount (fiat)",
        )),
    }
}

fn validate_btc_threshold(sats: i64, alert_type: BalanceAlertType) -> ApiResult<()> {
    if sats < 0 {
        return Err(ApiError::bad_request(
            "negative_btc_threshold",
            "BTC threshold must be non-negative",
        ));
    }
    // A balance can never go below zero.
    if alert_type == BalanceAlertType::Below && sats == 0 {
        return Err(ApiError::bad_request(
            "below_zero_alert",
            "Cannot create alert for 'below 0' - balance cannot go below zero",
        ));
    }
    Ok(())
}

async fn resolve_fiat_threshold(
    app_services: &AppServicesState,
    currency: &str,
    amount: f64,
) -> ApiResult<FiatThreshold> {
    if amount <= 0.0 {
        return Err(ApiError::bad_request(
            "negative_fiat_threshold",
            "Fiat threshold amount must be positive",
        ));
    }
    if !exchange_rates::SUPPORTED_CURRENCIES.contains(&currency) {
        return Err(ApiError::bad_request(
            "unsupported_currency",
            format!(
                "Unsupported currency: {currency}. Supported currencies: {}",
                exchange_rates::SUPPORTED_CURRENCIES.join(", ")
            ),
        ));
    }

    let rates = app_services
        .metadata_db
        .get_exchange_rates()
        .await
        .or_internal("Failed to fetch exchange rates")?;
    let Some(rate) = rates.get(currency) else {
        return Err(ApiError::uncoded(
            StatusCode::SERVICE_UNAVAILABLE,
            format!("Exchange rate for {currency} is currently unavailable"),
        ));
    };

    Ok(FiatThreshold {
        currency: currency.to_owned(),
        amount,
        rate_per_btc: rate.rate_per_btc,
    })
}

/// Returns an explanation when the alert would fire as soon as it is created.
fn immediate_trigger_message(
    alert_type: BalanceAlertType,
    current_balance_sats: i64,
    threshold_sats: i64,
    fiat: Option<&FiatThreshold>,
) -> Option<String> {
    let alert_type_name = alert_type.as_str();
    if let Some(fiat) = fiat {
        let balance_fiat = sats_to_btc(current_balance_sats) * fiat.rate_per_btc;
        let would_trigger = match alert_type {
            BalanceAlertType::Above => balance_fiat > fiat.amount,
            BalanceAlertType::Below => balance_fiat < fiat.amount,
            BalanceAlertType::Equals => (balance_fiat - fiat.amount).abs() < 0.01,
        };
        let FiatThreshold {
            currency, amount, ..
        } = fiat;
        would_trigger.then(|| {
            format!(
                "Alert would trigger immediately. Current balance: {balance_fiat:.2} {currency}, threshold: {amount:.2} {currency} ({alert_type_name}). Try a different threshold or alert type."
            )
        })
    } else {
        let would_trigger = match alert_type {
            BalanceAlertType::Above => current_balance_sats > threshold_sats,
            BalanceAlertType::Below => current_balance_sats < threshold_sats,
            BalanceAlertType::Equals => current_balance_sats == threshold_sats,
        };
        would_trigger.then(|| {
            let balance_btc = sats_to_btc(current_balance_sats);
            let threshold_btc = sats_to_btc(threshold_sats);
            format!(
                "Alert would trigger immediately. Current balance: {balance_btc:.8} BTC, threshold: {threshold_btc:.8} BTC ({alert_type_name}). Try a different threshold or alert type."
            )
        })
    }
}

/// Create a new balance alert for a wallet
pub async fn create_wallet_balance_alert(
    AuthenticatedUser(user): AuthenticatedUser,
    Path(checksum): Path<String>,
    State(app_services): State<AppServicesState>,
    Json(request): Json<CreateBalanceAlertRequest>,
) -> ApiResult<Json<BalanceAlert>> {
    require_non_demo(&user)?;
    let wallet = verify_wallet_access(&app_services, &user, &checksum, "Database error").await?;

    let Some(contact_id) = request.contact_id.as_deref() else {
        return Err(ApiError::bad_request(
            "contact_required",
            "Balance alerts must belong to a contact",
        ));
    };
    let contact = app_services
        .metadata_db
        .get_single_contact_with_methods(contact_id, &checksum)
        .await
        .or_internal("Failed to verify contact")?
        .ok_or_else(|| ApiError::not_found("contact_not_found", "Contact not found"))?;
    if !contact.is_active {
        return Err(ApiError::bad_request(
            "contact_inactive",
            "Cannot create a balance alert for an inactive contact",
        ));
    }

    let validated = validate_balance_alert_request(
        &app_services,
        &checksum,
        wallet.balance_total,
        &request,
        true,
    )
    .await?;

    // The current balance is stored for threshold crossing detection.
    let alert = app_services
        .metadata_db
        .create_balance_alert_with_contact(CreateBalanceAlertInput {
            wallet_checksum: &checksum,
            contact_id: Some(contact_id),
            threshold_sats: validated.threshold_sats,
            alert_type: request.alert_type,
            threshold_currency: validated.threshold_currency,
            threshold_fiat_amount: validated.threshold_fiat_amount,
            current_balance_sats: wallet.balance_total,
        })
        .await
        .or_internal("Failed to create balance alert")?;
    Ok(Json(alert))
}

/// Delete a balance alert
pub async fn delete_balance_alert(
    AuthenticatedUser(user): AuthenticatedUser,
    Path(alert_id): Path<String>,
    State(app_services): State<AppServicesState>,
) -> ApiResult<(StatusCode, &'static str)> {
    require_non_demo(&user)?;

    let alert = app_services
        .metadata_db
        .get_balance_alert_by_id(&alert_id)
        .await
        .or_internal("Database error")?
        .ok_or_else(|| ApiError::not_found("alert_not_found", "Balance alert not found"))?;

    // Administrative roles do not bypass wallet ownership.
    let owns_wallet = app_services
        .metadata_db
        .is_wallet_owned_by_user(&alert.wallet_checksum, &user.user_id)
        .await
        .or_internal("Database error")?;
    if !owns_wallet {
        return Err(ApiError::forbidden("access_denied", "Access denied"));
    }

    app_services
        .metadata_db
        .delete_balance_alert(&alert_id)
        .await
        .or_internal("Failed to delete balance alert")?;
    Ok((StatusCode::OK, "Balance alert deleted"))
}
