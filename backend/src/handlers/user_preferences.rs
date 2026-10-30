//! User preferences handlers

use crate::api::{AppServicesState, ConfigState};
use crate::auth::{UpdateUserPreferencesRequest, UserPreferencesResponse};
use crate::config::AppConfig;
use crate::exchange_rates;
use crate::extractors::{AuthenticatedUser, require_non_demo};
use crate::handlers::{ApiError, ApiResult, OrInternal};
use crate::outbound_target::validate_outbound_url;
use axum::{extract::State, http::StatusCode, response::Json};

const SUPPORTED_LANGUAGES: [&str; 9] = [
    "en-US", "nb", "es-419", "pt-BR", "de-DE", "fr-FR", "ja", "da", "sv",
];

/// Whether the user has ntfy credentials stored, without exposing them.
struct NtfyAuthStatus {
    has_access_token: bool,
    has_credentials: bool,
    username: Option<String>,
}

async fn ntfy_auth_status(app_services: &AppServicesState, user_id: &str) -> NtfyAuthStatus {
    match app_services.metadata_db.get_user_ntfy_auth(user_id).await {
        Ok((access_token, username, password)) => NtfyAuthStatus {
            has_access_token: access_token.is_some(),
            has_credentials: username.is_some() && password.is_some(),
            username,
        },
        Err(_) => NtfyAuthStatus {
            has_access_token: false,
            has_credentials: false,
            username: None,
        },
    }
}

/// Get user preferences (currency, ntfy settings)
pub async fn get_user_preferences(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
) -> ApiResult<Json<UserPreferencesResponse>> {
    const LOAD_FAILED: &str = "Failed to get user preferences";
    let db = &app_services.metadata_db;
    let currency = db
        .get_user_preferred_currency(&user.user_id)
        .await
        .or_internal(LOAD_FAILED)?;
    let ntfy_server_url = db
        .get_user_ntfy_server_url(&user.user_id)
        .await
        .or_internal(LOAD_FAILED)?;
    let preferred_tx_explorer_id = db
        .get_user_preferred_tx_explorer_id(&user.user_id)
        .await
        .or_internal(LOAD_FAILED)?;
    let ntfy_auth = ntfy_auth_status(&app_services, &user.user_id).await;

    Ok(Json(UserPreferencesResponse {
        preferred_fiat_currency: currency,
        preferred_tx_explorer_id,
        ntfy_server_url,
        ntfy_has_access_token: ntfy_auth.has_access_token,
        ntfy_has_credentials: ntfy_auth.has_credentials,
        ntfy_username: ntfy_auth.username,
    }))
}

/// Update user preferences
pub async fn update_user_preferences(
    AuthenticatedUser(user): AuthenticatedUser,
    State(app_services): State<AppServicesState>,
    State(config): State<ConfigState>,
    Json(request): Json<UpdateUserPreferencesRequest>,
) -> ApiResult<Json<UserPreferencesResponse>> {
    require_non_demo(&user)?;
    let db = &app_services.metadata_db;

    let current_currency = if let Some(currency) = &request.preferred_fiat_currency {
        if !exchange_rates::SUPPORTED_CURRENCIES.contains(&currency.as_str()) {
            return Err(ApiError::uncoded(
                StatusCode::BAD_REQUEST,
                format!("Unsupported currency: {currency}"),
            ));
        }
        db.update_user_preferred_currency(&user.user_id, currency)
            .await
            .or_internal("Failed to update preferences")?;
        currency.clone()
    } else {
        db.get_user_preferred_currency(&user.user_id)
            .await
            .unwrap_or_else(|_| "USD".to_string())
    };

    if let Some(language) = &request.preferred_language {
        if !SUPPORTED_LANGUAGES.contains(&language.as_str()) {
            return Err(ApiError::uncoded(
                StatusCode::BAD_REQUEST,
                format!("Unsupported language: {language}"),
            ));
        }
        db.update_user_preferred_language(&user.user_id, language)
            .await
            .or_internal("Failed to update language preference")?;
    }

    if let Some(explorer_id_update) = &request.preferred_tx_explorer_id {
        let explorer_id_to_store = explorer_id_update
            .as_deref()
            .map(|explorer_id| validated_tx_explorer_id(&config, explorer_id))
            .transpose()?
            .flatten();
        db.update_user_preferred_tx_explorer_id(&user.user_id, explorer_id_to_store.as_deref())
            .await
            .or_internal("Failed to update tx explorer preference")?;
    }

    let current_preferred_tx_explorer_id = db
        .get_user_preferred_tx_explorer_id(&user.user_id)
        .await
        .or_internal("Failed to get tx explorer preference")?;

    // An empty string clears the custom ntfy server.
    let current_ntfy_url = if let Some(ntfy_url) = &request.ntfy_server_url {
        let url_to_store = if ntfy_url.is_empty() {
            None
        } else {
            validate_ntfy_server_url(&config, ntfy_url).await?;
            Some(ntfy_url.as_str())
        };
        db.update_user_ntfy_server_url(&user.user_id, url_to_store)
            .await
            .or_internal("Failed to update ntfy server URL")?;
        url_to_store.map(str::to_string)
    } else {
        db.get_user_ntfy_server_url(&user.user_id)
            .await
            .unwrap_or(None)
    };

    if let Some((access_token, username, password)) = ntfy_auth_update(&request)? {
        db.update_user_ntfy_auth(&user.user_id, access_token, username, password)
            .await
            .or_internal("Failed to update ntfy authentication")?;
    }

    let ntfy_auth = ntfy_auth_status(&app_services, &user.user_id).await;

    Ok(Json(UserPreferencesResponse {
        preferred_fiat_currency: current_currency,
        preferred_tx_explorer_id: current_preferred_tx_explorer_id,
        ntfy_server_url: current_ntfy_url,
        ntfy_has_access_token: ntfy_auth.has_access_token,
        ntfy_has_credentials: ntfy_auth.has_credentials,
        ntfy_username: ntfy_auth.username,
    }))
}

