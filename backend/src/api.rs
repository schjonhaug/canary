use crate::metadata::{ContactPerson, Language, SmsLog, TwilioConfig, WalletMetadata, TransactionEventWithWallet, EventType, DashboardUpdate};
use crate::wallet::WalletManager;
use crate::electrum::BlockHeader;
use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response, Sse},
    routing::{get, post},
};
use tower_http::cors::CorsLayer;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast};
use tokio_stream::wrappers::BroadcastStream;
use futures_util::StreamExt as FuturesStreamExt;
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;
use base64::{Engine as _, engine::general_purpose};
use phonenumber::PhoneNumber;
use std::str::FromStr;

#[derive(Deserialize, Serialize, ToSchema)]
pub struct CreateWalletRequest {
    /// The name of the wallet
    #[schema(example = "My Bitcoin Wallet")]
    pub name: String,
    /// The multipath output descriptor for the wallet
    #[schema(
        example = "wpkh(tpubD6NzVbkrYhZ4XgiXtGrdW5XDAPFCL9h7we1vwNCpn8tGbBcgfVYjXyhWo4E1xkh56hjod1RhGjxbaTLV3X4FyWuejifB9jusQ46QzG87VKp/<0;1>/*)"
    )]
    pub descriptor: String,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct UpdateWalletRequest {
    /// The new name for the wallet
    #[schema(example = "Updated Wallet Name")]
    pub name: String,
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

#[derive(Deserialize, Serialize, ToSchema)]
pub struct CreateContactRequest {
    /// The name of the contact person
    #[schema(example = "John Doe")]
    pub name: String,
    /// The phone number (must include country code)
    #[schema(example = "+4712345678")]
    pub phone_number: String,
    /// The language preference for SMS notifications
    #[schema(example = "en")]
    pub language: Language,
}

#[derive(Serialize, ToSchema)]
pub struct CreateContactResponse {
    /// Success message
    pub message: String,
    /// Contact ID
    pub contact_id: i64,
}

// This struct is no longer needed since contacts are created directly for wallets

#[derive(Deserialize, Serialize, ToSchema)]
pub struct TwilioConfigRequest {
    /// Twilio Account SID
    #[schema(example = "ACxxxxxxxxxxxxxxxxxxxxxxxxxxxxx")]
    pub account_sid: String,
    /// Twilio Auth Token
    #[schema(example = "your_auth_token")]
    pub auth_token: String,
    /// Twilio Messaging Service SID (use 'TEST' for test mode)
    #[schema(example = "MGxxxxxxxxxxxxxxxxxxxxxxxxxxxxx")]
    pub messaging_service_sid: String,
}

#[derive(Serialize, ToSchema)]
pub struct TwilioConfigResponse {
    /// Success message
    pub message: String,
}

pub type AppState = Arc<Mutex<WalletManager>>;
pub type BlockHeaderBroadcast = broadcast::Sender<BlockHeader>;
pub type DashboardBroadcast = broadcast::Sender<DashboardUpdate>;

/// Validates and normalizes a phone number
fn validate_phone_number(phone_number: &str) -> Result<String, String> {
    // Check if phone number starts with country code
    if !phone_number.starts_with('+') {
        return Err("Phone number must include country code (e.g., +4712345678)".to_string());
    }

    // Parse phone number
    let parsed_number = PhoneNumber::from_str(phone_number)
        .map_err(|_| "Invalid phone number format".to_string())?;

    // Check if it's a valid number
    if !parsed_number.is_valid() {
        return Err("Invalid phone number".to_string());
    }

    // Return normalized E.164 format
    Ok(parsed_number.format().mode(phonenumber::Mode::E164).to_string())
}

#[utoipa::path(
    post,
    path = "/api/wallets",
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
    match wallet_manager
        .lock()
        .await
        .create_from_multipath(&payload.name, &payload.descriptor)
        .await
    {
        Ok(wallet_metadata) => (
            StatusCode::CREATED,
            Json(CreateWalletResponse {
                message: "Wallet created successfully".to_string(),
                wallet: wallet_metadata,
            }),
        )
            .into_response(),
        Err(e) => {
            let error_msg = e.to_string();
            let status_code = match error_msg.as_str() {
                "Descriptor already exists" => StatusCode::CONFLICT,
                "Wallet already exists" | "Wallet file already exists" => StatusCode::CONFLICT,
                _ => StatusCode::BAD_REQUEST,
            };

            (status_code, Json(ErrorResponse { error: error_msg })).into_response()
        }
    }
}

#[utoipa::path(
    delete,
    path = "/api/wallets/{id}",
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
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            let error_msg = e.to_string();
            let status_code = match error_msg.as_str() {
                "Wallet not found" => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };

            (status_code, Json(ErrorResponse { error: error_msg })).into_response()
        }
    }
}

