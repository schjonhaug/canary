//! Wallet management handlers

use crate::admin_notifications::AdminNotifications;
use crate::api::AppServicesState;
use crate::config::{AppConfig, NetworkConfig};
use crate::extractors::{require_non_demo, AuthenticatedUser};
use crate::metadata::{ProviderType, WalletDetailResponse};
use crate::models::{
    CreateWalletRequest, CreateWalletResponse, ErrorResponse, UpdateWalletRequest,
};
use crate::stripe_billing::StripeBilling;
use crate::subscription::check_limit;
use crate::xpub_converter::XpubConverter;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use std::sync::Arc;
use tracing::info;

/// Type alias for Stripe billing state
pub type StripeBillingState = Option<Arc<StripeBilling>>;

/// Non-blocking wallet creation using AppServices (avoids WalletManager mutex)
/// This resolves the regression where wallet creation was taking 30+ seconds
pub async fn create_wallet_non_blocking(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(stripe_billing): State<StripeBillingState>,
    State(config): State<Arc<AppConfig>>,
    Json(payload): Json<CreateWalletRequest>,
) -> Response {
    let start_time = std::time::Instant::now();

    // Reject demo users from creating wallets
    if let Err(response) = require_non_demo(&user) {
        return response;
    }

    // Validate network compatibility early - before any database operations
    if let Err(e) = XpubConverter::validate_descriptor_network(
        &payload.descriptor,
        config.network.to_bdk_network(),
    ) {
        let server_network_name = match config.network {
            NetworkConfig::Mainnet => "mainnet",
            NetworkConfig::Testnet => "testnet",
            NetworkConfig::Regtest => "regtest",
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
            let bypass_limits = config.is_self_hosted_mode() || user_record.is_admin;

            if !bypass_limits {
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
            // Check if cloud mode - if so, auto-add user as contact
            if config.is_cloud_mode() {
                // Get user info for contact creation (NON-BLOCKING)
                let metadata_db = app_services.metadata_db.clone();
                let user_id = user.user_id.clone();
                let wallet_checksum = wallet_metadata.checksum.clone();

                // Spawn async task to create contact (don't block wallet creation if this fails)
                tokio::spawn(async move {
                    // Get user details from database (no mutex needed)
                    match metadata_db.get_user_by_id(&user_id).await {
                        Ok(Some(user_record)) => {
                            // Use user's name or fallback to "Me"
                            let contact_name = user_record.name.as_deref().unwrap_or("Me");

                            // Create contact with email notification using the user's email
                            let notification_methods =
                                vec![(ProviderType::Email, user_record.email)];

                            match metadata_db
                                .insert_contact_with_notification_methods(
                                    &wallet_checksum,
                                    contact_name,
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

/// Delete a wallet (soft delete)
pub async fn delete_wallet(
    AuthenticatedUser(user): AuthenticatedUser,
    Path(checksum): Path<String>,
    State(app_services): State<AppServicesState>,
) -> Response {
    // Reject demo users from deleting wallets
    if let Err(response) = require_non_demo(&user) {
        return response;
    }

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

/// Update a wallet's name
pub async fn update_wallet(
    AuthenticatedUser(user): AuthenticatedUser,
    Path(checksum): Path<String>,
    State(app_services): State<AppServicesState>,
    Json(payload): Json<UpdateWalletRequest>,
) -> Response {
    // Reject demo users from updating wallets
    if let Err(response) = require_non_demo(&user) {
        return response;
    }

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

/// Get a wallet by checksum
pub async fn get_wallet(
    AuthenticatedUser(user): AuthenticatedUser,
    Path(checksum): Path<String>,
    State(app_services): State<AppServicesState>,
) -> Response {
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

/// Get list of all wallets for the authenticated user
pub async fn get_wallets_list(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
) -> Response {
    // No mutex blocking! Direct access to metadata database
    match app_services
        .get_wallets_list_for_user(&user.user_id, user.is_admin)
        .await
    {
        Ok(mut wallets_response) => {
            // Add fiat values if user has a preferred currency
            if let Ok(Some(user_record)) =
                app_services.metadata_db.get_user_by_id(&user.user_id).await
            {
                if let Some(currency) = user_record.preferred_fiat_currency {
                    if let Ok(rates) = app_services.metadata_db.get_exchange_rates().await {
                        if let Some(rate) = rates.get(&currency) {
                            for wallet in &mut wallets_response.wallets {
                                if let Some(balance_sats) = wallet.balance_total {
                                    let balance_btc = balance_sats as f64 / 100_000_000.0;
                                    wallet.balance_fiat = Some(balance_btc * rate.rate_per_btc);
                                    wallet.fiat_currency = Some(currency.clone());
                                }
                            }
                        }
                    }
                }
            }

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

/// Get wallet detail with transactions
pub async fn get_wallet_detail(
    AuthenticatedUser(user): AuthenticatedUser,
    Path(checksum): Path<String>,
    State(app_services): State<AppServicesState>,
) -> Response {
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
            .get_contacts_with_notification_methods_filtered(&wallet.checksum, true)
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

        // Get balance alerts for the wallet
        let balance_alerts = match app_services
            .metadata_db
            .get_all_balance_alerts_for_wallet(&wallet.checksum)
            .await
        {
            Ok(alerts) => alerts,
            Err(e) => {
                eprintln!("Warning: Failed to get balance alerts: {}", e);
                vec![] // Return empty vec on error, don't fail the whole request
            }
        };

        let wallet_detail = WalletDetailResponse {
            timestamp,
            wallet,
            transactions: vec![], // Empty transactions for pending wallets
            contacts,
            balance_alerts,
        };

        return (StatusCode::OK, Json(wallet_detail)).into_response();
    }

    // Get transactions - no mutex blocking!
    let transactions = match app_services
        .metadata_db
        .get_transactions_by_wallet_checksum(&wallet.checksum, None)
        .await
    {
        Ok(transactions) => transactions,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to get transactions: {}", e),
                }),
            )
                .into_response();
        }
    };

    // Get contacts - no mutex blocking!
    let contacts = match app_services
        .metadata_db
        .get_contacts_with_notification_methods_filtered(&wallet.checksum, true)
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

    // Add fiat values if user has a preferred currency
    let mut wallet_with_fiat = wallet;
    if let Ok(Some(user_record)) = app_services.metadata_db.get_user_by_id(&user.user_id).await {
        if let Some(currency) = user_record.preferred_fiat_currency {
            if let Ok(rates) = app_services.metadata_db.get_exchange_rates().await {
                if let Some(rate) = rates.get(&currency) {
                    // Convert satoshis to BTC and multiply by rate
                    if let Some(balance_sats) = wallet_with_fiat.balance_total {
                        let balance_btc = balance_sats as f64 / 100_000_000.0;
                        wallet_with_fiat.balance_fiat = Some(balance_btc * rate.rate_per_btc);
                        wallet_with_fiat.fiat_currency = Some(currency.clone());
                    }
                }
            }
        }
    }

    // Get balance alerts for the wallet
    let balance_alerts = match app_services
        .metadata_db
        .get_all_balance_alerts_for_wallet(&wallet_with_fiat.checksum)
        .await
    {
        Ok(alerts) => alerts,
        Err(e) => {
            eprintln!("Warning: Failed to get balance alerts: {}", e);
            vec![] // Return empty vec on error, don't fail the whole request
        }
    };

    let wallet_detail = WalletDetailResponse {
        timestamp,
        wallet: wallet_with_fiat,
        transactions,
        contacts,
        balance_alerts,
    };

    (StatusCode::OK, Json(wallet_detail)).into_response()
}
