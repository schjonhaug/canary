use crate::config::{AppConfig, NetworkConfig, OperatingMode};

fn test_config() -> AppConfig {
    AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        "/tmp/test".to_string(),
        OperatingMode::SelfHosted,
        Some("http://localhost:3001".to_string()),
        None,
    )
}

#[test]
fn test_btcpay_not_enabled_by_default() {
    let config = test_config();
    assert!(!config.is_btcpay_enabled());
}

#[test]
fn test_btcpay_enabled_with_required_fields() {
    let config = test_config().with_btcpay(
        Some("http://localhost:14142".to_string()),
        Some("api-key-123".to_string()),
        Some("store-id-456".to_string()),
        None,
        None,
    );
    assert!(config.is_btcpay_enabled());
}

#[test]
fn test_btcpay_not_enabled_with_partial_config() {
    // Only URL set
    let config = test_config().with_btcpay(
        Some("http://localhost:14142".to_string()),
        None,
        None,
        None,
        None,
    );
    assert!(!config.is_btcpay_enabled());

    // URL + API key but no store ID
    let config = test_config().with_btcpay(
        Some("http://localhost:14142".to_string()),
        Some("api-key-123".to_string()),
        None,
        None,
        None,
    );
    assert!(!config.is_btcpay_enabled());
}

#[test]
fn test_btcpay_client_creation_with_full_config() {
    let config = test_config().with_btcpay(
        Some("http://localhost:14142".to_string()),
        Some("api-key-123".to_string()),
        Some("store-id-456".to_string()),
        Some("offering-789".to_string()),
        Some("plan-abc".to_string()),
    );

    assert!(config.is_btcpay_enabled());
    assert_eq!(config.btcpay_url(), Some("http://localhost:14142"));
    assert_eq!(config.btcpay_api_key(), Some("api-key-123"));
    assert_eq!(config.btcpay_store_id(), Some("store-id-456"));
    assert_eq!(config.btcpay_offering_id(), Some("offering-789"));
    assert_eq!(config.btcpay_plan_id(), Some("plan-abc"));
}

#[test]
fn test_btcpay_client_creation_without_recurring() {
    let config = test_config().with_btcpay(
        Some("http://localhost:14142".to_string()),
        Some("api-key-123".to_string()),
        Some("store-id-456".to_string()),
        None,
        None,
    );

    assert!(config.is_btcpay_enabled());
    assert!(!config.is_btcpay_recurring_enabled());
    assert_eq!(config.btcpay_offering_id(), None);
    assert_eq!(config.btcpay_plan_id(), None);
}

#[test]
fn test_btcpay_recurring_enabled_with_full_config() {
    let config = test_config().with_btcpay(
        Some("http://localhost:14142".to_string()),
        Some("api-key-123".to_string()),
        Some("store-id-456".to_string()),
        Some("offering-789".to_string()),
        Some("plan-abc".to_string()),
    );

    assert!(config.is_btcpay_enabled());
    assert!(config.is_btcpay_recurring_enabled());
}

#[test]
fn test_redirect_response_valid_url() {
    use crate::handlers::donations::redirect_response;
    use axum::response::IntoResponse;

    let response = redirect_response("https://btcpay.example.com/invoice/123");
    let response = response.into_response();
    assert_eq!(response.status(), axum::http::StatusCode::FOUND);
    assert_eq!(
        response.headers().get("Location").unwrap(),
        "https://btcpay.example.com/invoice/123"
    );
}

#[test]
fn test_redirect_response_invalid_url() {
    use crate::handlers::donations::redirect_response;
    use axum::response::IntoResponse;

    // Header values cannot contain certain control characters
    let response = redirect_response("http://example.com/\x00bad");
    let response = response.into_response();
    assert_eq!(response.status(), axum::http::StatusCode::BAD_GATEWAY);
}

#[test]
fn test_thank_you_url_with_frontend_url() {
    use crate::handlers::donations::thank_you_url;

    let config = test_config();
    assert_eq!(
        thank_you_url(&config),
        "http://localhost:3001/donations/thank-you"
    );
}

#[test]
fn test_thank_you_url_strips_trailing_slash() {
    use crate::handlers::donations::thank_you_url;

    let config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        "/tmp/test".to_string(),
        OperatingMode::SelfHosted,
        Some("http://localhost:3001/".to_string()),
        None,
    );
    assert_eq!(
        thank_you_url(&config),
        "http://localhost:3001/donations/thank-you"
    );
}

#[test]
fn test_thank_you_url_fallback_without_frontend_url() {
    use crate::handlers::donations::thank_you_url;

    let config = AppConfig::new_for_test(
        NetworkConfig::Regtest,
        Some("tcp://127.0.0.1:50001".to_string()),
        "127.0.0.1:3000".to_string(),
        "/tmp/test".to_string(),
        OperatingMode::SelfHosted,
        None,
        None,
    );
    assert_eq!(
        thank_you_url(&config),
        "https://canarybitcoin.com/donations/thank-you"
    );
}
