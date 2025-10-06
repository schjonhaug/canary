// SAAS-specific modules
// These modules are only compiled when the 'saas' feature is enabled

pub mod auth;
pub mod email_provider;
pub mod email_service;
pub mod stripe_billing;
pub mod stripe_client_service;
pub mod subscription;
pub mod twilio_provider;

// Re-export commonly used types for convenience
pub use auth::{
    authenticate_user, load_twilio_config_from_env, AuthResponse, AuthService, AuthUser,
    AuthUserResponse, Claims, ForgotPasswordRequest, LoginRequest, RegisterRequest,
    ResetPasswordRequest, UpdateUserPreferencesRequest, UpdateUserRequest, UpdateUserResponse,
    UserPreferencesResponse,
};
pub use email_provider::EmailProvider;
pub use email_service::{EmailConfig, EmailService};
pub use stripe_billing::{
    CheckoutSessionResponse, CustomerPortalResponse, FrontendPriceInfo, FrontendTierPricing,
    PricingInfo, StripeBilling,
};
pub use stripe_client_service::StripeClientService;
pub use subscription::{LimitError, SubscriptionTier, TierLimits};
pub use twilio_provider::TwilioProvider;
