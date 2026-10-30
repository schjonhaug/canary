//! Billing and subscription management handlers.
//!
//! - `checkout`: starting a checkout, the Stripe customer portal and
//!   checkout session lookups
//! - `status`: the signed-in user's billing status and the public price list
//! - `stripe_webhook`: Stripe webhook events
//! - `btcpay_webhook`: `BTCPay` subscription webhook events

mod btcpay_webhook;
mod checkout;
mod status;
mod stripe_webhook;

pub use btcpay_webhook::handle_btcpay_webhook;
pub use checkout::{
    create_checkout_session, create_customer_portal, create_stripe_checkout_session,
    create_stripe_customer_portal, get_checkout_session_details,
};
pub use status::{get_billing_pricing, get_billing_status};
pub use stripe_webhook::handle_stripe_webhook;

use crate::api::{AppServicesState, BtcPayClientState, StripeBillingState};
use crate::btcpay_client::BtcPayClient;
use crate::config::BillingProvider;
use crate::handlers::helpers::get_user_or_error;
use crate::handlers::{ApiError, ApiResult};
use crate::stripe_billing::StripeBilling;
use axum::http::StatusCode;

/// Body Stripe and `BTCPay` expect on a webhook they should not retry.
const WEBHOOK_ACK: &str = "OK";
const WEBHOOK_STATE_UPDATE_FAILED: &str = "Webhook state update failed";

/// A 500 with a fixed message whose cause is not logged here.
///
fn bad_request(message: impl Into<std::borrow::Cow<'static, str>>) -> ApiError {
    ApiError::uncoded(StatusCode::BAD_REQUEST, message)
}

fn service_unavailable(message: &'static str) -> ApiError {
    ApiError::uncoded(StatusCode::SERVICE_UNAVAILABLE, message)
}

fn stripe_client(stripe_billing: &StripeBillingState) -> ApiResult<&StripeBilling> {
    stripe_billing
        .as_deref()
        .ok_or_else(|| ApiError::opaque("Stripe billing not initialized"))
}

fn btcpay_client(btcpay: &BtcPayClientState) -> ApiResult<&BtcPayClient> {
    btcpay
        .as_deref()
        .ok_or_else(|| ApiError::opaque("BTCPay billing not initialized"))
}

async fn current_user_record(
    app_services: &AppServicesState,
    user_id: &str,
) -> ApiResult<crate::metadata::UserRecord> {
    get_user_or_error(
        app_services,
        user_id,
        None,
        "User not found",
        "Database error",
    )
    .await
}

fn active_billing_provider(
    config: &crate::config::AppConfig,
    stripe_billing: &StripeBillingState,
    btcpay: &BtcPayClientState,
) -> Option<BillingProvider> {
    match config.active_billing_provider() {
        Some(BillingProvider::Stripe) if stripe_billing.is_some() => Some(BillingProvider::Stripe),
        Some(BillingProvider::BtcPay) if btcpay.is_some() => Some(BillingProvider::BtcPay),
        _ => None,
    }
}
