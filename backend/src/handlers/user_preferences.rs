//! User preferences handlers

use crate::api::AppServicesState;
use crate::auth::{UpdateUserPreferencesRequest, UserPreferencesResponse};
use crate::exchange_rates;
use crate::extractors::{require_non_demo, AuthenticatedUser};
use crate::models::ErrorResponse;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};

const SUPPORTED_TX_EXPLORER_IDS: [&str; 4] =
    ["mempool-space", "mempool", "bitfeed", "btc-rpc-explorer"];

/// Get user preferences (currency, ntfy settings)
pub async fn get_user_preferences(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
) -> Response {
    // Get user's preferred currency
    let currency = match app_services
        .metadata_db
        .get_user_preferred_currency(&user.user_id)
        .await
    {
        Ok(currency) => currency,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to get user preferences: {}",
                    e
                ))),
            )
                .into_response();
        }
    };

    // Get user's ntfy server URL preference
    let ntfy_server_url = match app_services
        .metadata_db
        .get_user_ntfy_server_url(&user.user_id)
        .await
    {
        Ok(url) => url,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to get user preferences: {}",
                    e
                ))),
            )
                .into_response();
        }
    };
    let preferred_tx_explorer_id = match app_services
        .metadata_db
        .get_user_preferred_tx_explorer_id(&user.user_id)
        .await
    {
        Ok(explorer_id) => explorer_id,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to get user preferences: {}",
                    e
                ))),
            )
                .into_response();
        }
    };

    // Get ntfy authentication status (don't expose actual credentials)
    let (ntfy_has_access_token, ntfy_has_credentials, ntfy_username) = match app_services
        .metadata_db
        .get_user_ntfy_auth(&user.user_id)
        .await
    {
        Ok((access_token, username, password)) => (
            access_token.is_some(),
            username.is_some() && password.is_some(),
            username,
        ),
        Err(_) => (false, false, None),
    };

    Json(UserPreferencesResponse {
        preferred_fiat_currency: currency,
        preferred_tx_explorer_id,
        ntfy_server_url,
        ntfy_has_access_token,
        ntfy_has_credentials,
        ntfy_username,
    })
    .into_response()
}

