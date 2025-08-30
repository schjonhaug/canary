use crate::admin_notifications::AdminNotifications;
use crate::auth::{
    authenticate_user, load_twilio_config_from_env, AuthResponse, AuthService, AuthUser,
    AuthUserResponse, ForgotPasswordRequest, LoginRequest, RegisterRequest, ResetPasswordRequest,
    UpdateUserRequest, UpdateUserResponse,
};
use crate::config::AppConfig;
use crate::electrum::BlockHeader;
use crate::email_service::EmailService;
use crate::metadata::{
    Contact, EventType, Language, MetadataDb, NotificationMethod, ProviderType,
    TransactionEventWithWallet, WalletDetailResponse, WalletMetadata, WalletsListResponse,
};
use crate::notifications::{NotificationManager, ProviderInfo};
use crate::stripe_billing::{
    CheckoutSessionDetails, CheckoutSessionResponse, CustomerPortalResponse, FrontendPriceInfo,
    FrontendTierPricing, PricingInfo, StripeBilling,
};
use crate::subscription::{check_limit, SubscriptionTier};
use crate::wallet::WalletCreationService;
use crate::xpub_converter::XpubConverter;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get, post, put},
    Router,
};
use phonenumber::PhoneNumber;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;
use tracing::info;
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;

#[derive(Deserialize, Serialize, ToSchema)]
pub struct CreateWalletRequest {
    /// The name of the wallet
    #[schema(example = "My Bitcoin Wallet")]
    pub name: String,
    /// The multipath output descriptor for the wallet or extended public key (XPUB)
    #[schema(
        example = "wpkh(tpubD6NzVbkrYhZ4XgiXtGrdW5XDAPFCL9h7we1vwNCpn8tGbBcgfVYjXyhWo4E1xkh56hjod1RhGjxbaTLV3X4FyWuejifB9jusQ46QzG87VKp/<0;1>/*)"
    )]
    pub descriptor: String,
    /// The user's preferred language for notifications (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(example = "en")]
    pub preferred_language: Option<String>,
    /// Whether this is a fresh wallet with no transaction history (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(example = false)]
    pub is_fresh_wallet: Option<bool>,
    /// Script type for XPUB wallets (required when is_fresh_wallet=true and descriptor is XPUB, optional for advanced settings)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(example = "p2wpkh")]
    pub script_type: Option<String>,
    /// Stop gap for advanced users (auto, 250, 500, 750, 1000)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(example = "auto")]
    pub stop_gap: Option<String>,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct UpdateWalletRequest {
    /// The new name for the wallet
    #[schema(example = "Updated Wallet Name")]
    pub name: String,
}

#[derive(Serialize, ToSchema)]
pub struct CreateWalletResponse {
    /// Success message
    pub message: String,
    /// Created wallet metadata
    pub wallet: WalletMetadata,
}

#[derive(Serialize, ToSchema)]
pub struct ErrorResponse {
    /// Error description
    pub error: String,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct NotificationMethodRequest {
    /// The provider type (sms or ntfy)
    #[schema(example = "sms")]
    pub provider_type: ProviderType,
    /// The notification target (phone number or ntfy topic)
    #[schema(example = "+4712345678")]
    pub notification_target: String,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct CreateContactWithMethodsRequest {
    /// The name of the contact person
    #[schema(example = "John Doe")]
    pub name: String,
    /// The language preference for notifications
    #[schema(example = "en")]
    pub language: Language,
    /// List of notification methods for this contact
    pub notification_methods: Vec<NotificationMethodRequest>,
}

#[derive(Serialize, ToSchema)]
pub struct CreateContactResponse {
    /// Success message
    pub message: String,
    /// Contact ID
    pub contact_id: String, // UUIDv4
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct SendContactVerificationRequest {
    /// Contact name
    pub name: String,
    /// Language for notifications
    pub language: String,
    /// Phone number to verify (optional)
    pub phone_number: Option<String>,
    /// Email address to verify (optional)
    pub email_address: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct ProvidersResponse {
    /// Available notification providers
    pub providers: Vec<ProviderInfo>,
}

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
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

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
        is_admin: bool,
    ) -> Result<(), anyhow::Error> {
        if is_admin {
            tracing::info!("🎯 Applying unlimited limits for admin user {}", user_id);
        } else {
            tracing::info!("🎯 Applying {} tier limits for user {}", tier, user_id);
        }

        // Get all wallets for this user ordered by creation time (oldest first)
        let wallets = self
            .metadata_db
            .get_wallets_for_user_oldest_first(user_id)
            .await?;

        // Determine wallet limit based on tier or admin status
        let wallet_limit = if is_admin {
            usize::MAX // Unlimited for admin
        } else {
            match tier {
                "personal" => 1,
                "team" => 5,
                _ => 1, // Default to personal limits for unknown tiers
            }
        };

        // Update wallet active status
        for (index, wallet) in wallets.iter().enumerate() {
            let should_be_active = index < wallet_limit;

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
                tracing::info!(
                    "📵 Deactivated wallet '{}' (#{}) - exceeds {} tier limit",
                    wallet.name,
                    index + 1,
                    tier
                );
            }
        }

        // Handle contacts for each wallet
        for wallet in &wallets {
            let contacts = self
                .metadata_db
                .get_contacts_oldest_first_for_limits(&wallet.checksum)
                .await?;

            // Determine contact limit based on tier or admin status
            let contact_limit = if is_admin {
                usize::MAX // Unlimited for admin
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

// Stripe billing request/response structures
#[derive(Deserialize, Serialize, ToSchema)]
pub struct CreateCheckoutSessionRequest {
    /// The subscription tier to purchase
    #[schema(example = "team")]
    pub tier: String,
    /// Whether to use yearly billing (default is monthly)
    #[schema(example = false)]
    pub is_yearly: Option<bool>,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct CreateCustomerPortalRequest {
    /// Return URL after customer portal session
    #[schema(example = "https://app.canarybitcoin.com/billing")]
    pub return_url: String,
}

#[derive(Serialize, ToSchema)]
pub struct BillingTierLimits {
    pub max_wallets: i32,             // -1 for unlimited
    pub max_contacts_per_wallet: i32, // -1 for unlimited
    pub sync_interval_seconds: u64,
}

#[derive(Serialize, ToSchema)]
pub struct BillingStatusResponse {
    /// User ID
    pub user_id: String,
    /// Current subscription tier
    pub subscription_tier: String,
    /// Subscription status (trial, active, expired, cancelled)
    pub subscription_status: String,
    /// Trial end date (if in trial)
    pub trial_ends_at: Option<String>,
    /// Subscription started date (if active)
    pub subscription_started_at: Option<String>,
    /// Stripe customer ID
    pub stripe_customer_id: Option<String>,
    /// Current wallet count
    pub wallet_count: usize,
    /// Current contact count across all wallets
    pub contact_count: usize,
    /// Subscription tier limits
    pub limits: BillingTierLimits,
}

/// Validates and normalizes a phone number
fn validate_phone_number(phone: &str) -> Result<String, String> {
    // Check if phone number starts with country code
    if !phone.starts_with('+') {
        return Err(
            "Phone number must include country code (e.g., +1 for US, +44 for UK, +47 for Norway)"
                .to_string(),
        );
    }

    // Parse phone number using the phonenumber crate
    let parsed_number =
        PhoneNumber::from_str(phone).map_err(|_| "Invalid phone number format".to_string())?;

    // Check if it's a valid number
    if !parsed_number.is_valid() {
        return Err("Invalid phone number".to_string());
    }

    // Return normalized E.164 format
    Ok(parsed_number
        .format()
        .mode(phonenumber::Mode::E164)
        .to_string())
}

/// Generates an ntfy topic from contact name, language, and wallet descriptor
fn generate_ntfy_topic(name: &str, language: &Language, descriptor: &str) -> String {
    // Extract checksum from descriptor
    let checksum = descriptor
        .rfind('#')
        .map(|i| &descriptor[i + 1..])
        .unwrap_or("unknown");

    // Sanitize name for topic
    let sanitized_name = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();

    // Combine into topic (max 64 chars)
    let topic = format!("{}-{}-{}", sanitized_name, language.as_str(), checksum);
    if topic.len() > 64 {
        topic[..64].to_string()
    } else {
        topic
    }
}

/// Authenticate user based on operating mode
/// In SAAS mode: authenticate using JWT token
/// In FOSS mode: return hardcoded foss-user
fn authenticate_user_mode_aware(
    config: &AppConfig,
    auth_header: Option<&str>,
) -> Result<AuthUser, String> {
    if config.is_foss_mode() {
        // FOSS mode: return hardcoded user
        Ok(AuthUser {
            user_id: "foss-user".to_string(),
            is_admin: true,
        })
    } else {
        // SAAS mode: authenticate using JWT
        authenticate_user(auth_header).map_err(|_| "Authentication required".to_string())
    }
}

/// Non-blocking wallet creation using AppServices (avoids WalletManager mutex)
/// This resolves the regression where wallet creation was taking 30+ seconds
#[utoipa::path(
    post,
    path = "/api/wallets",
    request_body = CreateWalletRequest,
    responses(
        (status = 201, description = "Wallet created successfully", body = CreateWalletResponse),
        (status = 400, description = "Bad Request", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden - subscription limit exceeded", body = ErrorResponse),
        (status = 409, description = "Conflict - wallet already exists", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    ),
    tag = "wallet",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn create_wallet_non_blocking(
    State((app_services, stripe_billing, config)): State<(
        AppServicesState,
        StripeBillingState,
        ConfigState,
    )>,
    headers: HeaderMap,
    Json(payload): Json<CreateWalletRequest>,
) -> Response {
    let start_time = std::time::Instant::now();

    // Authenticate user based on operating mode
    let user = match authenticate_user_mode_aware(
        &config,
        headers.get("authorization").and_then(|h| h.to_str().ok()),
    ) {
        Ok(user) => user,
        Err(err) => {
            return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: err })).into_response();
        }
    };

    // Validate network compatibility early - before any database operations
    if let Err(e) = XpubConverter::validate_descriptor_network(
        &payload.descriptor,
        config.network.to_bdk_network(),
    ) {
        let server_network_name = match config.network {
            crate::config::NetworkConfig::Mainnet => "mainnet",
            crate::config::NetworkConfig::Testnet => "testnet",
            crate::config::NetworkConfig::Regtest => "regtest",
        };
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("{}. Please use a {} key.", e, server_network_name),
            }),
        )
            .into_response();
    }

    // Helper function to detect output descriptor format
    let is_descriptor_format = |input: &str| -> bool {
        let descriptor_regex = regex::Regex::new(r"^(wpkh|wsh|sh|pkh|tr)\(").unwrap();
        descriptor_regex.is_match(input.trim())
    };

    // Validate advanced settings: custom stop gap requires specific script type (except for output descriptors)
    if let Some(stop_gap) = &payload.stop_gap {
        if stop_gap != "auto" {
            // Skip script type requirement for output descriptors (they already contain script type info)
            if !is_descriptor_format(&payload.descriptor) {
                // Custom stop gap requires specific script type for XPUBs
                match &payload.script_type {
                    Some(script_type) if script_type != "auto" => {
                        // Valid: custom stop gap with specific script type
                    }
                    _ => {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse {
                                error: "Custom stop gap requires selecting a specific script type (not auto)".to_string(),
                            }),
                        )
                            .into_response();
                    }
                }
            }

            // Validate stop gap values
            if !["250", "500", "750", "1000"].contains(&stop_gap.as_str()) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "Invalid stop gap. Allowed values: auto, 250, 500, 750, 1000"
                            .to_string(),
                    }),
                )
                    .into_response();
            }
        }
    }

    // NON-BLOCKING: Use AppServices metadata_db directly (no wallet mutex)
    // Get user's subscription tier and check wallet limit
    match app_services.metadata_db.get_user_by_id(&user.user_id).await {
        Ok(Some(user_record)) => {
            // Count existing wallets for the user
            match app_services
                .metadata_db
                .count_wallets_for_user(&user.user_id)
                .await
            {
                Ok(wallet_count) => {
                    // Check limit based on subscription tier
                    let tier_limits = user_record.subscription_tier.limits_for_api();
                    if let Err(limit_err) =
                        check_limit(wallet_count, tier_limits.max_wallets, "Wallet")
                    {
                        return (
                            StatusCode::FORBIDDEN,
                            Json(ErrorResponse {
                                error: limit_err.to_string(),
                            }),
                        )
                            .into_response();
                    }
                }
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: format!("Failed to check wallet limit: {}", e),
                        }),
                    )
                        .into_response();
                }
            }
        }
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "User not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to get user information: {}", e),
                }),
            )
                .into_response();
        }
    }

    // NON-BLOCKING: Check if input is an XPUB and handle conversion
    let descriptor = if XpubConverter::is_xpub(&payload.descriptor) {
        // For fresh wallets with known script type, convert immediately
        if payload.is_fresh_wallet == Some(true) {
            match &payload.script_type {
                Some(script_type) => {
                    println!(
                        "Fresh wallet detected, using provided script type: {}",
                        script_type
                    );
                    // Use static XPUB conversion (TODO: extract this to avoid electrum client dependency)
                    payload.descriptor.clone() // For now, pass XPUB directly to creation service
                }
                None => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            error: "script_type is required for fresh XPUB wallets".to_string(),
                        }),
                    )
                        .into_response();
                }
            }
        } else {
            // Existing wallet - pass XPUB directly to background task for smart script detection
            println!(
                "Detected XPUB format for existing wallet, will probe script types in background"
            );
            payload.descriptor.clone()
        }
    } else {
        // Input is already a descriptor
        payload.descriptor.clone()
    };

    // NON-BLOCKING: Use WalletCreationService instead of WalletManager mutex
    match app_services
        .wallet_creation_service
        .create_wallet_non_blocking(
            &payload.name,
            &descriptor,
            &user.user_id,
            payload.is_fresh_wallet.unwrap_or(false),
            payload.script_type.as_deref(),
            payload.stop_gap.as_deref(),
        )
        .await
    {
        Ok(wallet_metadata) => {
            // Check if SAAS mode - if so, auto-add user as contact
            if config.is_saas_mode() {
                // Log the received language from frontend
                eprintln!(
                    "Received preferred_language from frontend: {:?}",
                    payload.preferred_language
                );

                // Get user info for contact creation (NON-BLOCKING)
                let metadata_db = app_services.metadata_db.clone();
                let user_id = user.user_id.clone();
                let wallet_checksum = wallet_metadata.checksum.clone();
                let preferred_language = payload.preferred_language.clone();

                // Spawn async task to create contact (don't block wallet creation if this fails)
                tokio::spawn(async move {
                    // Get user details from database (no mutex needed)
                    match metadata_db.get_user_by_id(&user_id).await {
                        Ok(Some(user_record)) => {
                            // Map browser language to supported languages
                            let language = match preferred_language.as_deref() {
                                Some(lang)
                                    if lang.starts_with("no")
                                        || lang.starts_with("nb")
                                        || lang.starts_with("nn") =>
                                {
                                    eprintln!("Mapping language '{}' to Norwegian", lang);
                                    Language::Norwegian
                                }
                                Some(lang) => {
                                    eprintln!("Mapping language '{}' to English (default)", lang);
                                    Language::English
                                }
                                None => {
                                    eprintln!("No language provided, defaulting to English");
                                    Language::English
                                }
                            };

                            // Use user's name or fallback to "Me"
                            let contact_name = user_record.name.as_deref().unwrap_or("Me");

                            // Create contact with email notification using the user's email
                            let notification_methods =
                                vec![(ProviderType::Email, user_record.email)];

                            match metadata_db
                                .insert_contact_with_notification_methods(
                                    &wallet_checksum,
                                    contact_name,
                                    &language,
                                    notification_methods,
                                )
                                .await
                            {
                                Ok(contact_id) => {
                                    eprintln!(
                                        "Auto-created contact {} for user {} in wallet {}",
                                        contact_id, user_id, wallet_checksum
                                    );
                                }
                                Err(e) => {
                                    eprintln!(
                                        "Failed to auto-create contact for user {}: {}",
                                        user_id, e
                                    );
                                }
                            }
                        }
                        Ok(None) => {
                            eprintln!(
                                "User {} not found in database for auto-contact creation",
                                user_id
                            );
                        }
                        Err(e) => {
                            eprintln!(
                                "Error getting user {} for auto-contact creation: {}",
                                user_id, e
                            );
                        }
                    }
                });
            }

            // Check if this is the user's first wallet and they're in 'pending' status
            // If so, activate their trial now! (NON-BLOCKING)
            let user_record = app_services
                .metadata_db
                .get_user_by_id(&user.user_id)
                .await
                .ok()
                .flatten();
            if let Some(user_record) = user_record {
                if user_record.subscription_status == "pending" {
                    // Count their wallets (should be 1 now after creation)
                    let wallet_count = app_services
                        .metadata_db
                        .count_wallets_for_user(&user.user_id)
                        .await
                        .unwrap_or(0);

                    if wallet_count == 1 {
                        tracing::info!(
                            "🎉 Activating trial for user {} on first wallet creation",
                            user_record.email
                        );

                        // Calculate trial end date (30 days from now)
                        let trial_ends_at = chrono::Utc::now() + chrono::Duration::days(30);

                        // Update user status to 'trialing' in database (NON-BLOCKING)
                        if let Err(e) = app_services
                            .metadata_db
                            .update_user_trial_status(
                                &user.user_id,
                                "trialing",
                                Some(trial_ends_at.to_rfc3339()),
                            )
                            .await
                        {
                            tracing::error!("Failed to update user trial status: {}", e);
                        }

                        // If Stripe is available, create the trial subscription (NON-BLOCKING)
                        if let Some(stripe_service) = &stripe_billing {
                            // Create trial subscription for Team tier
                            if let Err(e) = stripe_service
                                .create_trial_subscription(
                                    &user_record,
                                    crate::subscription::SubscriptionTier::Team,
                                    &app_services.metadata_db,
                                )
                                .await
                            {
                                tracing::error!(
                                    "Failed to create Stripe trial subscription for user {}: {}",
                                    user_record.email,
                                    e
                                );
                                // Don't fail wallet creation if Stripe fails, but log the error
                                // User can still use the service with database-only trial
                            } else {
                                tracing::info!(
                                    "✅ Stripe trial subscription created successfully for user {}",
                                    user_record.email
                                );
                            }
                        } else if user_record.stripe_customer_id.is_some() {
                            tracing::warn!(
                                "User {} has Stripe customer ID but Stripe service is not available",
                                user_record.email
                            );
                        }
                    }
                }
            }

            // Send admin notification for new wallet creation (fire-and-forget)
            {
                let admin_notifications = AdminNotifications::new();
                if admin_notifications.is_enabled() {
                    let wallet_name = wallet_metadata.name.clone();
                    let wallet_checksum = wallet_metadata.checksum.clone();
                    // Get user email for notification
                    if let Ok(Some(user_record)) =
                        app_services.metadata_db.get_user_by_id(&user.user_id).await
                    {
                        let user_email = user_record.email;
                        tokio::spawn(async move {
                            admin_notifications
                                .notify_wallet_creation(&wallet_name, &user_email, &wallet_checksum)
                                .await;
                        });
                    }
                }
            }

            let elapsed = start_time.elapsed();
            info!("create_wallet_non_blocking completed in {:?}", elapsed);

            (
                StatusCode::CREATED,
                Json(CreateWalletResponse {
                    message: "Wallet created successfully".to_string(),
                    wallet: wallet_metadata,
                }),
            )
                .into_response()
        }
        Err(e) => {
            let error_msg = e.to_string();
            let status_code = match error_msg.as_str() {
                "Descriptor already exists" => StatusCode::CONFLICT,
                "Wallet already exists" | "Wallet file already exists" => StatusCode::CONFLICT,
                _ => StatusCode::BAD_REQUEST,
            };

            (status_code, Json(ErrorResponse { error: error_msg })).into_response()
        }
    }
}

