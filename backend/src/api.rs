use crate::config::AppConfig;
use crate::electrum::ElectrumClientManager;
use crate::handlers::{
    create_stripe_checkout_session, create_stripe_customer_portal, create_wallet_balance_alert,
    create_wallet_contact, create_wallet_non_blocking, delete_balance_alert, delete_wallet,
    delete_wallet_contact, demo_login, donate_one_time, donate_recurring, forgot_password,
    get_billing_pricing, get_billing_status, get_checkout_session_details, get_config,
    get_current_block_header, get_database_health, get_exchange_rates, get_providers,
    get_transaction_notifications, get_user_preferences, get_wallet, get_wallet_balance_alerts,
    get_wallet_contacts, get_wallet_detail, get_wallets_list, handle_stripe_webhook, login, logout,
    me, register, reset_password, run_integrity_check, send_contact_verification,
    send_test_ntfy_notification, submit_contact_form, update_user, update_user_preferences,
    update_wallet, update_wallet_contact, verify_contact, verify_email,
};
use crate::metadata::{MetadataDb, WalletsListResponse};
use crate::notifications::NotificationManager;
use crate::stripe_billing::StripeBilling;
use crate::utils::current_unix_timestamp;
use crate::wallet::WalletCreationService;
use axum::http::{HeaderName, HeaderValue, Method};
use axum::{
    extract::FromRef,
    routing::{get, post, put},
    Router,
};
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;

// New architecture: Separate web serving from wallet sync operations
pub struct AppServices {
    pub metadata_db: MetadataDb, // Fast access for web endpoints (no mutex needed)
    pub wallet_creation_service: WalletCreationService, // Non-blocking wallet creation
}

impl AppServices {
    /// Non-blocking version that only uses metadata database
    pub async fn get_wallets_list_for_user(
        &self,
        user_id: &str,
        is_admin: bool,
    ) -> Result<WalletsListResponse, anyhow::Error> {
        // Get current timestamp
        let timestamp = current_unix_timestamp()
            .map_err(|error| anyhow::anyhow!("system clock is before UNIX_EPOCH: {}", error))?;

        // Get wallets based on user permissions - directly from metadata DB
        let wallets = if is_admin {
            self.metadata_db.get_all_wallets().await?
        } else {
            self.metadata_db.get_wallets_for_user(Some(user_id)).await?
        };

        Ok(WalletsListResponse { timestamp, wallets })
    }

