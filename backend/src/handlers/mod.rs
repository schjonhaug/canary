//! API request handlers organized by domain

mod balance_alerts;
mod blockchain;
mod providers;
mod user_preferences;
mod wallet;

pub use balance_alerts::{
    create_wallet_balance_alert, delete_balance_alert, get_wallet_balance_alerts,
};
pub use blockchain::{get_current_block_header, get_exchange_rates};
pub use providers::get_providers;
pub use user_preferences::{get_user_preferences, update_user_preferences};
pub use wallet::{
    create_wallet_non_blocking, delete_wallet, get_wallet, get_wallet_detail, get_wallets_list,
    update_wallet,
};
