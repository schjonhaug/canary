// Canary Backend Library
// Core Bitcoin wallet functionality

pub mod admin_notifications;
pub mod api;
pub mod auth;
pub mod config;
pub mod electrum;
pub mod email_service;
pub mod message_formatter;
pub mod metadata;
pub mod migrations;
pub mod notifications;
pub mod ntfy_provider;
pub mod stripe_billing;
pub mod stripe_client_service;
pub mod subscription;
pub mod sync;
pub mod twilio_provider;
pub mod wallet;
pub mod xpub_converter;

// Re-export commonly used types
pub use admin_notifications::AdminNotifications;
pub use config::AppConfig;
pub use electrum::{BlockHeader, ElectrumClient};
pub use email_service::{EmailConfig, EmailService};
pub use message_formatter::MessageFormatter;
pub use metadata::{
    Contact, EventType, Language, MetadataDb, NotificationMethod, NotificationStatus, ProviderType,
    TransactionEvent, TransactionEventWithWallet, WalletDetailResponse, WalletMetadata,
    WalletsListResponse,
};
pub use migrations::MigrationRunner;
pub use notifications::{
    NotificationManager, NotificationProvider, NotificationResult, ProviderInfo,
};
pub use ntfy_provider::NtfyProvider;
pub use stripe_billing::{
    CheckoutSessionResponse, CustomerPortalResponse, FrontendPriceInfo, FrontendTierPricing,
    PricingInfo, StripeBilling,
};
pub use twilio_provider::TwilioProvider;
pub use wallet::{WalletCreationService, WalletManager};
pub use xpub_converter::{ScriptType, XpubConverter};

// Test modules
#[cfg(test)]
mod tests;
