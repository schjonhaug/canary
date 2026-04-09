//! Application configuration handler

use crate::config::{AppConfig, NtfyTargetOption};
use axum::{extract::State, response::Json};
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct ConfigResponse {
    mempool_url: Option<String>,
    mempool_port: Option<u16>,
    ntfy_target_options: Vec<NtfyTargetOption>,
}

/// GET /api/config - Returns public application configuration
/// Custom mempool settings are only exposed in self-hosted mode;
/// cloud mode always uses mempool.space.
pub async fn get_config(State(config): State<Arc<AppConfig>>) -> Json<ConfigResponse> {
    if config.is_self_hosted_mode() {
        Json(ConfigResponse {
            mempool_url: config.mempool_url().map(|s| s.to_string()),
            mempool_port: config.mempool_port(),
            ntfy_target_options: config.ntfy_target_options(),
        })
    } else {
        Json(ConfigResponse {
            mempool_url: None,
            mempool_port: None,
            ntfy_target_options: vec![],
        })
    }
}