/// Validates a requested tx explorer ID and returns the value to store.
///
/// A blank ID clears the preference and yields `None`.
fn validated_tx_explorer_id(config: &AppConfig, explorer_id: &str) -> ApiResult<Option<String>> {
    let explorer_id = explorer_id.trim();
    if explorer_id.is_empty() {
        return Ok(None);
    }
    if let Some(custom_explorer_id) = canonical_custom_tx_explorer_id(explorer_id) {
        return Ok(Some(custom_explorer_id));
    }
    let is_supported_public_explorer = matches!(
        explorer_id,
        "mempool-space" | "bitfeed-public" | "btc-rpc-explorer-public"
    );
    let is_configured_local_explorer = config.is_self_hosted_mode()
        && config
            .tx_explorers()
            .iter()
            .any(|explorer| explorer.id == explorer_id);
    if is_supported_public_explorer || is_configured_local_explorer {
        Ok(Some(explorer_id.to_string()))
    } else {
        Err(ApiError::bad_request(
            "unsupported_tx_explorer",
            format!("Unsupported tx explorer: {explorer_id}"),
        ))
    }
}

async fn validate_ntfy_server_url(config: &AppConfig, ntfy_url: &str) -> ApiResult<()> {
    if config.is_cloud_mode() {
        return Err(ApiError::bad_request(
            "custom_ntfy_server_unsupported",
            "Custom ntfy servers are only available in self-hosted mode",
        ));
    }
    if !ntfy_url.starts_with("http://") && !ntfy_url.starts_with("https://") {
        return Err(ApiError::uncoded(
            StatusCode::BAD_REQUEST,
            "ntfy server URL must start with http:// or https://",
        ));
    }
    validate_outbound_url(ntfy_url, config.ntfy_outbound_policy())
        .await
        .map_err(|error| ApiError::bad_request("invalid_ntfy_url", error))?;
    Ok(())
}

type NtfyAuthUpdate<'a> = (Option<&'a str>, Option<&'a str>, Option<&'a str>);

/// Works out the ntfy credentials to store, or `None` if the request leaves
/// them unchanged.
///
/// An access token takes precedence and clears username and password. Basic
/// auth needs both username and password; sending both empty clears them.
fn ntfy_auth_update(
    request: &UpdateUserPreferencesRequest,
) -> ApiResult<Option<NtfyAuthUpdate<'_>>> {
    if let Some(token) = &request.ntfy_access_token {
        let token = (!token.is_empty()).then_some(token.as_str());
        return Ok(Some((token, None, None)));
    }
    let username = request.ntfy_username.as_deref();
    let password = request.ntfy_password.as_deref();
    if username.is_none() && password.is_none() {
        return Ok(None);
    }
    let is_clearing = username.is_none_or(str::is_empty) && password.is_none_or(str::is_empty);
    if is_clearing {
        Ok(Some((None, None, None)))
    } else if username.is_none() || password.is_none() {
        Err(ApiError::uncoded(
            StatusCode::BAD_REQUEST,
            "Both ntfy_username and ntfy_password are required for Basic auth",
        ))
    } else {
        Ok(Some((None, username, password)))
    }
}

fn canonical_custom_tx_explorer_id(preferred_tx_explorer_id: &str) -> Option<String> {
    let template = preferred_tx_explorer_id.strip_prefix("custom:")?;
    let trimmed_template = template.trim();

    if trimmed_template.contains("{txid}")
        && (trimmed_template.starts_with("http://") || trimmed_template.starts_with("https://"))
    {
        Some(format!("custom:{trimmed_template}"))
    } else {
        None
    }
}
