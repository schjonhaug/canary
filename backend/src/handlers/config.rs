//! Application configuration handler

use crate::config::{AppConfig, NtfyServerConfig, TxExplorerConfig};
use axum::{extract::State, response::Json};
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct ConfigResponse {
    tx_explorers: Vec<TxExplorerConfig>,
    default_tx_explorer_id: String,
    ntfy_servers: Vec<NtfyServerConfig>,
    default_ntfy_server_id: String,
}

/// GET /api/config - Returns public application configuration
/// Custom mempool settings are only exposed in self-hosted mode;
/// cloud mode always uses mempool.space.
pub async fn get_config(State(config): State<Arc<AppConfig>>) -> Json<ConfigResponse> {
    if config.is_self_hosted_mode() {
        let tx_explorers = config.tx_explorers().to_vec();
        let default_tx_explorer_id = if tx_explorers.len() == 1 {
            tx_explorers[0].id.clone()
        } else {
            "mempool-space".to_string()
        };

        Json(ConfigResponse {
            tx_explorers,
            default_tx_explorer_id,
            ntfy_servers: config.ntfy_servers().to_vec(),
            default_ntfy_server_id: config.default_ntfy_server_id(),
        })
    } else {
        Json(ConfigResponse {
            tx_explorers: Vec::new(),
            default_tx_explorer_id: "mempool-space".to_string(),
            ntfy_servers: Vec::new(),
            default_ntfy_server_id: config.default_ntfy_server_id(),
        })
    }
}
