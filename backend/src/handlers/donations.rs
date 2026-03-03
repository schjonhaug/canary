use crate::api::BtcPayClientState;
use crate::config::AppConfig;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

pub(crate) fn redirect_response(url: &str) -> Response {
    let mut headers = HeaderMap::new();
    match url.parse() {
        Ok(value) => {
            headers.insert("Location", value);
            (StatusCode::FOUND, headers, "").into_response()
        }
        Err(e) => {
            tracing::error!("BTCPay returned invalid redirect URL: {}", e);
            (StatusCode::BAD_GATEWAY, "Invalid redirect URL from payment server").into_response()
        }
    }
}

pub(crate) fn thank_you_url(config: &AppConfig) -> String {
    let frontend_url = config.frontend_url().unwrap_or("https://canarybitcoin.com");
    format!("{}/donations/thank-you", frontend_url.trim_end_matches('/'))
}

pub async fn donate_one_time(
    State(btcpay): State<BtcPayClientState>,
    State(config): State<crate::api::ConfigState>,
) -> Response {
    let client = match &btcpay {
        Some(c) => c,
        None => {
            return (StatusCode::SERVICE_UNAVAILABLE, "BTCPay not configured").into_response();
        }
    };

    let redirect_url = thank_you_url(&config);

    match client.create_invoice(&redirect_url).await {
        Ok(checkout_link) => redirect_response(&checkout_link),
        Err(e) => {
            tracing::error!("Failed to create BTCPay invoice: {}", e);
            (StatusCode::BAD_GATEWAY, "Failed to create invoice").into_response()
        }
    }
}

pub async fn donate_recurring(
    State(btcpay): State<BtcPayClientState>,
    State(config): State<crate::api::ConfigState>,
) -> Response {
    let client = match &btcpay {
        Some(c) => c,
        None => {
            return (StatusCode::SERVICE_UNAVAILABLE, "BTCPay not configured").into_response();
        }
    };

    let redirect_url = thank_you_url(&config);

    match client.create_plan_checkout(&redirect_url).await {
        Ok(checkout_url) => redirect_response(&checkout_url),
        Err(e) => {
            tracing::error!("Failed to create BTCPay plan checkout: {}", e);
            (StatusCode::BAD_GATEWAY, "Failed to create plan checkout").into_response()
        }
    }
}
