//! Blockchain data handlers

use crate::api::AppServicesState;
use crate::config::{AppConfig, NetworkConfig};
use crate::exchange_rates;
use crate::models::{BlockHeaderResponse, ErrorResponse};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use std::sync::Arc;

/// Get current block header from database
pub async fn get_current_block_header(
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
) -> Response {
    let result = app_services.metadata_db.get_current_block_header().await;

    // Get network name from config
    let network = match config.network {
        NetworkConfig::Mainnet => "mainnet",
        NetworkConfig::Testnet => "testnet",
        NetworkConfig::Regtest => "regtest",
    };

    match result {
        Ok(Some(block_header)) => {
            let response = BlockHeaderResponse {
                height: block_header.height,
                timestamp: block_header.timestamp,
                network: network.to_string(),
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "No block header found".to_string(),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

/// Get all cached exchange rates
pub async fn get_exchange_rates(State(app_services): State<AppServicesState>) -> Response {
    match app_services.metadata_db.get_exchange_rates().await {
        Ok(rates) => Json(serde_json::json!({
            "rates": rates,
            "supported_currencies": exchange_rates::SUPPORTED_CURRENCIES,
        }))
        .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to get exchange rates: {}", e),
            }),
        )
            .into_response(),
    }
}
