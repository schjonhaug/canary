use crate::metadata::{Contact, NotificationMethod, ProviderType, Language, WalletMetadata, TransactionEventWithWallet, EventType, WalletsListResponse, WalletDetailResponse};
use crate::notifications::{NotificationManager, ProviderInfo};
use crate::wallet::WalletManager;
use crate::electrum::BlockHeader;
use crate::auth::{AuthService, SendOtpRequest, VerifyOtpRequest, AuthResponse, AuthUserResponse, authenticate_user};
use axum::{
    Router,
    extract::{Path, State},
    http::{StatusCode, HeaderMap},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
};
use tower_http::cors::CorsLayer;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;
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
pub struct NotificationMethodRequest {
    /// The provider type (sms or ntfy)
    #[schema(example = "sms")]
    pub provider_type: ProviderType,
    /// The notification target (phone number or ntfy topic)
    #[schema(example = "+4712345678")]
    pub notification_target: String,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct CreateContactWithMethodsRequest {
    /// The name of the contact person
    #[schema(example = "John Doe")]
    pub name: String,
    /// The language preference for notifications
    #[schema(example = "en")]
    pub language: Language,
    /// List of notification methods for this contact
    pub notification_methods: Vec<NotificationMethodRequest>,
}

#[derive(Serialize, ToSchema)]
pub struct CreateContactResponse {
    /// Success message
    pub message: String,
    /// Contact ID
    pub contact_id: i64,
}


#[derive(Serialize, ToSchema)]
pub struct ProvidersResponse {
    /// Available notification providers
    pub providers: Vec<ProviderInfo>,
}

pub type AppState = Arc<Mutex<WalletManager>>;
pub type NotificationManagerState = Arc<Mutex<NotificationManager>>;

/// Validates and normalizes a phone number
fn validate_phone_number(phone: &str) -> Result<String, String> {
    // Check if phone number starts with country code
    if !phone.starts_with('+') {
        return Err("Phone number must include country code (e.g., +1 for US, +44 for UK, +47 for Norway)".to_string());
    }

    // Parse phone number using the phonenumber crate
    let parsed_number = PhoneNumber::from_str(phone)
        .map_err(|_| "Invalid phone number format".to_string())?;

    // Check if it's a valid number
    if !parsed_number.is_valid() {
        return Err("Invalid phone number".to_string());
    }

    // Return normalized E.164 format
    Ok(parsed_number.format().mode(phonenumber::Mode::E164).to_string())
}

/// Generates an ntfy topic from contact name, language, and wallet descriptor
fn generate_ntfy_topic(name: &str, language: &Language, descriptor: &str) -> String {
    // Extract checksum from descriptor
    let checksum = descriptor
        .rfind('#')
        .map(|i| &descriptor[i + 1..])
        .unwrap_or("unknown");
    
    // Sanitize name for topic
    let sanitized_name = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    
    // Combine into topic (max 64 chars)
    let topic = format!("{}-{}-{}", sanitized_name, language.as_str(), checksum);
    if topic.len() > 64 {
        topic[..64].to_string()
    } else {
        topic
    }
}

#[utoipa::path(
    post,
    path = "/api/wallets",
    request_body = CreateWalletRequest,
    responses(
        (status = 201, description = "Wallet created successfully", body = CreateWalletResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 409, description = "Descriptor already exists", body = ErrorResponse),
    ),
    tag = "wallet",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn create_wallet(
    State(wallet_manager): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateWalletRequest>,
) -> Response {
    // Authenticate user
    let user = match authenticate_user(headers.get("authorization").and_then(|h| h.to_str().ok())) {
        Ok(user) => user,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Authentication required".to_string(),
                }),
            )
                .into_response();
        }
    };
    
    match wallet_manager
        .lock()
        .await
        .create_from_multipath(&payload.name, &payload.descriptor, user.user_id)
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
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden - wallet belongs to another user", body = ErrorResponse),
        (status = 404, description = "Wallet not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    ),
    tag = "wallet",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn delete_wallet(
    State(wallet_manager): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Response {
    // Authenticate user
    let user = match authenticate_user(headers.get("authorization").and_then(|h| h.to_str().ok())) {
        Ok(user) => user,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Authentication required".to_string(),
                }),
            )
                .into_response();
        }
    };

    #[allow(unused_mut)]
    let mut manager = wallet_manager.lock().await;
    
    // Check if wallet exists and belongs to user (or user is admin)
    match manager.metadata_db.get_wallet_by_id(id).await {
        Ok(Some(_)) => {
            // Check ownership
            if !user.is_admin {
                match manager.metadata_db.is_wallet_owned_by_user(id, user.user_id).await {
                    Ok(true) => {} // User owns the wallet
                    Ok(false) => {
                        return (
                            StatusCode::FORBIDDEN,
                            Json(ErrorResponse {
                                error: "Access denied".to_string(),
                            }),
                        )
                            .into_response();
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Database error: {}", e),
                            }),
                        )
                            .into_response();
                    }
                }
            }
        }
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
                    error: format!("Database error: {}", e),
                }),
            )
                .into_response();
        }
    }
    
    match manager.delete_wallet_by_id(id).await {
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
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden - wallet belongs to another user", body = ErrorResponse),
        (status = 404, description = "Wallet not found", body = ErrorResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
    ),
    tag = "wallet",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn update_wallet(
    State(wallet_manager): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateWalletRequest>,
) -> Response {
    // Authenticate user
    let user = match authenticate_user(headers.get("authorization").and_then(|h| h.to_str().ok())) {
        Ok(user) => user,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Authentication required".to_string(),
                }),
            )
                .into_response();
        }
    };
    
    if payload.name.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Wallet name cannot be empty".to_string(),
            }),
        )
            .into_response();
    }

    #[allow(unused_mut)]
    let mut manager = wallet_manager.lock().await;
    
    // Check if wallet exists and belongs to user (or user is admin)
    match manager.metadata_db.get_wallet_by_id(id).await {
        Ok(Some(_)) => {
            // Check ownership
            if !user.is_admin {
                match manager.metadata_db.is_wallet_owned_by_user(id, user.user_id).await {
                    Ok(true) => {} // User owns the wallet
                    Ok(false) => {
                        return (
                            StatusCode::FORBIDDEN,
                            Json(ErrorResponse {
                                error: "Access denied".to_string(),
                            }),
                        )
                            .into_response();
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Database error: {}", e),
                            }),
                        )
                            .into_response();
                    }
                }
            }
        }
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
                    error: format!("Database error: {}", e),
                }),
            )
                .into_response();
        }
    }

    match manager.update_wallet(id, &payload.name).await {
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
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden - wallet belongs to another user", body = ErrorResponse),
        (status = 404, description = "Wallet not found", body = ErrorResponse),
    ),
    tag = "wallet",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn get_wallet(
    State(wallet_manager): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>
) -> Response {
    // Authenticate user
    let user = match authenticate_user(headers.get("authorization").and_then(|h| h.to_str().ok())) {
        Ok(user) => user,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Authentication required".to_string(),
                }),
            )
                .into_response();
        }
    };
    
    #[allow(unused_mut)]
    let mut manager = wallet_manager.lock().await;
    match manager.get_wallet_by_id(id).await {
        Ok(Some(wallet)) => {
            // Check if user has access to this wallet
            if !user.is_admin {
                match manager.metadata_db.is_wallet_owned_by_user(id, user.user_id).await {
                    Ok(true) => {} // User owns the wallet
                    Ok(false) => {
                        return (
                            StatusCode::FORBIDDEN,
                            Json(ErrorResponse {
                                error: "Access denied".to_string(),
                            }),
                        )
                            .into_response();
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Database error: {}", e),
                            }),
                        )
                            .into_response();
                    }
                }
            }
            (StatusCode::OK, Json(wallet)).into_response()
        }
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
    request_body = CreateContactWithMethodsRequest,
    params(
        ("id" = i64, Path, description = "The wallet ID")
    ),
    responses(
        (status = 201, description = "Contact created successfully", body = CreateContactResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden - wallet belongs to another user", body = ErrorResponse),
        (status = 400, description = "Invalid request or phone number", body = ErrorResponse),
        (status = 404, description = "Wallet not found", body = ErrorResponse),
    ),
    tag = "contact",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn create_wallet_contact(
    State(wallet_manager): State<AppState>,
    headers: HeaderMap,
    Path(wallet_id): Path<i64>,
    Json(payload): Json<CreateContactWithMethodsRequest>,
) -> Response {
    // Authenticate user
    let user = match authenticate_user(headers.get("authorization").and_then(|h| h.to_str().ok())) {
        Ok(user) => user,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Authentication required".to_string(),
                }),
            )
                .into_response();
        }
    };
    
    #[allow(unused_mut)]
    let mut manager = wallet_manager.lock().await;

    // Check if wallet exists and user has access
    let wallet = match manager.get_wallet_by_id(wallet_id).await {
        Ok(Some(wallet)) => {
            if !user.is_admin {
                match manager.metadata_db.is_wallet_owned_by_user(wallet_id, user.user_id).await {
                    Ok(true) => {} // User owns the wallet
                    Ok(false) => {
                        return (
                            StatusCode::FORBIDDEN,
                            Json(ErrorResponse {
                                error: "Access denied".to_string(),
                            }),
                        )
                            .into_response();
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Database error: {}", e),
                            }),
                        )
                            .into_response();
                    }
                }
            }
            wallet
        }
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
    };

    // Process notification methods
    let mut processed_methods = Vec::new();
    
    for method in &payload.notification_methods {
        match method.provider_type {
            ProviderType::Sms => {
                // Validate phone number
                match validate_phone_number(&method.notification_target) {
                    Ok(normalized_phone) => {
                        processed_methods.push((ProviderType::Sms, normalized_phone));
                    }
                    Err(error) => {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse { error }),
                        )
                            .into_response();
                    }
                }
            }
            ProviderType::Ntfy => {
                // Auto-generate ntfy topic
                let topic = generate_ntfy_topic(&payload.name, &payload.language, &wallet.descriptor);
                processed_methods.push((ProviderType::Ntfy, topic));
            }
        }
    }

    // Ensure at least one method was provided
    if processed_methods.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "At least one notification method must be provided".to_string(),
            }),
        )
            .into_response();
    }

    match manager.metadata_db.insert_contact_with_notification_methods(
        wallet_id, 
        &payload.name, 
        &payload.language,
        processed_methods
    ).await {
        Ok(contact_id) => {
            
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
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden - wallet belongs to another user", body = ErrorResponse),
        (status = 404, description = "Contact not found", body = ErrorResponse),
    ),
    tag = "contact",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn delete_wallet_contact(
    State(wallet_manager): State<AppState>,
    headers: HeaderMap,
    Path((wallet_id, contact_id)): Path<(i64, i64)>,
) -> Response {
    // Authenticate user
    let user = match authenticate_user(headers.get("authorization").and_then(|h| h.to_str().ok())) {
        Ok(user) => user,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Authentication required".to_string(),
                }),
            )
                .into_response();
        }
    };
    
    #[allow(unused_mut)]
    let mut manager = wallet_manager.lock().await;
    
    // Check if wallet exists and user has access
    match manager.metadata_db.get_wallet_by_id(wallet_id).await {
        Ok(Some(_)) => {
            if !user.is_admin {
                match manager.metadata_db.is_wallet_owned_by_user(wallet_id, user.user_id).await {
                    Ok(true) => {} // User owns the wallet
                    Ok(false) => {
                        return (
                            StatusCode::FORBIDDEN,
                            Json(ErrorResponse {
                                error: "Access denied".to_string(),
                            }),
                        )
                            .into_response();
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Database error: {}", e),
                            }),
                        )
                            .into_response();
                    }
                }
            }
        }
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
                    error: format!("Database error: {}", e),
                }),
            )
                .into_response();
        }
    }
    
    match manager.metadata_db.delete_contact_with_methods(contact_id).await {
        Ok(true) => {
            
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
        (status = 200, description = "List of contacts for wallet", body = Vec<Contact>),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden - wallet belongs to another user", body = ErrorResponse),
        (status = 404, description = "Wallet not found", body = ErrorResponse),
    ),
    tag = "wallet",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn get_wallet_contacts(
    State(wallet_manager): State<AppState>,
    headers: HeaderMap,
    Path(wallet_id): Path<i64>,
) -> Response {
    // Authenticate user
    let user = match authenticate_user(headers.get("authorization").and_then(|h| h.to_str().ok())) {
        Ok(user) => user,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Authentication required".to_string(),
                }),
            )
                .into_response();
        }
    };
    
    #[allow(unused_mut)]
    let mut manager = wallet_manager.lock().await;

    // Check if wallet exists and user has access
    match manager.get_wallet_by_id(wallet_id).await {
        Ok(Some(_)) => {
            if !user.is_admin {
                match manager.metadata_db.is_wallet_owned_by_user(wallet_id, user.user_id).await {
                    Ok(true) => {} // User owns the wallet
                    Ok(false) => {
                        return (
                            StatusCode::FORBIDDEN,
                            Json(ErrorResponse {
                                error: "Access denied".to_string(),
                            }),
                        )
                            .into_response();
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Database error: {}", e),
                            }),
                        )
                            .into_response();
                    }
                }
            }
        }
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

    match manager.metadata_db.get_contacts_with_notification_methods(wallet_id).await {
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
    get,
    path = "/api/block-headers/current",
    responses(
        (status = 200, description = "Current block header from database", body = BlockHeader),
        (status = 404, description = "No block header found", body = ErrorResponse),
    ),
    tag = "blockchain"
)]
pub async fn get_current_block_header(State(wallet_manager): State<AppState>) -> Response {
    #[allow(unused_mut)]
    let mut manager = wallet_manager.lock().await;
    match manager.metadata_db.get_current_block_header().await {
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
    path = "/api/wallets",
    responses(
        (status = 200, description = "List of all wallets", body = WalletsListResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "wallet",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn get_wallets_list(
    State(wallet_manager): State<AppState>,
    headers: HeaderMap,
) -> Response {
    // Authenticate user
    let user = match authenticate_user(headers.get("authorization").and_then(|h| h.to_str().ok())) {
        Ok(user) => user,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Authentication required".to_string(),
                }),
            )
                .into_response();
        }
    };
    
    #[allow(unused_mut)]
    let mut manager = wallet_manager.lock().await;
    match manager.get_wallets_list_for_user(user.user_id, user.is_admin).await {
        Ok(wallets_response) => (StatusCode::OK, Json(wallets_response)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to get wallets list: {}", e),
            }),
        )
            .into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/wallets/{id}/detail",
    responses(
        (status = 200, description = "Wallet detail with transaction events", body = WalletDetailResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Access denied", body = ErrorResponse),
        (status = 404, description = "Wallet not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    params(
        ("id" = i64, Path, description = "Wallet ID")
    ),
    tag = "wallet",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn get_wallet_detail(
    State(wallet_manager): State<AppState>,
    Path(wallet_id): Path<i64>,
    headers: HeaderMap,
) -> Response {
    // Authenticate user
    let user = match authenticate_user(headers.get("authorization").and_then(|h| h.to_str().ok())) {
        Ok(user) => user,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Authentication required".to_string(),
                }),
            )
                .into_response();
        }
    };
    
    #[allow(unused_mut)]
    let mut manager = wallet_manager.lock().await;
    match manager.get_wallet_detail_for_user(wallet_id, user.user_id, user.is_admin).await {
        Ok(wallet_detail) => (StatusCode::OK, Json(wallet_detail)).into_response(),
        Err(e) => {
            let error_msg = e.to_string();
            let status_code = match error_msg.as_str() {
                "Wallet not found" => StatusCode::NOT_FOUND,
                "Access denied to wallet" => StatusCode::FORBIDDEN,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            
            (status_code, Json(ErrorResponse { error: error_msg })).into_response()
        }
    }
}