#[utoipa::path(
    delete,
    path = "/api/wallets/{id}",
    params(
        ("id" = i64, Path, description = "The wallet ID to delete")
    ),
    responses(
        (status = 204, description = "Wallet deleted successfully"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden - wallet belongs to another user", body = ErrorResponse),
        (status = 404, description = "Wallet not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    ),
    tag = "wallet",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn delete_wallet(
    State((app_services, _stripe_billing, config)): State<(
        AppServicesState,
        StripeBillingState,
        ConfigState,
    )>,
    headers: HeaderMap,
    Path(checksum): Path<String>,
) -> Response {
    // Authenticate user based on operating mode
    let user = match authenticate_user_mode_aware(
        &config,
        headers.get("authorization").and_then(|h| h.to_str().ok()),
    ) {
        Ok(user) => user,
        Err(err) => {
            return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: err })).into_response();
        }
    };

    // NON-BLOCKING: Use AppServices metadata_db directly (no wallet mutex)
    // Check if wallet exists and belongs to user (or user is admin)
    match app_services
        .metadata_db
        .get_wallet_by_checksum(&checksum)
        .await
    {
        Ok(Some(_)) => {
            // Check ownership
            if !user.is_admin {
                match app_services
                    .metadata_db
                    .is_wallet_owned_by_user(&checksum, &user.user_id)
                    .await
                {
                    Ok(true) => {} // User owns the wallet
                    Ok(false) => {
                        return (
                            StatusCode::FORBIDDEN,
                            Json(ErrorResponse {
                                error: "Access denied".to_string(),
                            }),
                        )
                            .into_response();
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Database error: {}", e),
                            }),
                        )
                            .into_response();
                    }
                }
            }
        }
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Wallet not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Database error: {}", e),
                }),
            )
                .into_response();
        }
    }

    // SOFT DELETE: Mark wallet as deleted instead of immediate deletion
    // This allows for instant response while background cleanup happens during next sync cycle
    match app_services
        .metadata_db
        .mark_wallet_as_deleted(&checksum)
        .await
    {
        Ok(true) => {
            println!("[{}] Wallet marked as deleted (soft delete)", checksum);
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Wallet not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Database error: {}", e),
                }),
            )
                .into_response();
        }
    }
}

#[utoipa::path(
    put,
    path = "/api/wallets/{id}",
    params(
        ("id" = i64, Path, description = "The wallet ID to update")
    ),
    request_body = UpdateWalletRequest,
    responses(
        (status = 200, description = "Wallet updated successfully"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden - wallet belongs to another user", body = ErrorResponse),
        (status = 404, description = "Wallet not found", body = ErrorResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
    ),
    tag = "wallet",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn update_wallet(
    State((app_services, _stripe_billing, config)): State<(
        AppServicesState,
        StripeBillingState,
        ConfigState,
    )>,
    headers: HeaderMap,
    Path(checksum): Path<String>,
    Json(payload): Json<UpdateWalletRequest>,
) -> Response {
    // Authenticate user based on operating mode
    let user = match authenticate_user_mode_aware(
        &config,
        headers.get("authorization").and_then(|h| h.to_str().ok()),
    ) {
        Ok(user) => user,
        Err(err) => {
            return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: err })).into_response();
        }
    };

    if payload.name.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Wallet name cannot be empty".to_string(),
            }),
        )
            .into_response();
    }

    let start_time = std::time::Instant::now();

    // Direct metadata access - no mutex blocking!
    match app_services
        .metadata_db
        .get_wallet_by_checksum(&checksum)
        .await
    {
        Ok(Some(_)) => {
            // Check ownership
            if !user.is_admin {
                match app_services
                    .metadata_db
                    .is_wallet_owned_by_user(&checksum, &user.user_id)
                    .await
                {
                    Ok(true) => {} // User owns the wallet
                    Ok(false) => {
                        return (
                            StatusCode::FORBIDDEN,
                            Json(ErrorResponse {
                                error: "Access denied".to_string(),
                            }),
                        )
                            .into_response();
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Database error: {}", e),
                            }),
                        )
                            .into_response();
                    }
                }
            }
        }
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Wallet not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Database error: {}", e),
                }),
            )
                .into_response();
        }
    }

    let update_result = app_services
        .metadata_db
        .update_wallet_by_checksum(&checksum, &payload.name)
        .await;

    let elapsed = start_time.elapsed();
    info!("update_wallet completed in {:?}", elapsed);

    match update_result {
        Ok(true) => StatusCode::OK.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Wallet not found".to_string(),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Database error: {}", e),
            }),
        )
            .into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/wallets/{id}",
    params(
        ("id" = i64, Path, description = "The wallet ID to retrieve")
    ),
    responses(
        (status = 200, description = "Wallet found", body = WalletMetadata),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden - wallet belongs to another user", body = ErrorResponse),
        (status = 404, description = "Wallet not found", body = ErrorResponse),
    ),
    tag = "wallet",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn get_wallet(
    State((app_services, _stripe_billing, config)): State<(
        AppServicesState,
        StripeBillingState,
        ConfigState,
    )>,
    headers: HeaderMap,
    Path(checksum): Path<String>,
) -> Response {
    let start_time = std::time::Instant::now();

    // Authenticate user based on operating mode
    let user = match authenticate_user_mode_aware(
        &config,
        headers.get("authorization").and_then(|h| h.to_str().ok()),
    ) {
        Ok(user) => user,
        Err(err) => {
            return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: err })).into_response();
        }
    };

    // No mutex blocking! Direct access to metadata database
    match app_services
        .metadata_db
        .get_wallet_by_checksum(&checksum)
        .await
    {
        Ok(Some(wallet)) => {
            // Check if user has access to this wallet
            if !user.is_admin {
                match app_services
                    .metadata_db
                    .is_wallet_owned_by_user(&checksum, &user.user_id)
                    .await
                {
                    Ok(true) => {} // User owns the wallet
                    Ok(false) => {
                        return (
                            StatusCode::FORBIDDEN,
                            Json(ErrorResponse {
                                error: "Access denied".to_string(),
                            }),
                        )
                            .into_response();
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Database error: {}", e),
                            }),
                        )
                            .into_response();
                    }
                }
            }

            let response_time = start_time.elapsed();
            println!(
                "⚡ Non-blocking wallet metadata served in {:?}",
                response_time
            );
            (StatusCode::OK, Json(wallet)).into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Wallet not found".to_string(),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

