// Canary Wallet Backend Library
// Core Bitcoin wallet functionality

#[macro_use]
extern crate rust_i18n;

// Initialize i18n with locale files from ./locales directory
// Fallback to English (US) if translation is missing
i18n!("locales", fallback = "en-US");

pub mod admin_notifications;
pub mod api;
pub mod auth;
pub mod btcpay_client;
pub mod config;
pub mod electrum;
mod electrum_history;
pub mod email_provider;
pub mod email_queue;
pub mod email_service;
pub mod exchange_rates;
pub mod extractors;
pub mod handlers;
pub mod message_formatter;
pub mod metadata;
pub mod migrations;
pub mod models;
pub mod nostr_provider;
pub mod notification_failure_tracker;
pub mod notifications;
pub mod ntfy_provider;
pub mod outbound_target;
pub mod stripe_billing;
pub mod stripe_client_service;
pub mod subscription;
pub mod sync;
pub mod tls;
pub mod twilio_provider;
pub mod utils;
pub mod wallet;
pub mod webhook_provider;
pub mod xpub_converter;

// Re-export commonly used types
pub use admin_notifications::AdminNotifications;
pub use config::AppConfig;
pub use electrum::{BlockHeader, ElectrumClient};
pub use email_service::{EmailConfig, EmailService};
pub use message_formatter::MessageFormatter;
pub use metadata::{
    Contact, EventType, Language, MetadataDb, NotificationMethod, NotificationStatus, ProviderType,
    WalletDetailResponse, WalletMetadata, WalletsListResponse,
};
pub use migrations::MigrationRunner;
pub use nostr_provider::NostrProvider;
pub use notification_failure_tracker::{NotificationFailureTracker, ProviderErrorCategory};
pub use notifications::{
    NotificationManager, NotificationProvider, NotificationResult, ProviderInfo,
};
pub use ntfy_provider::NtfyProvider;
pub use stripe_billing::{
    CheckoutSessionResponse, CustomerPortalResponse, FrontendPriceInfo, FrontendTierPricing,
    PricingInfo, StripeBilling,
};
pub use twilio_provider::TwilioProvider;
pub use utils::{parse_multipath_descriptor, strip_key_origin};
pub use wallet::{WalletCreationService, WalletManager};
pub use webhook_provider::WebhookProvider;
pub use xpub_converter::{ScriptType, XpubConverter};

// Test modules
#[cfg(test)]
mod tests;