    /// Apply subscription tier limits by setting is_active status on wallets and contacts
    /// Non-blocking version that only uses metadata database (no wallet mutex)
    pub async fn apply_subscription_limits(
        &self,
        user_id: &str,
        tier: &str,
        subscription_status: &str,
        is_admin: bool,
        trial_ends_at: Option<String>,
        subscription_ends_at: Option<String>,
    ) -> Result<(), anyhow::Error> {
        // Check if subscription has expired or failed payment
        let is_subscription_active = crate::subscription::is_subscription_active(
            subscription_status,
            trial_ends_at.as_deref(),
            subscription_ends_at.as_deref(),
        );

        if is_admin {
            tracing::info!("🎯 Applying unlimited limits for admin user {}", user_id);
        } else if !is_subscription_active {
            tracing::info!(
                "🎯 Deactivating all wallets for user {} (status: {})",
                user_id,
                subscription_status
            );
        } else {
            tracing::info!(
                "🎯 Applying {} tier limits for user {} (status: {})",
                tier,
                user_id,
                subscription_status
            );
        }

        // Get all wallets for this user ordered by creation time (oldest first)
        let wallets = self
            .metadata_db
            .get_wallets_for_user_oldest_first(user_id)
            .await?;

        // Determine wallet limit based on subscription status, tier, and admin status
        let wallet_limit = if is_admin {
            usize::MAX // Unlimited for admin
        } else if !is_subscription_active {
            0 // No active wallets for inactive subscriptions (expired, past_due, or canceled with no remaining access)
        } else {
            match tier {
                "personal" => 1,
                "team" => 5,
                _ => 1, // Default to personal limits for unknown tiers
            }
        };

        // Update wallet active status. Failed wallets are recoverable records, not active
        // subscriptions slots, so keep them inactive and skip them when counting.
        let mut active_wallet_count = 0;
        for wallet in &wallets {
            let should_be_active = wallet.status != "failed" && active_wallet_count < wallet_limit;
            let wallet_position = active_wallet_count + usize::from(!should_be_active);
            if should_be_active {
                active_wallet_count += 1;
            }

            if let Err(e) = self
                .metadata_db
                .update_wallet_active_status(&wallet.checksum, should_be_active)
                .await
            {
                tracing::error!(
                    "Failed to update wallet {} active status: {}",
                    wallet.checksum,
                    e
                );
            } else if !should_be_active {
                if wallet.status == "failed" {
                    tracing::info!(
                        "📵 Deactivated wallet '{}' - wallet is in failed state",
                        wallet.name
                    );
                } else {
                    tracing::info!(
                        "📵 Deactivated wallet '{}' (#{}) - exceeds {} tier limit",
                        wallet.name,
                        wallet_position,
                        tier
                    );
                }
            }
        }

        // Handle contacts for each wallet
        for wallet in &wallets {
            let contacts = self
                .metadata_db
                .get_contacts_oldest_first_for_limits(&wallet.checksum)
                .await?;

            // Determine contact limit based on subscription status, tier, and admin status
            let contact_limit = if is_admin {
                usize::MAX // Unlimited for admin
            } else if !is_subscription_active {
                0 // No active contacts for inactive subscriptions (expired, past_due, or canceled with no remaining access)
            } else {
                match tier {
                    "personal" => 1,
                    "team" => 5,
                    _ => 1, // Default to personal limits
                }
            };

            for (index, contact) in contacts.iter().enumerate() {
                let within_count_limit = index < contact_limit;
                let should_be_active = within_count_limit;

                if let Some(contact_id) = &contact.id {
                    tracing::debug!("🔍 Contact '{}' (index: {}, created_at: {:?}) - within_limit: {}, should_be_active: {}", 
                        contact.name, index, contact.created_at, within_count_limit, should_be_active);

                    if let Err(e) = self
                        .metadata_db
                        .update_contact_active_status(contact_id, should_be_active)
                        .await
                    {
                        tracing::error!(
                            "Failed to update contact {} active status: {}",
                            contact_id,
                            e
                        );
                    } else if !should_be_active {
                        let reason =
                            format!("exceeds {} tier limit of {} contacts", tier, contact_limit);
                        tracing::info!(
                            "📵 Deactivated contact '{}' in wallet '{}' - {}",
                            contact.name,
                            wallet.name,
                            reason
                        );
                    }
                }
            }
        }

        Ok(())
    }
}

pub type AppServicesState = Arc<AppServices>; // New non-blocking architecture
pub type NotificationManagerState = Arc<Mutex<NotificationManager>>;
pub type StripeBillingState = Option<Arc<StripeBilling>>;
pub type ConfigState = Arc<AppConfig>;
pub type ElectrumClientManagerState = Option<Arc<ElectrumClientManager>>;
pub type BtcPayClientState = Option<Arc<crate::btcpay_client::BtcPayClient>>;

/// Unified application state for all handlers.
/// Contains all state components and implements FromRef for each,
/// allowing custom extractors (like AuthenticatedUser) to access specific state.
#[derive(Clone)]
pub struct AppState {
    pub app_services: AppServicesState,
    pub notification_manager: NotificationManagerState,
    pub stripe_billing: StripeBillingState,
    pub config: ConfigState,
    pub electrum_manager: ElectrumClientManagerState,
    pub btcpay_client: BtcPayClientState,
}

// FromRef implementations allow extractors to access individual state components
impl FromRef<AppState> for AppServicesState {
    fn from_ref(state: &AppState) -> Self {
        state.app_services.clone()
    }
}