// Auth endpoints
#[utoipa::path(
    post,
    path = "/api/auth/send-otp",
    request_body = SendOtpRequest,
    responses(
        (status = 200, description = "OTP sent successfully", body = serde_json::Value),
        (status = 400, description = "Invalid phone number or bad request", body = ErrorResponse),
        (status = 429, description = "Rate limit exceeded", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "auth"
)]
pub async fn send_otp(
    State(wallet_manager): State<AppState>,
    Json(request): Json<SendOtpRequest>,
) -> Response {
    #[allow(unused_mut)]
    let mut manager = wallet_manager.lock().await;
    
    // Check if this is a dev test phone
    let is_dev_phone = cfg!(debug_assertions) && 
        (request.phone_number == crate::auth::DEV_ADMIN_PHONE || 
         ["+4799999901", "+4699999902", "+3399999903"].contains(&request.phone_number.as_str()));
    
    // Check rate limit (skip for dev phones)
    if !is_dev_phone {
        match manager.metadata_db.check_rate_limit(&request.phone_number).await {
            Ok(true) => {} // Allowed
            Ok(false) => {
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(ErrorResponse {
                        error: "Too many OTP attempts. Please try again later.".to_string(),
                    }),
                )
                    .into_response();
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Failed to check rate limit: {}", e),
                    }),
                )
                    .into_response();
            }
        }
    }
    
    // Check if Twilio is enabled (skip for dev phones in dev mode)
    let twilio_enabled = std::env::var("CANARY_ENABLE_TWILIO")
        .map(|v| v.to_lowercase() == "true" || v == "1")
        .unwrap_or(false);
    
    if !twilio_enabled && !is_dev_phone {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Twilio SMS is not enabled".to_string(),
            }),
        )
            .into_response();
    }
    
    // Load Twilio config from environment (create dummy config for dev phones)
    let twilio_config = if is_dev_phone {
        // Create a dummy config for dev phones
        crate::metadata::TwilioConfig {
            id: None,
            account_sid: "dummy".to_string(),
            auth_token: "dummy".to_string(),
            messaging_service_sid: "dummy".to_string(),
            verify_service_sid: Some("dummy".to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    } else {
        match crate::auth::load_twilio_config_from_env() {
            Ok(config) => config,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Twilio configuration error: {}", e),
                    }),
                )
                    .into_response();
            }
        }
    };
    
    // Check that Verify service is configured for auth (skip for dev phones)
    if !is_dev_phone && twilio_config.verify_service_sid.is_none() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "TWILIO_VERIFY_SERVICE_SID must be set for SMS authentication".to_string(),
            }),
        )
            .into_response();
    }
    
    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "your-secret-key-change-in-production".to_string());
    let auth_service = AuthService::new(jwt_secret);
    
    match auth_service.send_otp(&twilio_config, &request.phone_number).await {
        Ok(_) => {
            Json(serde_json::json!({
                "message": "OTP sent successfully"
            }))
            .into_response()
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("Failed to send OTP: {}", e),
            }),
        )
            .into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/auth/verify-otp",
    request_body = VerifyOtpRequest,
    responses(
        (status = 200, description = "OTP verified successfully", body = AuthResponse),
        (status = 400, description = "Invalid OTP or bad request", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "auth"
)]
pub async fn verify_otp(
    State(wallet_manager): State<AppState>,
    Json(request): Json<VerifyOtpRequest>,
) -> Response {
    #[allow(unused_mut)]
    let mut manager = wallet_manager.lock().await;
    
    // Check if this is a dev test phone
    let is_dev_phone = cfg!(debug_assertions) && 
        (request.phone_number == crate::auth::DEV_ADMIN_PHONE || 
         ["+4799999901", "+4699999902", "+3399999903"].contains(&request.phone_number.as_str()));
    
    // Check if Twilio is enabled (skip for dev phones in dev mode)
    let twilio_enabled = std::env::var("CANARY_ENABLE_TWILIO")
        .map(|v| v.to_lowercase() == "true" || v == "1")
        .unwrap_or(false);
    
    if !twilio_enabled && !is_dev_phone {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Twilio SMS is not enabled".to_string(),
            }),
        )
            .into_response();
    }
    
    // Load Twilio config from environment (create dummy config for dev phones)
    let twilio_config = if is_dev_phone {
        // Create a dummy config for dev phones
        crate::metadata::TwilioConfig {
            id: None,
            account_sid: "dummy".to_string(),
            auth_token: "dummy".to_string(),
            messaging_service_sid: "dummy".to_string(),
            verify_service_sid: Some("dummy".to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    } else {
        match crate::auth::load_twilio_config_from_env() {
            Ok(config) => config,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Twilio configuration error: {}", e),
                    }),
                )
                    .into_response();
            }
        }
    };
    
    // Check that Verify service is configured for auth (skip for dev phones)
    if !is_dev_phone && twilio_config.verify_service_sid.is_none() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "TWILIO_VERIFY_SERVICE_SID must be set for SMS authentication".to_string(),
            }),
        )
            .into_response();
    }
    
    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "your-secret-key-change-in-production".to_string());
    let auth_service = AuthService::new(jwt_secret.clone());
    
    // Verify OTP
    match auth_service.verify_otp(&twilio_config, &request.phone_number, &request.code).await {
        Ok(true) => {
            // Clear rate limit on successful verification
            let _ = manager.metadata_db.clear_rate_limit(&request.phone_number).await;
            
            // Check if user exists first
            let existing_user = match manager.metadata_db.get_user_by_phone(&request.phone_number).await {
                Ok(user) => user,
                Err(e) => {
                    eprintln!("Error checking user existence: {:?}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: format!("Failed to check user: {}", e),
                        }),
                    )
                        .into_response();
                }
            };
            
            // If user doesn't exist and no name provided, return a special response
            if existing_user.is_none() && request.name.is_none() {
                return Json(serde_json::json!({
                    "requires_name": true,
                    "message": "New user registration requires a name"
                })).into_response();
            }
            
            eprintln!("User exists: {:?}, Name provided: {:?}", existing_user.is_some(), request.name);
            
            // Create or get user
            let user_id = match manager.metadata_db.create_user(&request.phone_number, request.name.as_deref()).await {
                Ok(id) => id,
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: format!("Failed to create user: {}", e),
                        }),
                    )
                        .into_response();
                }
            };
            
            // Update last login
            if let Err(e) = manager.metadata_db.update_last_login(user_id).await {
                eprintln!("Failed to update last login for user {}: {:?}", user_id, e);
            }
            
            // Check if user is admin
            let admin_phone = std::env::var("ADMIN_PHONE_NUMBER").ok();
            let is_admin = admin_phone.map_or(false, |phone| phone == request.phone_number) 
                || (cfg!(debug_assertions) && request.phone_number == crate::auth::DEV_ADMIN_PHONE);
            
            // Generate JWT token
            let token = match auth_service.generate_token(user_id, &request.phone_number, is_admin) {
                Ok(t) => t,
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: format!("Failed to generate token: {}", e),
                        }),
                    )
                        .into_response();
                }
            };
            
            // Create session
            let token_hash = AuthService::hash_token(&token);
            let expires_at = chrono::Utc::now() + chrono::Duration::days(7);
            
            if let Err(e) = manager.metadata_db.create_session(user_id, &token_hash, expires_at).await {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Failed to create session: {}", e),
                    }),
                )
                    .into_response();
            }
            
            // Get user info
            eprintln!("Getting user info for user_id: {}", user_id);
            let user_info = match manager.metadata_db.get_user_by_id(user_id).await {
                Ok(Some(db_user)) => {
                    eprintln!("Found user in DB: {:?}", db_user.name);
                    AuthUserResponse {
                        id: db_user.id,
                        phone_number: db_user.phone_number,
                        name: db_user.name,
                        created_at: db_user.created_at,
                    }
                },
                Ok(None) => {
                    eprintln!("User not found in DB, creating response from request");
                    AuthUserResponse {
                        id: user_id,
                        phone_number: request.phone_number.clone(),
                        name: request.name.clone(),
                        created_at: chrono::Utc::now().to_rfc3339(),
                    }
                },
                Err(e) => {
                    eprintln!("Error getting user by ID: {:?}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: format!("Failed to get user info: {}", e),
                        }),
                    )
                        .into_response();
                }
            };
            
            eprintln!("Sending successful response for user: {:?}", user_info.name);
            Json(AuthResponse {
                token,
                user: user_info,
            })
            .into_response()
        }
        Ok(false) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid OTP code".to_string(),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("Failed to verify OTP: {}", e),
            }),
        )
            .into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/auth/logout",
    responses(
        (status = 200, description = "Logged out successfully", body = serde_json::Value),
        (status = 401, description = "Unauthorized", body = ErrorResponse)
    ),
    tag = "auth",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn logout(
    State(wallet_manager): State<AppState>,
    headers: HeaderMap,
) -> Response {
    // Get the token from the Authorization header
    let auth_header = match headers.get("authorization").and_then(|h| h.to_str().ok()) {
        Some(header) => header,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Authentication required".to_string(),
                }),
            )
                .into_response();
        }
    };

    if !auth_header.starts_with("Bearer ") {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Invalid authorization header".to_string(),
            }),
        )
            .into_response();
    }

    let token = &auth_header[7..]; // Skip "Bearer "
    
    // Hash the token to find it in the database
    let token_hash = AuthService::hash_token(token);
    
    #[allow(unused_mut)]
    let mut manager = wallet_manager.lock().await;
    
    // Delete the session from the database
    if let Err(e) = manager.metadata_db.delete_session(&token_hash).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to delete session: {}", e),
            }),
        )
            .into_response();
    }
    
    Json(serde_json::json!({
        "message": "Logged out successfully"
    }))
    .into_response()
}

