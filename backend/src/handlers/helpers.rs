use crate::api::AppServicesState;
use crate::auth::AuthUser;
use crate::config::AppConfig;
use crate::metadata::{UserRecord, WalletMetadata};
use crate::models::ErrorResponse;
use crate::subscription::check_limit;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};

pub(crate) enum DatabaseErrorMessage {
    Raw,
    Prefix(&'static str),
}

pub(crate) enum ResourceLimit<'a> {
    Wallet { user_id: &'a str },
    Contact { wallet_checksum: &'a str },
}

fn error_response(
    status: StatusCode,
    code: Option<&'static str>,
    message: impl Into<String>,
) -> Response {
    let message = message.into();
    match code {
        Some(code) => (status, Json(ErrorResponse::coded(code, message))).into_response(),
        None => (status, Json(ErrorResponse::new(message))).into_response(),
    }
}

fn database_error_response(style: DatabaseErrorMessage, error: impl std::fmt::Display) -> Response {
    match style {
        DatabaseErrorMessage::Raw => {
            error_response(StatusCode::INTERNAL_SERVER_ERROR, None, error.to_string())
        }
        DatabaseErrorMessage::Prefix(prefix) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            None,
            format!("{prefix}: {error}"),
        ),
    }
}

pub(crate) async fn verify_wallet_access(
    app_services: &AppServicesState,
    user: &AuthUser,
    checksum: &str,
    error_style: DatabaseErrorMessage,
) -> Result<WalletMetadata, Response> {
    let wallet = match app_services
        .metadata_db
        .get_wallet_by_checksum(checksum)
        .await
    {
        Ok(Some(wallet)) => wallet,
        Ok(None) => {
            return Err(error_response(
                StatusCode::NOT_FOUND,
                Some("wallet_not_found"),
                "Wallet not found",
            ));
        }
        Err(error) => return Err(database_error_response(error_style, error)),
    };

    if !user.is_admin && wallet.user_id != user.user_id {
        return Err(error_response(
            StatusCode::FORBIDDEN,
            Some("access_denied"),
            "Access denied",
        ));
    }

    Ok(wallet)
}

pub(crate) async fn get_user_or_error(
    app_services: &AppServicesState,
    user_id: &str,
    not_found_code: Option<&'static str>,
    not_found_message: &'static str,
    error_style: DatabaseErrorMessage,
) -> Result<UserRecord, Response> {
    match app_services.metadata_db.get_user_by_id(user_id).await {
        Ok(Some(user_record)) => Ok(user_record),
        Ok(None) => Err(error_response(
            StatusCode::NOT_FOUND,
            not_found_code,
            not_found_message,
        )),
        Err(error) => Err(database_error_response(error_style, error)),
    }
}

pub(crate) async fn require_recent_verification(
    app_services: &AppServicesState,
    wallet_checksum: &str,
    notification_target: &str,
    verification_code: &'static str,
    verification_message: &'static str,
    error_prefix: &'static str,
) -> Result<(), Response> {
    match app_services
        .metadata_db
        .was_recently_verified(wallet_checksum, notification_target)
        .await
    {
        Ok(true) => Ok(()),
        Ok(false) => Err(error_response(
            StatusCode::BAD_REQUEST,
            Some(verification_code),
            verification_message,
        )),
        Err(error) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            None,
            format!("{error_prefix}: {error}"),
        )),
    }
}

pub(crate) async fn check_resource_limit(
    app_services: &AppServicesState,
    config: &AppConfig,
    user_record: &UserRecord,
    resource: ResourceLimit<'_>,
) -> Result<(), Response> {
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
                .map_err(|error| {
                    error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        None,
                        format!("Failed to check wallet limit: {error}"),
                    )
                })?;

            check_limit(wallet_count, tier_limits.max_wallets, "Wallet").map_err(|limit_error| {
                error_response(
                    StatusCode::FORBIDDEN,
                    Some("wallet_limit_reached"),
                    limit_error.to_string(),
                )
            })
        }
        ResourceLimit::Contact { wallet_checksum } => {
            let contact_count = app_services
                .metadata_db
                .count_contacts_for_wallet(wallet_checksum)
                .await
                .map_err(|error| {
                    error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        None,
                        format!("Failed to check contact limit: {error}"),
                    )
                })?;

            check_limit(
                contact_count,
                tier_limits.max_contacts_per_wallet,
                "Contact",
            )
            .map_err(|limit_error| {
                error_response(
                    StatusCode::FORBIDDEN,
                    Some("contact_limit_reached"),
                    limit_error.to_string(),
                )
            })
        }
    }
}