#[utoipa::path(
    put,
    path = "/api/wallets/{id}",
    params(
        ("id" = i64, Path, description = "The wallet ID to update")
    ),
    request_body = UpdateWalletRequest,
    responses(
        (status = 200, description = "Wallet updated successfully"),
        (status = 404, description = "Wallet not found", body = ErrorResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
    ),
    tag = "wallet"
)]
pub async fn update_wallet(
    State(wallet_manager): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateWalletRequest>,
) -> Response {
    if payload.name.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Wallet name cannot be empty".to_string(),
            }),
        )
            .into_response();
    }

    match wallet_manager.lock().await.update_wallet(id, &payload.name).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) => {
            let error_msg = e.to_string();
            let status_code = match error_msg.as_str() {
                "Wallet not found" => StatusCode::NOT_FOUND,
                _ => StatusCode::BAD_REQUEST,
            };

            (status_code, Json(ErrorResponse { error: error_msg })).into_response()
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/wallets/{id}",
    params(
        ("id" = i64, Path, description = "The wallet ID to retrieve")
    ),
    responses(
        (status = 200, description = "Wallet found", body = WalletMetadata),
        (status = 404, description = "Wallet not found", body = ErrorResponse),
    ),
    tag = "wallet"
)]
pub async fn get_wallet(State(wallet_manager): State<AppState>, Path(id): Path<i64>) -> Response {
    match wallet_manager.lock().await.get_wallet_by_id(id) {
        Ok(Some(wallet)) => (StatusCode::OK, Json(wallet)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Wallet not found".to_string(),
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


// Wallet-specific contact management endpoints

#[utoipa::path(
    post,
    path = "/api/wallets/{id}/contacts",
    request_body = CreateContactRequest,
    params(
        ("id" = i64, Path, description = "The wallet ID")
    ),
    responses(
        (status = 201, description = "Contact created successfully", body = CreateContactResponse),
        (status = 400, description = "Invalid request or phone number", body = ErrorResponse),
        (status = 404, description = "Wallet not found", body = ErrorResponse),
    ),
    tag = "contact"
)]
pub async fn create_wallet_contact(
    State(wallet_manager): State<AppState>,
    Path(wallet_id): Path<i64>,
    Json(payload): Json<CreateContactRequest>,
) -> Response {
    let manager = wallet_manager.lock().await;

    // Check if wallet exists
    match manager.get_wallet_by_id(wallet_id) {
        Ok(Some(_)) => {},
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Wallet not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
                .into_response();
        }
    }

    // Validate phone number first
    let normalized_phone = match validate_phone_number(&payload.phone_number) {
        Ok(phone) => phone,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse { error }),
            )
                .into_response();
        }
    };

    match manager.metadata_db.insert_contact(wallet_id, &payload.name, &normalized_phone, &payload.language) {
        Ok(contact_id) => {
            // Send dashboard update to notify clients of contact count change
            if let Err(e) = manager.send_dashboard_update().await {
                eprintln!("Failed to send dashboard update after contact creation: {}", e);
            }
            
            (
                StatusCode::CREATED,
                Json(CreateContactResponse {
                    message: "Contact created successfully".to_string(),
                    contact_id,
                }),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

// Global contacts endpoint removed - contacts are now wallet-specific

#[utoipa::path(
    delete,
    path = "/api/wallets/{wallet_id}/contacts/{contact_id}",
    params(
        ("wallet_id" = i64, Path, description = "The wallet ID"),
        ("contact_id" = i64, Path, description = "The contact ID")
    ),
    responses(
        (status = 204, description = "Contact deleted successfully"),
        (status = 404, description = "Contact not found", body = ErrorResponse),
    ),
    tag = "contact"
)]
pub async fn delete_wallet_contact(
    State(wallet_manager): State<AppState>,
    Path((_wallet_id, contact_id)): Path<(i64, i64)>,
) -> Response {
    let manager = wallet_manager.lock().await;
    match manager.metadata_db.delete_contact(contact_id) {
        Ok(true) => {
            // Send dashboard update to notify clients of contact count change
            if let Err(e) = manager.send_dashboard_update().await {
                eprintln!("Failed to send dashboard update after contact deletion: {}", e);
            }
            
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Contact not found".to_string(),
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

// This endpoint is no longer needed since contacts are created directly for wallets

// This function is now handled by delete_wallet_contact above

#[utoipa::path(
    get,
    path = "/api/wallets/{id}/contacts",
    params(
        ("id" = i64, Path, description = "The wallet ID")
    ),
    responses(
        (status = 200, description = "List of contacts for wallet", body = Vec<ContactPerson>),
        (status = 404, description = "Wallet not found", body = ErrorResponse),
    ),
    tag = "wallet"
)]
pub async fn get_wallet_contacts(
    State(wallet_manager): State<AppState>,
    Path(wallet_id): Path<i64>,
) -> Response {
    let manager = wallet_manager.lock().await;

    // Check if wallet exists
    match manager.get_wallet_by_id(wallet_id) {
        Ok(Some(_)) => {}
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Wallet not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
                .into_response();
        }
    }

    match manager.metadata_db.get_contacts_for_wallet(wallet_id) {
        Ok(contacts) => (StatusCode::OK, Json(contacts)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

// Twilio configuration endpoints

#[utoipa::path(
    post,
    path = "/api/twilio/config",
    request_body = TwilioConfigRequest,
    responses(
        (status = 201, description = "Twilio configuration saved successfully", body = TwilioConfigResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
    ),
    tag = "twilio"
)]
pub async fn save_twilio_config(
    State(wallet_manager): State<AppState>,
    Json(payload): Json<TwilioConfigRequest>,
) -> Response {
    // Skip validation if messaging_service_sid is 'TEST' (for test mode)
    if payload.messaging_service_sid == "TEST" {
        // Save directly to database without validation
        let manager = wallet_manager.lock().await;
        match manager.metadata_db.upsert_twilio_config(
            &payload.account_sid,
            &payload.auth_token,
            &payload.messaging_service_sid,
        ) {
            Ok(_) => (
                StatusCode::CREATED,
                Json(TwilioConfigResponse {
                    message: "Twilio configuration saved successfully (TEST mode - validation skipped)".to_string(),
                }),
            )
                .into_response(),
            Err(e) => (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
                .into_response(),
        }
    } else {
        // Validate credentials with Twilio
        let client = reqwest::Client::new();
        let auth_header = general_purpose::STANDARD.encode(format!("{}:{}", payload.account_sid, payload.auth_token));
        
        let validation_response = client
            .get("https://api.twilio.com/2010-04-01/Accounts.json")
            .header("Authorization", format!("Basic {}", auth_header))
            .send()
            .await;

        match validation_response {
            Ok(response) => {
                if !response.status().is_success() {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            error: "Invalid Twilio credentials. Please check your Account SID and Auth Token.".to_string(),
                        }),
                    ).into_response();
                }

                // Parse response to verify account SID matches
                match response.json::<serde_json::Value>().await {
                    Ok(data) => {
                        if let Some(accounts) = data.get("accounts").and_then(|a| a.as_array()) {
                            if let Some(account) = accounts.first() {
                                if let Some(account_sid) = account.get("sid").and_then(|s| s.as_str()) {
                                    if account_sid != payload.account_sid {
                                        return (
                                            StatusCode::BAD_REQUEST,
                                            Json(ErrorResponse {
                                                error: "Account SID mismatch in Twilio response.".to_string(),
                                            }),
                                        ).into_response();
                                    }
                                }
                            }
                        }
                    }
                    Err(_) => {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse {
                                error: "Failed to parse Twilio response.".to_string(),
                            }),
                        ).into_response();
                    }
                }
            }
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "Failed to validate credentials with Twilio.".to_string(),
                    }),
                ).into_response();
            }
        }

        // If validation passed, save to database
        let manager = wallet_manager.lock().await;
        match manager.metadata_db.upsert_twilio_config(
            &payload.account_sid,
            &payload.auth_token,
            &payload.messaging_service_sid,
        ) {
            Ok(_) => (
                StatusCode::CREATED,
                Json(TwilioConfigResponse {
                    message: "Twilio configuration validated and saved successfully".to_string(),
                }),
            )
                .into_response(),
            Err(e) => (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
                .into_response(),
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/twilio/config",
    responses(
        (status = 200, description = "Twilio configuration", body = TwilioConfig),
        (status = 404, description = "No Twilio configuration found", body = ErrorResponse),
    ),
    tag = "twilio"
)]
pub async fn get_twilio_config(State(wallet_manager): State<AppState>) -> Response {
    let manager = wallet_manager.lock().await;
    match manager.metadata_db.get_twilio_config() {
        Ok(Some(config)) => (StatusCode::OK, Json(config)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "No Twilio configuration found".to_string(),
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


#[utoipa::path(
    get,
    path = "/api/block-headers/current",
    responses(
        (status = 200, description = "Current block header from database", body = BlockHeader),
        (status = 404, description = "No block header found", body = ErrorResponse),
    ),
    tag = "blockchain"
)]
pub async fn get_current_block_header(State(wallet_manager): State<AppState>) -> Response {
    let manager = wallet_manager.lock().await;
    match manager.metadata_db.get_current_block_header() {
        Ok(Some(block_header)) => (StatusCode::OK, Json(block_header)).into_response(),
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

#[utoipa::path(
    get,
    path = "/api/block-headers/stream",
    responses(
        (status = 200, description = "Server-sent events stream of block headers", content_type = "text/event-stream"),
    ),
    tag = "blockchain"
)]
pub async fn block_headers_stream(
    State(block_header_tx): State<BlockHeaderBroadcast>,
) -> Response {
    use axum::response::sse::Event;
    
    let stream = FuturesStreamExt::filter_map(
        BroadcastStream::new(block_header_tx.subscribe()),
        |result| async move {
            match result {
                Ok(block_header) => {
                    // Convert to SSE format
                    let data = serde_json::to_string(&block_header).unwrap_or_default();
                    Some(Ok::<Event, axum::Error>(Event::default().data(data)))
                }
                Err(_) => None,
            }
        }
    );

    Sse::new(stream).into_response()
}

#[utoipa::path(
    get,
    path = "/api/dashboard/stream",
    responses(
        (status = 200, description = "Server-sent events stream of dashboard updates", content_type = "text/event-stream"),
    ),
    tag = "dashboard"
)]
pub async fn dashboard_stream(
    State(dashboard_tx): State<DashboardBroadcast>,
) -> Response {
    use axum::response::sse::Event;
    
    let stream = FuturesStreamExt::filter_map(
        BroadcastStream::new(dashboard_tx.subscribe()),
        |result| async move {
            match result {
                Ok(dashboard_update) => {
                    // Convert to SSE format
                    let data = serde_json::to_string(&dashboard_update).unwrap_or_default();
                    Some(Ok::<Event, axum::Error>(Event::default().data(data)))
                }
                Err(_) => None,
            }
        }
    );

    Sse::new(stream).into_response()
}

