use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Json, IntoResponse, Response},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;
use crate::wallet::WalletManager;
use crate::metadata::WalletMetadata;

#[derive(Deserialize, ToSchema)]
pub struct CreateWalletRequest {
    /// The name of the wallet
    #[schema(example = "My Bitcoin Wallet")]
    pub name: String,
    /// The multipath output descriptor for the wallet
    #[schema(example = "wpkh(tpubD6NzVbkrYhZ4XgiXtGrdW5XDAPFCL9h7we1vwNCpn8tGbBcgfVYjXyhWo4E1xkh56hjod1RhGjxbaTLV3X4FyWuejifB9jusQ46QzG87VKp/<0;1>/*)")] 
    pub descriptor: String,
}

#[derive(Serialize, ToSchema)]
pub struct CreateWalletResponse {
    /// Success message
    pub message: String,
    /// Created wallet metadata
    pub wallet: WalletMetadata,
}


#[derive(Serialize, ToSchema)]
pub struct ErrorResponse {
    /// Error description
    pub error: String,
}

pub type AppState = Arc<Mutex<WalletManager>>;

#[utoipa::path(
    post,
    path = "/wallets",
    request_body = CreateWalletRequest,
    responses(
        (status = 201, description = "Wallet created successfully", body = CreateWalletResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 409, description = "Descriptor already exists", body = ErrorResponse),
    ),
    tag = "wallet"
)]
pub async fn create_wallet(
    State(wallet_manager): State<AppState>,
    Json(payload): Json<CreateWalletRequest>,
) -> Response {
    match wallet_manager.lock().await.create_from_multipath(&payload.name, &payload.descriptor).await {
        Ok(wallet_metadata) => {
            (
                StatusCode::CREATED,
                Json(CreateWalletResponse {
                    message: "Wallet created successfully".to_string(),
                    wallet: wallet_metadata,
                }),
            ).into_response()
        }
        Err(e) => {
            let error_msg = e.to_string();
            let status_code = match error_msg.as_str() {
                "Descriptor already exists" => StatusCode::CONFLICT,
                "Wallet already exists" | "Wallet file already exists" => StatusCode::CONFLICT,
                _ => StatusCode::BAD_REQUEST,
            };
            
            (
                status_code,
                Json(ErrorResponse {
                    error: error_msg,
                }),
            ).into_response()
        }
    }
}

#[utoipa::path(
    delete,
    path = "/wallets/{id}",
    params(
        ("id" = i64, Path, description = "The wallet ID to delete")
    ),
    responses(
        (status = 204, description = "Wallet deleted successfully"),
        (status = 404, description = "Wallet not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    ),
    tag = "wallet"
)]
pub async fn delete_wallet(
    State(wallet_manager): State<AppState>,
    Path(id): Path<i64>,
) -> Response {
    match wallet_manager.lock().await.delete_wallet_by_id(id).await {
        Ok(()) => {
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => {
            let error_msg = e.to_string();
            let status_code = match error_msg.as_str() {
                "Wallet not found" => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            
            (
                status_code,
                Json(ErrorResponse {
                    error: error_msg,
                }),
            ).into_response()
        }
    }
}

#[utoipa::path(
    get,
    path = "/wallets/{id}",
    params(
        ("id" = i64, Path, description = "The wallet ID to retrieve")
    ),
    responses(
        (status = 200, description = "Wallet found", body = WalletMetadata),
        (status = 404, description = "Wallet not found", body = ErrorResponse),
    ),
    tag = "wallet"
)]
pub async fn get_wallet(
    State(wallet_manager): State<AppState>,
    Path(id): Path<i64>,
) -> Response {
    match wallet_manager.lock().await.get_wallet_by_id(id) {
        Ok(Some(wallet)) => {
            (StatusCode::OK, Json(wallet)).into_response()
        }
        Ok(None) => {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Wallet not found".to_string(),
                }),
            ).into_response()
        }
        Err(e) => {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            ).into_response()
        }
    }
}

#[utoipa::path(
    get,
    path = "/wallets",
    responses(
        (status = 200, description = "List of all wallets", body = Vec<WalletMetadata>),
    ),
    tag = "wallet"
)]
pub async fn get_all_wallets(
    State(wallet_manager): State<AppState>,
) -> Response {
    match wallet_manager.lock().await.get_all_wallets() {
        Ok(wallets) => {
            (StatusCode::OK, Json(wallets)).into_response()
        }
        Err(e) => {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            ).into_response()
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(create_wallet, delete_wallet, get_wallet, get_all_wallets),
    components(schemas(CreateWalletRequest, CreateWalletResponse, ErrorResponse, WalletMetadata)),
    tags(
        (name = "wallet", description = "Wallet management endpoints")
    ),
    info(
        title = "TxRay Wallet API",
        version = "0.1.0",
        description = "REST API for creating Bitcoin wallets from multipath descriptors",
    )
)]
pub struct ApiDoc;

pub fn create_router(wallet_manager: AppState) -> Router {
    Router::new()
        .route("/wallets", post(create_wallet).get(get_all_wallets))
        .route("/wallets/{id}", get(get_wallet).delete(delete_wallet))
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .with_state(wallet_manager)
}