//! Wallet management handlers

use super::error::{ApiError, ApiResult, OrInternal};
use crate::admin_notifications::AdminNotifications;
use crate::api::{AppServicesState, ElectrumClientManagerState};
use crate::config::{AppConfig, NetworkConfig};
use crate::extractors::{require_non_demo, AuthenticatedUser};
use crate::handlers::helpers::{
    check_resource_limit, get_user_or_error, verify_wallet_access, verify_wallet_read_access,
    ResourceLimit,
};
use crate::metadata::{
    BalanceAlert, Contact, NotificationStatus, ProviderType, SubscriptionStatus, TransactionCursor,
    TransactionPageRequest, TransactionSummary, WalletDetailPagination, WalletDetailResponse,
    WalletMetadata, WalletStatus, WalletsListResponse,
};
use crate::models::{CreateWalletRequest, CreateWalletResponse, UpdateWalletRequest};
use crate::stripe_billing::StripeBilling;
use crate::utils::amount::sats_to_btc;
use crate::utils::{compact_wallet_key_input, current_unix_timestamp, DescriptorError};
use crate::xpub_converter::XpubConverter;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, LazyLock};
use tracing::{debug, info, warn};

/// Pre-compiled regex for detecting output descriptor format
static DESCRIPTOR_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"^(wpkh|wsh|sh|pkh|pk|tr)\(")
        .expect("invariant: descriptor prefix regex is valid")
});

/// Error message for Electrum server unavailability
const ELECTRUM_UNAVAILABLE_ERROR: &str = "Electrum server is unavailable. Please try again later.";

/// Type alias for Stripe billing state
pub type StripeBillingState = Option<Arc<StripeBilling>>;

const DEFAULT_WALLET_DETAIL_PAGE_SIZE: usize = 100;
const MAX_WALLET_DETAIL_PAGE_SIZE: usize = 250;
const MAX_SQL_TIMESTAMP: u64 = i64::MAX as u64;
const MAX_BIP329_IMPORT_BYTES: usize = 1_000_000;
const MAX_BIP329_IMPORT_RECORDS: usize = 10_000;
const MAX_LABEL_CHARS: usize = 256;
const ALLOWED_CUSTOM_STOP_GAPS: [&str; 4] = ["250", "500", "750", "1000"];

