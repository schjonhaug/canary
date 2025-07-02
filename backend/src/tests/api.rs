use crate::api::{
    AddContactToWalletRequest, CreateContactRequest, CreateWalletRequest, TwilioConfigRequest,
    create_router,
};
use crate::wallet::WalletManager;
use bdk_wallet::bitcoin::Network;
use axum::{
    body::Body,
    http::{self, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::Value;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast};
use tower::ServiceExt;
use uuid::Uuid;

async fn setup_test_app() -> (axum::Router, PathBuf) {
    // Use a unique temp directory for each test run
    let base_temp = env::temp_dir();
    let unique_dir = base_temp.join(format!("txray_test_{}", Uuid::new_v4()));
    fs::create_dir_all(&unique_dir).unwrap();
    let wallet_dir = unique_dir.join("wallets");
    fs::create_dir_all(&wallet_dir).unwrap();

    let (event_tx, _) = broadcast::channel(100);
    let metadata_db_path = unique_dir.join("txray.sqlite");
    let wallet_manager = WalletManager::new(
        event_tx,
        wallet_dir.clone(),
        metadata_db_path.to_str().unwrap(),
        Network::Regtest,
        "tcp://127.0.0.1:50001"
    )
    .await;

    let app = create_router(Arc::new(Mutex::new(wallet_manager)));
    (app, wallet_dir)
}

// ===== WALLET MANAGEMENT TESTS =====

#[tokio::test]
async fn test_create_wallet_success() {
    let (app, _temp_dir) = setup_test_app().await;

    let wallet_request = CreateWalletRequest {
        name: "Test Wallet".to_string(),
        descriptor: "wpkh(tpubD6NzVbkrYhZ4XgiXtGrdW5XDAPFCL9h7we1vwNCpn8tGbBcgfVYjXyhWo4E1xkh56hjod1RhGjxbaTLV3X4FyWuejifB9jusQ46QzG87VKp/<0;1>/*)".to_string(),
    };

    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/wallets")
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(serde_json::to_string(&wallet_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(body["message"], "Wallet created successfully");
    assert_eq!(body["wallet"]["name"], "Test Wallet");
}

#[tokio::test]
async fn test_create_wallet_duplicate_descriptor() {
    let (app, _temp_dir) = setup_test_app().await;

    let wallet_request = CreateWalletRequest {
        name: "Test Wallet 1".to_string(),
        descriptor: "wpkh(tpubD6NzVbkrYhZ4XgiXtGrdW5XDAPFCL9h7we1vwNCpn8tGbBcgfVYjXyhWo4E1xkh56hjod1RhGjxbaTLV3X4FyWuejifB9jusQ46QzG87VKp/<0;1>/*)".to_string(),
    };

    // Create first wallet
    let response1 = app
        .clone()
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/wallets")
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(serde_json::to_string(&wallet_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response1.status(), StatusCode::CREATED);

    // Try to create second wallet with same descriptor
    let response2 = app
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/wallets")
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(serde_json::to_string(&wallet_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response2.status(), StatusCode::CONFLICT);

    let body = response2.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["error"], "Descriptor already exists");
}

#[tokio::test]
async fn test_create_wallet_invalid_descriptor() {
    let (app, _temp_dir) = setup_test_app().await;

    let wallet_request = CreateWalletRequest {
        name: "Test Wallet".to_string(),
        descriptor: "invalid_descriptor".to_string(),
    };

    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/wallets")
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(serde_json::to_string(&wallet_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("Invalid descriptor")
    );
}

#[tokio::test]
async fn test_get_all_wallets_empty() {
    let (app, _temp_dir) = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::GET)
                .uri("/wallets")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();

    assert!(body.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_get_all_wallets_with_data() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create a wallet first
    let wallet_request = CreateWalletRequest {
        name: "Test Wallet".to_string(),
        descriptor: "wpkh(tpubD6NzVbkrYhZ4XgiXtGrdW5XDAPFCL9h7we1vwNCpn8tGbBcgfVYjXyhWo4E1xkh56hjod1RhGjxbaTLV3X4FyWuejifB9jusQ46QzG87VKp/<0;1>/*)".to_string(),
    };

    app.clone()
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/wallets")
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(serde_json::to_string(&wallet_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Get all wallets
    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::GET)
                .uri("/wallets")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();

    let wallets = body.as_array().unwrap();
    assert_eq!(wallets.len(), 1);
    assert_eq!(wallets[0]["name"], "Test Wallet");
}

#[tokio::test]
async fn test_get_wallet_by_id() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create a wallet first
    let wallet_request = CreateWalletRequest {
        name: "Test Wallet".to_string(),
        descriptor: "wpkh(tpubD6NzVbkrYhZ4XgiXtGrdW5XDAPFCL9h7we1vwNCpn8tGbBcgfVYjXyhWo4E1xkh56hjod1RhGjxbaTLV3X4FyWuejifB9jusQ46QzG87VKp/<0;1>/*)".to_string(),
    };

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/wallets")
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(serde_json::to_string(&wallet_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let create_body = create_response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let create_body: Value = serde_json::from_slice(&create_body).unwrap();
    let wallet_id = create_body["wallet"]["id"].as_i64().unwrap();

    // Get wallet by ID
    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::GET)
                .uri(&format!("/wallets/{}", wallet_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(body["id"], wallet_id);
    assert_eq!(body["name"], "Test Wallet");
}

#[tokio::test]
async fn test_get_wallet_by_id_not_found() {
    let (app, _temp_dir) = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::GET)
                .uri("/wallets/999")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["error"], "Wallet not found");
}

#[tokio::test]
async fn test_delete_wallet() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create a wallet first
    let wallet_request = CreateWalletRequest {
        name: "Test Wallet".to_string(),
        descriptor: "wpkh(tpubD6NzVbkrYhZ4XgiXtGrdW5XDAPFCL9h7we1vwNCpn8tGbBcgfVYjXyhWo4E1xkh56hjod1RhGjxbaTLV3X4FyWuejifB9jusQ46QzG87VKp/<0;1>/*)".to_string(),
    };

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/wallets")
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(serde_json::to_string(&wallet_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let create_body = create_response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let create_body: Value = serde_json::from_slice(&create_body).unwrap();
    let wallet_id = create_body["wallet"]["id"].as_i64().unwrap();

    // Delete wallet
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(http::Method::DELETE)
                .uri(&format!("/wallets/{}", wallet_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Verify wallet is deleted
    let get_response = app
        .oneshot(
            Request::builder()
                .method(http::Method::GET)
                .uri(&format!("/wallets/{}", wallet_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(get_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_delete_wallet_not_found() {
    let (app, _temp_dir) = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::DELETE)
                .uri("/wallets/999")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["error"], "Wallet not found");
}

// ===== CONTACT MANAGEMENT TESTS =====

#[tokio::test]
async fn test_create_contact() {
    let (app, _temp_dir) = setup_test_app().await;

    let contact_request = CreateContactRequest {
        name: "John Doe".to_string(),
        phone_number: "12345678".to_string(),
    };

    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/contacts")
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(serde_json::to_string(&contact_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();

    if status != StatusCode::CREATED {
        println!("Error response: {:?}", body);
    }

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["message"], "Contact created successfully");
    assert!(body["contact_id"].as_i64().is_some());
}

#[tokio::test]
async fn test_get_all_contacts_empty() {
    let (app, _temp_dir) = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::GET)
                .uri("/contacts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();

    assert!(body.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_get_all_contacts_with_data() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create a contact first
    let contact_request = CreateContactRequest {
        name: "John Doe".to_string(),
        phone_number: "12345678".to_string(),
    };

    app.clone()
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/contacts")
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(serde_json::to_string(&contact_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Get all contacts
    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::GET)
                .uri("/contacts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();

    let contacts = body.as_array().unwrap();
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0]["name"], "John Doe");
    assert_eq!(contacts[0]["phone_number"], "12345678");
}

#[tokio::test]
async fn test_delete_contact() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create a contact first
    let contact_request = CreateContactRequest {
        name: "John Doe".to_string(),
        phone_number: "12345678".to_string(),
    };

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/contacts")
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(serde_json::to_string(&contact_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let create_body = create_response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let create_body: Value = serde_json::from_slice(&create_body).unwrap();
    let contact_id = create_body["contact_id"].as_i64().unwrap();

    // Delete contact
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(http::Method::DELETE)
                .uri(&format!("/contacts/{}", contact_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Verify contact is deleted
    let get_response = app
        .oneshot(
            Request::builder()
                .method(http::Method::GET)
                .uri("/contacts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let get_body = get_response.into_body().collect().await.unwrap().to_bytes();
    let get_body: Value = serde_json::from_slice(&get_body).unwrap();
    assert!(get_body.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_delete_contact_not_found() {
    let (app, _temp_dir) = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::DELETE)
                .uri("/contacts/999")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["error"], "Contact not found");
}

// ===== WALLET-CONTACT RELATIONSHIP TESTS =====

#[tokio::test]
async fn test_add_contact_to_wallet() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create a wallet first
    let wallet_request = CreateWalletRequest {
        name: "Test Wallet".to_string(),
        descriptor: "wpkh(tpubD6NzVbkrYhZ4XgiXtGrdW5XDAPFCL9h7we1vwNCpn8tGbBcgfVYjXyhWo4E1xkh56hjod1RhGjxbaTLV3X4FyWuejifB9jusQ46QzG87VKp/<0;1>/*)".to_string(),
    };

    let wallet_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/wallets")
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(serde_json::to_string(&wallet_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let wallet_body = wallet_response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let wallet_body: Value = serde_json::from_slice(&wallet_body).unwrap();
    let wallet_id = wallet_body["wallet"]["id"].as_i64().unwrap();

    // Create a contact
    let contact_request = CreateContactRequest {
        name: "John Doe".to_string(),
        phone_number: "12345678".to_string(),
    };

    let contact_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/contacts")
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(serde_json::to_string(&contact_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let contact_body = contact_response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let contact_body: Value = serde_json::from_slice(&contact_body).unwrap();
    let contact_id = contact_body["contact_id"].as_i64().unwrap();

    // Add contact to wallet
    let add_request = AddContactToWalletRequest { contact_id };

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri(&format!("/wallets/{}/contacts", wallet_id))
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(serde_json::to_string(&add_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn test_add_contact_to_wallet_not_found() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create a contact
    let contact_request = CreateContactRequest {
        name: "John Doe".to_string(),
        phone_number: "12345678".to_string(),
    };

    let contact_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/contacts")
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(serde_json::to_string(&contact_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let contact_body = contact_response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let contact_body: Value = serde_json::from_slice(&contact_body).unwrap();
    let contact_id = contact_body["contact_id"].as_i64().unwrap();

    // Try to add contact to non-existent wallet
    let add_request = AddContactToWalletRequest { contact_id };

    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/wallets/999/contacts")
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(serde_json::to_string(&add_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["error"], "Wallet not found");
}

#[tokio::test]
async fn test_get_wallet_contacts() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create a wallet first
    let wallet_request = CreateWalletRequest {
        name: "Test Wallet".to_string(),
        descriptor: "wpkh(tpubD6NzVbkrYhZ4XgiXtGrdW5XDAPFCL9h7we1vwNCpn8tGbBcgfVYjXyhWo4E1xkh56hjod1RhGjxbaTLV3X4FyWuejifB9jusQ46QzG87VKp/<0;1>/*)".to_string(),
    };

    let wallet_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/wallets")
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(serde_json::to_string(&wallet_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let wallet_body = wallet_response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let wallet_body: Value = serde_json::from_slice(&wallet_body).unwrap();
    let wallet_id = wallet_body["wallet"]["id"].as_i64().unwrap();

    // Create a contact
    let contact_request = CreateContactRequest {
        name: "John Doe".to_string(),
        phone_number: "12345678".to_string(),
    };

    let contact_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/contacts")
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(serde_json::to_string(&contact_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let contact_body = contact_response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let contact_body: Value = serde_json::from_slice(&contact_body).unwrap();
    let contact_id = contact_body["contact_id"].as_i64().unwrap();

    // Add contact to wallet
    let add_request = AddContactToWalletRequest { contact_id };

    app.clone()
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri(&format!("/wallets/{}/contacts", wallet_id))
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(serde_json::to_string(&add_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Get wallet contacts
    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::GET)
                .uri(&format!("/wallets/{}/contacts", wallet_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();

    let contacts = body.as_array().unwrap();
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0]["name"], "John Doe");
    assert_eq!(contacts[0]["phone_number"], "12345678");
}

#[tokio::test]
async fn test_get_wallet_contacts_wallet_not_found() {
    let (app, _temp_dir) = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::GET)
                .uri("/wallets/999/contacts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["error"], "Wallet not found");
}

#[tokio::test]
async fn test_remove_contact_from_wallet() {
    let (app, _temp_dir) = setup_test_app().await;

    // Create a wallet first
    let wallet_request = CreateWalletRequest {
        name: "Test Wallet".to_string(),
        descriptor: "wpkh(tpubD6NzVbkrYhZ4XgiXtGrdW5XDAPFCL9h7we1vwNCpn8tGbBcgfVYjXyhWo4E1xkh56hjod1RhGjxbaTLV3X4FyWuejifB9jusQ46QzG87VKp/<0;1>/*)".to_string(),
    };

    let wallet_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/wallets")
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(serde_json::to_string(&wallet_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let wallet_body = wallet_response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let wallet_body: Value = serde_json::from_slice(&wallet_body).unwrap();
    let wallet_id = wallet_body["wallet"]["id"].as_i64().unwrap();

    // Create a contact
    let contact_request = CreateContactRequest {
        name: "John Doe".to_string(),
        phone_number: "12345678".to_string(),
    };

    let contact_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/contacts")
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(serde_json::to_string(&contact_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let contact_body = contact_response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let contact_body: Value = serde_json::from_slice(&contact_body).unwrap();
    let contact_id = contact_body["contact_id"].as_i64().unwrap();

    // Add contact to wallet
    let add_request = AddContactToWalletRequest { contact_id };

    app.clone()
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri(&format!("/wallets/{}/contacts", wallet_id))
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(serde_json::to_string(&add_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Remove contact from wallet
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(http::Method::DELETE)
                .uri(&format!("/wallets/{}/contacts/{}", wallet_id, contact_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Verify contact is removed
    let get_response = app
        .oneshot(
            Request::builder()
                .method(http::Method::GET)
                .uri(&format!("/wallets/{}/contacts", wallet_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let get_body = get_response.into_body().collect().await.unwrap().to_bytes();
    let get_body: Value = serde_json::from_slice(&get_body).unwrap();
    assert!(get_body.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_remove_contact_from_wallet_not_found() {
    let (app, _temp_dir) = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::DELETE)
                .uri("/wallets/999/contacts/999")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["error"], "Contact not found in wallet");
}

// ===== TWILIO CONFIGURATION TESTS =====

#[tokio::test]
async fn test_save_twilio_config() {
    let (app, _temp_dir) = setup_test_app().await;

    let twilio_request = TwilioConfigRequest {
        account_sid: "ACxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string(),
        auth_token: "your_auth_token".to_string(),
        messaging_service_sid: "MGxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string(),
    };

    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/twilio/config")
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(serde_json::to_string(&twilio_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["message"], "Twilio configuration saved successfully");
}

#[tokio::test]
async fn test_get_twilio_config() {
    let (app, _temp_dir) = setup_test_app().await;

    // Save config first
    let twilio_request = TwilioConfigRequest {
        account_sid: "ACxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string(),
        auth_token: "your_auth_token".to_string(),
        messaging_service_sid: "MGxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string(),
    };

    app.clone()
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/twilio/config")
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(serde_json::to_string(&twilio_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Get config
    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::GET)
                .uri("/twilio/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(body["account_sid"], "ACxxxxxxxxxxxxxxxxxxxxxxxxxxxxx");
    assert_eq!(body["auth_token"], "your_auth_token");
    assert_eq!(
        body["messaging_service_sid"],
        "MGxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
    );
}

#[tokio::test]
async fn test_get_twilio_config_not_found() {
    let (app, _temp_dir) = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::GET)
                .uri("/twilio/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["error"], "No Twilio configuration found");
}

// ===== ERROR HANDLING TESTS =====

#[tokio::test]
async fn test_invalid_json_request() {
    let (app, _temp_dir) = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/wallets")
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from("{ invalid json }"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_missing_content_type() {
    let (app, _temp_dir) = setup_test_app().await;

    let wallet_request = CreateWalletRequest {
        name: "Test Wallet".to_string(),
        descriptor: "wpkh(tpubD6NzVbkrYhZ4XgiXtGrdW5XDAPFCL9h7we1vwNCpn8tGbBcgfVYjXyhWo4E1xkh56hjod1RhGjxbaTLV3X4FyWuejifB9jusQ46QzG87VKp/<0;1>/*)".to_string(),
    };

    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/wallets")
                .body(Body::from(serde_json::to_string(&wallet_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[tokio::test]
async fn test_swagger_ui_endpoint() {
    let (app, _temp_dir) = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::GET)
                .uri("/swagger-ui/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_openapi_spec_endpoint() {
    let (app, _temp_dir) = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::GET)
                .uri("/api-docs/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();

    // Verify it's a valid OpenAPI spec
    assert_eq!(body["info"]["title"], "TxRay Wallet API");
    assert_eq!(body["info"]["version"], "0.1.0");
}
