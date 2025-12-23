//! Provider-related handlers

use crate::api::NotificationManagerState;
use crate::models::ProvidersResponse;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};

/// Get list of available notification providers
pub async fn get_providers(
    State(notification_manager): State<NotificationManagerState>,
) -> Response {
    #[allow(unused_mut)]
    let mut manager = notification_manager.lock().await;
    let providers = manager.list_providers();
    (StatusCode::OK, Json(ProvidersResponse { providers })).into_response()
}
