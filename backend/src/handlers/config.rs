//! Application configuration handler

use crate::config::AppConfig;
use axum::{extract::State, response::Json};
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct ConfigResponse {
    pub mempool_url: Option<String>,
    pub mempool_port: Option<u16>,
}

/// GET /api/config - Returns public application configuration
/// Custom mempool settings are only exposed in self-hosted mode;
/// cloud mode always uses mempool.space.
pub async fn get_config(State(config): State<Arc<AppConfig>>) -> Json<ConfigResponse> {
    if config.is_self_hosted_mode() {
        Json(ConfigResponse {
            mempool_url: config.mempool_url().map(|s| s.to_string()),
            mempool_port: config.mempool_port(),
        })
    } else {
        Json(ConfigResponse {
            mempool_url: None,
            mempool_port: None,
        })
    }
}