#[utoipa::path(
    get,
    path = "/api/dashboard",
    responses(
        (status = 200, description = "Current dashboard state", body = DashboardUpdate),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "dashboard"
)]
pub async fn get_dashboard(State(wallet_manager): State<AppState>) -> Response {
    let manager = wallet_manager.lock().await;
    match manager.get_current_dashboard_state().await {
        Ok(dashboard_update) => Json(dashboard_update).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to get dashboard state: {}", e),
            }),
        )
            .into_response(),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(
        create_wallet, update_wallet, delete_wallet, get_wallet,
        create_wallet_contact, delete_wallet_contact, get_wallet_contacts,
        save_twilio_config, get_twilio_config,
        get_current_block_header, block_headers_stream,
        dashboard_stream, get_dashboard
    ),
    components(schemas(
        CreateWalletRequest, UpdateWalletRequest, CreateWalletResponse, ErrorResponse, WalletMetadata,
        CreateContactRequest, CreateContactResponse,
        TwilioConfigRequest, TwilioConfigResponse,
        ContactPerson, TwilioConfig, SmsLog, TransactionEventWithWallet, EventType, Language,
        BlockHeader, DashboardUpdate
    )),
    tags(
        (name = "wallet", description = "Wallet management endpoints"),
        (name = "contact", description = "Contact management endpoints"),
        (name = "twilio", description = "Twilio configuration endpoints"),
        (name = "transaction", description = "Transaction events endpoints"),
        (name = "blockchain", description = "Blockchain information endpoints"),
        (name = "dashboard", description = "Dashboard real-time updates endpoints")
    ),
    info(
        title = "Canary Wallet API",
        version = "0.2.0",
        description = "REST API for creating Bitcoin wallets from multipath descriptors",
    )
)]
pub struct ApiDoc;