impl FromRef<AppState> for NotificationManagerState {
    fn from_ref(state: &AppState) -> Self {
        state.notification_manager.clone()
    }
}

impl FromRef<AppState> for StripeBillingState {
    fn from_ref(state: &AppState) -> Self {
        state.stripe_billing.clone()
    }
}

// ConfigState = Arc<AppConfig>, so this also enables AuthenticatedUser extractor
impl FromRef<AppState> for ConfigState {
    fn from_ref(state: &AppState) -> Self {
        state.config.clone()
    }
}

impl FromRef<AppState> for ElectrumClientManagerState {
    fn from_ref(state: &AppState) -> Self {
        state.electrum_manager.clone()
    }
}

impl FromRef<AppState> for BtcPayClientState {
    fn from_ref(state: &AppState) -> Self {
        state.btcpay_client.clone()
    }
}

/// Build CORS layer based on operating mode
/// - Cloud mode: Restrict to configured FRONTEND_URL only
/// - Self-hosted mode: Allow any origin (single-user local setup)
fn build_cors_layer(config: &AppConfig) -> CorsLayer {
    if config.is_cloud_mode() {
        // Cloud mode: Restrict to configured FRONTEND_URL
        let cors = CorsLayer::new()
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .allow_headers([
                HeaderName::from_static("content-type"),
                HeaderName::from_static("authorization"),
                HeaderName::from_static("accept"),
            ])
            .allow_credentials(true);

        if let Some(frontend_url) = config.frontend_url() {
            let origin = match frontend_url.parse::<HeaderValue>() {
                Ok(val) => val,
                Err(e) => {
                    tracing::error!(
                        "Invalid FRONTEND_URL for CORS: {}. Error: {}",
                        frontend_url,
                        e
                    );
                    // Fallback to a restrictive origin if parsing fails
                    "https://invalid.localhost".parse().unwrap()
                }
            };
            cors.allow_origin(origin)
        } else {
            tracing::warn!("Cloud mode without FRONTEND_URL - using restrictive CORS");
            cors.allow_origin("https://invalid.localhost".parse::<HeaderValue>().unwrap())
        }
    } else {
        // Self-hosted mode: Mirror origin and allow credentials
        // Note: CorsLayer::permissive() uses "*" which doesn't work with credentials: 'include'
        // Also cannot use Any for headers when credentials are enabled
        CorsLayer::new()
            .allow_origin(tower_http::cors::AllowOrigin::mirror_request())
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .allow_headers([
                HeaderName::from_static("content-type"),
                HeaderName::from_static("authorization"),
                HeaderName::from_static("accept"),
            ])
            .allow_credentials(true)
    }
}

