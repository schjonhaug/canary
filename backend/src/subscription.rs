use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub enum SubscriptionTier {
    Personal,
    Pro,
    Business, // Ready for future implementation
}

#[derive(Debug, Clone)]
pub struct TierLimits {
    pub max_wallets: Option<usize>,
    pub max_contacts_per_wallet: Option<usize>,
    pub sync_interval_secs: u64,
    pub allows_sms: bool,
    pub allows_push: bool,
    pub allows_transaction_analysis: bool,
}

impl SubscriptionTier {
    pub fn limits(&self) -> TierLimits {
        match self {
            Self::Personal => TierLimits {
                max_wallets: Some(1),
                max_contacts_per_wallet: Some(1),
                sync_interval_secs: 300, // 5 minutes
                allows_sms: false,
                allows_push: false,
                allows_transaction_analysis: false,
            },
            Self::Pro => TierLimits {
                max_wallets: Some(15),
                max_contacts_per_wallet: Some(10),
                sync_interval_secs: 60, // 1 minute
                allows_sms: true,
                allows_push: true,
                allows_transaction_analysis: true,
            },
            Self::Business => TierLimits {
                max_wallets: None, // unlimited
                max_contacts_per_wallet: None,
                sync_interval_secs: 5,
                allows_sms: true,
                allows_push: true,
                allows_transaction_analysis: true,
            },
        }
    }
    
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Personal => "personal",
            Self::Pro => "pro",
            Self::Business => "business",
        }
    }
    
    pub fn stripe_price_id(&self, yearly: bool) -> &'static str {
        match (self, yearly) {
            (Self::Personal, false) => "price_personal_monthly",
            (Self::Personal, true) => "price_personal_yearly",
            (Self::Pro, false) => "price_pro_monthly",
            (Self::Pro, true) => "price_pro_yearly",
            (Self::Business, false) => "price_business_monthly", // Future
            (Self::Business, true) => "price_business_yearly",   // Future
        }
    }
}

impl From<&str> for SubscriptionTier {
    fn from(s: &str) -> Self {
        match s {
            "personal" => Self::Personal,
            "pro" => Self::Pro,
            "business" => Self::Business,
            _ => Self::Personal, // Default fallback
        }
    }
}

impl From<String> for SubscriptionTier {
    fn from(s: String) -> Self {
        SubscriptionTier::from(s.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct LimitError {
    pub resource: String,
    pub current: usize,
    pub limit: usize,
    pub tier: SubscriptionTier,
    pub upgrade_required: bool,
}

impl std::fmt::Display for LimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} limit reached ({}/{}). Upgrade to Pro for more {}.",
            self.resource, self.current, self.limit, self.resource.to_lowercase()
        )
    }
}

impl std::error::Error for LimitError {}

/// Generic limit checker that works for any resource type and tier
pub fn check_limit(
    current: usize,
    limit: Option<usize>,
    resource: &str,
    tier: SubscriptionTier,
) -> Result<(), LimitError> {
    if let Some(max) = limit {
        if current >= max {
            return Err(LimitError {
                resource: resource.to_string(),
                current,
                limit: max,
                tier,
                upgrade_required: true,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_limits() {
        let personal = SubscriptionTier::Personal;
        let pro = SubscriptionTier::Pro;
        let business = SubscriptionTier::Business;
        
        // Personal limits
        let personal_limits = personal.limits();
        assert_eq!(personal_limits.max_wallets, Some(1));
        assert_eq!(personal_limits.max_contacts_per_wallet, Some(1));
        assert_eq!(personal_limits.sync_interval_secs, 300);
        assert!(!personal_limits.allows_sms);
        assert!(!personal_limits.allows_push);
        
        // Pro limits
        let pro_limits = pro.limits();
        assert_eq!(pro_limits.max_wallets, Some(15));
        assert_eq!(pro_limits.max_contacts_per_wallet, Some(10));
        assert_eq!(pro_limits.sync_interval_secs, 60);
        assert!(pro_limits.allows_sms);
        assert!(pro_limits.allows_push);
        
        // Business limits (unlimited)
        let business_limits = business.limits();
        assert_eq!(business_limits.max_wallets, None); // Unlimited
        assert_eq!(business_limits.max_contacts_per_wallet, None); // Unlimited
        assert_eq!(business_limits.sync_interval_secs, 5); // Fastest sync
        assert!(business_limits.allows_sms);
        assert!(business_limits.allows_push);
        assert!(business_limits.allows_transaction_analysis);
    }
    
    #[test]
    fn test_limit_checking() {
        // Should pass - under limit
        let result = check_limit(0, Some(1), "wallets", SubscriptionTier::Personal);
        assert!(result.is_ok());
        
        // Should fail - at limit
        let result = check_limit(1, Some(1), "wallets", SubscriptionTier::Personal);
        assert!(result.is_err());
        
        // Should pass - unlimited (None)
        let result = check_limit(1000, None, "wallets", SubscriptionTier::Business);
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_tier_conversion() {
        assert_eq!(SubscriptionTier::from("personal"), SubscriptionTier::Personal);
        assert_eq!(SubscriptionTier::from("pro"), SubscriptionTier::Pro);
        assert_eq!(SubscriptionTier::from("business"), SubscriptionTier::Business);
        assert_eq!(SubscriptionTier::from("invalid"), SubscriptionTier::Personal); // Fallback
    }
}