// Wallet-specific contact management endpoints

#[utoipa::path(
    post,
    path = "/api/wallets/{id}/contacts",
    request_body = CreateContactWithMethodsRequest,
    params(
        ("id" = i64, Path, description = "The wallet ID")
    ),
    responses(
        (status = 201, description = "Contact created successfully", body = CreateContactResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden - wallet belongs to another user", body = ErrorResponse),
        (status = 400, description = "Invalid request or phone number", body = ErrorResponse),
        (status = 404, description = "Wallet not found", body = ErrorResponse),
    ),
    tag = "contact",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn create_wallet_contact(
    State((app_services, _stripe_billing, config)): State<(
        AppServicesState,
        StripeBillingState,
        ConfigState,
    )>,
    headers: HeaderMap,
    Path(wallet_checksum): Path<String>,
    Json(payload): Json<CreateContactWithMethodsRequest>,
) -> Response {
    let start_time = std::time::Instant::now();

    // Authenticate user based on operating mode
    let user = match authenticate_user_mode_aware(
        &config,
        headers.get("authorization").and_then(|h| h.to_str().ok()),
    ) {
        Ok(user) => user,
        Err(err) => {
            return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: err })).into_response();
        }
    };

    // Direct metadata access - no mutex blocking!
    let wallet = match app_services
        .metadata_db
        .get_wallet_by_checksum(&wallet_checksum)
        .await
    {
        Ok(Some(wallet)) => {
            if !user.is_admin {
                match app_services
                    .metadata_db
                    .is_wallet_owned_by_user(&wallet_checksum, &user.user_id)
                    .await
                {
                    Ok(true) => {} // User owns the wallet
                    Ok(false) => {
                        return (
                            StatusCode::FORBIDDEN,
                            Json(ErrorResponse {
                                error: "Access denied".to_string(),
                            }),
                        )
                            .into_response();
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Database error: {}", e),
                            }),
                        )
                            .into_response();
                    }
                }
            }
            wallet
        }
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Wallet not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
                .into_response();
        }
    };

    // Get user's subscription tier and check contact limit
    let user_record = match app_services.metadata_db.get_user_by_id(&user.user_id).await {
        Ok(Some(record)) => record,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "User not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to get user information: {}", e),
                }),
            )
                .into_response();
        }
    };

    // Count existing contacts for the wallet and check limit
    match app_services
        .metadata_db
        .count_contacts_for_wallet(&wallet_checksum)
        .await
    {
        Ok(contact_count) => {
            let tier_limits = user_record.subscription_tier.limits_for_api();
            if let Err(limit_err) = check_limit(
                contact_count,
                tier_limits.max_contacts_per_wallet,
                "Contact",
            ) {
                return (
                    StatusCode::FORBIDDEN,
                    Json(ErrorResponse {
                        error: limit_err.to_string(),
                    }),
                )
                    .into_response();
            }
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to check contact limit: {}", e),
                }),
            )
                .into_response();
        }
    }

    // Process notification methods
    let mut processed_methods = Vec::new();

    for method in &payload.notification_methods {
        match method.provider_type {
            ProviderType::Sms => {
                // Validate and normalize the phone number
                let normalized_phone = match validate_phone_number(&method.notification_target) {
                    Ok(phone) => phone,
                    Err(e) => {
                        return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e }))
                            .into_response();
                    }
                };

                // SECURITY: Check if this phone number was recently verified
                match app_services
                    .metadata_db
                    .was_recently_verified(&wallet_checksum, &normalized_phone)
                    .await
                {
                    Ok(true) => {
                        // Phone was verified, safe to add
                        processed_methods.push((ProviderType::Sms, normalized_phone));
                    }
                    Ok(false) => {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse {
                                error: "Phone number must be verified before adding contact"
                                    .to_string(),
                            }),
                        )
                            .into_response();
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Failed to check phone verification: {}", e),
                            }),
                        )
                            .into_response();
                    }
                }
            }
            ProviderType::Ntfy => {
                // Push notifications are always allowed (ntfy is free)
                // Auto-generate ntfy topic
                let topic =
                    generate_ntfy_topic(&payload.name, &payload.language, &wallet.descriptor);
                processed_methods.push((ProviderType::Ntfy, topic));
            }
            ProviderType::Email => {
                // Basic email validation
                let email = method.notification_target.trim().to_lowercase();
                if !email.contains('@') || email.len() < 5 {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            error: "Invalid email address format".to_string(),
                        }),
                    )
                        .into_response();
                }

                // SECURITY: Check if this email address was recently verified
                match app_services
                    .metadata_db
                    .was_recently_verified(&wallet_checksum, &email)
                    .await
                {
                    Ok(true) => {
                        // Email was verified, safe to add
                        processed_methods.push((ProviderType::Email, email));
                    }
                    Ok(false) => {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse {
                                error: "Email address must be verified before adding contact"
                                    .to_string(),
                            }),
                        )
                            .into_response();
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Failed to check email verification: {}", e),
                            }),
                        )
                            .into_response();
                    }
                }
            }
        }
    }

    // Ensure at least one method was provided
    if processed_methods.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "At least one notification method must be provided".to_string(),
            }),
        )
            .into_response();
    }

    // Check for duplicate notification targets (email/phone) within the same wallet
    let methods_for_validation: Vec<(String, String)> = processed_methods
        .iter()
        .map(|(provider, target)| (provider.as_str().to_string(), target.clone()))
        .collect();

    match app_services
        .metadata_db
        .check_duplicate_notification_targets(&wallet_checksum, &methods_for_validation, None)
        .await
    {
        Ok(duplicates) => {
            if !duplicates.is_empty() {
                return (
                    StatusCode::CONFLICT,
                    Json(ErrorResponse {
                        error: format!("Duplicate notification targets: {}", duplicates.join(", ")),
                    }),
                )
                    .into_response();
            }
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to check for duplicates: {}", e),
                }),
            )
                .into_response();
        }
    }

    let create_result = app_services
        .metadata_db
        .insert_contact_with_notification_methods(
            &wallet_checksum,
            &payload.name,
            &payload.language,
            processed_methods,
        )
        .await;

    let elapsed = start_time.elapsed();
    info!("create_wallet_contact completed in {:?}", elapsed);

    match create_result {
        Ok(contact_id) => (
            StatusCode::CREATED,
            Json(CreateContactResponse {
                message: "Contact created successfully".to_string(),
                contact_id,
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

// Global contacts endpoint removed - contacts are now wallet-specific

#[utoipa::path(
    delete,
    path = "/api/wallets/{wallet_id}/contacts/{contact_id}",
    params(
        ("wallet_id" = i64, Path, description = "The wallet ID"),
        ("contact_id" = String, Path, description = "The contact ID")
    ),
    responses(
        (status = 204, description = "Contact deleted successfully"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden - wallet belongs to another user", body = ErrorResponse),
        (status = 404, description = "Contact not found", body = ErrorResponse),
    ),
    tag = "contact",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn delete_wallet_contact(
    State((app_services, _stripe_billing, config)): State<(
        AppServicesState,
        StripeBillingState,
        ConfigState,
    )>,
    headers: HeaderMap,
    Path((wallet_checksum, contact_id)): Path<(String, String)>,
) -> Response {
    let start_time = std::time::Instant::now();

    // Authenticate user based on operating mode
    let user = match authenticate_user_mode_aware(
        &config,
        headers.get("authorization").and_then(|h| h.to_str().ok()),
    ) {
        Ok(user) => user,
        Err(err) => {
            return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: err })).into_response();
        }
    };

    // Direct metadata access - no mutex blocking!
    match app_services
        .metadata_db
        .get_wallet_by_checksum(&wallet_checksum)
        .await
    {
        Ok(Some(_)) => {
            if !user.is_admin {
                match app_services
                    .metadata_db
                    .is_wallet_owned_by_user(&wallet_checksum, &user.user_id)
                    .await
                {
                    Ok(true) => {} // User owns the wallet
                    Ok(false) => {
                        return (
                            StatusCode::FORBIDDEN,
                            Json(ErrorResponse {
                                error: "Access denied".to_string(),
                            }),
                        )
                            .into_response();
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Database error: {}", e),
                            }),
                        )
                            .into_response();
                    }
                }
            }
        }
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Wallet not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Database error: {}", e),
                }),
            )
                .into_response();
        }
    }

    let delete_result = app_services
        .metadata_db
        .delete_wallet_contact(&wallet_checksum, &contact_id)
        .await;

    let elapsed = start_time.elapsed();
    info!("delete_wallet_contact completed in {:?}", elapsed);

    match delete_result {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Contact not found".to_string(),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct UpdateContactRequest {
    /// Contact name
    pub name: String,
    /// Contact language
    pub language: Language,
    /// Notification methods for the contact
    pub notification_methods: Vec<NotificationMethodRequest>,
}