#[utoipa::path(
    get,
    path = "/api/auth/me",
    responses(
        (status = 200, description = "Current user info", body = AuthUserResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse)
    ),
    tag = "auth",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn me(
    State(wallet_manager): State<AppState>,
    headers: HeaderMap,
) -> Response {
    // Authenticate user
    let user = match authenticate_user(headers.get("authorization").and_then(|h| h.to_str().ok())) {
        Ok(user) => user,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Authentication required".to_string(),
                }),
            )
                .into_response();
        }
    };
    
    // Get user info from database
    #[allow(unused_mut)]
    let mut manager = wallet_manager.lock().await;
    let user_info = match manager.metadata_db.get_user_by_id(user.user_id).await {
        Ok(Some(db_user)) => AuthUserResponse {
            id: db_user.id,
            phone_number: db_user.phone_number,
            name: db_user.name,
            created_at: db_user.created_at,
        },
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "User not found".to_string(),
                }),
            )
                .into_response();
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to get user info".to_string(),
                }),
            )
                .into_response();
        }
    };
    
    Json(serde_json::json!({ "user": user_info })).into_response()
}

#[utoipa::path(
    get,
    path = "/api/providers",
    responses(
        (status = 200, description = "Available notification providers", body = ProvidersResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    ),
    tag = "providers"
)]
pub async fn get_providers(State(notification_manager): State<NotificationManagerState>) -> Response {
    #[allow(unused_mut)]
    let mut manager = notification_manager.lock().await;
    let providers = manager.list_providers();
    (StatusCode::OK, Json(ProvidersResponse { providers })).into_response()
}

