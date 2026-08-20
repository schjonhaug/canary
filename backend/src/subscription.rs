use crate::config::NetworkConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

/// Decide whether a wallet should consume an active subscription slot.
///
/// Failed wallets are recoverable records, not active subscription slots, so
/// they are kept inactive and excluded from position accounting.
pub fn wallet_active_limit_decision(
    status: &str,
    wallet_limit: usize,
    active_wallet_count: &mut usize,
    non_failed_wallet_count: &mut usize,
) -> (bool, Option<usize>) {
    if status == "failed" {
        return (false, None);
    }

    *non_failed_wallet_count += 1;
    let wallet_position = *non_failed_wallet_count;
    let should_be_active = *active_wallet_count < wallet_limit;
    if should_be_active {
        *active_wallet_count += 1;
    }

    (should_be_active, Some(wallet_position))
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

    /// Get network-appropriate sync intervals for this tier (cloud mode only)
    /// This should only be called in cloud mode with subscription tiers
    pub fn get_sync_intervals(&self, network: &NetworkConfig) -> (u64, u64) {
        // Check for tier-specific environment variable overrides first
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

        // Cloud tier-specific defaults
        let (default_personal, default_team) = match network {
            NetworkConfig::Regtest => {
                // Moderate intervals for regtest to prevent startup conflicts
                (30, 15) // 30s Personal, 15s Team
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
            "{} limit reached ({}/{}). Upgrade to Team for more {}.",
            self.resource,
            self.current,
            self.limit,
            self.resource.to_lowercase()
        )
    }
}

impl std::error::Error for LimitError {}

/// Parse a datetime string and check if it's in the future
fn parse_datetime(date_str: &str) -> Option<chrono::NaiveDateTime> {
    chrono::DateTime::parse_from_rfc3339(date_str)
        .ok()
        .map(|dt| dt.naive_utc())
        .or_else(|| chrono::NaiveDateTime::parse_from_str(date_str, "%Y-%m-%d %H:%M:%S").ok())
}

fn is_future_datetime(date_str: &str) -> Option<bool> {
    parse_datetime(date_str).map(|dt| dt > chrono::Utc::now().naive_utc())
}

/// Check if a subscription is currently active
///
/// A subscription is active if:
/// - Status is "active", OR
/// - Status is "trialing" AND trial_ends_at is in the future, OR
/// - Status is "canceled" AND subscription_ends_at is in the future
pub fn is_subscription_active(
    subscription_status: &str,
    trial_ends_at: Option<&str>,
    subscription_ends_at: Option<&str>,
) -> bool {
    match subscription_status {
        "active" => true,
        "trialing" => {
            if let Some(trial_ends_at_str) = trial_ends_at {
                // Assume active if parse fails (shouldn't happen with valid DB data)
                is_future_datetime(trial_ends_at_str).unwrap_or(true)
            } else {
                true // If no trial_ends_at, assume active
            }
        }
        "canceled" => {
            // Active if subscription_ends_at is in the future (user paid for remaining period).
            // Fail closed: if missing or unparseable, treat as inactive.
            subscription_ends_at
                .and_then(is_future_datetime)
                .unwrap_or(false)
        }
        _ => false,
    }
}

/// Get the effective subscription status for display
///
/// Returns "expired" if trial has ended, otherwise returns the original status
pub fn get_effective_subscription_status(
    subscription_status: &str,
    trial_ends_at: Option<&str>,
) -> String {
    if subscription_status == "trialing" {
        if let Some(trial_ends_at_str) = trial_ends_at {
            if let Some(trial_ends_at) = parse_datetime(trial_ends_at_str) {
                let now = chrono::Utc::now().naive_utc();
                if trial_ends_at < now {
                    return "expired".to_string();
                }
            }
        }
    }
    subscription_status.to_string()
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_specific_intervals() {
        // Set tier-specific environment variables
        std::env::set_var("CANARY_SYNC_INTERVAL_PERSONAL_MAINNET", "15");
        std::env::set_var("CANARY_SYNC_INTERVAL_TEAM_MAINNET", "30");

        let (personal, team) =
            SubscriptionTier::Personal.get_sync_intervals(&NetworkConfig::Mainnet);
        assert_eq!(personal, 15);
        assert_eq!(team, 30);

        // Clean up
        std::env::remove_var("CANARY_SYNC_INTERVAL_PERSONAL_MAINNET");
        std::env::remove_var("CANARY_SYNC_INTERVAL_TEAM_MAINNET");
    }

    #[test]
    fn test_defaults_when_no_env_vars() {
        // Clear all sync interval environment variables
        std::env::remove_var("CANARY_SYNC_INTERVAL");
        std::env::remove_var("CANARY_SYNC_INTERVAL_PERSONAL_MAINNET");
        std::env::remove_var("CANARY_SYNC_INTERVAL_TEAM_MAINNET");

        let (personal, team) =
            SubscriptionTier::Personal.get_sync_intervals(&NetworkConfig::Mainnet);
        assert_eq!(personal, 600); // Mainnet default for Personal
        assert_eq!(team, 120); // Mainnet default for Team
    }

    #[test]
    fn test_is_subscription_active_with_active_status() {
        assert!(is_subscription_active("active", None, None));
        assert!(is_subscription_active(
            "active",
            Some("2025-01-01 00:00:00"),
            None
        ));
    }

    #[test]
    fn test_is_subscription_active_with_valid_trial() {
        // Trial ends in the future
        let future_date = chrono::Utc::now().naive_utc() + chrono::Duration::days(30);
        let future_str = future_date.format("%Y-%m-%d %H:%M:%S").to_string();
        assert!(is_subscription_active("trialing", Some(&future_str), None));
    }

    #[test]
    fn test_is_subscription_active_with_expired_trial() {
        // Trial ended in the past
        let past_date = chrono::Utc::now().naive_utc() - chrono::Duration::days(30);
        let past_str = past_date.format("%Y-%m-%d %H:%M:%S").to_string();
        assert!(!is_subscription_active("trialing", Some(&past_str), None));
    }

    #[test]
    fn test_subscription_dates_accept_rfc3339() {
        let future = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
        let past = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();

        assert!(is_subscription_active("trialing", Some(&future), None));
        assert_eq!(
            get_effective_subscription_status("trialing", Some(&past)),
            "expired"
        );
    }

    #[test]
    fn test_is_subscription_active_with_other_statuses() {
        assert!(!is_subscription_active("expired", None, None));
        assert!(!is_subscription_active("canceled", None, None));
        assert!(!is_subscription_active("past_due", None, None));
        assert!(!is_subscription_active("pending", None, None));
    }

    #[test]
    fn test_is_subscription_active_with_canceled_future_end() {
        // Canceled but subscription_ends_at is in the future — should be active
        let future_date = chrono::Utc::now().naive_utc() + chrono::Duration::days(15);
        let future_str = future_date.format("%Y-%m-%d %H:%M:%S").to_string();
        assert!(is_subscription_active("canceled", None, Some(&future_str)));
    }

    #[test]
    fn test_is_subscription_active_with_canceled_past_end() {
        // Canceled and subscription_ends_at is in the past — should be inactive
        let past_date = chrono::Utc::now().naive_utc() - chrono::Duration::days(5);
        let past_str = past_date.format("%Y-%m-%d %H:%M:%S").to_string();
        assert!(!is_subscription_active("canceled", None, Some(&past_str)));
    }

    #[test]
    fn test_is_subscription_active_with_canceled_invalid_date() {
        // Canceled with invalid date format — should fail closed (inactive)
        assert!(!is_subscription_active(
            "canceled",
            None,
            Some("not-a-date")
        ));
    }

    #[test]
    fn test_get_effective_subscription_status_active() {
        assert_eq!(get_effective_subscription_status("active", None), "active");
    }

    #[test]
    fn test_get_effective_subscription_status_valid_trial() {
        let future_date = chrono::Utc::now().naive_utc() + chrono::Duration::days(30);
        let future_str = future_date.format("%Y-%m-%d %H:%M:%S").to_string();
        assert_eq!(
            get_effective_subscription_status("trialing", Some(&future_str)),
            "trialing"
        );
    }

    #[test]
    fn test_get_effective_subscription_status_expired_trial() {
        let past_date = chrono::Utc::now().naive_utc() - chrono::Duration::days(30);
        let past_str = past_date.format("%Y-%m-%d %H:%M:%S").to_string();
        assert_eq!(
            get_effective_subscription_status("trialing", Some(&past_str)),
            "expired"
        );
    }

    #[test]
    fn test_get_effective_subscription_status_other_statuses() {
        assert_eq!(
            get_effective_subscription_status("expired", None),
            "expired"
        );
        assert_eq!(
            get_effective_subscription_status("canceled", None),
            "canceled"
        );
    }
}