#[utoipa::path(
    put,
    path = "/api/wallets/{wallet_checksum}/contacts/{contact_id}",
    params(
        ("wallet_checksum" = String, Path, description = "The wallet checksum"),
        ("contact_id" = String, Path, description = "The contact ID")
    ),
    request_body = UpdateContactRequest,
    responses(
        (status = 200, description = "Contact updated successfully", body = Contact),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 404, description = "Contact not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    ),
    tag = "contacts",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn update_wallet_contact(
    State((app_services, _stripe_billing, config)): State<(
        AppServicesState,
        StripeBillingState,
        ConfigState,
    )>,
    headers: HeaderMap,
    Path((wallet_checksum, contact_id)): Path<(String, String)>,
    Json(payload): Json<UpdateContactRequest>,
) -> Response {
    let start_time = std::time::Instant::now();

    // Authenticate user based on operating mode
    let user = match authenticate_user_mode_aware(
        &config,
        headers.get("authorization").and_then(|h| h.to_str().ok()),
    ) {
        Ok(user) => user,
        Err(err) => {
            return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: err })).into_response();
        }
    };

    // Check if wallet exists and user has access
    let wallet = match app_services
        .metadata_db
        .get_wallet_by_checksum(&wallet_checksum)
        .await
    {
        Ok(Some(wallet)) => {
            if !user.is_admin {
                match app_services
                    .metadata_db
                    .is_wallet_owned_by_user(&wallet_checksum, &user.user_id)
                    .await
                {
                    Ok(true) => {} // User owns the wallet
                    Ok(false) => {
                        return (
                            StatusCode::FORBIDDEN,
                            Json(ErrorResponse {
                                error: "Access denied".to_string(),
                            }),
                        )
                            .into_response();
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Database error: {}", e),
                            }),
                        )
                            .into_response();
                    }
                }
            }
            wallet
        }
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Wallet not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
                .into_response();
        }
    };

    // Get existing contact to compare notification methods
    let existing_contact = match app_services
        .metadata_db
        .get_single_contact_with_methods(&contact_id, &wallet_checksum)
        .await
    {
        Ok(Some(contact)) => contact,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Contact not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to get existing contact: {}", e),
                }),
            )
                .into_response();
        }
    };

    // Helper function to check if a notification method has changed
    let has_method_changed = |new_method: &NotificationMethodRequest| -> bool {
        let existing_method = existing_contact
            .notification_methods
            .iter()
            .find(|m| m.provider_type == new_method.provider_type);

        match existing_method {
            Some(existing) => {
                // For SMS, normalize both numbers for comparison
                if new_method.provider_type == ProviderType::Sms {
                    let new_normalized = validate_phone_number(&new_method.notification_target)
                        .unwrap_or_else(|_| new_method.notification_target.clone());
                    existing.notification_target != new_normalized
                } else {
                    // For email and ntfy, compare normalized strings
                    let new_normalized = if new_method.provider_type == ProviderType::Email {
                        new_method.notification_target.trim().to_lowercase()
                    } else {
                        new_method.notification_target.clone()
                    };
                    existing.notification_target != new_normalized
                }
            }
            None => true, // Method doesn't exist, so it's new
        }
    };

    // Process notification methods with security checks
    let mut processed_methods = Vec::new();

    for method in &payload.notification_methods {
        match method.provider_type {
            ProviderType::Sms => {
                // Validate and normalize the phone number
                let normalized_phone = match validate_phone_number(&method.notification_target) {
                    Ok(phone) => phone,
                    Err(e) => {
                        return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e }))
                            .into_response();
                    }
                };

                // SECURITY: Only verify if the phone number has changed
                if has_method_changed(method) {
                    // Check if this phone number was recently verified
                    match app_services
                        .metadata_db
                        .was_recently_verified(&wallet_checksum, &normalized_phone)
                        .await
                    {
                        Ok(true) => {
                            processed_methods.push((ProviderType::Sms, normalized_phone));
                        }
                        Ok(false) => {
                            return (
                                StatusCode::BAD_REQUEST,
                                Json(ErrorResponse {
                                    error: "Phone number must be verified before updating contact"
                                        .to_string(),
                                }),
                            )
                                .into_response();
                        }
                        Err(e) => {
                            return (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(ErrorResponse {
                                    error: format!("Failed to check phone verification: {}", e),
                                }),
                            )
                                .into_response();
                        }
                    }
                } else {
                    // Phone number hasn't changed, so we can reuse it without verification
                    processed_methods.push((ProviderType::Sms, normalized_phone));
                }
            }
            ProviderType::Ntfy => {
                // Push notifications are always allowed (ntfy is free)
                // Auto-generate ntfy topic
                let topic =
                    generate_ntfy_topic(&payload.name, &payload.language, &wallet.descriptor);
                processed_methods.push((ProviderType::Ntfy, topic));
            }
            ProviderType::Email => {
                // Basic email validation
                let email = method.notification_target.trim().to_lowercase();
                if !email.contains('@') || email.len() < 5 {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            error: "Invalid email address format".to_string(),
                        }),
                    )
                        .into_response();
                }

                // SECURITY: Only verify if the email address has changed
                if has_method_changed(method) {
                    // Check if this email address was recently verified
                    match app_services
                        .metadata_db
                        .was_recently_verified(&wallet_checksum, &email)
                        .await
                    {
                        Ok(true) => {
                            processed_methods.push((ProviderType::Email, email));
                        }
                        Ok(false) => {
                            return (
                                StatusCode::BAD_REQUEST,
                                Json(ErrorResponse {
                                    error: "Email address must be verified before updating contact"
                                        .to_string(),
                                }),
                            )
                                .into_response();
                        }
                        Err(e) => {
                            return (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(ErrorResponse {
                                    error: format!("Failed to check email verification: {}", e),
                                }),
                            )
                                .into_response();
                        }
                    }
                } else {
                    // Email address hasn't changed, so we can reuse it without verification
                    processed_methods.push((ProviderType::Email, email));
                }
            }
        }
    }

    // Ensure at least one method was provided
    if processed_methods.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "At least one notification method must be provided".to_string(),
            }),
        )
            .into_response();
    }

    // Check for duplicate notification targets (email/phone) within the same wallet, excluding current contact
    let methods_for_validation: Vec<(String, String)> = processed_methods
        .iter()
        .map(|(provider, target)| (provider.as_str().to_string(), target.clone()))
        .collect();

    match app_services
        .metadata_db
        .check_duplicate_notification_targets(
            &wallet_checksum,
            &methods_for_validation,
            Some(&contact_id),
        )
        .await
    {
        Ok(duplicates) => {
            if !duplicates.is_empty() {
                return (
                    StatusCode::CONFLICT,
                    Json(ErrorResponse {
                        error: format!("Duplicate notification targets: {}", duplicates.join(", ")),
                    }),
                )
                    .into_response();
            }
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to check for duplicates: {}", e),
                }),
            )
                .into_response();
        }
    }

    // Update contact using transaction
    match app_services
        .metadata_db
        .update_contact_with_methods(
            &contact_id,
            &wallet_checksum,
            &payload.name,
            &payload.language,
            processed_methods,
        )
        .await
    {
        Ok(()) => {
            // Get the updated contact to return
            match app_services
                .metadata_db
                .get_contacts_with_notification_methods(&wallet_checksum)
                .await
            {
                Ok(contacts) => {
                    if let Some(updated_contact) =
                        contacts.iter().find(|c| c.id.as_ref() == Some(&contact_id))
                    {
                        let elapsed = start_time.elapsed();
                        info!("update_wallet_contact completed in {:?}", elapsed);
                        (StatusCode::OK, Json(updated_contact.clone())).into_response()
                    } else {
                        (
                            StatusCode::NOT_FOUND,
                            Json(ErrorResponse {
                                error: "Updated contact not found".to_string(),
                            }),
                        )
                            .into_response()
                    }
                }
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Failed to fetch updated contact: {}", e),
                    }),
                )
                    .into_response(),
            }
        }
        Err(e) => {
            let elapsed = start_time.elapsed();
            info!("update_wallet_contact failed in {:?}", elapsed);
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
                .into_response()
        }
    }
}

// This endpoint is no longer needed since contacts are created directly for wallets

// This function is now handled by delete_wallet_contact above

#[utoipa::path(
    get,
    path = "/api/wallets/{id}/contacts",
    params(
        ("id" = i64, Path, description = "The wallet ID")
    ),
    responses(
        (status = 200, description = "List of contacts for wallet", body = Vec<Contact>),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden - wallet belongs to another user", body = ErrorResponse),
        (status = 404, description = "Wallet not found", body = ErrorResponse),
    ),
    tag = "wallet",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn get_wallet_contacts(
    State((app_services, _stripe_billing, config)): State<(
        AppServicesState,
        StripeBillingState,
        ConfigState,
    )>,
    headers: HeaderMap,
    Path(wallet_checksum): Path<String>,
) -> Response {
    let start_time = std::time::Instant::now();

    // Authenticate user based on operating mode
    let user = match authenticate_user_mode_aware(
        &config,
        headers.get("authorization").and_then(|h| h.to_str().ok()),
    ) {
        Ok(user) => user,
        Err(err) => {
            return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: err })).into_response();
        }
    };

    // Direct metadata access - no mutex blocking!
    match app_services
        .metadata_db
        .get_wallet_by_checksum(&wallet_checksum)
        .await
    {
        Ok(Some(_)) => {
            if !user.is_admin {
                match app_services
                    .metadata_db
                    .is_wallet_owned_by_user(&wallet_checksum, &user.user_id)
                    .await
                {
                    Ok(true) => {} // User owns the wallet
                    Ok(false) => {
                        return (
                            StatusCode::FORBIDDEN,
                            Json(ErrorResponse {
                                error: "Access denied".to_string(),
                            }),
                        )
                            .into_response();
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Database error: {}", e),
                            }),
                        )
                            .into_response();
                    }
                }
            }
        }
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Wallet not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
                .into_response();
        }
    }

    let contacts_result = app_services
        .metadata_db
        .get_contacts_with_notification_methods(&wallet_checksum)
        .await;

    let elapsed = start_time.elapsed();
    info!("get_wallet_contacts completed in {:?}", elapsed);

    match contacts_result {
        Ok(contacts) => (StatusCode::OK, Json(contacts)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/wallets/{checksum}/contacts/send-verification",
    request_body = SendContactVerificationRequest,
    params(
        ("checksum" = String, Path, description = "The wallet checksum")
    ),
    responses(
        (status = 200, description = "Verification code sent", body = serde_json::Value),
        (status = 400, description = "Invalid phone number", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden - wallet belongs to another user", body = ErrorResponse),
        (status = 404, description = "Wallet not found", body = ErrorResponse),
        (status = 429, description = "Rate limit exceeded", body = ErrorResponse),
    ),
    tag = "contact",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn send_contact_verification(
    State((app_services, _stripe_billing, config)): State<(
        AppServicesState,
        StripeBillingState,
        ConfigState,
    )>,
    headers: HeaderMap,
    Path(wallet_checksum): Path<String>,
    Json(request): Json<SendContactVerificationRequest>,
) -> Response {
    let start_time = std::time::Instant::now();
    // Authenticate user based on operating mode
    let user = match authenticate_user_mode_aware(
        &config,
        headers.get("authorization").and_then(|h| h.to_str().ok()),
    ) {
        Ok(user) => user,
        Err(err) => {
            return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: err })).into_response();
        }
    };

    // Check if wallet exists and user has access - no mutex blocking!
    match app_services
        .metadata_db
        .get_wallet_by_checksum(&wallet_checksum)
        .await
    {
        Ok(Some(_)) => {
            if !user.is_admin {
                match app_services
                    .metadata_db
                    .is_wallet_owned_by_user(&wallet_checksum, &user.user_id)
                    .await
                {
                    Ok(true) => {} // User owns the wallet
                    Ok(false) => {
                        return (
                            StatusCode::FORBIDDEN,
                            Json(ErrorResponse {
                                error: "Access denied".to_string(),
                            }),
                        )
                            .into_response();
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Database error: {}", e),
                            }),
                        )
                            .into_response();
                    }
                }
            }
        }
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Wallet not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
                .into_response();
        }
    }

    // Determine verification type and validate input
    let (provider_type, notification_target, is_dev_mode) = if let Some(phone_number) =
        &request.phone_number
    {
        // SMS verification
        let normalized_phone = match validate_phone_number(phone_number) {
            Ok(phone) => phone,
            Err(e) => {
                return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e })).into_response();
            }
        };

        // Check for duplicate phone number in this wallet BEFORE any verification
        match app_services
            .metadata_db
            .check_duplicate_notification_target(&wallet_checksum, "sms", &normalized_phone, None)
            .await
        {
            Ok(Some(existing_contact_name)) => {
                return (
                    StatusCode::CONFLICT,
                    Json(ErrorResponse {
                        error: format!(
                            "Phone number '{}' is already used by contact '{}' in this wallet",
                            normalized_phone, existing_contact_name
                        ),
                    }),
                )
                    .into_response();
            }
            Ok(None) => {} // No duplicate found, continue
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Failed to check for duplicate phone number: {}", e),
                    }),
                )
                    .into_response();
            }
        }

        let is_dev_phone = cfg!(debug_assertions)
            && ["+4799999901", "+4699999902", "+3399999903"].contains(&normalized_phone.as_str());

        ("sms", normalized_phone, is_dev_phone)
    } else if let Some(email_address) = &request.email_address {
        // Email verification
        let email = email_address.trim().to_lowercase();

        // Basic email validation
        if !email.contains('@') || email.len() < 5 {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Invalid email address format".to_string(),
                }),
            )
                .into_response();
        }

        // Check for duplicate email in this wallet BEFORE any verification
        match app_services
            .metadata_db
            .check_duplicate_notification_target(&wallet_checksum, "email", &email, None)
            .await
        {
            Ok(Some(existing_contact_name)) => {
                return (
                    StatusCode::CONFLICT,
                    Json(ErrorResponse {
                        error: format!(
                            "Email '{}' is already used by contact '{}' in this wallet",
                            email, existing_contact_name
                        ),
                    }),
                )
                    .into_response();
            }
            Ok(None) => {} // No duplicate found, continue
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Failed to check for duplicate email: {}", e),
                    }),
                )
                    .into_response();
            }
        }

        // Check if email matches current user's account email (skip verification) - no mutex blocking!
        if let Ok(Some(user_record)) = app_services.metadata_db.get_user_by_id(&user.user_id).await
        {
            let jwt_secret = std::env::var("JWT_SECRET")
                .unwrap_or_else(|_| "your-secret-key-change-in-production".to_string());
            let auth_service = AuthService::new(jwt_secret.clone(), None);

            if auth_service.should_skip_email_verification(&email, &user_record.email) {
                // Auto-approve for user's own email, but still create verification record
                match app_services
                    .metadata_db
                    .create_pending_contact_verification(
                        &wallet_checksum,
                        "email",
                        &email,
                        &request.name,
                        &request.language,
                        None, // No code needed for auto-verification
                    )
                    .await
                {
                    Ok(verification_id) => {
                        // Mark as verified immediately
                        if let Err(e) = app_services
                            .metadata_db
                            .mark_verification_completed(verification_id)
                            .await
                        {
                            return (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(ErrorResponse {
                                    error: format!("Failed to complete auto-verification: {}", e),
                                }),
                            )
                                .into_response();
                        }

                        return Json(serde_json::json!({
                            "message": "Email verified automatically for user accounts",
                            "auto_verified": true
                        }))
                        .into_response();
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Failed to create verification record: {}", e),
                            }),
                        )
                            .into_response();
                    }
                }
            }
        }

        let is_dev_email = cfg!(debug_assertions)
            && ["test@example.com", "dev@canary.local"].contains(&email.as_str());

        ("email", email, is_dev_email)
    } else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Either phone_number or email_address must be provided".to_string(),
            }),
        )
            .into_response();
    };

    // Check rate limit (skip for dev mode) - no mutex blocking!
    if !is_dev_mode {
        match app_services
            .metadata_db
            .check_rate_limit(&notification_target)
            .await
        {
            Ok(true) => {} // Allowed
            Ok(false) => {
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(ErrorResponse {
                        error: "Too many verification attempts. Please try again later."
                            .to_string(),
                    }),
                )
                    .into_response();
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Failed to check rate limit: {}", e),
                    }),
                )
                    .into_response();
            }
        }
    }

    // Clean up any expired verifications first - no mutex blocking!
    let _ = app_services
        .metadata_db
        .cleanup_expired_verifications()
        .await;

    // Generate verification code
    let verification_code = if is_dev_mode {
        "123456".to_string()
    } else if provider_type == "email" {
        use crate::email_service::EmailService;
        EmailService::generate_otp_code()
    } else {
        // SMS uses Twilio Verify which generates its own codes
        "".to_string() // Will be None in database
    };

    // Store pending verification
    let stored_code = if verification_code.is_empty() {
        None
    } else {
        Some(verification_code.as_str())
    };

    match app_services
        .metadata_db
        .create_pending_contact_verification(
            &wallet_checksum,
            provider_type,
            &notification_target,
            &request.name,
            &request.language,
            stored_code,
        )
        .await
    {
        Ok(_) => {}
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to store verification: {}", e),
                }),
            )
                .into_response();
        }
    }

    // Send verification code
    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "your-secret-key-change-in-production".to_string());

    let result = if provider_type == "sms" {
        // SMS verification via Twilio
        let twilio_config = match load_twilio_config_from_env() {
            Ok(config) => config,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Twilio configuration error: {}", e),
                    }),
                )
                    .into_response();
            }
        };

        if !is_dev_mode && twilio_config.verify_service_sid.is_none() {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Twilio Verify service not configured".to_string(),
                }),
            )
                .into_response();
        }

        let auth_service = AuthService::new(jwt_secret, None);
        auth_service
            .send_contact_otp(&twilio_config, &notification_target)
            .await
    } else {
        // Email verification via Resend
        use crate::email_service::EmailService;
        let email_service = match EmailService::from_env() {
            Ok(service) => service,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Email service configuration error: {}", e),
                    }),
                )
                    .into_response();
            }
        };

        let auth_service = AuthService::new(jwt_secret, Some(email_service));
        auth_service
            .send_email_contact_otp(&notification_target, &request.name, &verification_code)
            .await
    };

    let final_result = match result {
        Ok(_) => Json(serde_json::json!({
            "message": "Verification code sent successfully"
        }))
        .into_response(),
        Err(e) => {
            // Clean up pending verification on error - no mutex blocking!
            let _ = app_services
                .metadata_db
                .cleanup_expired_verifications()
                .await;

            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Failed to send verification: {}", e),
                }),
            )
                .into_response()
        }
    };

    let elapsed = start_time.elapsed();
    info!("send_contact_verification completed in {:?}", elapsed);
    final_result
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct VerifyContactRequest {
    /// Phone number being verified (optional)
    pub phone_number: Option<String>,
    /// Email address being verified (optional)
    pub email_address: Option<String>,
    /// Verification code
    pub code: String,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct VerifyContactResponse {
    pub valid: bool,
    pub message: String,
}

#[utoipa::path(
    post,
    path = "/api/wallets/{checksum}/contacts/verify",
    request_body = VerifyContactRequest,
    responses(
        (status = 200, description = "Contact verification result", body = VerifyContactResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 401, description = "Authentication required", body = ErrorResponse),
        (status = 403, description = "Access denied", body = ErrorResponse),
        (status = 404, description = "Wallet not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    ),
    tag = "contacts",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn verify_contact(
    State((app_services, _stripe_billing, config)): State<(
        AppServicesState,
        StripeBillingState,
        ConfigState,
    )>,
    headers: HeaderMap,
    Path(wallet_checksum): Path<String>,
    Json(request): Json<VerifyContactRequest>,
) -> Response {
    let start_time = std::time::Instant::now();
    // Authenticate user based on operating mode
    let user = match authenticate_user_mode_aware(
        &config,
        headers.get("authorization").and_then(|h| h.to_str().ok()),
    ) {
        Ok(user) => user,
        Err(err) => {
            return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: err })).into_response();
        }
    };

    // Check if wallet exists and user has access - no mutex blocking!
    match app_services
        .metadata_db
        .get_wallet_by_checksum(&wallet_checksum)
        .await
    {
        Ok(Some(_)) => {
            if !user.is_admin {
                match app_services
                    .metadata_db
                    .is_wallet_owned_by_user(&wallet_checksum, &user.user_id)
                    .await
                {
                    Ok(true) => {} // User owns the wallet
                    Ok(false) => {
                        return (
                            StatusCode::FORBIDDEN,
                            Json(ErrorResponse {
                                error: "Access denied".to_string(),
                            }),
                        )
                            .into_response();
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Database error: {}", e),
                            }),
                        )
                            .into_response();
                    }
                }
            }
        }
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Wallet not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
                .into_response();
        }
    }

    // Determine what we're verifying and validate input
    let (provider_type, notification_target) = if let Some(phone_number) = &request.phone_number {
        // SMS verification
        let normalized_phone = match validate_phone_number(phone_number) {
            Ok(phone) => phone,
            Err(e) => {
                return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e })).into_response();
            }
        };
        ("sms", normalized_phone)
    } else if let Some(email_address) = &request.email_address {
        // Email verification
        let email = email_address.trim().to_lowercase();

        // Basic email validation
        if !email.contains('@') || email.len() < 5 {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Invalid email address format".to_string(),
                }),
            )
                .into_response();
        }
        ("email", email)
    } else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Either phone_number or email_address must be provided".to_string(),
            }),
        )
            .into_response();
    };

    // Look up the pending verification - no mutex blocking!
    let (verification_id, _contact_name, _language, verification_code) = match app_services
        .metadata_db
        .get_pending_verification(&wallet_checksum, &notification_target)
        .await
    {
        Ok(Some(verification)) => verification,
        Ok(None) => {
            let error_msg = if provider_type == "email" {
                "No pending verification found for this email address"
            } else {
                "No pending verification found for this phone number"
            };
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: error_msg.to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Database error: {}", e),
                }),
            )
                .into_response();
        }
    };

    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "your-secret-key-change-in-production".to_string());

    // Verify the code based on provider type
    let is_valid = if provider_type == "email" {
        // Email verification - direct code comparison
        if let Some(stored_code) = verification_code {
            let auth_service = AuthService::new(jwt_secret, None);
            auth_service.verify_email_contact_otp(&stored_code, &request.code)
        } else {
            false // No stored code means invalid
        }
    } else {
        // SMS verification
        if let Some(stored_code) = verification_code {
            // Dev mode - direct comparison
            stored_code == request.code
        } else {
            // Production mode - verify with Twilio
            let twilio_config = match load_twilio_config_from_env() {
                Ok(config) => config,
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: format!("Twilio configuration error: {}", e),
                        }),
                    )
                        .into_response();
                }
            };

            let auth_service = AuthService::new(jwt_secret, None);
            match auth_service
                .verify_contact_otp(&twilio_config, &notification_target, &request.code)
                .await
            {
                Ok(valid) => valid,
                Err(e) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            error: format!("Failed to verify code: {}", e),
                        }),
                    )
                        .into_response();
                }
            }
        }
    };

    if is_valid {
        // Clear rate limit on successful verification - no mutex blocking!
        let _ = app_services
            .metadata_db
            .clear_rate_limit(&notification_target)
            .await;

        // Mark verification as completed - no mutex blocking!
        let _ = app_services
            .metadata_db
            .mark_verification_completed(verification_id)
            .await;

        let success_message = if provider_type == "email" {
            "Email address verified successfully"
        } else {
            "Phone number verified successfully"
        };

        let elapsed = start_time.elapsed();
        info!("verify_contact completed in {:?}", elapsed);

        (
            StatusCode::OK,
            Json(VerifyContactResponse {
                valid: true,
                message: success_message.to_string(),
            }),
        )
            .into_response()
    } else {
        let elapsed = start_time.elapsed();
        info!("verify_contact completed in {:?}", elapsed);

        (
            StatusCode::BAD_REQUEST,
            Json(VerifyContactResponse {
                valid: false,
                message: "Invalid verification code".to_string(),
            }),
        )
            .into_response()
    }
}