pub fn create_router_with_services(
    app_services: AppServicesState,
    notification_manager: NotificationManagerState,
    stripe_billing: StripeBillingState,
    config: AppConfig,
    electrum_manager: ElectrumClientManagerState,
) -> Router {
    // Build CORS layer before moving config into Arc
    let cors_layer = build_cors_layer(&config);

    // Build BtcPayClient once at startup if configured
    let btcpay_client = if config.is_btcpay_enabled() {
        Some(crate::btcpay_client::BtcPayClient::new(
            config.btcpay_url().unwrap().to_string(),
            config.btcpay_api_key().unwrap().to_string(),
            config.btcpay_store_id().unwrap().to_string(),
            config.btcpay_offering_id().map(|s| s.to_string()),
            config.btcpay_plan_id().map(|s| s.to_string()),
        ))
    } else {
        None
    };

    let config_state = Arc::new(config);

    // Create unified AppState for handlers that use the AuthenticatedUser extractor
    let app_state = AppState {
        app_services: app_services.clone(),
        notification_manager: notification_manager.clone(),
        stripe_billing: stripe_billing.clone(),
        config: config_state.clone(),
        electrum_manager,
        btcpay_client: btcpay_client.map(Arc::new),
    };

    // Routes using unified AppState with domain handlers
    let app_state_routes = Router::new()
        // Auth routes (public, no authentication required)
        .route("/auth/register", post(register))
        .route("/auth/verify-email/{token}", get(verify_email))
        .route("/auth/forgot-password", post(forgot_password))
        .route("/auth/reset-password/{token}", post(reset_password))
        .route("/auth/login", post(login))
        .route("/auth/demo-login", post(demo_login))
        .route("/auth/logout", post(logout))
        // Auth routes (authenticated)
        .route("/auth/me", get(me))
        .route("/auth/user", put(update_user))
        // Contact form (public)
        .route("/contact", post(submit_contact_form))
        // User preferences (authenticated)
        .route(
            "/user/preferences",
            get(get_user_preferences).put(update_user_preferences),
        )
        // Balance alerts (authenticated)
        .route(
            "/wallets/{checksum}/balance-alerts",
            get(get_wallet_balance_alerts).post(create_wallet_balance_alert),
        )
        .route(
            "/balance-alerts/{alert_id}",
            axum::routing::delete(delete_balance_alert),
        )
        // Public configuration route (no auth required)
        .route("/config", get(get_config))
        // Blockchain data routes (no auth required)
        .route("/block-headers/current", get(get_current_block_header))
        .route("/exchange-rates", get(get_exchange_rates))
        // Wallet routes (authenticated)
        .route(
            "/wallets",
            get(get_wallets_list).post(create_wallet_non_blocking),
        )
        .route(
            "/wallets/{checksum}",
            get(get_wallet).put(update_wallet).delete(delete_wallet),
        )
        .route("/wallets/{checksum}/detail", get(get_wallet_detail))
        .route(
            "/wallets/{checksum}/transactions/{txid}/notifications",
            get(get_transaction_notifications),
        )
        // Contact routes (authenticated)
        .route(
            "/wallets/{checksum}/contacts",
            get(get_wallet_contacts).post(create_wallet_contact),
        )
        .route(
            "/wallets/{wallet_checksum}/contacts/{contact_id}",
            put(update_wallet_contact).delete(delete_wallet_contact),
        )
        // Contact verification routes (authenticated)
        .route(
            "/wallets/{checksum}/contacts/send-verification",
            post(send_contact_verification),
        )
        .route("/wallets/{checksum}/contacts/verify", post(verify_contact))
        // Billing status route (authenticated)
        .route("/billing/status", get(get_billing_status))
        // Test notification route (self-hosted only)
        .route("/ntfy/test", post(send_test_ntfy_notification))
        // Database health & integrity (admin only)
        .route("/health/database", get(get_database_health))
        .route("/admin/database/integrity", post(run_integrity_check))
        .with_state(app_state.clone());

    let provider_routes = Router::new()
        .route("/providers", get(get_providers))
        .with_state(notification_manager);

    // Stripe routes - only mounted if Stripe billing is available
    let stripe_routes = if stripe_billing.is_some() {
        Router::new()
            // Authenticated routes
            .route("/stripe/checkout", post(create_stripe_checkout_session))
            .route("/stripe/portal", post(create_stripe_customer_portal))
            // Unauthenticated routes
            .route("/stripe/webhook", post(handle_stripe_webhook))
            .route("/billing/pricing", get(get_billing_pricing))
            .route(
                "/billing/session/{session_id}",
                get(get_checkout_session_details),
            )
            .with_state(app_state.clone())
    } else {
        Router::new() // Empty router if Stripe not configured
    };

    // Donation routes - BTCPay redirect endpoints (no auth required)
    let donation_routes = if config_state.is_btcpay_enabled() {
        let router = Router::new().route("/donations/one-time", get(donate_one_time));

        let router = if config_state.is_btcpay_recurring_enabled() {
            router.route("/donations/recurring", get(donate_recurring))
        } else {
            router
        };

        router.with_state(app_state.clone())
    } else {
        Router::new()
    };

    let api_routes = app_state_routes
        .merge(provider_routes)
        .merge(stripe_routes)
        .merge(donation_routes);

    Router::new().nest("/api", api_routes).layer(cors_layer)
}
