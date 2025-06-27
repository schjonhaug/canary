use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use utoipa::{OpenApi, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};
use utoipa_swagger_ui::SwaggerUi;
use crate::wallet::WalletManager;

#[derive(Deserialize, ToSchema)]
pub struct CreateWalletRequest {
    /// The multipath output descriptor for the wallet
    #[schema(example = "wpkh(tpubD6NzVbkrYhZ4XgiXtGrdW5XDAPFCL9h7we1vwNCpn8tGbBcgfVYjXyhWo4E1xkh56hjod1RhGjxbaTLV3X4FyWuejifB9jusQ46QzG87VKp/<0;1>/*)")] 
    pub descriptor: String,
}

#[derive(Serialize, ToSchema)]
pub struct CreateWalletResponse {
    /// Success message
    pub message: String,
}

#[derive(Serialize, ToSchema)]
pub struct ErrorResponse {
    /// Error description
    pub error: String,
}

pub type AppState = Arc<Mutex<WalletManager>>;

#[utoipa::path(
    post,
    path = "/wallet",
    request_body = CreateWalletRequest,
    responses(
        (status = 201, description = "Wallet created successfully", body = CreateWalletResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Wallet already exists", body = ErrorResponse),
    ),
    tag = "wallet"
)]
pub async fn create_wallet(
    State(wallet_manager): State<AppState>,
    Json(payload): Json<CreateWalletRequest>,
) -> Result<(StatusCode, Json<CreateWalletResponse>), (StatusCode, Json<ErrorResponse>)> {
    match wallet_manager.lock().await.create_from_multipath(&payload.descriptor).await {
        Ok(()) => {
            Ok((
                StatusCode::CREATED,
                Json(CreateWalletResponse {
                    message: "Wallet created successfully".to_string(),
                }),
            ))
        }
        Err(e) => {
            let error_msg = e.to_string();
            let status_code = if error_msg == "Wallet already exists" {
                StatusCode::INTERNAL_SERVER_ERROR
            } else {
                StatusCode::BAD_REQUEST
            };
            
            Err((
                status_code,
                Json(ErrorResponse {
                    error: error_msg,
                }),
            ))
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(create_wallet),
    components(schemas(CreateWalletRequest, CreateWalletResponse, ErrorResponse)),
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

pub fn create_router(wallet_manager: WalletManager) -> Router {
    let state = Arc::new(Mutex::new(wallet_manager));
    
    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(create_wallet))
        .split_for_parts();
    
    router
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", api))
        .with_state(state)
}