#[utoipa::path(
    get,
    path = "/api/block-headers/current",
    responses(
        (status = 200, description = "Current block header from database", body = BlockHeader),
        (status = 404, description = "No block header found", body = ErrorResponse),
    ),
    tag = "blockchain"
)]
pub async fn get_current_block_header(
    State((app_services, _stripe_billing, _config)): State<(
        AppServicesState,
        StripeBillingState,
        ConfigState,
    )>,
) -> Response {
    let start_time = std::time::Instant::now();

    let result = app_services.metadata_db.get_current_block_header().await;

    let elapsed = start_time.elapsed();
    info!("get_current_block_header completed in {:?}", elapsed);

    match result {
        Ok(Some(block_header)) => (StatusCode::OK, Json(block_header)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "No block header found".to_string(),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/wallets",
    responses(
        (status = 200, description = "List of all wallets", body = WalletsListResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "wallet",
    security(
        ("bearer_auth" = [])
    )
)]

pub async fn get_wallets_list(
    State((app_services, _stripe_billing, config)): State<(
        AppServicesState,
        StripeBillingState,
        ConfigState,
    )>,
    headers: HeaderMap,
) -> Response {
    let start_time = std::time::Instant::now();

    // Authenticate user based on operating mode
    let user = match authenticate_user_mode_aware(
        &config,
        headers.get("authorization").and_then(|h| h.to_str().ok()),
    ) {
        Ok(user) => user,
        Err(err) => {
            return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: err })).into_response();
        }
    };

    // No mutex blocking! Direct access to metadata database
    match app_services
        .get_wallets_list_for_user(&user.user_id, user.is_admin)
        .await
    {
        Ok(wallets_response) => {
            let response_time = start_time.elapsed();
            println!("⚡ Non-blocking wallet list served in {:?}", response_time);
            (StatusCode::OK, Json(wallets_response)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to get wallets list: {}", e),
            }),
        )
            .into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/wallets/{id}/detail",
    responses(
        (status = 200, description = "Wallet detail with transaction events", body = WalletDetailResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Access denied", body = ErrorResponse),
        (status = 404, description = "Wallet not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    params(
        ("id" = i64, Path, description = "Wallet ID")
    ),
    tag = "wallet",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn get_wallet_detail(
    State((app_services, _stripe_billing, config)): State<(
        AppServicesState,
        StripeBillingState,
        ConfigState,
    )>,
    Path(checksum): Path<String>,
    headers: HeaderMap,
) -> Response {
    let start_time = std::time::Instant::now();

    // Authenticate user based on operating mode
    let user = match authenticate_user_mode_aware(
        &config,
        headers.get("authorization").and_then(|h| h.to_str().ok()),
    ) {
        Ok(user) => user,
        Err(err) => {
            return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: err })).into_response();
        }
    };

    // Get current timestamp
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Get the specific wallet - no mutex blocking!
    let wallet = match app_services
        .metadata_db
        .get_wallet_by_checksum(&checksum)
        .await
    {
        Ok(Some(wallet)) => wallet,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Wallet not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Database error: {}", e),
                }),
            )
                .into_response();
        }
    };

    // Check if user has permission to access this wallet
    if !user.is_admin {
        match app_services
            .metadata_db
            .is_wallet_owned_by_user(&checksum, &user.user_id)
            .await
        {
            Ok(true) => {} // User owns the wallet
            Ok(false) => {
                return (
                    StatusCode::FORBIDDEN,
                    Json(ErrorResponse {
                        error: "Access denied to wallet".to_string(),
                    }),
                )
                    .into_response();
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Database error: {}", e),
                    }),
                )
                    .into_response();
            }
        }
    }

    // Check if wallet is pending - if so, return minimal data only
    if wallet.status == "pending" {
        // Get contacts - these are available even for pending wallets
        let contacts = match app_services
            .metadata_db
            .get_contacts_with_notification_methods(&wallet.checksum)
            .await
        {
            Ok(contacts) => contacts,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Failed to get contacts: {}", e),
                    }),
                )
                    .into_response();
            }
        };

        let response_time = start_time.elapsed();
        println!(
            "⚡ Non-blocking wallet detail (pending) served in {:?}",
            response_time
        );

        let wallet_detail = WalletDetailResponse {
            timestamp,
            wallet,
            events: vec![], // Empty events for pending wallets
            contacts,
        };

        return (StatusCode::OK, Json(wallet_detail)).into_response();
    }

    // Get transaction events - no mutex blocking!
    let events = match app_services
        .metadata_db
        .get_events_by_wallet_checksum(&wallet.checksum, None)
        .await
    {
        Ok(events) => events,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to get events: {}", e),
                }),
            )
                .into_response();
        }
    };

    // Get contacts - no mutex blocking!
    let contacts = match app_services
        .metadata_db
        .get_contacts_with_notification_methods(&wallet.checksum)
        .await
    {
        Ok(contacts) => contacts,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to get contacts: {}", e),
                }),
            )
                .into_response();
        }
    };

    let response_time = start_time.elapsed();
    println!(
        "⚡ Non-blocking wallet detail served in {:?}",
        response_time
    );

    let wallet_detail = WalletDetailResponse {
        timestamp,
        wallet,
        events,
        contacts,
    };

    (StatusCode::OK, Json(wallet_detail)).into_response()
}