/// Update user preferences
pub async fn update_user_preferences(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    Json(request): Json<UpdateUserPreferencesRequest>,
) -> Response {
    // Reject demo users from updating preferences
    if let Err(response) = require_non_demo(&user) {
        return response;
    }

    // Update preferred_fiat_currency if provided
    let current_currency = if let Some(ref currency) = request.preferred_fiat_currency {
        // Validate currency is supported
        if !exchange_rates::SUPPORTED_CURRENCIES.contains(&currency.as_str()) {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(format!(
                    "Unsupported currency: {}",
                    currency
                ))),
            )
                .into_response();
        }

        // Update user's preferred currency
        if let Err(e) = app_services
            .metadata_db
            .update_user_preferred_currency(&user.user_id, currency)
            .await
        {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to update preferences: {}",
                    e
                ))),
            )
                .into_response();
        }
        currency.clone()
    } else {
        // Get current currency
        app_services
            .metadata_db
            .get_user_preferred_currency(&user.user_id)
            .await
            .unwrap_or_else(|_| "USD".to_string())
    };

    // Update preferred_language if provided
    if let Some(ref language) = request.preferred_language {
        // Validate language is supported
        const SUPPORTED_LANGUAGES: [&str; 9] = [
            "en-US", "nb", "es-419", "pt-BR", "de-DE", "fr-FR", "ja", "da", "sv",
        ];
        if !SUPPORTED_LANGUAGES.contains(&language.as_str()) {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(format!(
                    "Unsupported language: {}",
                    language
                ))),
            )
                .into_response();
        }

        // Update user's preferred language
        if let Err(e) = app_services
            .metadata_db
            .update_user_preferred_language(&user.user_id, language)
            .await
        {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to update language preference: {}",
                    e
                ))),
            )
                .into_response();
        }
    }

    if let Some(ref preferred_tx_explorer_id) = request.preferred_tx_explorer_id {
        let explorer_id_to_store = if preferred_tx_explorer_id.is_empty() {
            None
        } else {
            if !SUPPORTED_TX_EXPLORER_IDS.contains(&preferred_tx_explorer_id.as_str()) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new(format!(
                        "Unsupported tx explorer: {}",
                        preferred_tx_explorer_id
                    ))),
                )
                    .into_response();
            }
            Some(preferred_tx_explorer_id.as_str())
        };

        if let Err(e) = app_services
            .metadata_db
            .update_user_preferred_tx_explorer_id(&user.user_id, explorer_id_to_store)
            .await
        {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to update tx explorer preference: {}",
                    e
                ))),
            )
                .into_response();
        }
    }

    let current_preferred_tx_explorer_id = match app_services
        .metadata_db
        .get_user_preferred_tx_explorer_id(&user.user_id)
        .await
    {
        Ok(explorer_id) => explorer_id,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to get tx explorer preference: {}",
                    e
                ))),
            )
                .into_response();
        }
    };

    // Update ntfy_server_url if the field was provided in the request
    // Note: We check if the outer Option is Some (field was in JSON)
    // The inner value can be Some(url) to set, or could be empty string to clear
    let current_ntfy_url = if let Some(ref ntfy_url) = request.ntfy_server_url {
        // Validate URL format if not empty
        let url_to_store = if ntfy_url.is_empty() {
            None
        } else {
            // Basic URL validation
            if !ntfy_url.starts_with("http://") && !ntfy_url.starts_with("https://") {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new(
                        "ntfy server URL must start with http:// or https://",
                    )),
                )
                    .into_response();
            }
            Some(ntfy_url.as_str())
        };

        if let Err(e) = app_services
            .metadata_db
            .update_user_ntfy_server_url(&user.user_id, url_to_store)
            .await
        {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to update ntfy server URL: {}",
                    e
                ))),
            )
                .into_response();
        }
        url_to_store.map(|s| s.to_string())
    } else {
        // Get current ntfy URL
        app_services
            .metadata_db
            .get_user_ntfy_server_url(&user.user_id)
            .await
            .unwrap_or(None)
    };

    // Update ntfy authentication if any auth fields were provided
    // Access token takes precedence - if set, it clears username/password
    // Username/password are set together - both required
    let should_update_auth = request.ntfy_access_token.is_some()
        || request.ntfy_username.is_some()
        || request.ntfy_password.is_some();

    if should_update_auth {
        let (access_token, username, password) = if let Some(ref token) = request.ntfy_access_token
        {
            // Access token auth - clear username/password
            let token = if token.is_empty() {
                None
            } else {
                Some(token.as_str())
            };
            (token, None, None)
        } else if request.ntfy_username.is_some() || request.ntfy_password.is_some() {
            // Basic auth - both username and password required
            let username = request.ntfy_username.as_deref();
            let password = request.ntfy_password.as_deref();

            // Allow clearing by setting both to empty
            let is_clearing =
                username.is_none_or(|u| u.is_empty()) && password.is_none_or(|p| p.is_empty());

            if is_clearing {
                (None, None, None)
            } else if username.is_none() || password.is_none() {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new(
                        "Both ntfy_username and ntfy_password are required for Basic auth",
                    )),
                )
                    .into_response();
            } else {
                (None, username, password)
            }
        } else {
            (None, None, None)
        };

        if let Err(e) = app_services
            .metadata_db
            .update_user_ntfy_auth(&user.user_id, access_token, username, password)
            .await
        {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to update ntfy authentication: {}",
                    e
                ))),
            )
                .into_response();
        }
    }

    // Get current ntfy auth status for response
    let (ntfy_has_access_token, ntfy_has_credentials, ntfy_username) = match app_services
        .metadata_db
        .get_user_ntfy_auth(&user.user_id)
        .await
    {
        Ok((access_token, username, password)) => (
            access_token.is_some(),
            username.is_some() && password.is_some(),
            username,
        ),
        Err(_) => (false, false, None),
    };

    Json(UserPreferencesResponse {
        preferred_fiat_currency: current_currency,
        preferred_tx_explorer_id: current_preferred_tx_explorer_id,
        ntfy_server_url: current_ntfy_url,
        ntfy_has_access_token,
        ntfy_has_credentials,
        ntfy_username,
    })
    .into_response()
}
