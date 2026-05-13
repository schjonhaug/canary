//! Application configuration handler

use crate::config::{AppConfig, NtfyServerConfig, TxExplorerConfig};
use axum::{extract::State, response::Json};
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct ConfigResponse {
    pub tx_explorers: Vec<TxExplorerConfig>,
    pub default_tx_explorer_id: String,
    pub ntfy_servers: Vec<NtfyServerConfig>,
    pub default_ntfy_server_id: String,
}

/// GET /api/config - Returns public application configuration
/// Custom mempool and ntfy settings are only exposed in self-hosted mode.
/// Detected ntfy URLs are returned so the frontend can select local integrations,
/// but browser-facing links still hide Docker-internal hostnames.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{NetworkConfig, OperatingMode};

    #[tokio::test]
    async fn cloud_config_does_not_expose_self_hosted_ntfy_servers() {
        let config = Arc::new(AppConfig::new_for_test(
            NetworkConfig::Mainnet,
            None,
            "127.0.0.1:3000".to_string(),
            "./database".to_string(),
            OperatingMode::Cloud,
            Some("http://localhost:3001".to_string()),
            Some("test-jwt-secret".to_string()),
        ));

        let Json(response) = get_config(State(config)).await;

        assert!(response.ntfy_servers.is_empty());
        assert_eq!(response.default_ntfy_server_id, "ntfy-sh");
    }
}