// Auth endpoints
#[utoipa::path(
    post,
    path = "/api/auth/register",
    request_body = RegisterRequest,
    responses(
        (status = 200, description = "Registration successful, verification email sent", body = serde_json::Value),
        (status = 400, description = "Invalid email, weak password, or user already exists", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "auth"
)]
pub async fn register(
    State((app_services, stripe_billing)): State<(AppServicesState, StripeBillingState)>,
    Json(request): Json<RegisterRequest>,
) -> Response {
    let start_time = std::time::Instant::now();

    // Validate email format
    if !request.email.contains('@') || request.email.len() < 5 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid email format".to_string(),
            }),
        )
            .into_response();
    }

    // Validate password strength
    if request.password.len() < 6 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Password must be at least 6 characters long".to_string(),
            }),
        )
            .into_response();
    }

    // Check if user already exists - no mutex blocking!
    match app_services
        .metadata_db
        .get_user_by_email(&request.email)
        .await
    {
        Ok(Some(_)) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "User with this email already exists".to_string(),
                }),
            )
                .into_response();
        }
        Ok(None) => {} // User doesn't exist, proceed
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to check user existence: {}", e),
                }),
            )
                .into_response();
        }
    }

    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "your-secret-key-change-in-production".to_string());

    let email_service = match EmailService::from_env() {
        Ok(service) => Some(service),
        Err(_) => None, // Email service not configured, will work in dev mode
    };

    let auth_service = AuthService::new(jwt_secret, email_service);

    // Check if this is a dev test email
    let is_dev_email = auth_service.is_dev_test_email(&request.email);

    // Hash password
    let password_hash = match auth_service.hash_password(&request.password) {
        Ok(hash) => hash,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to hash password: {}", e),
                }),
            )
                .into_response();
        }
    };

    // For dev mode, auto-verify emails. For production, require verification.
    let email_verified = is_dev_email;

    // Create user - no mutex blocking!
    let user_id = match app_services
        .metadata_db
        .create_user(
            &request.email,
            &password_hash,
            Some(&request.name),
            email_verified,
        )
        .await
    {
        Ok(id) => id,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to create user: {}", e),
                }),
            )
                .into_response();
        }
    };

    // Send admin notification for new user signup (fire-and-forget)
    {
        let admin_notifications = AdminNotifications::new();
        if admin_notifications.is_enabled() {
            let email = request.email.clone();
            let name = request.name.clone();
            tokio::spawn(async move {
                admin_notifications
                    .notify_user_signup(&email, Some(&name))
                    .await;
            });
        }
    }

    // Create Stripe trial subscription for the user
    {
        // Get the created user to pass to Stripe - no mutex blocking!
        let user_record = match app_services.metadata_db.get_user_by_id(&user_id).await {
            Ok(Some(user)) => user,
            Ok(None) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "User was created but could not be retrieved".to_string(),
                    }),
                )
                    .into_response();
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Failed to retrieve user: {}", e),
                    }),
                )
                    .into_response();
            }
        };

        // Create Stripe customer (but not subscription yet - that starts when they add their first wallet)
        if let Some(stripe_service) = &stripe_billing {
            if let Err(e) = stripe_service
                .create_stripe_customer_only(&user_record, &app_services.metadata_db)
                .await
            {
                tracing::error!(
                    "Failed to create Stripe customer for user {}: {}",
                    user_record.email,
                    e
                );
                // Don't fail registration if Stripe fails, but log the error
                // User can still use the service, they just won't have Stripe integration
            }
        } else {
            tracing::info!(
                "Stripe not enabled, user {} registered without Stripe integration",
                user_record.email
            );
        }
    }

    // Add to marketing audience if opted in
    if request.marketing_emails_opt_in {
        if let Some(email_service) = &auth_service.email_service {
            tokio::spawn({
                let email = request.email.clone();
                let name = request.name.clone();
                let email_service = email_service.clone();
                async move {
                    if let Err(e) = email_service.add_to_marketing_audience(&email, &name).await {
                        eprintln!("Failed to add user to marketing audience: {}", e);
                    }
                }
            });
        }
    }

    // Send verification email for non-dev accounts
    if !is_dev_email {
        let token = auth_service.generate_verification_token();

        // Store verification token - no mutex blocking!
        if let Err(e) = app_services
            .metadata_db
            .create_email_verification_token(&user_id, &token)
            .await
        {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to create verification token: {}", e),
                }),
            )
                .into_response();
        }

        // Send verification email
        if let Err(e) = auth_service
            .send_email_verification(&request.email, &request.name, &token)
            .await
        {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Failed to send verification email: {}", e),
                }),
            )
                .into_response();
        }

        let elapsed = start_time.elapsed();
        info!("register completed in {:?}", elapsed);

        Json(serde_json::json!({
            "message": "Registration successful. Please check your email to verify your account."
        }))
        .into_response()
    } else {
        // Dev mode: return success immediately
        let elapsed = start_time.elapsed();
        info!("register completed in {:?}", elapsed);

        Json(serde_json::json!({
            "message": "Registration successful (dev mode - no email verification required)."
        }))
        .into_response()
    }
}

#[utoipa::path(
    post,
    path = "/api/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = AuthResponse),
        (status = 400, description = "Invalid credentials", body = ErrorResponse),
        (status = 403, description = "Email not verified", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "auth"
)]
pub async fn login(
    State((app_services, _stripe_billing, _config)): State<(
        AppServicesState,
        StripeBillingState,
        ConfigState,
    )>,
    Json(request): Json<LoginRequest>,
) -> Response {
    let start_time = std::time::Instant::now();

    // Check if user exists - no mutex blocking!
    let user_record = match app_services
        .metadata_db
        .get_user_by_email(&request.email)
        .await
    {
        Ok(Some(user)) => user,
        Ok(None) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Invalid credentials".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to check user: {}", e),
                }),
            )
                .into_response();
        }
    };

    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "your-secret-key-change-in-production".to_string());

    let email_service = match EmailService::from_env() {
        Ok(service) => {
            println!("✅ Email service initialized successfully");
            Some(service)
        }
        Err(e) => {
            eprintln!("❌ Failed to initialize email service: {}", e);
            None // Email service not configured, will work in dev mode
        }
    };

    let auth_service = AuthService::new(jwt_secret, email_service);

    // Check if this is a dev test email (bypass password check)
    let is_dev_email = auth_service.is_dev_test_email(&request.email);

    // Verify password
    let password_valid = if is_dev_email {
        // For dev emails, check against dev test password
        request.password == auth_service.get_dev_test_password()
    } else {
        // For regular users, verify against stored hash
        match auth_service.verify_password(&request.password, &user_record.password_hash) {
            Ok(valid) => valid,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Failed to verify password: {}", e),
                    }),
                )
                    .into_response();
            }
        }
    };

    if !password_valid {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid credentials".to_string(),
            }),
        )
            .into_response();
    }

    // Check email verification (skip for dev emails)
    if !is_dev_email && !user_record.email_verified {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error:
                    "Email not verified. Please check your email and click the verification link."
                        .to_string(),
            }),
        )
            .into_response();
    }

    // Update last login
    if let Err(e) = app_services
        .metadata_db
        .update_last_login(&user_record.id)
        .await
    {
        eprintln!(
            "Failed to update last login for user {}: {:?}",
            user_record.id, e
        );
    }

    // Generate JWT token
    let token = match auth_service.generate_token(
        &user_record.id,
        &user_record.email,
        user_record.is_admin,
    ) {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to generate token: {}", e),
                }),
            )
                .into_response();
        }
    };

    // Create session
    let token_hash = AuthService::hash_token(&token);
    let expires_at = chrono::Utc::now() + chrono::Duration::days(7);

    if let Err(e) = app_services
        .metadata_db
        .create_session(&user_record.id, &token_hash, expires_at)
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to create session: {}", e),
            }),
        )
            .into_response();
    }

    // Build user response
    let user_info = AuthUserResponse {
        id: user_record.id,
        email: user_record.email,
        name: user_record.name,
        is_admin: user_record.is_admin,
        email_verified: user_record.email_verified,
        subscription_tier: user_record.subscription_tier,
        created_at: user_record.created_at,
    };

    let response_time = start_time.elapsed();
    println!("⚡ Non-blocking login completed in {:?}", response_time);

    Json(AuthResponse {
        token,
        user: user_info,
        requires_name: None, // No longer used with email auth
    })
    .into_response()
}

#[utoipa::path(
    get,
    path = "/api/auth/verify-email/{token}",
    params(
        ("token" = String, Path, description = "Email verification token")
    ),
    responses(
        (status = 200, description = "Email verified successfully", body = serde_json::Value),
        (status = 400, description = "Invalid or expired token", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "auth"
)]
pub async fn verify_email(
    State((app_services, _stripe_billing)): State<(AppServicesState, StripeBillingState)>,
    Path(token): Path<String>,
) -> Response {
    let start_time = std::time::Instant::now();

    // Direct metadata access - no mutex blocking!
    let result = match app_services.metadata_db.verify_email_token(&token).await {
        Ok(Some(_user_id)) => Json(serde_json::json!({
            "message": "Email verified successfully. You can now log in."
        }))
        .into_response(),
        Ok(None) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid or expired verification token".to_string(),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to verify email: {}", e),
            }),
        )
            .into_response(),
    };

    let elapsed = start_time.elapsed();
    info!("verify_email completed in {:?}", elapsed);
    result
}

#[utoipa::path(
    post,
    path = "/api/auth/forgot-password",
    request_body = ForgotPasswordRequest,
    responses(
        (status = 200, description = "Password reset email sent", body = serde_json::Value),
        (status = 400, description = "User not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "auth"
)]
pub async fn forgot_password(
    State((app_services, _stripe_billing)): State<(AppServicesState, StripeBillingState)>,
    Json(request): Json<ForgotPasswordRequest>,
) -> Response {
    let start_time = std::time::Instant::now();

    // Check if user exists - no mutex blocking!
    let user_record = match app_services
        .metadata_db
        .get_user_by_email(&request.email)
        .await
    {
        Ok(Some(user)) => user,
        Ok(None) => {
            // Don't reveal whether user exists or not
            return Json(serde_json::json!({
                "message": "If an account with that email exists, a password reset link has been sent."
            }))
            .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to check user: {}", e),
                }),
            )
                .into_response();
        }
    };

    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "your-secret-key-change-in-production".to_string());

    let email_service = match EmailService::from_env() {
        Ok(service) => Some(service),
        Err(_) => None,
    };

    let auth_service = AuthService::new(jwt_secret, email_service);
    let token = auth_service.generate_verification_token();

    // Store password reset token - no mutex blocking!
    if let Err(e) = app_services
        .metadata_db
        .create_password_reset_token(&user_record.id, &token)
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to create reset token: {}", e),
            }),
        )
            .into_response();
    }

    // Send password reset email
    if let Err(e) = auth_service
        .send_password_reset(
            &user_record.email,
            &user_record.name.unwrap_or_else(|| "User".to_string()),
            &token,
        )
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to send reset email: {}", e),
            }),
        )
            .into_response();
    }

    let elapsed = start_time.elapsed();
    info!("forgot_password completed in {:?}", elapsed);

    Json(serde_json::json!({
        "message": "If an account with that email exists, a password reset link has been sent."
    }))
    .into_response()
}

#[utoipa::path(
    post,
    path = "/api/auth/reset-password/{token}",
    params(
        ("token" = String, Path, description = "Password reset token")
    ),
    request_body = ResetPasswordRequest,
    responses(
        (status = 200, description = "Password reset successfully", body = serde_json::Value),
        (status = 400, description = "Invalid or expired token", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "auth"
)]
pub async fn reset_password(
    State((app_services, _stripe_billing)): State<(AppServicesState, StripeBillingState)>,
    Path(token): Path<String>,
    Json(request): Json<ResetPasswordRequest>,
) -> Response {
    let start_time = std::time::Instant::now();

    // Validate password strength
    if request.password.len() < 6 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Password must be at least 6 characters long".to_string(),
            }),
        )
            .into_response();
    }

    // Verify token and get user ID - no mutex blocking!
    let user_id = match app_services
        .metadata_db
        .verify_password_reset_token(&token)
        .await
    {
        Ok(Some(id)) => id,
        Ok(None) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Invalid or expired reset token".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to verify token: {}", e),
                }),
            )
                .into_response();
        }
    };

    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "your-secret-key-change-in-production".to_string());

    let auth_service = AuthService::new(jwt_secret, None);

    // Hash new password
    let password_hash = match auth_service.hash_password(&request.password) {
        Ok(hash) => hash,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to hash password: {}", e),
                }),
            )
                .into_response();
        }
    };

    // Update password and clear reset tokens - no mutex blocking!
    if let Err(e) = app_services
        .metadata_db
        .update_user_password(user_id, &password_hash)
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to update password: {}", e),
            }),
        )
            .into_response();
    }

    let elapsed = start_time.elapsed();
    info!("reset_password completed in {:?}", elapsed);

    Json(serde_json::json!({
        "message": "Password reset successfully. You can now log in with your new password."
    }))
    .into_response()
}

