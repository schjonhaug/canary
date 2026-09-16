//! Provider-related handlers

use crate::api::AppState;
use crate::models::ProvidersResponse;
use crate::telegram_provider::is_configured;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};

/// Get list of available notification providers
pub async fn get_providers(State(app_state): State<AppState>) -> Response {
    let mut providers = {
        let manager = app_state.notification_manager.lock().await;
        manager.list_providers()
    };
    let hide_telegram = app_state.config.is_cloud_mode()
        || !is_configured(&app_state.app_services.metadata_db).await;
    if hide_telegram {
        providers.retain(|provider| provider.name != "telegram");
    }
    (StatusCode::OK, Json(ProvidersResponse { providers })).into_response()
}
