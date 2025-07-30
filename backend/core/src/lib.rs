// Canary Core Library
// This library contains the core Bitcoin wallet functionality

pub mod config;
pub mod electrum;
pub mod metadata;
pub mod migrations;
pub mod notifications;
pub mod wallet;
pub mod ntfy_provider;

// Re-export commonly used types
pub use config::AppConfig;
pub use electrum::{ElectrumClient, BlockHeader};
pub use metadata::{
    ContactPerson, EventType, Language, TransactionEvent, TransactionEventWithWallet, 
    WalletMetadata, DashboardUpdate, MetadataDb
};
pub use migrations::MigrationRunner;
pub use notifications::{NotificationManager, NotificationProvider, NotificationResult, ProviderInfo};
pub use wallet::WalletManager;
pub use ntfy_provider::NtfyProvider;