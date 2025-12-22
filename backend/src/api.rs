use crate::admin_notifications::AdminNotifications;
use crate::handlers::{
    create_wallet_balance_alert, create_wallet_non_blocking, delete_balance_alert, delete_wallet,
    get_current_block_header, get_exchange_rates, get_providers, get_user_preferences, get_wallet,
    get_wallet_balance_alerts, get_wallet_detail, get_wallets_list, update_user_preferences,
    update_wallet,
};
use crate::auth::{
    authenticate_user, load_twilio_config_from_env, AuthResponse, AuthService, AuthUser,
    AuthUserResponse, ForgotPasswordRequest, LoginRequest, RegisterRequest, ResetPasswordRequest,
    UpdateUserRequest, UpdateUserResponse,
};
use crate::config::AppConfig;
use crate::email_service::EmailService;
use crate::exchange_rates;
use crate::metadata::{Language, MetadataDb, ProviderType, WalletsListResponse};
use crate::models::{
    BillingStatusResponse, BillingTierLimits, ContactFormRequest, ContactFormResponse,
    CreateCheckoutSessionRequest, CreateContactResponse, CreateContactWithMethodsRequest,
    CreateCustomerPortalRequest, ErrorResponse, NotificationMethodRequest,
    SendContactVerificationRequest, UpdateContactRequest, VerifyContactRequest,
    VerifyContactResponse,
};
use crate::notifications::NotificationManager;
use crate::stripe_billing::StripeBilling;
use crate::subscription::{check_limit, SubscriptionTier};
use crate::wallet::WalletCreationService;
use axum::{
    extract::{FromRef, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get, post, put},
    Router,
};
use phonenumber::PhoneNumber;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;
use tracing::info;


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
        subscription_status: &str,
        is_admin: bool,
        trial_ends_at: Option<String>,
    ) -> Result<(), anyhow::Error> {
        // Check if subscription has expired or failed payment
        let is_subscription_active = crate::subscription::is_subscription_active(
            subscription_status,
            trial_ends_at.as_deref(),
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
            0 // No active wallets for expired/past_due/canceled subscriptions
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

            // Determine contact limit based on subscription status, tier, and admin status
            let contact_limit = if is_admin {
                usize::MAX // Unlimited for admin
            } else if !is_subscription_active {
                0 // No active contacts for expired/past_due/canceled subscriptions
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

/// Unified application state for all handlers.
/// Contains all state components and implements FromRef for each,
/// allowing custom extractors (like AuthenticatedUser) to access specific state.
#[derive(Clone)]
pub struct AppState {
    pub app_services: AppServicesState,
    pub notification_manager: NotificationManagerState,
    pub stripe_billing: StripeBillingState,
    pub config: ConfigState,
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
/// In cloud mode: authenticate using JWT token
/// In self-hosted mode: return hardcoded self-hosted user
fn authenticate_user_mode_aware(
    config: &AppConfig,
    auth_header: Option<&str>,
) -> Result<AuthUser, String> {
    if config.is_self_hosted_mode() {
        // Self-hosted mode: return hardcoded user
        Ok(AuthUser {
            user_id: "foss-user".to_string(),
            is_admin: true,
            is_demo: false,
        })
    } else {
        // Cloud mode: authenticate using JWT
        authenticate_user(auth_header).map_err(|_| "Authentication required".to_string())
    }
}

/// Check if user is demo and reject write operations
fn reject_if_demo(user: &AuthUser) -> Result<(), Response> {
    if user.is_demo {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "Demo account is read-only. Sign up to create your own wallet at https://canarybitcoin.com".to_string(),
            }),
        )
            .into_response());
    }
    Ok(())
}

