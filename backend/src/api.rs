use crate::admin_notifications::AdminNotifications;
use crate::handlers::{
    create_stripe_checkout_session, create_stripe_customer_portal, create_wallet_balance_alert,
    create_wallet_contact, create_wallet_non_blocking, delete_balance_alert, delete_wallet,
    delete_wallet_contact, get_billing_pricing, get_billing_status, get_checkout_session_details,
    get_current_block_header, get_exchange_rates, get_providers, get_user_preferences, get_wallet,
    get_wallet_balance_alerts, get_wallet_contacts, get_wallet_detail, get_wallets_list,
    handle_stripe_webhook, send_contact_verification, update_user_preferences, update_wallet,
    update_wallet_contact, verify_contact,
};
use crate::auth::{
    authenticate_user, AuthResponse, AuthService, AuthUserResponse, ForgotPasswordRequest,
    LoginRequest, RegisterRequest, ResetPasswordRequest, UpdateUserRequest, UpdateUserResponse,
};
use crate::config::AppConfig;
use crate::email_service::EmailService;
use crate::exchange_rates;
use crate::metadata::{Language, MetadataDb, WalletsListResponse};
use crate::models::{ContactFormRequest, ContactFormResponse, ErrorResponse};
use crate::notifications::NotificationManager;
use crate::stripe_billing::StripeBilling;
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
pub(crate) fn validate_phone_number(phone: &str) -> Result<String, String> {
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
pub(crate) fn generate_ntfy_topic(name: &str, language: &Language, descriptor: &str) -> String {
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
        // Contact routes (migrated to use AuthenticatedUser extractor)
        .route(
            "/wallets/{checksum}/contacts",
            get(get_wallet_contacts).post(create_wallet_contact),
        )
        .route(
            "/wallets/{wallet_checksum}/contacts/{contact_id}",
            put(update_wallet_contact).delete(delete_wallet_contact),
        )
        // Contact verification routes (migrated to use AuthenticatedUser extractor)
        .route(
            "/wallets/{checksum}/contacts/send-verification",
            post(send_contact_verification),
        )
        .route("/wallets/{checksum}/contacts/verify", post(verify_contact))
        // Billing status route (always available, uses AuthenticatedUser extractor)
        .route("/billing/status", get(get_billing_status))
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

    // Add the remaining AppServices routes (auth)
    let app_routes_metadata = Router::new()
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
        .with_state((
            app_services.clone(),
            stripe_billing.clone(),
            config_state.clone(),
        ));

    let api_routes = app_routes_2param
        .merge(app_routes_metadata)
        .merge(app_routes_auth)
        .merge(app_state_routes)
        .merge(provider_routes)
        .merge(stripe_routes);

    Router::new()
        .nest("/api", api_routes)
        .layer(CorsLayer::permissive())
}
