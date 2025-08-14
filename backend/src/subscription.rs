use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
pub enum SubscriptionTier {
    Personal,
    Team,
}

#[derive(Debug, Clone)]
pub struct TierLimits {
    pub max_wallets: Option<usize>,
    pub max_contacts_per_wallet: Option<usize>,
    pub sync_interval_secs: u64,
}

impl SubscriptionTier {
    pub fn limits(&self) -> TierLimits {
        match self {
            Self::Personal => TierLimits {
                max_wallets: Some(1),
                max_contacts_per_wallet: Some(1),
                sync_interval_secs: 600, // 10 minutes
            },
            Self::Team => TierLimits {
                max_wallets: Some(5),
                max_contacts_per_wallet: Some(5),
                sync_interval_secs: 60, // 1 minute
            },
        }
    }
    
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Personal => "personal",
            Self::Team => "team",
        }
    }
}

impl From<&str> for SubscriptionTier {
    fn from(s: &str) -> Self {
        match s {
            "personal" => Self::Personal,
            "team" => Self::Team,
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
}

impl std::fmt::Display for LimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} limit reached ({}/{}). Upgrade to {} for more {}.",
            self.resource, 
            self.current, 
            self.limit,
            "Team",
            self.resource.to_lowercase()
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
            });
        }
    }
    Ok(())
}