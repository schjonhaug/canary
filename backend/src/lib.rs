// Canary Backend Library
// Core Bitcoin wallet functionality

pub mod api;
pub mod auth;
pub mod config;
pub mod electrum;
pub mod email_service;
pub mod message_formatter;
pub mod metadata;
pub mod migrations;
pub mod notifications;
pub mod wallet;
pub mod ntfy_provider;
pub mod twilio_provider;

// Re-export commonly used types
pub use config::AppConfig;
pub use electrum::{ElectrumClient, BlockHeader};
pub use email_service::{EmailService, EmailConfig};
pub use message_formatter::MessageFormatter;
pub use metadata::{
    Contact, NotificationMethod, ProviderType, EventType, Language, TransactionEvent, TransactionEventWithWallet, 
    WalletMetadata, WalletsListResponse, WalletDetailResponse, NotificationStatus, MetadataDb
};
pub use migrations::MigrationRunner;
pub use notifications::{NotificationManager, NotificationProvider, NotificationResult, ProviderInfo};
pub use wallet::WalletManager;
pub use ntfy_provider::NtfyProvider;
pub use twilio_provider::TwilioProvider;

// Test modules
#[cfg(test)]
mod tests;