//! The signed-in user's subscription, usage and tier limits, and the
//! public price list of the active billing provider.

use super::{
    active_billing_provider, btcpay_client, current_user_record, service_unavailable, stripe_client,
};
use crate::api::{AppServicesState, BtcPayClientState, ConfigState, StripeBillingState};
use crate::config::BillingProvider;
use crate::extractors::AuthenticatedUser;
use crate::handlers::{ApiResult, OrInternal};
use crate::models::{BillingStatusResponse, BillingTierLimits};
use crate::stripe_billing::PricingInfo;
use axum::{extract::State, response::Json};
use std::sync::Arc;
use tracing::info;

/// Get billing status for the authenticated user
pub async fn get_billing_status(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(stripe_billing): State<StripeBillingState>,
    State(btcpay): State<BtcPayClientState>,
    State(config): State<Arc<crate::config::AppConfig>>,
) -> ApiResult<Json<BillingStatusResponse>> {
    let start_time = std::time::Instant::now();

    let user_record = current_user_record(&app_services, &user.user_id).await?;

    let wallet_count = app_services
        .metadata_db
        .count_wallets_for_user(&user.user_id)
        .await
        .or_internal("Database error")?;

    let user_wallets = app_services
        .metadata_db
        .get_wallets_for_user(Some(&user.user_id))
        .await
        .or_internal("Database error")?;
    let mut contact_count = 0;
    for wallet in &user_wallets {
        contact_count += app_services
            .metadata_db
            .count_contacts_for_wallet(&wallet.checksum)
            .await
            .or_internal("Database error")?;
    }

    // Report the network-specific sync interval rather than the tier default.
    let tier_limits = user_record.subscription_tier.limits_for_api();
    let actual_sync_interval = config
        .tier_sync_intervals()
        .for_tier(user_record.subscription_tier);

    let limits = BillingTierLimits {
        max_wallets: tier_limits
            .max_wallets
            .map_or(-1, |n| i32::try_from(n).unwrap_or(i32::MAX)),
        max_contacts_per_wallet: tier_limits
            .max_contacts_per_wallet
            .map_or(-1, |n| i32::try_from(n).unwrap_or(i32::MAX)),
        sync_interval_seconds: actual_sync_interval,
    };

    let effective_subscription_status = crate::subscription::get_effective_subscription_status(
        user_record.subscription_status,
        user_record.trial_ends_at.as_deref(),
    );

    let billing_provider = active_billing_provider(&config, &stripe_billing, &btcpay);
    let response = BillingStatusResponse {
        user_id: user.user_id.clone(),
        subscription_tier: user_record.subscription_tier.as_str().to_string(),
        subscription_status: effective_subscription_status,
        trial_ends_at: user_record.trial_ends_at,
        subscription_started_at: user_record.subscription_started_at,
        subscription_ends_at: user_record.subscription_ends_at,
        billing_provider: billing_provider.map(|provider| provider.as_str().to_string()),
        can_manage_billing: billing_provider == Some(BillingProvider::Stripe)
            && user_record.stripe_customer_id.is_some(),
        stripe_customer_id: user_record.stripe_customer_id,
        wallet_count,
        contact_count,
        limits,
    };

    info!("get_billing_status completed in {:?}", start_time.elapsed());

    Ok(Json(response))
}

/// Get pricing information from the active billing provider (no auth required)
pub async fn get_billing_pricing(
    State(stripe_billing): State<StripeBillingState>,
    State(btcpay): State<BtcPayClientState>,
    State(config): State<ConfigState>,
) -> ApiResult<Json<PricingInfo>> {
    let pricing = match active_billing_provider(&config, &stripe_billing, &btcpay) {
        Some(BillingProvider::Stripe) => stripe_client(&stripe_billing)?.get_pricing_for_frontend(),
        Some(BillingProvider::BtcPay) => btcpay_client(&btcpay)?
            .get_cloud_pricing_for_frontend()
            .or_internal("Failed to load BTCPay pricing")?,
        None => return Err(service_unavailable("No billing provider configured")),
    };
    Ok(Json(pricing))
}
