//! Balance alert handlers

use crate::api::AppServicesState;
use crate::exchange_rates;
use crate::extractors::{require_non_demo, AuthenticatedUser};
use crate::handlers::helpers::verify_wallet_access;
use crate::metadata::BalanceAlertType;
use crate::models::{BalanceAlertsResponse, CreateBalanceAlertRequest, ErrorResponse};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};

/// Get all balance alerts for a wallet
pub async fn get_wallet_balance_alerts(
    AuthenticatedUser(user): AuthenticatedUser,
    Path(checksum): Path<String>,
    State(app_services): State<AppServicesState>,
) -> Response {
    // Check if wallet exists and user has access
    let _wallet = match verify_wallet_access(&app_services, &user, &checksum).await {
        Ok(wallet) => wallet,
        Err(response) => return response,
    };

    // Get all balance alerts for the wallet
    match app_services
        .metadata_db
        .get_all_balance_alerts_for_wallet(&checksum)
        .await
    {
        Ok(alerts) => Json(BalanceAlertsResponse { alerts }).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(format!(
                "Failed to get balance alerts: {}",
                e
            ))),
        )
            .into_response(),
    }
}

/// Create a new balance alert for a wallet
pub async fn create_wallet_balance_alert(
    AuthenticatedUser(user): AuthenticatedUser,
    Path(checksum): Path<String>,
    State(app_services): State<AppServicesState>,
    Json(request): Json<CreateBalanceAlertRequest>,
) -> Response {
    // Reject demo users from creating balance alerts
    if let Err(response) = require_non_demo(&user) {
        return response;
    }

    // Check if wallet exists and user has access
    let wallet = match verify_wallet_access(&app_services, &user, &checksum).await {
        Ok(wallet) => wallet,
        Err(response) => return response,
    };

    // Validate threshold type: exactly one must be provided (BTC OR fiat, not both or neither)
    let is_btc_threshold = request.threshold_sats.is_some();
    let is_fiat_threshold =
        request.threshold_currency.is_some() && request.threshold_fiat_amount.is_some();

    if is_btc_threshold == is_fiat_threshold {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::coded("invalid_threshold_config", "Exactly one threshold type must be provided: either threshold_sats (BTC) OR threshold_currency + threshold_fiat_amount (fiat)")),
        )
            .into_response();
    }

    // Determine threshold_sats based on threshold type
    let (threshold_sats, threshold_currency, threshold_fiat_amount) = if is_btc_threshold {
        // BTC threshold
        let sats = request.threshold_sats.unwrap();

        // Validate BTC threshold
        if sats < 0 {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::coded(
                    "negative_btc_threshold",
                    "BTC threshold must be non-negative",
                )),
            )
                .into_response();
        }

        // Validate "below 0" alert (logically impossible)
        if request.alert_type == BalanceAlertType::Below && sats == 0 {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::coded(
                    "below_zero_alert",
                    "Cannot create alert for 'below 0' - balance cannot go below zero",
                )),
            )
                .into_response();
        }

        (sats, None, None)
    } else {
        // Fiat threshold
        let currency = request.threshold_currency.unwrap();
        let fiat_amount = request.threshold_fiat_amount.unwrap();

        // Validate fiat amount
        if fiat_amount <= 0.0 {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::coded(
                    "negative_fiat_threshold",
                    "Fiat threshold amount must be positive",
                )),
            )
                .into_response();
        }

        // Validate currency is supported
        if !exchange_rates::SUPPORTED_CURRENCIES.contains(&currency.as_str()) {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::coded(
                    "unsupported_currency",
                    format!(
                        "Unsupported currency: {}. Supported currencies: {}",
                        currency,
                        exchange_rates::SUPPORTED_CURRENCIES.join(", ")
                    ),
                )),
            )
                .into_response();
        }

        // Get current exchange rate
        let exchange_rates_map = match app_services.metadata_db.get_exchange_rates().await {
            Ok(rates) => rates,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(format!(
                        "Failed to fetch exchange rates: {}",
                        e
                    ))),
                )
                    .into_response();
            }
        };

        let rate = match exchange_rates_map.get(&currency) {
            Some(rate) => rate.rate_per_btc,
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(ErrorResponse::new(format!(
                        "Exchange rate for {} is currently unavailable",
                        currency
                    ))),
                )
                    .into_response();
            }
        };

        // Convert fiat amount to satoshis: fiat_amount / rate_per_btc * 100_000_000
        let btc_amount = fiat_amount / rate;
        let threshold_sats = (btc_amount * 100_000_000.0) as i64;

        if threshold_sats <= 0 {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::coded(
                    "fiat_amount_too_small",
                    format!(
                        "Fiat amount {} {} is too small to convert to satoshis",
                        fiat_amount, currency
                    ),
                )),
            )
                .into_response();
        }

        (threshold_sats, Some(currency), Some(fiat_amount))
    };

    // Check for duplicate balance alert
    match app_services
        .metadata_db
        .check_duplicate_balance_alert(&checksum, threshold_sats, request.alert_type)
        .await
    {
        Ok(Some(_existing_alert)) => {
            return (
                StatusCode::CONFLICT,
                Json(ErrorResponse::coded(
                    "duplicate_alert",
                    "An alert with this type and threshold already exists",
                )),
            )
                .into_response();
        }
        Ok(None) => {
            // No duplicate, continue with creation
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to check for duplicate alert: {}",
                    e
                ))),
            )
                .into_response();
        }
    }

    // Check if alert would trigger immediately based on current balance
    if let Some(current_balance_sats) = wallet.balance_total {
        // Determine if alert would trigger
        let would_trigger = if is_fiat_threshold {
            // For fiat alerts, convert current balance to fiat and compare
            let currency = threshold_currency.as_ref().unwrap();
            let fiat_threshold = threshold_fiat_amount.unwrap();

            // Get exchange rate (we already fetched it above for fiat alerts)
            if let Ok(exchange_rates_map) = app_services.metadata_db.get_exchange_rates().await {
                if let Some(rate) = exchange_rates_map.get(currency) {
                    let balance_btc = current_balance_sats as f64 / 100_000_000.0;
                    let balance_fiat = balance_btc * rate.rate_per_btc;

                    match request.alert_type {
                        BalanceAlertType::Above => balance_fiat > fiat_threshold,
                        BalanceAlertType::Below => balance_fiat < fiat_threshold,
                        BalanceAlertType::Equals => (balance_fiat - fiat_threshold).abs() < 0.01,
                    }
                } else {
                    false // Rate not available, skip check
                }
            } else {
                false // Rates not available, skip check
            }
        } else {
            // For BTC alerts, compare satoshis directly
            match request.alert_type {
                BalanceAlertType::Above => current_balance_sats > threshold_sats,
                BalanceAlertType::Below => current_balance_sats < threshold_sats,
                BalanceAlertType::Equals => current_balance_sats == threshold_sats,
            }
        };

        if would_trigger {
            // Build helpful error message
            let error_msg = if is_fiat_threshold {
                let currency = threshold_currency.as_ref().unwrap();
                let fiat_threshold = threshold_fiat_amount.unwrap();

                // Calculate current balance in fiat
                if let Ok(exchange_rates_map) = app_services.metadata_db.get_exchange_rates().await
                {
                    if let Some(rate) = exchange_rates_map.get(currency) {
                        let balance_btc = current_balance_sats as f64 / 100_000_000.0;
                        let balance_fiat = balance_btc * rate.rate_per_btc;

                        format!(
                            "Alert would trigger immediately. Current balance: {:.2} {}, threshold: {:.2} {} ({}). Try a different threshold or alert type.",
                            balance_fiat, currency, fiat_threshold, currency, request.alert_type.as_str()
                        )
                    } else {
                        "Alert would trigger immediately based on current balance. Try a different threshold or alert type.".to_string()
                    }
                } else {
                    "Alert would trigger immediately based on current balance. Try a different threshold or alert type.".to_string()
                }
            } else {
                let balance_btc = current_balance_sats as f64 / 100_000_000.0;
                let threshold_btc = threshold_sats as f64 / 100_000_000.0;

                format!(
                    "Alert would trigger immediately. Current balance: {:.8} BTC, threshold: {:.8} BTC ({}). Try a different threshold or alert type.",
                    balance_btc, threshold_btc, request.alert_type.as_str()
                )
            };

            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::coded(
                    "alert_would_trigger_immediately",
                    error_msg,
                )),
            )
                .into_response();
        }
    }

    // Create the balance alert with current balance for threshold crossing detection
    match app_services
        .metadata_db
        .create_balance_alert(
            &checksum,
            threshold_sats,
            request.alert_type,
            threshold_currency,
            threshold_fiat_amount,
            wallet.balance_total,
        )
        .await
    {
        Ok(alert) => Json(alert).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(format!(
                "Failed to create balance alert: {}",
                e
            ))),
        )
            .into_response(),
    }
}

