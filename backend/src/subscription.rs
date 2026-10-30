use crate::config::{NetworkConfig, SyncIntervalOverrides};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SubscriptionTier {
    Personal,
    Team,
}

/// Sync intervals, in seconds, for each cloud subscription tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TierSyncIntervals {
    pub personal_secs: u64,
    pub team_secs: u64,
}

impl TierSyncIntervals {
    /// Network defaults, chosen so that syncs do not overlap.
    pub fn defaults_for(network: &NetworkConfig) -> Self {
        let (personal_secs, team_secs) = match network {
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
        Self {
            personal_secs,
            team_secs,
        }
    }

    /// Applies the operator's `CANARY_SYNC_INTERVAL_{TIER}_{NETWORK}` overrides.
    #[must_use]
    pub fn with_overrides(self, overrides: SyncIntervalOverrides) -> Self {
        Self {
            personal_secs: overrides.personal_secs.unwrap_or(self.personal_secs),
            team_secs: overrides.team_secs.unwrap_or(self.team_secs),
        }
    }

    /// The interval for `tier`.
    pub fn for_tier(self, tier: SubscriptionTier) -> u64 {
        match tier {
            SubscriptionTier::Personal => self.personal_secs,
            SubscriptionTier::Team => self.team_secs,
        }
    }
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
    /// Get tier limits with the given cloud sync intervals.
    pub fn limits(self, sync_intervals: TierSyncIntervals) -> TierLimits {
        match self {
            Self::Personal => TierLimits {
                max_wallets: Some(Self::Personal.max_wallets()),
                max_contacts_per_wallet: Some(Self::Personal.max_contacts_per_wallet()),
                sync_interval_secs: sync_intervals.personal_secs,
            },
            Self::Team => TierLimits {
                max_wallets: Some(Self::Team.max_wallets()),
                max_contacts_per_wallet: Some(Self::Team.max_contacts_per_wallet()),
                sync_interval_secs: sync_intervals.team_secs,
            },
        }
    }

    /// Get limits for API limit checking - uses reasonable default intervals
    /// This is used by API endpoints where network config isn't available
    pub fn limits_for_api(self) -> TierLimits {
        // For API limit checking, we use moderate intervals since it doesn't affect sync
        // The important part is the wallet/contact count limits, not the sync intervals
        match self {
            Self::Personal => TierLimits {
                max_wallets: Some(Self::Personal.max_wallets()),
                max_contacts_per_wallet: Some(Self::Personal.max_contacts_per_wallet()),
                sync_interval_secs: 60, // Default interval for API context
            },
            Self::Team => TierLimits {
                max_wallets: Some(Self::Team.max_wallets()),
                max_contacts_per_wallet: Some(Self::Team.max_contacts_per_wallet()),
                sync_interval_secs: 60, // Default interval for API context
            },
        }
    }

    /// Maximum number of active wallets on this tier.
    pub const fn max_wallets(self) -> usize {
        match self {
            Self::Personal => 1,
            Self::Team => 5,
        }
    }

    /// Maximum number of active contacts per wallet on this tier.
    pub const fn max_contacts_per_wallet(self) -> usize {
        match self {
            Self::Personal => 1,
            Self::Team => 5,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Personal => "personal",
            Self::Team => "team",
        }
    }
}

impl From<&str> for SubscriptionTier {
    fn from(s: &str) -> Self {
        match s {
            "team" => Self::Team,
            // "personal" and unknown tiers get Personal limits
            _ => Self::Personal,
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
/// - Status is "trialing" AND `trial_ends_at` is in the future, OR
/// - Status is "canceled" AND `subscription_ends_at` is in the future
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
        let intervals = TierSyncIntervals::defaults_for(&NetworkConfig::Mainnet).with_overrides(
            SyncIntervalOverrides {
                personal_secs: Some(15),
                team_secs: Some(30),
            },
        );
        assert_eq!(intervals.personal_secs, 15);
        assert_eq!(intervals.team_secs, 30);
        assert_eq!(intervals.for_tier(SubscriptionTier::Personal), 15);
        assert_eq!(intervals.for_tier(SubscriptionTier::Team), 30);

        let partial = TierSyncIntervals::defaults_for(&NetworkConfig::Mainnet).with_overrides(
            SyncIntervalOverrides {
                personal_secs: None,
                team_secs: Some(30),
            },
        );
        assert_eq!(partial.personal_secs, 600);
        assert_eq!(partial.team_secs, 30);
    }

    #[test]
    fn test_defaults_when_no_env_vars() {
        let intervals = TierSyncIntervals::defaults_for(&NetworkConfig::Mainnet)
            .with_overrides(SyncIntervalOverrides::default());
        assert_eq!(intervals.personal_secs, 600); // Mainnet default for Personal
        assert_eq!(intervals.team_secs, 120); // Mainnet default for Team
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
