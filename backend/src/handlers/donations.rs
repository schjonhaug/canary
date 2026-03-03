use crate::api::ConfigState;
use crate::btcpay_client::BtcPayClient;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

fn build_btcpay_client(config: &crate::config::AppConfig) -> Option<BtcPayClient> {
    if !config.is_btcpay_enabled() {
        return None;
    }
    Some(BtcPayClient::new(
        config.btcpay_url().unwrap().to_string(),
        config.btcpay_api_key().unwrap().to_string(),
        config.btcpay_store_id().unwrap().to_string(),
        config.btcpay_offering_id().map(|s| s.to_string()),
        config.btcpay_plan_id().map(|s| s.to_string()),
    ))
}

fn redirect_response(url: &str) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert("Location", url.parse().unwrap());
    (StatusCode::FOUND, headers, "").into_response()
}

fn thank_you_url(config: &crate::config::AppConfig) -> String {
    let frontend_url = config.frontend_url().unwrap_or("https://canarybitcoin.com");
    format!("{}/donations/thank-you", frontend_url.trim_end_matches('/'))
}

pub async fn donate_one_time(State(config): State<ConfigState>) -> Response {
    let client = match build_btcpay_client(&config) {
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

pub async fn donate_recurring(State(config): State<ConfigState>) -> Response {
    let client = match build_btcpay_client(&config) {
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