// Wallet-specific contact management endpoints

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

    // Reject demo users from creating contacts
    if let Err(response) = reject_if_demo(&user) {
        return response;
    }

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

    // Count existing contacts for the wallet and check limit unless limits are bypassed
    let bypass_limits = config.is_self_hosted_mode() || user_record.is_admin;
    if !bypass_limits {
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

    // Reject demo users from deleting contacts
    if let Err(response) = reject_if_demo(&user) {
        return response;
    }

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

    // Reject demo users from updating contacts
    if let Err(response) = reject_if_demo(&user) {
        return response;
    }

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
        .get_contacts_with_notification_methods_filtered(&wallet_checksum, true)
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
            .send_email_contact_otp(&notification_target, &request.name, &verification_code, &request.language)
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

// Auth endpoints
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

    // Email service not configured, will work in dev mode
    let email_service = EmailService::from_env().ok();

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

    // Determine preferred currency and language from browser locale
    let preferred_currency = request
        .browser_locale
        .as_ref()
        .map(|locale| exchange_rates::ExchangeRateService::locale_to_currency(locale));
    let preferred_language = request
        .browser_locale
        .as_ref()
        .map(|locale| crate::metadata::locale_to_language(locale));

    // Create user - no mutex blocking!
    let user_id = match app_services
        .metadata_db
        .create_user(
            &request.email,
            &password_hash,
            Some(&request.name),
            email_verified,
            preferred_currency,
            preferred_language,
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
        let user_language = preferred_language.unwrap_or("en");
        if let Err(e) = auth_service
            .send_email_verification(&request.email, &request.name, &token, user_language)
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

pub async fn login(
    State((app_services, _stripe_billing, _config)): State<(
        AppServicesState,
        StripeBillingState,
        ConfigState,
    )>,
    Json(request): Json<LoginRequest>,
) -> Response {
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
        user_record.is_demo,
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
        is_demo: user_record.is_demo,
        email_verified: user_record.email_verified,
        subscription_tier: user_record.subscription_tier,
        created_at: user_record.created_at,
        preferred_fiat_currency: user_record.preferred_fiat_currency,
    };

    Json(AuthResponse {
        token,
        user: user_info,
        requires_name: None, // No longer used with email auth
    })
    .into_response()
}

/// Auto-login endpoint for demo account (no password required)
pub async fn demo_login(
    State((app_services, _stripe_billing, config)): State<(
        AppServicesState,
        StripeBillingState,
        ConfigState,
    )>,
) -> Response {
    let start_time = std::time::Instant::now();

    // Only works in cloud mode
    if !config.is_cloud_mode() {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "Demo login is only available in cloud mode".to_string(),
            }),
        )
            .into_response();
    }

    let demo_email = "demo@canarybitcoin.com";

    // Get demo user from database
    let user_record = match app_services.metadata_db.get_user_by_email(demo_email).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Demo user not found. Please ensure backend is running in dev mode."
                        .to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to get demo user: {}", e),
                }),
            )
                .into_response();
        }
    };

    // Verify this is actually a demo user
    if !user_record.is_demo {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "This user is not a demo account".to_string(),
            }),
        )
            .into_response();
    }

    // Generate JWT token for demo user
    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "your-secret-key-change-in-production".to_string());
    let auth_service = AuthService::new(jwt_secret, None);

    let token = match auth_service.generate_token(
        &user_record.id,
        &user_record.email,
        user_record.is_admin,
        user_record.is_demo,
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

    // Clean up any existing sessions for this user to prevent token collision
    let _ = app_services
        .metadata_db
        .delete_user_sessions(&user_record.id)
        .await;

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
        is_demo: user_record.is_demo,
        email_verified: user_record.email_verified,
        subscription_tier: user_record.subscription_tier,
        created_at: user_record.created_at,
        preferred_fiat_currency: user_record.preferred_fiat_currency,
    };

    let elapsed = start_time.elapsed();
    info!("demo_login completed in {:?}", elapsed);

    Json(AuthResponse {
        token,
        user: user_info,
        requires_name: None,
    })
    .into_response()
}

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

    let email_service = EmailService::from_env().ok();

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
    let user_language = user_record.preferred_language.as_deref().unwrap_or("en");
    if let Err(e) = auth_service
        .send_password_reset(
            &user_record.email,
            &user_record.name.unwrap_or_else(|| "User".to_string()),
            &token,
            user_language,
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

pub async fn submit_contact_form(
    State((_app_services, _stripe_billing)): State<(AppServicesState, StripeBillingState)>,
    Json(payload): Json<ContactFormRequest>,
) -> Response {
    let email = payload.email.trim();
    let message = payload.message.trim();

    // Validate email format
    if email.is_empty() || !email.contains('@') || email.len() > 255 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Please provide a valid email address".to_string(),
            }),
        )
            .into_response();
    }

    // Validate message length
    if message.len() < 10 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Message must be at least 10 characters".to_string(),
            }),
        )
            .into_response();
    }

    if message.len() > 5000 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Message must be less than 5000 characters".to_string(),
            }),
        )
            .into_response();
    }

    // Send email using EmailService
    match EmailService::from_env() {
        Ok(email_service) => {
            match email_service
                .send_contact_form_submission(email, message)
                .await
            {
                Ok(_) => {
                    info!("Contact form submitted from {}", email);
                    (
                        StatusCode::OK,
                        Json(ContactFormResponse {
                            message: "Thank you for your message. We'll get back to you soon."
                                .to_string(),
                        }),
                    )
                        .into_response()
                }
                Err(e) => {
                    tracing::error!("Failed to send contact form email: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: "Failed to send message. Please try again later.".to_string(),
                        }),
                    )
                        .into_response()
                }
            }
        }
        Err(e) => {
            tracing::error!("Email service not configured: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Contact form is temporarily unavailable.".to_string(),
                }),
            )
                .into_response()
        }
    }
}

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
            is_demo: user.is_demo,
            email_verified: db_user.email_verified,
            subscription_tier: db_user.subscription_tier,
            created_at: db_user.created_at,
            preferred_fiat_currency: db_user.preferred_fiat_currency,
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
            is_demo: user.is_demo,
            email_verified: db_user.email_verified,
            subscription_tier: db_user.subscription_tier,
            created_at: db_user.created_at,
            preferred_fiat_currency: db_user.preferred_fiat_currency,
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

