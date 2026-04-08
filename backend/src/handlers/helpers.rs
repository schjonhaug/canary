use crate::api::AppServicesState;
use crate::auth::AuthUser;
use crate::metadata::WalletMetadata;
use crate::models::ErrorResponse;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};

pub async fn verify_wallet_access(
    app_services: &AppServicesState,
    user: &AuthUser,
    checksum: &str,
) -> Result<WalletMetadata, Response> {
    let wallet = app_services
        .metadata_db
        .get_wallet_by_checksum(checksum)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!("Database error: {}", e))),
            )
                .into_response()
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::coded("wallet_not_found", "Wallet not found")),
            )
                .into_response()
        })?;

    if !user.is_admin {
        let owns_wallet = app_services
            .metadata_db
            .is_wallet_owned_by_user(checksum, &user.user_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(format!("Database error: {}", e))),
                )
                    .into_response()
            })?;

        if !owns_wallet {
            return Err((
                StatusCode::FORBIDDEN,
                Json(ErrorResponse::coded("access_denied", "Access denied")),
            )
                .into_response());
        }
    }

    Ok(wallet)
}
