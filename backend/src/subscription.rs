use crate::config::NetworkConfig;
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
    /// Get tier limits with network-aware sync intervals
    ///
    /// Sync intervals are designed to prevent overlapping sync operations:
    /// - Regtest: Fast intervals (5s/10s) since syncs are instant
    /// - Mainnet: Longer intervals (120s/600s) since syncs take 60+ seconds
    pub fn limits(&self, network: &NetworkConfig) -> TierLimits {
        let (personal_sync, team_sync) = self.get_sync_intervals(network);

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

    /// Get network-appropriate sync intervals for this tier
    pub fn get_sync_intervals(&self, network: &NetworkConfig) -> (u64, u64) {
        // Check for environment variable overrides first
        let env_personal = match network {
            NetworkConfig::Regtest => std::env::var("CANARY_SYNC_INTERVAL_PERSONAL_REGTEST")
                .ok()
                .and_then(|s| s.parse().ok()),
            NetworkConfig::Testnet => std::env::var("CANARY_SYNC_INTERVAL_PERSONAL_TESTNET")
                .ok()
                .and_then(|s| s.parse().ok()),
            NetworkConfig::Mainnet => std::env::var("CANARY_SYNC_INTERVAL_PERSONAL_MAINNET")
                .ok()
                .and_then(|s| s.parse().ok()),
        };

        let env_team = match network {
            NetworkConfig::Regtest => std::env::var("CANARY_SYNC_INTERVAL_TEAM_REGTEST")
                .ok()
                .and_then(|s| s.parse().ok()),
            NetworkConfig::Testnet => std::env::var("CANARY_SYNC_INTERVAL_TEAM_TESTNET")
                .ok()
                .and_then(|s| s.parse().ok()),
            NetworkConfig::Mainnet => std::env::var("CANARY_SYNC_INTERVAL_TEAM_MAINNET")
                .ok()
                .and_then(|s| s.parse().ok()),
        };

        // Use defaults if no environment override
        let (default_personal, default_team) = match network {
            NetworkConfig::Regtest => {
                // Fast intervals for regtest since syncs are instant
                (10, 5) // 10s Personal, 5s Team
            }
            NetworkConfig::Testnet => {
                // Reasonable intervals for testnet (not used in this project)
                (60, 30) // 60s Personal, 30s Team
            }
            NetworkConfig::Mainnet => {
                // Long intervals for mainnet to prevent sync overlap
                // Mainnet syncs take 60+ seconds, so we need longer intervals
                (600, 120) // 600s (10min) Personal, 120s (2min) Team
            }
        };

        (
            env_personal.unwrap_or(default_personal),
            env_team.unwrap_or(default_team),
        )
    }

    /// Get limits for API limit checking - uses reasonable default intervals
    /// This is used by API endpoints where network config isn't available
    pub fn limits_for_api(&self) -> TierLimits {
        // For API limit checking, we use moderate intervals since it doesn't affect sync
        // The important part is the wallet/contact count limits, not the sync intervals
        match self {
            Self::Personal => TierLimits {
                max_wallets: Some(1),
                max_contacts_per_wallet: Some(1),
                sync_interval_secs: 60, // Default interval for API context
            },
            Self::Team => TierLimits {
                max_wallets: Some(5),
                max_contacts_per_wallet: Some(5),
                sync_interval_secs: 60, // Default interval for API context
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
pub fn check_limit(current: usize, limit: Option<usize>, resource: &str) -> Result<(), LimitError> {
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
