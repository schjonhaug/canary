use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::post,
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::wallet::WalletManager;

#[derive(Deserialize)]
pub struct CreateWalletRequest {
    pub descriptor: String,
}

#[derive(Serialize)]
pub struct CreateWalletResponse {
    pub message: String,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

pub type AppState = Arc<WalletManager>;

pub async fn create_wallet(
    State(wallet_manager): State<AppState>,
    Json(payload): Json<CreateWalletRequest>,
) -> Result<(StatusCode, Json<CreateWalletResponse>), (StatusCode, Json<ErrorResponse>)> {
    match wallet_manager.create_from_multipath(&payload.descriptor).await {
        Ok(first_address) => {
            println!("First address: {}", first_address);
            Ok((
                StatusCode::CREATED,
                Json(CreateWalletResponse {
                    message: "Wallet created successfully".to_string(),
                }),
            ))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )),
    }
}

pub fn create_router(wallet_manager: WalletManager) -> Router {
    let state = Arc::new(wallet_manager);
    
    Router::new()
        .route("/wallet", post(create_wallet))
        .with_state(state)
}