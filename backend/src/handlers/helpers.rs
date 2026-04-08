use crate::api::AppServicesState;
use crate::auth::AuthUser;
use crate::metadata::{UserRecord, WalletMetadata};
use crate::models::ErrorResponse;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};

pub(crate) enum DatabaseErrorMessage {
    Raw,
    Prefix(&'static str),
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
