// Canary Backend Library
// Core Bitcoin wallet functionality

pub mod admin_notifications;
pub mod api;
pub mod config;
pub mod electrum;
pub mod exchange_rates;
pub mod message_formatter;
pub mod metadata;
pub mod migrations;
pub mod notifications;
pub mod ntfy_provider;
pub mod sync;
pub mod wallet;
pub mod xpub_converter;

// SAAS-specific modules (only compiled with 'saas' feature)
pub mod saas;

// Re-export commonly used types
pub use admin_notifications::AdminNotifications;
pub use config::AppConfig;
pub use electrum::{BlockHeader, ElectrumClient};
pub use message_formatter::MessageFormatter;
pub use metadata::{
    Contact, EventType, Language, MetadataDb, NotificationMethod, NotificationStatus, ProviderType,
    WalletDetailResponse, WalletMetadata, WalletsListResponse,
};
pub use migrations::MigrationRunner;
pub use notifications::{
    NotificationManager, NotificationProvider, NotificationResult, ProviderInfo,
};
pub use ntfy_provider::NtfyProvider;
pub use wallet::{WalletCreationService, WalletManager};
pub use xpub_converter::{ScriptType, XpubConverter};

// Re-export SAAS types for convenience
pub use saas::{
    auth, authenticate_user, email_provider, email_service, load_twilio_config_from_env,
    stripe_billing, stripe_client_service, subscription, twilio_provider, AuthResponse,
    AuthService, AuthUser, AuthUserResponse, CheckoutSessionResponse, Claims,
    CustomerPortalResponse, EmailConfig, EmailProvider, EmailService, ForgotPasswordRequest,
    FrontendPriceInfo, FrontendTierPricing, LimitError, LoginRequest, PricingInfo,
    RegisterRequest, ResetPasswordRequest, StripeBilling, StripeClientService, SubscriptionTier,
    TierLimits, TwilioProvider, UpdateUserPreferencesRequest, UpdateUserRequest,
    UpdateUserResponse, UserPreferencesResponse,
};

// Test modules
#[cfg(test)]
mod tests;
