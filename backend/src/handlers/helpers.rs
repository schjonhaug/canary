//! Access checks and guards shared by several handlers.

use super::error::{ApiError, ApiResult, OrInternal};
use crate::api::AppServicesState;
use crate::auth::AuthUser;
use crate::config::AppConfig;
use crate::metadata::{UserRecord, WalletMetadata};
use crate::subscription::check_limit;

pub(crate) enum ResourceLimit<'a> {
    Wallet { user_id: &'a str },
    Contact { wallet_checksum: &'a str },
}

fn self_hosted_only(
    config: &AppConfig,
    code: &'static str,
    message: &'static str,
) -> ApiResult<()> {
    if config.is_cloud_mode() {
        Err(ApiError::forbidden(code, message))
    } else {
        Ok(())
    }
}

pub(crate) fn reject_nostr_in_cloud_mode(config: &AppConfig) -> ApiResult<()> {
    self_hosted_only(
        config,
        "nostr_self_hosted_only",
        "Nostr notifications are only available in self-hosted mode",
    )
}

pub(crate) fn reject_webhook_in_cloud_mode(config: &AppConfig) -> ApiResult<()> {
    self_hosted_only(
        config,
        "webhook_self_hosted_only",
        "Webhook notifications are only available in self-hosted mode",
    )
}

pub(crate) fn reject_telegram_in_cloud_mode(config: &AppConfig) -> ApiResult<()> {
    self_hosted_only(
        config,
        "telegram_self_hosted_only",
        "Telegram notifications are only available in self-hosted mode",
    )
}

pub(crate) async fn reject_telegram_if_unconfigured(
    app_services: &AppServicesState,
    config: &AppConfig,
) -> ApiResult<()> {
    if crate::telegram_provider::is_configured(&app_services.metadata_db, config).await {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "telegram_not_configured",
            "Telegram notifications are not configured on this instance",
        ))
    }
}

/// Loads a wallet and checks that `user` owns it.
///
/// `error_message` is the public message for unexpected database failures.
pub(crate) async fn verify_wallet_access(
    app_services: &AppServicesState,
    user: &AuthUser,
    checksum: &str,
    error_message: &'static str,
) -> ApiResult<WalletMetadata> {
    let wallet = find_wallet(app_services, checksum, error_message).await?;
    let owns_wallet = app_services
        .metadata_db
        .is_wallet_owned_by_user(checksum, &user.user_id)
        .await
        .or_internal(error_message)?;
    if !owns_wallet {
        return Err(ApiError::forbidden("access_denied", "Access denied"));
    }
    Ok(wallet)
}

/// Like [`verify_wallet_access`], but also lets admins read wallets of users
/// who granted them temporary support access.
pub(crate) async fn verify_wallet_read_access(
    app_services: &AppServicesState,
    user: &AuthUser,
    checksum: &str,
    error_message: &'static str,
) -> ApiResult<WalletMetadata> {
    let denied = match verify_wallet_access(app_services, user, checksum, error_message).await {
        Ok(wallet) => return Ok(wallet),
        Err(denied) => denied,
    };
    if !user.is_admin || user.is_demo {
        return Err(denied);
    }

    let wallet = find_wallet(app_services, checksum, error_message).await?;
    let has_grant = app_services
        .metadata_db
        .has_active_admin_support_grant(&user.user_id, &wallet.user_id)
        .await
        .or_internal(error_message)?;
    if has_grant { Ok(wallet) } else { Err(denied) }
}

async fn find_wallet(
    app_services: &AppServicesState,
    checksum: &str,
    error_message: &'static str,
) -> ApiResult<WalletMetadata> {
    app_services
        .metadata_db
        .get_wallet_by_checksum(checksum)
        .await
        .or_internal(error_message)?
        .ok_or_else(|| ApiError::not_found("wallet_not_found", "Wallet not found"))
}

/// Loads a user record, mapping a missing user to 404 with the given code.
pub(crate) async fn get_user_or_error(
    app_services: &AppServicesState,
    user_id: &str,
    not_found_code: Option<&'static str>,
    not_found_message: &'static str,
    error_message: &'static str,
) -> ApiResult<UserRecord> {
    app_services
        .metadata_db
        .get_user_by_id(user_id)
        .await
        .or_internal(error_message)?
        .ok_or_else(|| ApiError::Client {
            status: axum::http::StatusCode::NOT_FOUND,
            code: not_found_code,
            message: not_found_message.into(),
        })
}

/// Requires that `notification_target` was verified for the wallet recently.
pub(crate) async fn require_recent_verification(
    app_services: &AppServicesState,
    wallet_checksum: &str,
    notification_target: &str,
    verification_code: &'static str,
    verification_message: &'static str,
    error_message: &'static str,
) -> ApiResult<()> {
    let verified = app_services
        .metadata_db
        .was_recently_verified(wallet_checksum, notification_target)
        .await
        .or_internal(error_message)?;
    if verified {
        Ok(())
    } else {
        Err(ApiError::bad_request(
            verification_code,
            verification_message,
        ))
    }
}

/// Enforce per-tier creation limits using the current persisted resource count.
pub(crate) async fn check_resource_limit(
    app_services: &AppServicesState,
    config: &AppConfig,
    user_record: &UserRecord,
    resource: ResourceLimit<'_>,
) -> ApiResult<()> {
    if config.is_self_hosted_mode() || user_record.is_admin {
        return Ok(());
    }

    let tier_limits = user_record.subscription_tier.limits_for_api();
    match resource {
        ResourceLimit::Wallet { user_id } => {
            let wallet_count = app_services
                .metadata_db
                .count_wallets_for_user(user_id)
                .await
                .or_internal("Failed to check wallet limit")?;
            check_limit(wallet_count, tier_limits.max_wallets, "Wallet").map_err(|limit_error| {
                ApiError::forbidden("wallet_limit_reached", limit_error.to_string())
            })
        }
        ResourceLimit::Contact { wallet_checksum } => {
            let contact_count = app_services
                .metadata_db
                .count_active_contacts_for_wallet(wallet_checksum)
                .await
                .or_internal("Failed to check contact limit")?;
            check_limit(
                contact_count,
                tier_limits.max_contacts_per_wallet,
                "Contact",
            )
            .map_err(|limit_error| {
                ApiError::forbidden("contact_limit_reached", limit_error.to_string())
            })
        }
    }
}
