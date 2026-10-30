//! Starting a subscription checkout with the active provider, opening the
//! Stripe customer portal, and looking up a checkout session afterwards.

use super::{active_billing_provider, btcpay_client, current_user_record, stripe_client};
use crate::api::{AppServicesState, BtcPayClientState, ConfigState, StripeBillingState};
use crate::config::BillingProvider;
use crate::extractors::AuthenticatedUser;
use crate::handlers::{ApiError, ApiResult};
use crate::metadata::SubscriptionStatus;
use crate::models::{CreateCheckoutSessionRequest, CreateCustomerPortalRequest};
use crate::stripe_billing::{
    CheckoutSessionDetails, CheckoutSessionResponse, CustomerPortalResponse, StripeBilling,
};
use crate::subscription::SubscriptionTier;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use tracing::info;
use uuid::Uuid;

fn btcpay_checkout_allowed(subscription_status: SubscriptionStatus) -> bool {
    !matches!(
        subscription_status,
        SubscriptionStatus::Active | SubscriptionStatus::PastDue | SubscriptionStatus::Canceled
    )
}

/// Create a billing checkout session for subscription
pub async fn create_checkout_session(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(stripe_billing): State<StripeBillingState>,
    State(btcpay): State<BtcPayClientState>,
    State(config): State<ConfigState>,
    Json(payload): Json<CreateCheckoutSessionRequest>,
) -> ApiResult<Json<CheckoutSessionResponse>> {
    let start_time = std::time::Instant::now();

    let tier = match payload.tier.as_str() {
        "personal" => SubscriptionTier::Personal,
        "team" => SubscriptionTier::Team,
        _ => {
            return Err(ApiError::bad_request(
                "invalid_subscription_tier",
                "Invalid subscription tier",
            ));
        }
    };

    let user_record = current_user_record(&app_services, &user.user_id).await?;

    let is_yearly = payload.is_yearly.unwrap_or(false);
    let billing_cycle = if is_yearly { "yearly" } else { "monthly" };
    let frontend_url = config
        .frontend_url()
        .ok_or_else(|| ApiError::opaque("FRONTEND_URL not configured"))?;
    let success_url =
        format!("{frontend_url}/subscription?success=true&session={{CHECKOUT_SESSION_ID}}");
    let cancel_url = format!("{frontend_url}/subscription?cancelled=true");

    let result = match active_billing_provider(&config, &stripe_billing, &btcpay) {
        Some(BillingProvider::Stripe) => {
            stripe_client(&stripe_billing)?
                .create_checkout_session(
                    &user_record.id,
                    tier,
                    billing_cycle,
                    &success_url,
                    &cancel_url,
                    &app_services.metadata_db,
                )
                .await
        }
        Some(BillingProvider::BtcPay) => {
            if !btcpay_checkout_allowed(user_record.subscription_status) {
                return Err(ApiError::conflict(
                    "billing_management_unavailable",
                    "Manage the existing BTCPay subscription before starting a new checkout",
                ));
            }
            if is_yearly {
                return Err(ApiError::bad_request(
                    "unsupported_billing_period",
                    "BTCPay cloud billing currently supports monthly plans only",
                ));
            }
            let btcpay = btcpay_client(&btcpay)?;

            let checkout_token = Uuid::new_v4().to_string();
            app_services
                .metadata_db
                .create_pending_billing_checkout(
                    &checkout_token,
                    &user_record.id,
                    "btcpay",
                    tier.as_str(),
                    billing_cycle,
                )
                .await
                .map_err(|_| {
                    tracing::error!("Failed to persist checkout correlation");
                    ApiError::opaque("Failed to initialize checkout session")
                })?;

            let redirect_url = format!(
                "{frontend_url}/subscription?success=true&provider=btcpay&session={checkout_token}"
            );

            btcpay
                .create_cloud_subscription_checkout(
                    tier,
                    &redirect_url,
                    &checkout_token,
                    &user_record.email,
                )
                .await
                .map(|url| CheckoutSessionResponse {
                    url,
                    session_id: checkout_token,
                })
        }
        None => Err(anyhow::anyhow!("No billing provider configured")),
    };

    info!(
        "create_checkout_session completed in {:?}",
        start_time.elapsed()
    );

    result.map(Json).map_err(|_| {
        tracing::error!("Checkout session creation failed");
        ApiError::opaque("Failed to create checkout session")
    })
}