pub fn create_router(wallet_manager: AppState, block_header_tx: BlockHeaderBroadcast, dashboard_tx: DashboardBroadcast) -> Router {
    let wallet_routes = Router::new()
        .route("/wallets", post(create_wallet))
        .route("/wallets/{id}", get(get_wallet).put(update_wallet).delete(delete_wallet))
        .route(
            "/wallets/{id}/contacts",
            post(create_wallet_contact).get(get_wallet_contacts),
        )
        .route(
            "/wallets/{wallet_id}/contacts/{contact_id}",
            axum::routing::delete(delete_wallet_contact),
        )
        .route(
            "/twilio/config",
            post(save_twilio_config).get(get_twilio_config),
        )
        .route("/block-headers/current", get(get_current_block_header))
        .route("/dashboard", get(get_dashboard))
        .with_state(wallet_manager);

    let block_header_routes = Router::new()
        .route("/block-headers/stream", get(block_headers_stream))
        .with_state(block_header_tx);

    let dashboard_stream_routes = Router::new()
        .route("/dashboard/stream", get(dashboard_stream))
        .with_state(dashboard_tx);

    Router::new()
        .nest("/api", wallet_routes.merge(block_header_routes).merge(dashboard_stream_routes))
        .layer(CorsLayer::permissive())
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
}
