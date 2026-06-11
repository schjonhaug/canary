//! API request handlers organized by domain

pub(crate) mod auth;
mod balance_alerts;
mod billing;
mod blockchain;
mod config;
mod contact;
mod contact_verification;
pub(crate) mod donations;
mod health;
mod helpers;
mod notifications;
mod providers;
mod user_preferences;
mod wallet;

pub use auth::{
    demo_login, extract_token_from_cookies, forgot_password, login, logout, me, register,
    reset_password, submit_contact_form, update_user, verify_email,
};
pub use balance_alerts::{
    create_wallet_balance_alert, delete_balance_alert, get_wallet_balance_alerts,
    validate_wallet_balance_alert,
};
pub use billing::{
    create_stripe_checkout_session, create_stripe_customer_portal, get_billing_pricing,
    get_billing_status, get_checkout_session_details, handle_stripe_webhook,
};
pub use blockchain::{get_current_block_header, get_exchange_rates};
pub use config::get_config;
pub use contact::{
    create_wallet_contact, delete_wallet_contact, get_wallet_contacts, update_wallet_contact,
};
pub use contact_verification::{send_contact_verification, verify_contact};
pub use donations::{donate_one_time, donate_recurring};
pub use health::{get_database_health, run_integrity_check};
pub use notifications::send_test_ntfy_notification;
pub use providers::get_providers;
pub use user_preferences::{get_user_preferences, update_user_preferences};
pub use wallet::{
    create_wallet_non_blocking, delete_wallet, get_transaction_notifications, get_wallet,
    get_wallet_detail, get_wallet_notifications, get_wallets_list, update_wallet,
};