#[derive(Debug, Deserialize, Default)]
pub struct WalletDetailQueryParams {
    pub cursor: Option<String>,
    pub page_size: Option<usize>,
    pub since_timestamp: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTransactionLabelRequest {
    pub label: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ImportBip329LabelsRequest {
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct WalletNotificationsResponse {
    pub timestamp: u64,
    pub wallet: WalletMetadata,
    pub contacts: Vec<Contact>,
    pub balance_alerts: Vec<BalanceAlert>,
}

fn validate_sql_timestamp(timestamp: u64, code: &'static str) -> ApiResult<()> {
    if timestamp > MAX_SQL_TIMESTAMP {
        return Err(ApiError::bad_request(code, "Timestamp is out of range"));
    }
    Ok(())
}

/// Non-blocking wallet creation using `AppServices` (avoids `WalletManager` mutex)
/// This resolves the regression where wallet creation was taking 30+ seconds
pub async fn create_wallet_non_blocking(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(stripe_billing): State<StripeBillingState>,
    State(config): State<Arc<AppConfig>>,
    State(electrum_manager): State<ElectrumClientManagerState>,
    Json(mut payload): Json<CreateWalletRequest>,
) -> ApiResult<(StatusCode, Json<CreateWalletResponse>)> {
    let start_time = std::time::Instant::now();
    payload.descriptor = compact_wallet_key_input(&payload.descriptor);

    require_non_demo(&user)?;

    if config.is_cloud_mode() && user.is_admin {
        return Err(ApiError::forbidden(
            "admin_wallets_unsupported",
            "Cloud administrators cannot add wallets. Use support access to view a customer account.",
        ));
    }

    ensure_electrum_connected(&electrum_manager).await?;
    validate_descriptor_network(&payload.descriptor, &config.network)?;

    let is_pubkey = XpubConverter::is_bitcoin_public_key(payload.descriptor.trim());
    let is_address = XpubConverter::is_bitcoin_address(payload.descriptor.trim());

    // Stop gap and script type are irrelevant for watch-only pubkeys and addresses.
    if !is_pubkey && !is_address {
        validate_stop_gap(&payload)?;
    }

    let user_record = get_user_or_error(
        &app_services,
        &user.user_id,
        None,
        "User not found",
        "Failed to get user information",
    )
    .await?;
    check_resource_limit(
        &app_services,
        config.as_ref(),
        &user_record,
        ResourceLimit::Wallet {
            user_id: &user.user_id,
        },
    )
    .await?;

    let descriptor = creation_descriptor(&payload, is_pubkey || is_address)?;

    let wallet_metadata = app_services
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
        .map_err(|error| wallet_creation_error(&error))?;

    if config.is_cloud_mode() {
        spawn_default_contact_creation(&app_services, &user.user_id, &wallet_metadata.checksum);
    }
    activate_trial_on_first_wallet(&app_services, stripe_billing.as_deref(), &user.user_id).await;
    notify_admins_of_wallet_creation(&config, &wallet_metadata.checksum);

    let elapsed = start_time.elapsed();
    info!("create_wallet_non_blocking completed in {elapsed:?}");

    Ok((
        StatusCode::CREATED,
        Json(CreateWalletResponse {
            message: "Wallet created successfully".to_string(),
            wallet: wallet_metadata,
        }),
    ))
}

/// Makes a real Electrum call so wallet creation fails fast when the server is down.
async fn ensure_electrum_connected(electrum_manager: &ElectrumClientManagerState) -> ApiResult<()> {
    let is_connected = match electrum_manager {
        Some(manager) => manager.verify_connection().await,
        None => false,
    };
    if is_connected {
        Ok(())
    } else {
        Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "electrum_unavailable",
            ELECTRUM_UNAVAILABLE_ERROR,
        ))
    }
}

/// Rejects keys for another network before any database work happens.
fn validate_descriptor_network(descriptor: &str, network: &NetworkConfig) -> ApiResult<()> {
    XpubConverter::validate_descriptor_network(descriptor, network.to_bdk_network()).map_err(
        |error| {
            let server_network_name = match network {
                NetworkConfig::Mainnet => "mainnet",
                NetworkConfig::Testnet => "testnet",
                NetworkConfig::Regtest => "regtest",
            };
            ApiError::bad_request(
                "network_mismatch",
                format!("{error}. Please use a {server_network_name} key."),
            )
        },
    )
}

fn validate_stop_gap(payload: &CreateWalletRequest) -> ApiResult<()> {
    let Some(stop_gap) = payload
        .stop_gap
        .as_deref()
        .filter(|stop_gap| *stop_gap != "auto")
    else {
        return Ok(());
    };

    // Output descriptors already carry their script type; XPUBs need one picked.
    let is_descriptor = DESCRIPTOR_REGEX.is_match(payload.descriptor.trim());
    let has_script_type = payload
        .script_type
        .as_deref()
        .is_some_and(|script_type| script_type != "auto");
    if !is_descriptor && !has_script_type {
        return Err(ApiError::bad_request(
            "script_type_required",
            "Custom stop gap requires selecting a specific script type (not auto)",
        ));
    }

    if !ALLOWED_CUSTOM_STOP_GAPS.contains(&stop_gap) {
        return Err(ApiError::bad_request(
            "invalid_stop_gap",
            "Invalid stop gap. Allowed values: auto, 250, 500, 750, 1000",
        ));
    }
    Ok(())
}

/// Picks the key string handed to the creation service.
///
/// Pubkeys and addresses are passed trimmed and converted by the service.
/// XPUBs and descriptors are passed as given; existing-wallet XPUBs have their
/// script type probed in the background.
fn creation_descriptor(payload: &CreateWalletRequest, is_single_key: bool) -> ApiResult<String> {
    if is_single_key {
        return Ok(payload.descriptor.trim().to_string());
    }
    if XpubConverter::is_xpub(&payload.descriptor) {
        if payload.is_fresh_wallet == Some(true) {
            let Some(script_type) = &payload.script_type else {
                return Err(ApiError::bad_request(
                    "script_type_required",
                    "script_type is required for fresh XPUB wallets",
                ));
            };
            debug!("Fresh wallet detected, using provided script type: {script_type}");
        } else {
            debug!(
                "Detected XPUB format for existing wallet, will probe script types in background"
            );
        }
    }
    Ok(payload.descriptor.clone())
}

fn wallet_creation_error(error: &anyhow::Error) -> ApiError {
    let message = error.to_string();
    if message.contains("already been added")
        || message == "Descriptor already exists"
        || message == "Wallet already exists"
        || message == "Wallet file already exists"
    {
        return ApiError::conflict("wallet_already_exists", message);
    }
    if message.contains("already being watched") {
        return ApiError::conflict("address_already_watched", message);
    }
    match error.downcast_ref::<DescriptorError>() {
        Some(DescriptorError::HardenedDerivationAfterXpub) => ApiError::bad_request(
            "invalid_descriptor_hardened_derivation",
            "Invalid descriptor: hardened account paths must be before the xpub, for example [fingerprint/84h/0h/0h]xpub.../<0;1>/*.",
        ),
        Some(DescriptorError::NotMultipath) => ApiError::bad_request(
            "invalid_descriptor_not_multipath",
            "Descriptor must include receive and change paths, for example xpub.../<0;1>/*.",
        ),
        Some(
            DescriptorError::InvalidMultipathCount
            | DescriptorError::SplitMultipath(_)
            | DescriptorError::InvalidDescriptor(_)
            | DescriptorError::InvalidStrippedDescriptor(_),
        ) => ApiError::bad_request(
            "invalid_descriptor",
            "Invalid descriptor. Please check the format and try again.",
        ),
        None => ApiError::uncoded(StatusCode::BAD_REQUEST, message),
    }
}

/// Adds the cloud user as an email contact of their new wallet.
///
/// Runs in the background so a failure never blocks wallet creation.
fn spawn_default_contact_creation(
    app_services: &AppServicesState,
    user_id: &str,
    wallet_checksum: &str,
) {
    let metadata_db = app_services.metadata_db.clone();
    let user_id = user_id.to_owned();
    let wallet_checksum = wallet_checksum.to_owned();

    tokio::spawn(async move {
        let user_record = match metadata_db.get_user_by_id(&user_id).await {
            Ok(Some(user_record)) => user_record,
            Ok(None) => {
                warn!("Default contact creation skipped: user unavailable");
                return;
            }
            Err(_) => {
                warn!("Default contact user lookup failed");
                return;
            }
        };

        let contact_name = user_record.name.as_deref().unwrap_or("Me");
        let notification_methods = vec![(ProviderType::Email, user_record.email)];
        if metadata_db
            .insert_contact_with_notification_methods(
                &wallet_checksum,
                contact_name,
                notification_methods,
            )
            .await
            .is_ok()
        {
            info!("Created default wallet contact");
        } else {
            warn!("Default contact creation failed");
        }
    });
}

/// Starts the trial when a user in `pending` status adds their first wallet.
///
/// Failures are logged but never fail wallet creation; without Stripe the
/// database-only trial still applies.
async fn activate_trial_on_first_wallet(
    app_services: &AppServicesState,
    stripe_billing: Option<&StripeBilling>,
    user_id: &str,
) {
    let Ok(Some(user_record)) = app_services.metadata_db.get_user_by_id(user_id).await else {
        return;
    };
    if user_record.subscription_status != SubscriptionStatus::Pending {
        return;
    }
    let wallet_count = app_services
        .metadata_db
        .count_wallets_for_user(user_id)
        .await
        .unwrap_or(0);
    if wallet_count != 1 {
        return;
    }

    info!("Activating trial on first wallet creation");
    let trial_ends_at = chrono::Utc::now() + chrono::Duration::days(30);
    if app_services
        .metadata_db
        .update_user_trial_status(
            user_id,
            SubscriptionStatus::Trialing,
            Some(trial_ends_at.to_rfc3339()),
        )
        .await
        .is_err()
    {
        tracing::error!("Trial status update failed");
    }

    if let Some(stripe_service) = stripe_billing {
        if stripe_service
            .create_trial_subscription(
                &user_record,
                crate::subscription::SubscriptionTier::Team,
                &app_services.metadata_db,
            )
            .await
            .is_ok()
        {
            info!("Trial subscription created");
        } else {
            tracing::error!("Trial subscription creation failed");
        }
    } else if user_record.stripe_customer_id.is_some() {
        warn!("Billing service unavailable for trial creation");
    }
}

fn notify_admins_of_wallet_creation(config: &AppConfig, wallet_checksum: &str) {
    if !AdminNotifications::is_enabled_for(config) {
        return;
    }
    let wallet_checksum = wallet_checksum.to_owned();
    AdminNotifications::spawn_if_enabled(config, move |admin_notifications| async move {
        admin_notifications
            .notify_wallet_creation(&wallet_checksum)
            .await;
    });
}

/// Delete a wallet (soft delete)
pub async fn delete_wallet(
    AuthenticatedUser(user): AuthenticatedUser,
    Path(checksum): Path<String>,
    State(app_services): State<AppServicesState>,
) -> ApiResult<StatusCode> {
    require_non_demo(&user)?;
    verify_wallet_access(&app_services, &user, &checksum, "Database error").await?;

    // Soft delete answers instantly; the next sync cycle does the cleanup.
    let deleted = app_services
        .metadata_db
        .mark_wallet_as_deleted(&checksum)
        .await
        .or_internal("Database error")?;
    if !deleted {
        return Err(wallet_not_found());
    }
    info!("[{checksum}] Wallet marked as deleted (soft delete)");
    Ok(StatusCode::NO_CONTENT)
}

/// Update a wallet's name
pub async fn update_wallet(
    AuthenticatedUser(user): AuthenticatedUser,
    Path(checksum): Path<String>,
    State(app_services): State<AppServicesState>,
    Json(payload): Json<UpdateWalletRequest>,
) -> ApiResult<StatusCode> {
    require_non_demo(&user)?;

    if payload.name.trim().is_empty() {
        return Err(ApiError::bad_request(
            "wallet_name_required",
            "Wallet name cannot be empty",
        ));
    }

    let start_time = std::time::Instant::now();
    verify_wallet_access(&app_services, &user, &checksum, "Database error").await?;

    let update_result = app_services
        .metadata_db
        .update_wallet_by_checksum(&checksum, &payload.name)
        .await;

    let elapsed = start_time.elapsed();
    info!("update_wallet completed in {elapsed:?}");

    if update_result.or_internal("Database error")? {
        Ok(StatusCode::OK)
    } else {
        Err(wallet_not_found())
    }
}

fn wallet_not_found() -> ApiError {
    ApiError::not_found("wallet_not_found", "Wallet not found")
}

/// Get a wallet by checksum
pub async fn get_wallet(
    AuthenticatedUser(user): AuthenticatedUser,
    Path(checksum): Path<String>,
    State(app_services): State<AppServicesState>,
) -> ApiResult<Json<WalletMetadata>> {
    let wallet =
        verify_wallet_read_access(&app_services, &user, &checksum, "Internal server error").await?;
    Ok(Json(wallet))
}

/// Get list of all wallets for the authenticated user
pub async fn get_wallets_list(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
) -> ApiResult<Json<WalletsListResponse>> {
    let mut wallets_response = app_services
        .get_wallets_list_for_user(&user.user_id)
        .await
        .or_internal("Failed to get wallets list")?;

    if let Some((currency, rate_per_btc)) = preferred_fiat_rate(&app_services, &user.user_id).await
    {
        for wallet in &mut wallets_response.wallets {
            apply_fiat_balance(wallet, &currency, rate_per_btc);
        }
    }

    Ok(Json(wallets_response))
}

/// The user's preferred fiat currency and its BTC rate, when both are known.
async fn preferred_fiat_rate(
    app_services: &AppServicesState,
    user_id: &str,
) -> Option<(String, f64)> {
    let Ok(Some(user_record)) = app_services.metadata_db.get_user_by_id(user_id).await else {
        return None;
    };
    let currency = user_record.preferred_fiat_currency?;
    let rates = app_services.metadata_db.get_exchange_rates().await.ok()?;
    let rate_per_btc = rates.get(&currency)?.rate_per_btc;
    Some((currency, rate_per_btc))
}

fn apply_fiat_balance(wallet: &mut WalletMetadata, currency: &str, rate_per_btc: f64) {
    if let Some(balance_sats) = wallet.balance_total {
        wallet.balance_fiat = Some(sats_to_btc(balance_sats) * rate_per_btc);
        wallet.fiat_currency = Some(currency.to_owned());
    }
}

/// Get wallet detail with transactions
pub async fn get_wallet_detail(
    AuthenticatedUser(user): AuthenticatedUser,
    Path(checksum): Path<String>,
    Query(query): Query<WalletDetailQueryParams>,
    State(app_services): State<AppServicesState>,
) -> ApiResult<Json<WalletDetailResponse>> {
    let timestamp = current_unix_timestamp().or_internal("Failed to get timestamp")?;
    let page_size = query
        .page_size
        .unwrap_or(DEFAULT_WALLET_DETAIL_PAGE_SIZE)
        .clamp(1, MAX_WALLET_DETAIL_PAGE_SIZE);
    let page_request = transaction_page_request(&query, page_size)?;

    let wallet =
        verify_wallet_read_access(&app_services, &user, &checksum, "Database error").await?;

    // Pending wallets have no transactions yet; contacts and alerts are still shown.
    if wallet.status == WalletStatus::Pending {
        let contacts = active_contacts(&app_services, &wallet.checksum).await?;
        let balance_alerts = balance_alerts_or_empty(&app_services, &wallet.checksum).await;
        return Ok(Json(WalletDetailResponse {
            timestamp,
            wallet,
            transactions: vec![],
            contacts,
            balance_alerts,
            pagination: WalletDetailPagination {
                page_size,
                next_cursor: None,
                has_more: false,
                applied_since_timestamp: query.since_timestamp,
            },
        }));
    }

    let transaction_page = app_services
        .metadata_db
        .get_transactions_page_by_wallet_checksum(&wallet.checksum, page_request)
        .await
        .or_internal("Failed to get transactions")?;
    let contacts = active_contacts(&app_services, &wallet.checksum).await?;

    let mut wallet = wallet;
    if let Some((currency, rate_per_btc)) = preferred_fiat_rate(&app_services, &user.user_id).await
    {
        apply_fiat_balance(&mut wallet, &currency, rate_per_btc);
    }

    let balance_alerts = balance_alerts_or_empty(&app_services, &wallet.checksum).await;

    Ok(Json(WalletDetailResponse {
        timestamp,
        wallet,
        transactions: transaction_page
            .transactions
            .into_iter()
            .map(TransactionSummary::from)
            .collect(),
        contacts,
        balance_alerts,
        pagination: WalletDetailPagination {
            page_size,
            next_cursor: transaction_page.next_cursor.map(|cursor| cursor.encode()),
            has_more: transaction_page.has_more,
            applied_since_timestamp: transaction_page.applied_since_timestamp,
        },
    }))
}

fn transaction_page_request(
    query: &WalletDetailQueryParams,
    page_size: usize,
) -> ApiResult<TransactionPageRequest> {
    let cursor = query.cursor.as_deref().map(decode_cursor).transpose()?;
    if let Some(since_timestamp) = query.since_timestamp {
        validate_sql_timestamp(since_timestamp, "invalid_since_timestamp")?;
    }
    if cursor.is_some() && query.since_timestamp.is_some() {
        return Err(ApiError::bad_request(
            "invalid_pagination_mode",
            "cursor and since_timestamp cannot be combined",
        ));
    }
    Ok(TransactionPageRequest {
        limit: page_size,
        cursor,
        since_timestamp: query.since_timestamp,
        include_notifications: false,
    })
}

fn decode_cursor(encoded: &str) -> ApiResult<TransactionCursor> {
    let cursor = TransactionCursor::decode(encoded)
        .map_err(|error| ApiError::bad_request("invalid_cursor", error.to_string()))?;
    validate_sql_timestamp(cursor.sort_timestamp, "invalid_cursor")?;
    Ok(cursor)
}

async fn active_contacts(
    app_services: &AppServicesState,
    wallet_checksum: &str,
) -> ApiResult<Vec<Contact>> {
    app_services
        .metadata_db
        .get_contacts_with_notification_methods_filtered(wallet_checksum, true)
        .await
        .or_internal("Failed to get contacts")
}

/// Balance alerts are optional on the detail page, so a lookup failure is
/// logged and shown as no alerts.
async fn balance_alerts_or_empty(
    app_services: &AppServicesState,
    wallet_checksum: &str,
) -> Vec<BalanceAlert> {
    app_services
        .metadata_db
        .get_all_balance_alerts_for_wallet(wallet_checksum)
        .await
        .unwrap_or_else(|error| {
            warn!(error = format!("{error:#}"), "Failed to get balance alerts");
            Vec::new()
        })
}

/// Get wallet notification settings without loading transaction history.
pub async fn get_wallet_notifications(
    AuthenticatedUser(user): AuthenticatedUser,
    Path(checksum): Path<String>,
    State(app_services): State<AppServicesState>,
) -> ApiResult<Json<WalletNotificationsResponse>> {
    let timestamp = current_unix_timestamp().or_internal("Failed to get timestamp")?;
    let wallet =
        verify_wallet_read_access(&app_services, &user, &checksum, "Database error").await?;

    let (contacts, balance_alerts) = tokio::join!(
        active_contacts(&app_services, &wallet.checksum),
        app_services
            .metadata_db
            .get_all_balance_alerts_for_wallet(&wallet.checksum)
    );
    let contacts = contacts?;
    let balance_alerts = balance_alerts.or_internal("Failed to get balance alerts")?;

    Ok(Json(WalletNotificationsResponse {
        timestamp,
        wallet,
        contacts,
        balance_alerts,
    }))
}

/// Get notifications for a specific transaction.
pub async fn get_transaction_notifications(
    AuthenticatedUser(user): AuthenticatedUser,
    Path((checksum, txid)): Path<(String, String)>,
    State(app_services): State<AppServicesState>,
) -> ApiResult<Json<Vec<NotificationStatus>>> {
    let wallet = verify_wallet_read_access(
        &app_services,
        &user,
        &checksum,
        "Failed to verify wallet access",
    )
    .await?;

    app_services
        .metadata_db
        .get_transaction_by_txid(&wallet.checksum, &txid)
        .await
        .or_internal("Database error")?
        .ok_or_else(transaction_not_found)?;

    let notifications = app_services
        .metadata_db
        .get_transaction_notifications(&wallet.checksum, &txid)
        .await
        .or_internal("Failed to get transaction notifications")?;
    Ok(Json(notifications))
}

fn transaction_not_found() -> ApiError {
    ApiError::not_found("transaction_not_found", "Transaction not found")
}

/// Update the BIP-329 label attached to a transaction.
pub async fn update_transaction_label(
    AuthenticatedUser(user): AuthenticatedUser,
    Path((checksum, txid)): Path<(String, String)>,
    State(app_services): State<AppServicesState>,
    Json(payload): Json<UpdateTransactionLabelRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    require_non_demo(&user)?;
    let wallet = verify_wallet_access(
        &app_services,
        &user,
        &checksum,
        "Failed to verify wallet access",
    )
    .await?;

    let label = payload.label.and_then(|value| {
        let trimmed = value.trim().to_string();
        (!trimmed.is_empty()).then_some(trimmed)
    });
    if label
        .as_ref()
        .is_some_and(|value| value.chars().count() > MAX_LABEL_CHARS)
    {
        return Err(ApiError::bad_request(
            "invalid_label",
            "label must be 256 characters or fewer",
        ));
    }

    let updated = app_services
        .metadata_db
        .update_transaction_label(
            &wallet.checksum,
            &txid.to_ascii_lowercase(),
            label.as_deref(),
        )
        .await
        .or_internal("Database error")?;
    if !updated {
        return Err(transaction_not_found());
    }
    Ok(Json(serde_json::json!({ "label": label })))
}

pub async fn export_bip329_labels(
    AuthenticatedUser(user): AuthenticatedUser,
    Path(checksum): Path<String>,
    State(app_services): State<AppServicesState>,
) -> ApiResult<Json<serde_json::Value>> {
    let wallet = verify_wallet_read_access(
        &app_services,
        &user,
        &checksum,
        "Failed to verify wallet access",
    )
    .await?;

    let labels = app_services
        .metadata_db
        .get_transaction_labels(&wallet.checksum)
        .await
        .or_internal("Database error")?;
    let lines = labels
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()
        .or_internal("Failed to serialize labels")?;
    Ok(Json(serde_json::json!({ "content": lines.join("\n") })))
}

pub async fn import_bip329_labels(
    AuthenticatedUser(user): AuthenticatedUser,
    Path(checksum): Path<String>,
    State(app_services): State<AppServicesState>,
    Json(payload): Json<ImportBip329LabelsRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    require_non_demo(&user)?;
    let wallet = verify_wallet_access(
        &app_services,
        &user,
        &checksum,
        "Failed to verify wallet access",
    )
    .await?;

    let parsed = parse_bip329_transaction_labels(&payload.content)?;
    let imported = app_services
        .metadata_db
        .update_transaction_labels(&wallet.checksum, &parsed.labels)
        .await
        .or_internal("Database error")?;

    let skipped = parsed.skipped + parsed.transaction_records.saturating_sub(imported);
    Ok(Json(
        serde_json::json!({ "imported": imported, "skipped": skipped }),
    ))
}

/// Transaction labels read from a BIP-329 JSON Lines export.
struct ParsedBip329 {
    /// Lowercase txid and label; `None` clears the label.
    labels: Vec<(String, Option<String>)>,
    /// Records that are not transaction labels.
    skipped: usize,
    transaction_records: usize,
}

fn parse_bip329_transaction_labels(content: &str) -> ApiResult<ParsedBip329> {
    if content.len() > MAX_BIP329_IMPORT_BYTES {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "invalid_bip329",
            "BIP-329 import is limited to 1 MB",
        ));
    }

    let mut parsed = ParsedBip329 {
        labels: Vec::new(),
        skipped: 0,
        transaction_records: 0,
    };
    for (line_index, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        if parsed.transaction_records + parsed.skipped >= MAX_BIP329_IMPORT_RECORDS {
            return Err(ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "invalid_bip329",
                "BIP-329 import is limited to 10,000 records",
            ));
        }
        let line_number = line_index + 1;
        let value: serde_json::Value = serde_json::from_str(line).map_err(|error| {
            invalid_bip329(format!("Invalid BIP-329 line {line_number}: {error}"))
        })?;

        // Canary stores transaction labels; other BIP-329 record types remain untouched.
        if value.get("type").and_then(serde_json::Value::as_str) != Some("tx") {
            parsed.skipped += 1;
            continue;
        }
        let invalid_record = || {
            invalid_bip329(format!(
                "Invalid transaction label record on line {line_number}"
            ))
        };
        let label = match value.get("label") {
            None => {
                parsed.skipped += 1;
                continue;
            }
            Some(serde_json::Value::String(label)) => label.trim().to_string(),
            Some(_) => return Err(invalid_record()),
        };
        parsed.transaction_records += 1;
        let Some(reference) = value.get("ref").and_then(serde_json::Value::as_str) else {
            return Err(invalid_record());
        };
        if reference.len() != 64 || !reference.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(invalid_bip329(
                "txid must be a 64-character hexadecimal transaction id",
            ));
        }
        if label.chars().count() > MAX_LABEL_CHARS {
            return Err(invalid_bip329("label must contain at most 256 characters"));
        }
        parsed.labels.push((
            reference.to_ascii_lowercase(),
            (!label.is_empty()).then_some(label),
        ));
    }
    Ok(parsed)
}

fn invalid_bip329(message: impl Into<std::borrow::Cow<'static, str>>) -> ApiError {
    ApiError::bad_request("invalid_bip329", message)
}
