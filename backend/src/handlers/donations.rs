//! `BTCPay` donation handlers that redirect the browser to a checkout page.
//!
//! These endpoints are opened by browser navigation, not by the API client,
//! so their errors are plain-text bodies rather than JSON [`super::ApiError`]s.

use crate::api::BtcPayClientState;
use crate::config::AppConfig;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

/// A plain-text error response for a donation redirect.
type DonationError = (StatusCode, &'static str);

const BTCPAY_NOT_CONFIGURED: DonationError =
    (StatusCode::SERVICE_UNAVAILABLE, "BTCPay not configured");

pub(crate) fn redirect_response(url: &str) -> Result<Response, DonationError> {
    let location = url.parse().map_err(|e| {
        tracing::error!("BTCPay returned invalid redirect URL: {e}");
        (
            StatusCode::BAD_GATEWAY,
            "Invalid redirect URL from payment server",
        )
    })?;
    let mut headers = HeaderMap::new();
    headers.insert("Location", location);
    Ok((StatusCode::FOUND, headers, "").into_response())
}

pub(crate) fn thank_you_url(config: &AppConfig) -> String {
    let frontend_url = config.frontend_url().unwrap_or("https://canarybitcoin.com");
    format!("{}/donations/thank-you", frontend_url.trim_end_matches('/'))
}

pub async fn donate_one_time(
    State(btcpay): State<BtcPayClientState>,
    State(config): State<crate::api::ConfigState>,
) -> Result<Response, DonationError> {
    let client = btcpay.as_ref().ok_or(BTCPAY_NOT_CONFIGURED)?;
    let checkout_link = client
        .create_invoice(&thank_you_url(&config))
        .await
        .map_err(|e| {
            tracing::error!("Failed to create BTCPay invoice: {e}");
            (StatusCode::BAD_GATEWAY, "Failed to create invoice")
        })?;
    redirect_response(&checkout_link)
}

pub async fn donate_recurring(
    State(btcpay): State<BtcPayClientState>,
    State(config): State<crate::api::ConfigState>,
) -> Result<Response, DonationError> {
    if !config.is_btcpay_recurring_enabled() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Recurring BTCPay donations are not configured",
        ));
    }

    let client = btcpay.as_ref().ok_or(BTCPAY_NOT_CONFIGURED)?;
    let checkout_url = client
        .create_plan_checkout(&thank_you_url(&config))
        .await
        .map_err(|e| {
            tracing::error!("Failed to create BTCPay plan checkout: {e}");
            (StatusCode::BAD_GATEWAY, "Failed to create plan checkout")
        })?;
    redirect_response(&checkout_url)
}