/// Create a customer billing management session when the provider supports it
pub async fn create_customer_portal(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(stripe_billing): State<StripeBillingState>,
    State(config): State<ConfigState>,
    Json(payload): Json<CreateCustomerPortalRequest>,
) -> ApiResult<Json<CustomerPortalResponse>> {
    let start_time = std::time::Instant::now();

    if config.active_billing_provider() != Some(BillingProvider::Stripe) || stripe_billing.is_none()
    {
        return Err(ApiError::bad_request(
            "billing_management_unavailable",
            "The active billing provider does not support in-app subscription management yet",
        ));
    }

    let user_record = current_user_record(&app_services, &user.user_id).await?;

    let Some(customer_id) = &user_record.stripe_customer_id else {
        return Err(ApiError::bad_request(
            "no_stripe_customer",
            "No Stripe customer found. Please create a subscription first.",
        ));
    };

    let stripe_billing = stripe_client(&stripe_billing)?;

    tracing::info!("Creating customer portal session");
    let result = stripe_billing
        .create_customer_portal_session(customer_id, &payload.return_url)
        .await;
    if result.is_ok() {
        tracing::info!("Customer portal session created");
    } else {
        tracing::error!("Customer portal session creation failed");
    }
    info!(
        "create_stripe_customer_portal completed in {:?}",
        start_time.elapsed()
    );

    result
        .map(Json)
        .map_err(|_| ApiError::opaque("Failed to create customer portal session"))
}

fn session_not_found() -> ApiError {
    ApiError::uncoded(StatusCode::NOT_FOUND, "Session not found")
}

/// Get checkout session details
pub async fn get_checkout_session_details(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(stripe_billing): State<StripeBillingState>,
    State(btcpay): State<BtcPayClientState>,
    State(config): State<ConfigState>,
    Path(session_id): Path<String>,
) -> ApiResult<Json<CheckoutSessionDetails>> {
    match active_billing_provider(&config, &stripe_billing, &btcpay) {
        Some(BillingProvider::BtcPay) => {
            btcpay_checkout_session_details(&app_services, &user.user_id, session_id).await
        }
        Some(BillingProvider::Stripe) => {
            stripe_checkout_session_details(
                &app_services,
                stripe_client(&stripe_billing)?,
                &user.user_id,
                &session_id,
            )
            .await
        }
        None => Err(ApiError::bad_request(
            "checkout_session_unsupported",
            "Checkout session lookups are only available for Stripe",
        )),
    }
    .map(Json)
}

async fn btcpay_checkout_session_details(
    app_services: &AppServicesState,
    user_id: &str,
    session_id: String,
) -> ApiResult<CheckoutSessionDetails> {
    let checkout = app_services
        .metadata_db
        .get_pending_billing_checkout(&session_id)
        .await
        .map_err(|_| {
            tracing::error!("Checkout session lookup failed");
            ApiError::opaque("Failed to load checkout session")
        })?
        .filter(|checkout| checkout.user_id == user_id && checkout.provider == "btcpay")
        .ok_or_else(session_not_found)?;

    let status = if checkout.completed_at.is_some() {
        "complete"
    } else {
        "pending"
    };
    Ok(CheckoutSessionDetails {
        session_id,
        customer_id: None,
        subscription_id: None,
        status: Some(status.to_string()),
        tier: Some(checkout.subscription_tier),
        billing_period: Some(checkout.billing_period),
        amount_total: None,
        currency: None,
    })
}

async fn stripe_checkout_session_details(
    app_services: &AppServicesState,
    stripe_billing: &StripeBilling,
    user_id: &str,
    session_id: &str,
) -> ApiResult<CheckoutSessionDetails> {
    let details = stripe_billing
        .get_checkout_session_details(session_id)
        .await
        .map_err(|_| session_not_found())?;
    let Some(customer_id) = details.customer_id.as_deref() else {
        return Err(session_not_found());
    };
    let owns_session = matches!(
        app_services.metadata_db.get_user_by_id(user_id).await,
        Ok(Some(current_user)) if current_user.stripe_customer_id.as_deref() == Some(customer_id)
    );
    if owns_session {
        Ok(details)
    } else {
        Err(session_not_found())
    }
}

pub async fn create_stripe_checkout_session(
    authenticated_user: AuthenticatedUser,
    app_services: State<AppServicesState>,
    stripe_billing: State<StripeBillingState>,
    btcpay: State<BtcPayClientState>,
    config: State<ConfigState>,
    payload: Json<CreateCheckoutSessionRequest>,
) -> ApiResult<Json<CheckoutSessionResponse>> {
    create_checkout_session(
        authenticated_user,
        app_services,
        stripe_billing,
        btcpay,
        config,
        payload,
    )
    .await
}

pub async fn create_stripe_customer_portal(
    authenticated_user: AuthenticatedUser,
    app_services: State<AppServicesState>,
    stripe_billing: State<StripeBillingState>,
    config: State<ConfigState>,
    payload: Json<CreateCustomerPortalRequest>,
) -> ApiResult<Json<CustomerPortalResponse>> {
    create_customer_portal(
        authenticated_user,
        app_services,
        stripe_billing,
        config,
        payload,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn btcpay_checkout_rejects_non_terminal_subscriptions() {
        assert!(!btcpay_checkout_allowed(SubscriptionStatus::Active));
        assert!(!btcpay_checkout_allowed(SubscriptionStatus::PastDue));
        assert!(!btcpay_checkout_allowed(SubscriptionStatus::Canceled));
        assert!(btcpay_checkout_allowed(SubscriptionStatus::Expired));
        assert!(btcpay_checkout_allowed(SubscriptionStatus::Trialing));
        assert!(btcpay_checkout_allowed(SubscriptionStatus::Pending));
    }
}