/// Delete a balance alert
pub async fn delete_balance_alert(
    AuthenticatedUser(user): AuthenticatedUser,
    Path(alert_id): Path<String>,
    State(app_services): State<AppServicesState>,
) -> Response {
    // Reject demo users from deleting balance alerts
    if let Err(response) = require_non_demo(&user) {
        return response;
    }

    // Get the alert first to verify ownership
    let alert = match app_services
        .metadata_db
        .get_balance_alert_by_id(&alert_id)
        .await
    {
        Ok(Some(alert)) => alert,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::coded(
                    "alert_not_found",
                    "Balance alert not found",
                )),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!("Database error: {}", e))),
            )
                .into_response();
        }
    };

    // Verify user owns the wallet containing this alert (unless admin)
    if !user.is_admin {
        match app_services
            .metadata_db
            .is_wallet_owned_by_user(&alert.wallet_checksum, &user.user_id)
            .await
        {
            Ok(true) => {} // User owns the wallet, proceed
            Ok(false) => {
                return (
                    StatusCode::FORBIDDEN,
                    Json(ErrorResponse::coded("access_denied", "Access denied")),
                )
                    .into_response();
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(format!("Database error: {}", e))),
                )
                    .into_response();
            }
        }
    }

    // Delete the balance alert
    match app_services
        .metadata_db
        .delete_balance_alert(&alert_id)
        .await
    {
        Ok(()) => (StatusCode::OK, "Balance alert deleted").into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(format!(
                "Failed to delete balance alert: {}",
                e
            ))),
        )
            .into_response(),
    }
}
