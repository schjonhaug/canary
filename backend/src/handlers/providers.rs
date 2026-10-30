//! Provider-related handlers

use crate::api::AppState;
use crate::models::ProvidersResponse;
use crate::telegram_provider::is_configured;
use axum::{extract::State, response::Json};

/// Get list of available notification providers
pub async fn get_providers(State(app_state): State<AppState>) -> Json<ProvidersResponse> {
    let mut providers = app_state.notification_manager.list_providers();
    let hide_telegram = app_state.config.is_cloud_mode()
        || !is_configured(&app_state.app_services.metadata_db, &app_state.config).await;
    if hide_telegram {
        providers.retain(|provider| provider.name != "telegram");
    }
    Json(ProvidersResponse { providers })
}