#[utoipa::path(
    post,
    path = "/api/auth/logout",
    responses(
        (status = 200, description = "Logged out successfully", body = serde_json::Value),
        (status = 401, description = "Unauthorized", body = ErrorResponse)
    ),
    tag = "auth",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn logout(
    State((app_services, _stripe_billing, _config)): State<(
        AppServicesState,
        StripeBillingState,
        ConfigState,
    )>,
    headers: HeaderMap,
) -> Response {
    let start_time = std::time::Instant::now();

    // Get the token from the Authorization header
    let auth_header = match headers.get("authorization").and_then(|h| h.to_str().ok()) {
        Some(header) => header,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Authentication required".to_string(),
                }),
            )
                .into_response();
        }
    };

    if !auth_header.starts_with("Bearer ") {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Invalid authorization header".to_string(),
            }),
        )
            .into_response();
    }

    let token = &auth_header[7..]; // Skip "Bearer "

    // Hash the token to find it in the database
    let token_hash = AuthService::hash_token(token);

    // Direct metadata access - no mutex blocking!
    let result = app_services.metadata_db.delete_session(&token_hash).await;

    let elapsed = start_time.elapsed();
    info!("logout completed in {:?}", elapsed);

    if let Err(e) = result {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to delete session: {}", e),
            }),
        )
            .into_response();
    }

    Json(serde_json::json!({
        "message": "Logged out successfully"
    }))
    .into_response()
}

#[utoipa::path(
    get,
    path = "/api/auth/me",
    responses(
        (status = 200, description = "Current user info", body = AuthUserResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse)
    ),
    tag = "auth",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn me(
    State((app_services, _stripe_billing, _config)): State<(
        AppServicesState,
        StripeBillingState,
        ConfigState,
    )>,
    headers: HeaderMap,
) -> Response {
    let start_time = std::time::Instant::now();

    // Authenticate user
    let user = match authenticate_user(headers.get("authorization").and_then(|h| h.to_str().ok())) {
        Ok(user) => user,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Authentication required".to_string(),
                }),
            )
                .into_response();
        }
    };

    // Get user info from database - no mutex blocking!
    let user_info = match app_services.metadata_db.get_user_by_id(&user.user_id).await {
        Ok(Some(db_user)) => AuthUserResponse {
            id: db_user.id,
            email: db_user.email,
            name: db_user.name,
            is_admin: user.is_admin,
            email_verified: db_user.email_verified,
            subscription_tier: db_user.subscription_tier,
            created_at: db_user.created_at,
        },
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "User not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to get user info".to_string(),
                }),
            )
                .into_response();
        }
    };

    let elapsed = start_time.elapsed();
    info!("me completed in {:?}", elapsed);

    Json(serde_json::json!({ "user": user_info })).into_response()
}

#[utoipa::path(
    put,
    path = "/api/auth/user",
    request_body = UpdateUserRequest,
    responses(
        (status = 200, description = "User profile updated successfully", body = UpdateUserResponse),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 401, description = "Authentication required", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "auth"
)]
pub async fn update_user(
    State((app_services, _stripe_billing)): State<(AppServicesState, StripeBillingState)>,
    headers: HeaderMap,
    Json(request): Json<UpdateUserRequest>,
) -> Response {
    let start_time = std::time::Instant::now();
    // Authenticate user
    let user = match authenticate_user(headers.get("authorization").and_then(|h| h.to_str().ok())) {
        Ok(user) => user,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Authentication required".to_string(),
                }),
            )
                .into_response();
        }
    };

    // Validate name is not empty
    if request.name.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Name cannot be empty".to_string(),
            }),
        )
            .into_response();
    }

    // Update user name in database - no mutex blocking!
    if let Err(e) = app_services
        .metadata_db
        .update_user_name(&user.user_id, request.name.trim())
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to update user: {}", e),
            }),
        )
            .into_response();
    }

    // Get updated user info - no mutex blocking!
    let user_info = match app_services.metadata_db.get_user_by_id(&user.user_id).await {
        Ok(Some(db_user)) => AuthUserResponse {
            id: db_user.id,
            email: db_user.email,
            name: db_user.name,
            is_admin: user.is_admin,
            email_verified: db_user.email_verified,
            subscription_tier: db_user.subscription_tier,
            created_at: db_user.created_at,
        },
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "User not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to get user: {}", e),
                }),
            )
                .into_response();
        }
    };

    let elapsed = start_time.elapsed();
    info!("update_user completed in {:?}", elapsed);

    Json(UpdateUserResponse { user: user_info }).into_response()
}

#[utoipa::path(
    get,
    path = "/api/providers",
    responses(
        (status = 200, description = "Available notification providers", body = ProvidersResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    ),
    tag = "providers"
)]
pub async fn get_providers(
    State(notification_manager): State<NotificationManagerState>,
) -> Response {
    #[allow(unused_mut)]
    let mut manager = notification_manager.lock().await;
    let providers = manager.list_providers();
    (StatusCode::OK, Json(ProvidersResponse { providers })).into_response()
}

// Stripe billing endpoints
#[utoipa::path(
    post,
    path = "/api/stripe/checkout",
    request_body = CreateCheckoutSessionRequest,
    responses(
        (status = 200, description = "Checkout session created successfully", body = CheckoutSessionResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 401, description = "Authentication required", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    )
)]
pub async fn create_stripe_checkout_session(
    State((app_services, stripe_billing)): State<(AppServicesState, StripeBillingState)>,
    headers: HeaderMap,
    Json(payload): Json<CreateCheckoutSessionRequest>,
) -> Response {
    let start_time = std::time::Instant::now();
    // Authenticate user
    let user = match authenticate_user(headers.get("authorization").and_then(|h| h.to_str().ok())) {
        Ok(user) => user,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Authentication required".to_string(),
                }),
            )
                .into_response();
        }
    };

    // Parse subscription tier
    let tier = match payload.tier.as_str() {
        "personal" => SubscriptionTier::Personal,
        "team" => SubscriptionTier::Team,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Invalid subscription tier".to_string(),
                }),
            )
                .into_response();
        }
    };

    // Get user record - no mutex blocking!
    let user_record = match app_services.metadata_db.get_user_by_id(&user.user_id).await {
        Ok(Some(user_record)) => user_record,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "User not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Database error: {}", e),
                }),
            )
                .into_response();
        }
    };

    // Get Stripe billing from state
    let stripe_billing = match stripe_billing.as_ref() {
        Some(billing) => billing.as_ref(),
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Stripe billing not initialized".to_string(),
                }),
            )
                .into_response();
        }
    };

    // Create checkout session with configurable URLs
    let is_yearly = payload.is_yearly.unwrap_or(false);
    let billing_cycle = if is_yearly { "yearly" } else { "monthly" };
    let frontend_url = std::env::var("FRONTEND_URL").expect("FRONTEND_URL must be set");
    let success_url = format!("{}/settings/subscription?success=true", frontend_url);
    let cancel_url = format!("{}/settings/subscription?cancelled=true", frontend_url);

    let result = stripe_billing
        .create_checkout_session(
            &user_record.id,
            tier,
            billing_cycle,
            &success_url,
            &cancel_url,
            &app_services.metadata_db,
        )
        .await;

    let elapsed = start_time.elapsed();
    info!("create_stripe_checkout_session completed in {:?}", elapsed);

    match result {
        Ok(session) => (StatusCode::OK, Json(session)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to create checkout session: {}", e),
            }),
        )
            .into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/stripe/portal",
    request_body = CreateCustomerPortalRequest,
    responses(
        (status = 200, description = "Customer portal session created", body = CustomerPortalResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 401, description = "Authentication required", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    )
)]
pub async fn create_stripe_customer_portal(
    State((app_services, stripe_billing)): State<(AppServicesState, StripeBillingState)>,
    headers: HeaderMap,
    Json(payload): Json<CreateCustomerPortalRequest>,
) -> Response {
    let start_time = std::time::Instant::now();
    // Authenticate user
    let user = match authenticate_user(headers.get("authorization").and_then(|h| h.to_str().ok())) {
        Ok(user) => user,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Authentication required".to_string(),
                }),
            )
                .into_response();
        }
    };

    // Get user record - no mutex blocking!
    let user_record = match app_services.metadata_db.get_user_by_id(&user.user_id).await {
        Ok(Some(user_record)) => user_record,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "User not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Database error: {}", e),
                }),
            )
                .into_response();
        }
    };

    // User must have a Stripe customer ID
    let customer_id = match &user_record.stripe_customer_id {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "No Stripe customer found. Please create a subscription first."
                        .to_string(),
                }),
            )
                .into_response();
        }
    };

    // Get Stripe billing from state
    let stripe_billing = match stripe_billing.as_ref() {
        Some(billing) => billing.as_ref(),
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Stripe billing not initialized".to_string(),
                }),
            )
                .into_response();
        }
    };

    // Create customer portal session
    tracing::info!(
        "Creating customer portal session for customer_id: {}, return_url: {}",
        customer_id,
        payload.return_url
    );
    match stripe_billing
        .create_customer_portal_session(customer_id, &payload.return_url)
        .await
    {
        Ok(session) => {
            tracing::info!(
                "✅ Customer portal session created successfully: {}",
                session.url
            );
            let elapsed = start_time.elapsed();
            info!("create_stripe_customer_portal completed in {:?}", elapsed);
            (StatusCode::OK, Json(session)).into_response()
        }
        Err(e) => {
            tracing::error!("❌ Failed to create customer portal session: {}", e);
            let elapsed = start_time.elapsed();
            info!("create_stripe_customer_portal completed in {:?}", elapsed);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to create customer portal session: {}", e),
                }),
            )
                .into_response()
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/billing/status",
    responses(
        (status = 200, description = "Current billing status", body = BillingStatusResponse),
        (status = 401, description = "Authentication required", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    )
)]
pub async fn get_billing_status(
    State((app_services, _stripe_billing, config)): State<(
        AppServicesState,
        StripeBillingState,
        ConfigState,
    )>,
    headers: HeaderMap,
) -> Response {
    let start_time = std::time::Instant::now();

    // Authenticate user
    let user = match authenticate_user(headers.get("authorization").and_then(|h| h.to_str().ok())) {
        Ok(user) => user,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Authentication required".to_string(),
                }),
            )
                .into_response();
        }
    };

    // Direct metadata access - no mutex blocking!
    let user_record = match app_services.metadata_db.get_user_by_id(&user.user_id).await {
        Ok(Some(user_record)) => user_record,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "User not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Database error: {}", e),
                }),
            )
                .into_response();
        }
    };

    // Get wallet count for this user
    let wallet_count = app_services
        .metadata_db
        .count_wallets_for_user(&user.user_id)
        .await
        .unwrap_or(0);

    // Get contact count across all wallets for this user
    let user_wallets = app_services
        .metadata_db
        .get_wallets_for_user(Some(&user.user_id))
        .await
        .unwrap_or_default();
    let contact_count = {
        let mut total = 0;
        for wallet in &user_wallets {
            total += app_services
                .metadata_db
                .count_contacts_for_wallet(&wallet.checksum)
                .await
                .unwrap_or(0);
        }
        total
    };

    // Get tier limits with actual network-specific sync intervals
    let tier_limits = user_record.subscription_tier.limits_for_api();
    let (personal_sync, team_sync) = user_record.subscription_tier.get_sync_intervals(&config.network);
    let actual_sync_interval = match user_record.subscription_tier {
        crate::subscription::SubscriptionTier::Personal => personal_sync,
        crate::subscription::SubscriptionTier::Team => team_sync,
    };
    
    let limits = BillingTierLimits {
        max_wallets: tier_limits.max_wallets.map(|n| n as i32).unwrap_or(-1),
        max_contacts_per_wallet: tier_limits
            .max_contacts_per_wallet
            .map(|n| n as i32)
            .unwrap_or(-1),
        sync_interval_seconds: actual_sync_interval,
    };

    let response = BillingStatusResponse {
        user_id: user.user_id.clone(),
        subscription_tier: user_record.subscription_tier.as_str().to_string(),
        subscription_status: user_record.subscription_status,
        trial_ends_at: user_record.trial_ends_at,
        subscription_started_at: user_record.subscription_started_at,
        stripe_customer_id: user_record.stripe_customer_id,
        wallet_count,
        contact_count,
        limits,
    };

    let elapsed = start_time.elapsed();
    info!("get_billing_status completed in {:?}", elapsed);

    (StatusCode::OK, Json(response)).into_response()
}

