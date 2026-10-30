//! The metadata database: users, wallets, contacts, transactions, alerts and
//! billing state in `SQLite`, reached through [`MetadataDb`]. Each submodule adds
//! the `MetadataDb` methods for one area.

mod alert;
mod contact;
mod db;
mod integrity;
mod pool;
mod status;
mod support;
mod transaction;
mod types;
mod user;
mod wallet;

pub use alert::CreateBalanceAlertInput;
pub use contact::ContactNotificationSettings;
pub use db::StripeEventClaim;
pub use pool::MetadataDb;
pub use status::{
    NotificationDeliveryStatus, SubscriptionStatus, TransactionStatus, UnknownValueError,
    WalletStatus, WalletType,
};
pub use types::*;
