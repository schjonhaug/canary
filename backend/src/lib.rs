// Canary Backend Library
// Core Bitcoin wallet functionality

pub mod config;
pub mod electrum;
pub mod message_formatter;
pub mod metadata;
pub mod migrations;
pub mod notifications;
pub mod wallet;
pub mod ntfy_provider;
pub mod twilio_provider;

// API module
pub mod api;

// Re-export commonly used types
pub use config::AppConfig;
pub use electrum::{ElectrumClient, BlockHeader};
pub use message_formatter::MessageFormatter;
pub use metadata::{
    Contact, NotificationMethod, ProviderType, EventType, Language, TransactionEvent, TransactionEventWithWallet, 
    WalletMetadata, DashboardUpdate, NotificationStatus, MetadataDb
};
pub use migrations::MigrationRunner;
pub use notifications::{NotificationManager, NotificationProvider, NotificationResult, ProviderInfo};
pub use wallet::WalletManager;
pub use ntfy_provider::NtfyProvider;
pub use twilio_provider::TwilioProvider;

// Test modules
#[cfg(test)]
mod tests;