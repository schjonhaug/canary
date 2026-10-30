//! Blockchain data handlers

use super::error::{ApiError, ApiResult, OrInternal};
use crate::api::AppServicesState;
use crate::config::{AppConfig, NetworkConfig};
use crate::exchange_rates;
use crate::models::BlockHeaderResponse;
use axum::{extract::State, http::StatusCode, response::Json};
use std::sync::Arc;

/// Get current block header from database
pub async fn get_current_block_header(
    State(app_services): State<AppServicesState>,
    State(config): State<Arc<AppConfig>>,
) -> ApiResult<Json<BlockHeaderResponse>> {
    let block_header = app_services
        .metadata_db
        .get_current_block_header()
        .await
        .or_internal("Failed to get block header")?
        .ok_or_else(|| ApiError::uncoded(StatusCode::NOT_FOUND, "No block header found"))?;

    let network = match config.network {
        NetworkConfig::Mainnet => "mainnet",
        NetworkConfig::Testnet => "testnet",
        NetworkConfig::Regtest => "regtest",
    };

    Ok(Json(BlockHeaderResponse {
        height: block_header.height,
        timestamp: block_header.timestamp,
        network: network.to_string(),
    }))
}

/// Get all cached exchange rates
pub async fn get_exchange_rates(
    State(app_services): State<AppServicesState>,
) -> ApiResult<Json<serde_json::Value>> {
    let rates = app_services
        .metadata_db
        .get_exchange_rates()
        .await
        .or_internal("Failed to get exchange rates")?;

    Ok(Json(serde_json::json!({
        "rates": rates,
        "supported_currencies": exchange_rates::SUPPORTED_CURRENCIES,
    })))
}
