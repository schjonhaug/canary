use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::{OpenApi, ToSchema};
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

pub type AppState = Arc<WalletManager>;

#[utoipa::path(
    post,
    path = "/wallet",
    request_body = CreateWalletRequest,
    responses(
        (status = 201, description = "Wallet created successfully", body = CreateWalletResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
    ),
    tag = "wallet"
)]
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
    let state = Arc::new(wallet_manager);
    
    Router::new()
        .route("/wallet", post(create_wallet))
        .route("/api-docs/openapi.json", get(serve_openapi))
        .route("/swagger-ui", get(serve_swagger_ui))
        .with_state(state)
}

async fn serve_openapi() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

async fn serve_swagger_ui() -> Html<String> {
    Html(format!(r#"
<!DOCTYPE html>
<html>
<head>
    <title>TxRay Wallet API</title>
    <link rel="stylesheet" type="text/css" href="https://unpkg.com/swagger-ui-dist@5.0.0/swagger-ui.css" />
    <style>
        html {{ box-sizing: border-box; overflow: -moz-scrollbars-vertical; overflow-y: scroll; }}
        *, *:before, *:after {{ box-sizing: inherit; }}
        body {{ margin:0; background: #fafafa; }}
    </style>
</head>
<body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui-dist@5.0.0/swagger-ui-bundle.js"></script>
    <script src="https://unpkg.com/swagger-ui-dist@5.0.0/swagger-ui-standalone-preset.js"></script>
    <script>
        window.onload = function() {{
            SwaggerUIBundle({{
                url: '/api-docs/openapi.json',
                dom_id: '#swagger-ui',
                deepLinking: true,
                presets: [
                    SwaggerUIBundle.presets.apis,
                    SwaggerUIStandalonePreset
                ],
                plugins: [
                    SwaggerUIBundle.plugins.DownloadUrl
                ],
                layout: "StandaloneLayout"
            }})
        }}
    </script>
</body>
</html>
    "#))
}