#[derive(OpenApi)]
#[openapi(
    paths(
        create_wallet, update_wallet, delete_wallet, get_wallet,
        get_wallets_list, get_wallet_detail,
        create_wallet_contact, delete_wallet_contact, get_wallet_contacts,
        get_current_block_header,
        get_providers,
        send_otp, verify_otp, logout, me
    ),
    components(schemas(
        CreateWalletRequest, UpdateWalletRequest, CreateWalletResponse, ErrorResponse, WalletMetadata,
        CreateContactWithMethodsRequest, NotificationMethodRequest, CreateContactResponse, ProvidersResponse,
        Contact, NotificationMethod, ProviderType, TransactionEventWithWallet, EventType, Language,
        BlockHeader, WalletsListResponse, WalletDetailResponse, ProviderInfo,
        SendOtpRequest, VerifyOtpRequest, AuthResponse, AuthUserResponse
    )),
    tags(
        (name = "wallet", description = "Wallet management endpoints"),
        (name = "contact", description = "Contact management endpoints"),
        (name = "providers", description = "Notification provider endpoints"),
        (name = "transaction", description = "Transaction events endpoints"),
        (name = "blockchain", description = "Blockchain information endpoints"),
        (name = "auth", description = "Authentication endpoints")
    ),
    info(
        title = "Canary Wallet API",
        version = "0.2.2",
        description = "REST API for creating Bitcoin wallets from multipath descriptors",
    )
)]
pub struct ApiDoc;

pub fn create_router(wallet_manager: AppState, notification_manager: NotificationManagerState) -> Router {
    // Auth routes (public)
    let auth_routes = Router::new()
        .route("/auth/send-otp", post(send_otp))
        .route("/auth/verify-otp", post(verify_otp))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .with_state(wallet_manager.clone());
    
    let wallet_routes = Router::new()
        .route("/wallets", post(create_wallet).get(get_wallets_list))
        .route("/wallets/{id}", get(get_wallet).put(update_wallet).delete(delete_wallet))
        .route("/wallets/{id}/detail", get(get_wallet_detail))
        .route(
            "/wallets/{id}/contacts",
            post(create_wallet_contact).get(get_wallet_contacts),
        )
        .route(
            "/wallets/{wallet_id}/contacts/{contact_id}",
            axum::routing::delete(delete_wallet_contact),
        )
        .route("/block-headers/current", get(get_current_block_header))
        .with_state(wallet_manager.clone());

    let provider_routes = Router::new()
        .route("/providers", get(get_providers))
        .with_state(notification_manager);

    Router::new()
        .nest("/api", auth_routes.merge(wallet_routes).merge(provider_routes))
        .layer(CorsLayer::permissive())
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
}