/// Get pricing information from Stripe
#[utoipa::path(
    get,
    path = "/api/billing/pricing",
    responses(
        (status = 200, description = "Pricing information", body = PricingInfo),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "Billing"
)]
pub async fn get_billing_pricing(
    State((_app_services, stripe_billing)): State<(AppServicesState, StripeBillingState)>,
) -> Response {
    // Get Stripe billing from state
    let stripe_billing = match stripe_billing.as_ref() {
        Some(billing) => billing.as_ref(),
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Stripe billing not initialized".to_string(),
                }),
            )
                .into_response();
        }
    };

    // Get cached pricing information (instant!)
    let pricing = stripe_billing.get_pricing_for_frontend();
    (StatusCode::OK, Json(pricing)).into_response()
}

/// Handle Stripe webhook events
#[utoipa::path(
    post,
    path = "/api/stripe/webhook",
    request_body = String,
    responses(
        (status = 200, description = "Webhook processed successfully"),
        (status = 400, description = "Invalid webhook signature", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "Billing"
)]
pub async fn handle_stripe_webhook(
    State((app_services, stripe_billing)): State<(AppServicesState, StripeBillingState)>,
    headers: HeaderMap,
    body: String,
) -> Response {
    let _start_time = std::time::Instant::now();
    // Debug: Log all headers
    tracing::info!("Webhook headers received: {:?}", headers);

    // Get Stripe signature from headers (case-insensitive lookup)
    let signature = match headers.get("stripe-signature") {
        Some(sig) => match sig.to_str() {
            Ok(s) => s,
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "Invalid stripe-signature header".to_string(),
                    }),
                )
                    .into_response();
            }
        },
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Missing stripe-signature header".to_string(),
                }),
            )
                .into_response();
        }
    };

    // Get Stripe billing from state
    let stripe_billing = match stripe_billing.as_ref() {
        Some(billing) => billing.as_ref(),
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Stripe billing not initialized".to_string(),
                }),
            )
                .into_response();
        }
    };

    // Handle the webhook
    tracing::info!("🎣 Processing Stripe webhook with signature: {}", signature);
    match stripe_billing
        .handle_webhook(body.as_bytes(), signature)
        .await
    {
        Ok(webhook_result) => {
            // Process any subscription updates - no mutex blocking!
            for update in webhook_result.subscription_updates {
                tracing::info!(
                    "Processing subscription update for user {}: {} -> {}",
                    update.user_id,
                    update.subscription_tier,
                    update.subscription_status
                );

                // Check if this is a customer ID lookup (from subscription cancellation)
                let actual_user_id = if update.user_id.starts_with("stripe_customer:") {
                    let customer_id = update.user_id.strip_prefix("stripe_customer:").unwrap();
                    tracing::info!("Looking up user by Stripe customer ID: {}", customer_id);

                    // Find user by Stripe customer ID - no mutex blocking!
                    match app_services
                        .metadata_db
                        .get_user_by_stripe_customer_id(customer_id)
                        .await
                    {
                        Ok(Some(user)) => {
                            tracing::info!("Found user {} for customer {}", user.id, customer_id);
                            user.id
                        }
                        Ok(None) => {
                            tracing::warn!("No user found for Stripe customer ID: {}", customer_id);
                            continue; // Skip this update
                        }
                        Err(e) => {
                            tracing::error!(
                                "Failed to lookup user by customer ID {}: {}",
                                customer_id,
                                e
                            );
                            continue; // Skip this update
                        }
                    }
                } else {
                    update.user_id.clone()
                };

                // Handle special "keep_current" tier for cancellations
                if update.subscription_tier == "keep_current" {
                    // For cancellations, just update the status, not the tier - no mutex blocking!
                    if let Err(e) = app_services
                        .metadata_db
                        .update_user_subscription_status(
                            &actual_user_id,
                            &update.subscription_status,
                            update.stripe_subscription_id.as_deref(),
                        )
                        .await
                    {
                        tracing::error!(
                            "Failed to update user {} subscription status: {}",
                            actual_user_id,
                            e
                        );
                    } else {
                        tracing::info!(
                            "✅ Updated user {} subscription status to {} (keeping current tier)",
                            actual_user_id,
                            update.subscription_status
                        );
                    }
                } else {
                    // Regular subscription update (tier + status) - no mutex blocking!
                    if let Err(e) = app_services
                        .metadata_db
                        .update_user_subscription(
                            &actual_user_id,
                            &update.subscription_tier,
                            &update.subscription_status,
                            update.stripe_subscription_id.as_deref(),
                            update.subscription_started_at.as_deref(),
                            update.subscription_ends_at.as_deref(),
                            update.trial_ends_at.as_deref(),
                        )
                        .await
                    {
                        tracing::error!(
                            "Failed to update user {} subscription: {}",
                            actual_user_id,
                            e
                        );
                    } else {
                        tracing::info!(
                            "✅ Updated user {} subscription to {} ({})",
                            actual_user_id,
                            update.subscription_tier,
                            update.subscription_status
                        );

                        // Apply subscription tier limits to wallets and contacts
                        // First, get user record to check admin status - no mutex blocking!
                        match app_services
                            .metadata_db
                            .get_user_by_id(&actual_user_id)
                            .await
                        {
                            Ok(Some(user_record)) => {
                                if let Err(e) = app_services
                                    .apply_subscription_limits(
                                        &actual_user_id,
                                        &update.subscription_tier,
                                        user_record.is_admin,
                                    )
                                    .await
                                {
                                    tracing::error!(
                                        "Failed to apply subscription limits for user {}: {}",
                                        actual_user_id,
                                        e
                                    );
                                } else {
                                    if user_record.is_admin {
                                        tracing::info!(
                                            "✅ Applied unlimited limits for admin user {}",
                                            actual_user_id
                                        );
                                    } else {
                                        tracing::info!(
                                            "✅ Applied {} tier limits for user {}",
                                            update.subscription_tier,
                                            actual_user_id
                                        );
                                    }
                                }
                            }
                            Ok(None) => {
                                tracing::error!(
                                    "User {} not found when applying limits",
                                    actual_user_id
                                );
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Failed to fetch user {} when applying limits: {}",
                                    actual_user_id,
                                    e
                                );
                            }
                        }
                    }
                }
            }

            tracing::info!("✅ Webhook processed successfully");
            (StatusCode::OK, "OK").into_response()
        }
        Err(e) => {
            tracing::error!("❌ Webhook processing failed: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Webhook processing failed: {}", e),
                }),
            )
                .into_response()
        }
    }
}

/// Get checkout session details
#[utoipa::path(
    get,
    path = "/api/billing/session/{session_id}",
    params(
        ("session_id" = String, Path, description = "Stripe checkout session ID")
    ),
    responses(
        (status = 200, description = "Session details", body = CheckoutSessionDetails),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "Billing"
)]
pub async fn get_checkout_session_details(
    State((_app_services, stripe_billing)): State<(AppServicesState, StripeBillingState)>,
    Path(session_id): Path<String>,
) -> Response {
    // Get Stripe billing from state
    let stripe_billing = match stripe_billing.as_ref() {
        Some(billing) => billing.as_ref(),
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Stripe billing not initialized".to_string(),
                }),
            )
                .into_response();
        }
    };

    // Get session details from Stripe
    match stripe_billing
        .get_checkout_session_details(&session_id)
        .await
    {
        Ok(details) => (StatusCode::OK, Json(details)).into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Session not found: {}", e),
            }),
        )
            .into_response(),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(
        create_wallet_non_blocking, update_wallet, delete_wallet, get_wallet,
        get_wallets_list, get_wallet_detail,
        create_wallet_contact, update_wallet_contact, delete_wallet_contact, get_wallet_contacts,
        send_contact_verification, verify_contact,
        get_current_block_header,
        get_providers,
        create_stripe_checkout_session, create_stripe_customer_portal, get_billing_status, get_billing_pricing, handle_stripe_webhook,
        register, login, verify_email, forgot_password, reset_password, logout, me, update_user
    ),
    components(schemas(
        CreateWalletRequest, UpdateWalletRequest, CreateWalletResponse, ErrorResponse, WalletMetadata,
        CreateContactWithMethodsRequest, UpdateContactRequest, NotificationMethodRequest, CreateContactResponse, ProvidersResponse,
        SendContactVerificationRequest, VerifyContactRequest, VerifyContactResponse,
        Contact, NotificationMethod, ProviderType, TransactionEventWithWallet, EventType, Language,
        BlockHeader, WalletsListResponse, WalletDetailResponse, ProviderInfo,
        CreateCheckoutSessionRequest, CreateCustomerPortalRequest, BillingStatusResponse, CheckoutSessionResponse, CustomerPortalResponse,
        PricingInfo, FrontendTierPricing, FrontendPriceInfo,
        RegisterRequest, LoginRequest, ForgotPasswordRequest, ResetPasswordRequest, AuthResponse, AuthUserResponse, UpdateUserRequest, UpdateUserResponse
    )),
    tags(
        (name = "wallet", description = "Wallet management endpoints"),
        (name = "contact", description = "Contact management endpoints"),
        (name = "providers", description = "Notification provider endpoints"),
        (name = "transaction", description = "Transaction events endpoints"),
        (name = "blockchain", description = "Blockchain information endpoints"),
        (name = "auth", description = "Authentication endpoints")
    ),
    info(
        title = "Canary Wallet API",
        version = "0.2.2",
        description = "REST API for creating Bitcoin wallets from multipath descriptors",
    )
)]
pub struct ApiDoc;

pub fn create_router_with_services(
    app_services: AppServicesState,
    notification_manager: NotificationManagerState,
    stripe_billing: StripeBillingState,
    config: AppConfig,
) -> Router {
    let config_state = Arc::new(config);

    // Auth routes (public) - only routes that still use wallet_manager
    // AppServices routes (non-blocking, metadata operations only)
    let app_routes_2param = Router::new()
        .route("/auth/register", post(register))
        .route("/auth/verify-email/{token}", get(verify_email))
        .route("/auth/forgot-password", post(forgot_password))
        .route("/auth/reset-password/{token}", post(reset_password))
        .route("/auth/user", put(update_user))
        .with_state((app_services.clone(), stripe_billing.clone()));

    // Contact verification routes (non-blocking)
    let app_routes_3param = Router::new()
        .route(
            "/wallets/{checksum}/contacts/send-verification",
            post(send_contact_verification),
        )
        .route("/wallets/{checksum}/contacts/verify", post(verify_contact))
        .with_state((
            app_services.clone(),
            stripe_billing.clone(),
            config_state.clone(),
        ));

    // Delete wallet route moved to app_services_routes for non-blocking operation

    let provider_routes = Router::new()
        .route("/providers", get(get_providers))
        .with_state(notification_manager);

    // Only create stripe routes if Stripe billing is available
    let stripe_routes = if stripe_billing.is_some() {
        Router::new()
            .route("/stripe/checkout", post(create_stripe_checkout_session))
            .route("/stripe/portal", post(create_stripe_customer_portal))
            .route("/stripe/webhook", post(handle_stripe_webhook))
            .route("/billing/pricing", get(get_billing_pricing))
            .route(
                "/billing/session/{session_id}",
                get(get_checkout_session_details),
            )
            .with_state((app_services.clone(), stripe_billing.clone()))
    } else {
        Router::new() // Empty router if Stripe not configured
    };

    // Add the remaining AppServices routes
    let app_routes_metadata = Router::new()
        .route(
            "/wallets",
            get(get_wallets_list).post(create_wallet_non_blocking),
        )
        .route(
            "/wallets/{checksum}",
            get(get_wallet).put(update_wallet).delete(delete_wallet),
        )
        .route("/wallets/{checksum}/detail", get(get_wallet_detail))
        .route("/wallets/{checksum}/contacts", get(get_wallet_contacts))
        .route("/wallets/{checksum}/contacts", post(create_wallet_contact))
        .route(
            "/wallets/{wallet_checksum}/contacts/{contact_id}",
            axum::routing::put(update_wallet_contact).delete(delete_wallet_contact),
        )
        .route("/auth/login", post(login))
        .route("/block-headers/current", get(get_current_block_header))
        .with_state((
            app_services.clone(),
            stripe_billing.clone(),
            config_state.clone(),
        ));

    let app_routes_auth = Router::new()
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/billing/status", get(get_billing_status))
        .with_state((
            app_services.clone(),
            stripe_billing.clone(),
            config_state.clone(),
        ));

    let api_routes = app_routes_2param
        .merge(app_routes_3param)
        .merge(app_routes_metadata)
        .merge(app_routes_auth)
        .merge(provider_routes)
        .merge(stripe_routes);

    Router::new()
        .nest("/api", api_routes)
        .layer(CorsLayer::permissive())
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
}