// Stripe billing endpoints
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
    let (personal_sync, team_sync) = user_record
        .subscription_tier
        .get_sync_intervals(&config.network);
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

    // Check if trial has expired and update status accordingly
    let effective_subscription_status = crate::subscription::get_effective_subscription_status(
        &user_record.subscription_status,
        user_record.trial_ends_at.as_deref(),
    );

    let response = BillingStatusResponse {
        user_id: user.user_id.clone(),
        subscription_tier: user_record.subscription_tier.as_str().to_string(),
        subscription_status: effective_subscription_status,
        trial_ends_at: user_record.trial_ends_at,
        subscription_started_at: user_record.subscription_started_at,
        subscription_ends_at: user_record.subscription_ends_at,
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
                    let parts: Vec<&str> = update
                        .user_id
                        .strip_prefix("stripe_customer:")
                        .unwrap()
                        .split(':')
                        .collect();

                    if parts.len() == 2 {
                        // Special case: subscription deletion with customer_id:subscription_id
                        let customer_id = parts[0];
                        let deleted_subscription_id = parts[1];

                        tracing::info!(
                            "Checking subscription deletion for customer {} - subscription {}",
                            customer_id,
                            deleted_subscription_id
                        );

                        // Find user and check if deleted subscription matches current subscription
                        match app_services
                            .metadata_db
                            .get_user_by_stripe_customer_id(customer_id)
                            .await
                        {
                            Ok(Some(user)) => {
                                if let Some(current_subscription_id) = &user.stripe_subscription_id
                                {
                                    if current_subscription_id == deleted_subscription_id {
                                        tracing::info!("📉 Marked user {} as expired (current subscription {} deleted)", user.id, deleted_subscription_id);
                                        user.id
                                    } else {
                                        tracing::info!("🚮 Ignoring deletion of old subscription {} for user {} (current: {})", deleted_subscription_id, user.id, current_subscription_id);
                                        continue; // Skip this update - it's an old subscription
                                    }
                                } else {
                                    tracing::info!("🚮 Ignoring deletion of subscription {} for user {} (no current subscription)", deleted_subscription_id, user.id);
                                    continue; // Skip this update - user has no current subscription
                                }
                            }
                            Ok(None) => {
                                tracing::warn!(
                                    "No user found for Stripe customer ID: {}",
                                    customer_id
                                );
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
                        // Regular customer ID lookup
                        let customer_id = parts[0];
                        tracing::info!("Looking up user by Stripe customer ID: {}", customer_id);

                        // Find user by Stripe customer ID - no mutex blocking!
                        match app_services
                            .metadata_db
                            .get_user_by_stripe_customer_id(customer_id)
                            .await
                        {
                            Ok(Some(user)) => {
                                tracing::info!(
                                    "Found user {} for customer {}",
                                    user.id,
                                    customer_id
                                );
                                user.id
                            }
                            Ok(None) => {
                                tracing::warn!(
                                    "No user found for Stripe customer ID: {}",
                                    customer_id
                                );
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
                                        &update.subscription_status,
                                        user_record.is_admin,
                                        user_record.trial_ends_at.clone(),
                                    )
                                    .await
                                {
                                    tracing::error!(
                                        "Failed to apply subscription limits for user {}: {}",
                                        actual_user_id,
                                        e
                                    );
                                } else if user_record.is_admin {
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

pub fn create_router_with_services(
    app_services: AppServicesState,
    notification_manager: NotificationManagerState,
    stripe_billing: StripeBillingState,
    config: AppConfig,
) -> Router {
    let config_state = Arc::new(config);

    // Create unified AppState for handlers that use the AuthenticatedUser extractor
    let app_state = AppState {
        app_services: app_services.clone(),
        notification_manager: notification_manager.clone(),
        stripe_billing: stripe_billing.clone(),
        config: config_state.clone(),
    };

    // Auth routes (public) - only routes that still use wallet_manager
    // AppServices routes (non-blocking, metadata operations only)
    let app_routes_2param = Router::new()
        .route("/auth/register", post(register))
        .route("/auth/verify-email/{token}", get(verify_email))
        .route("/auth/forgot-password", post(forgot_password))
        .route("/auth/reset-password/{token}", post(reset_password))
        .route("/auth/user", put(update_user))
        .route("/contact", post(submit_contact_form))
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

    // Routes using unified AppState with AuthenticatedUser extractor
    let app_state_routes = Router::new()
        .route(
            "/user/preferences",
            get(get_user_preferences).put(update_user_preferences),
        )
        .route(
            "/wallets/{checksum}/balance-alerts",
            get(get_wallet_balance_alerts).post(create_wallet_balance_alert),
        )
        .route(
            "/balance-alerts/{alert_id}",
            axum::routing::delete(delete_balance_alert),
        )
        // Blockchain data routes (no auth required)
        .route("/block-headers/current", get(get_current_block_header))
        .route("/exchange-rates", get(get_exchange_rates))
        // Wallet routes (migrated to use AuthenticatedUser extractor)
        .route(
            "/wallets",
            get(get_wallets_list).post(create_wallet_non_blocking),
        )
        .route(
            "/wallets/{checksum}",
            get(get_wallet).put(update_wallet).delete(delete_wallet),
        )
        .route("/wallets/{checksum}/detail", get(get_wallet_detail))
        .with_state(app_state.clone());

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

    // Add the remaining AppServices routes (contacts and auth)
    let app_routes_metadata = Router::new()
        .route("/wallets/{checksum}/contacts", get(get_wallet_contacts))
        .route("/wallets/{checksum}/contacts", post(create_wallet_contact))
        .route(
            "/wallets/{wallet_checksum}/contacts/{contact_id}",
            axum::routing::put(update_wallet_contact).delete(delete_wallet_contact),
        )
        .route("/auth/login", post(login))
        .route("/auth/demo-login", post(demo_login))
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
        .merge(app_state_routes)
        .merge(provider_routes)
        .merge(stripe_routes);

    Router::new()
        .nest("/api", api_routes)
        .layer(CorsLayer::permissive())
}
