use crate::metadata::{ContactPerson, SmsLog, TwilioConfig, WalletMetadata, TransactionEventWithWallet, EventType};
use crate::wallet::WalletManager;
use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::{get, post},
};
use tower_http::cors::CorsLayer;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;
use base64::{Engine as _, engine::general_purpose};

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
    /// The phone number (without country code)
    #[schema(example = "12345678")]
    pub phone_number: String,
}

#[derive(Serialize, ToSchema)]
pub struct CreateContactResponse {
    /// Success message
    pub message: String,
    /// Contact ID
    pub contact_id: i64,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct AddContactToWalletRequest {
    /// The contact ID to add
    pub contact_id: i64,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct TwilioConfigRequest {
    /// Twilio Account SID
    #[schema(example = "ACxxxxxxxxxxxxxxxxxxxxxxxxxxxxx")]
    pub account_sid: String,
    /// Twilio Auth Token
    #[schema(example = "your_auth_token")]
    pub auth_token: String,
    /// Twilio Messaging Service SID
    #[schema(example = "MGxxxxxxxxxxxxxxxxxxxxxxxxxxxxx")]
    pub messaging_service_sid: String,
}

#[derive(Serialize, ToSchema)]
pub struct TwilioConfigResponse {
    /// Success message
    pub message: String,
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

#[utoipa::path(
    get,
    path = "/wallets",
    responses(
        (status = 200, description = "List of all wallets", body = Vec<WalletMetadata>),
    ),
    tag = "wallet"
)]
pub async fn get_all_wallets(State(wallet_manager): State<AppState>) -> Response {
    match wallet_manager.lock().await.get_all_wallets() {
        Ok(wallets) => (StatusCode::OK, Json(wallets)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

// Contact management endpoints

#[utoipa::path(
    post,
    path = "/contacts",
    request_body = CreateContactRequest,
    responses(
        (status = 201, description = "Contact created successfully", body = CreateContactResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
    ),
    tag = "contact"
)]
pub async fn create_contact(
    State(wallet_manager): State<AppState>,
    Json(payload): Json<CreateContactRequest>,
) -> Response {
    let manager = wallet_manager.lock().await;
    match manager
        .metadata_db
        .insert_contact(&payload.name, &payload.phone_number)
    {
        Ok(contact_id) => (
            StatusCode::CREATED,
            Json(CreateContactResponse {
                message: "Contact created successfully".to_string(),
                contact_id,
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

#[utoipa::path(
    get,
    path = "/contacts",
    responses(
        (status = 200, description = "List of all contacts", body = Vec<ContactPerson>),
    ),
    tag = "contact"
)]
pub async fn get_all_contacts(State(wallet_manager): State<AppState>) -> Response {
    let manager = wallet_manager.lock().await;
    match manager.metadata_db.get_all_contacts() {
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

#[utoipa::path(
    delete,
    path = "/contacts/{id}",
    params(
        ("id" = i64, Path, description = "The contact ID to delete")
    ),
    responses(
        (status = 204, description = "Contact deleted successfully"),
        (status = 404, description = "Contact not found", body = ErrorResponse),
    ),
    tag = "contact"
)]
pub async fn delete_contact(
    State(wallet_manager): State<AppState>,
    Path(id): Path<i64>,
) -> Response {
    let manager = wallet_manager.lock().await;
    match manager.metadata_db.delete_contact(id) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
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

#[utoipa::path(
    post,
    path = "/wallets/{id}/contacts",
    params(
        ("id" = i64, Path, description = "The wallet ID")
    ),
    request_body = AddContactToWalletRequest,
    responses(
        (status = 201, description = "Contact added to wallet successfully"),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 404, description = "Wallet not found", body = ErrorResponse),
    ),
    tag = "wallet"
)]
pub async fn add_contact_to_wallet(
    State(wallet_manager): State<AppState>,
    Path(wallet_id): Path<i64>,
    Json(payload): Json<AddContactToWalletRequest>,
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

    match manager
        .metadata_db
        .add_contact_to_wallet(wallet_id, payload.contact_id)
    {
        Ok(_) => StatusCode::CREATED.into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

#[utoipa::path(
    delete,
    path = "/wallets/{wallet_id}/contacts/{contact_id}",
    params(
        ("wallet_id" = i64, Path, description = "The wallet ID"),
        ("contact_id" = i64, Path, description = "The contact ID to remove")
    ),
    responses(
        (status = 204, description = "Contact removed from wallet successfully"),
        (status = 404, description = "Contact not found in wallet", body = ErrorResponse),
    ),
    tag = "wallet"
)]
pub async fn remove_contact_from_wallet(
    State(wallet_manager): State<AppState>,
    Path((wallet_id, contact_id)): Path<(i64, i64)>,
) -> Response {
    let manager = wallet_manager.lock().await;
    match manager
        .metadata_db
        .remove_contact_from_wallet(wallet_id, contact_id)
    {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Contact not found in wallet".to_string(),
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
    path = "/wallets/{id}/contacts",
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
    path = "/twilio/config",
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
    // First validate credentials with Twilio
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

#[utoipa::path(
    get,
    path = "/twilio/config",
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
    path = "/transaction-events",
    responses(
        (status = 200, description = "List of all transaction events with wallet names", body = Vec<TransactionEventWithWallet>),
    ),
    tag = "transaction"
)]
pub async fn get_all_transaction_events(State(wallet_manager): State<AppState>) -> Response {
    let manager = wallet_manager.lock().await;
    match manager.metadata_db.get_all_events_with_wallets() {
        Ok(events) => (StatusCode::OK, Json(events)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(
        create_wallet, delete_wallet, get_wallet, get_all_wallets,
        create_contact, get_all_contacts, delete_contact,
        add_contact_to_wallet, remove_contact_from_wallet, get_wallet_contacts,
        save_twilio_config, get_twilio_config,
        get_all_transaction_events
    ),
    components(schemas(
        CreateWalletRequest, CreateWalletResponse, ErrorResponse, WalletMetadata,
        CreateContactRequest, CreateContactResponse, AddContactToWalletRequest,
        TwilioConfigRequest, TwilioConfigResponse,
        ContactPerson, TwilioConfig, SmsLog, TransactionEventWithWallet, EventType
    )),
    tags(
        (name = "wallet", description = "Wallet management endpoints"),
        (name = "contact", description = "Contact management endpoints"),
        (name = "twilio", description = "Twilio configuration endpoints"),
        (name = "transaction", description = "Transaction events endpoints")
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
        .route(
            "/wallets/{id}/contacts",
            post(add_contact_to_wallet).get(get_wallet_contacts),
        )
        .route(
            "/wallets/{wallet_id}/contacts/{contact_id}",
            axum::routing::delete(remove_contact_from_wallet),
        )
        .route("/contacts", post(create_contact).get(get_all_contacts))
        .route("/contacts/{id}", axum::routing::delete(delete_contact))
        .route(
            "/twilio/config",
            post(save_twilio_config).get(get_twilio_config),
        )
        .route("/transaction-events", get(get_all_transaction_events))
        .layer(CorsLayer::permissive())
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .with_state(wallet_manager)
}
