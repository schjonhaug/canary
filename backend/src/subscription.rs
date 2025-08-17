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
        // Use faster sync intervals for development (debug builds)
        let (personal_sync, team_sync) = if cfg!(debug_assertions) {
            (10, 5) // Development: 10s Personal, 5s Team
        } else {
            (600, 60) // Production: 10min Personal, 1min Team
        };

        match self {
            Self::Personal => TierLimits {
                max_wallets: Some(1),
                max_contacts_per_wallet: Some(1),
                sync_interval_secs: personal_sync,
            },
            Self::Team => TierLimits {
                max_wallets: Some(5),
                max_contacts_per_wallet: Some(5),
                sync_interval_secs: team_sync,
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

/// Generic limit checker that works for any resource type
pub fn check_limit(
    current: usize,
    limit: Option<usize>,
    resource: &str,
) -> Result<(), LimitError> {
    if let Some(max) = limit {
        if current >= max {
            return Err(LimitError {
                resource: resource.to_string(),
                current,
                limit: max,
            });
        }
    }
    Ok(())
}
