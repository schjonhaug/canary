//! Process-local rate limiting used only when a persistent limiter is unavailable.

use crate::metadata::RateLimitDecision;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

const MAX_TRACKED_IDENTIFIERS: usize = 1_024;

#[derive(Debug)]
struct RateLimitWindow {
    attempt_count: i64,
    window_expires_at: Instant,
    blocked_until: Option<Instant>,
}

#[derive(Debug, Default)]
pub(crate) struct InMemoryRateLimiter {
    windows: HashMap<(String, String), RateLimitWindow>,
}

impl InMemoryRateLimiter {
    pub(crate) fn check(
        &mut self,
        scope: &str,
        identifier: &str,
        max_attempts: i64,
        window: Duration,
    ) -> RateLimitDecision {
        self.check_at(scope, identifier, max_attempts, window, Instant::now())
    }

    fn check_at(
        &mut self,
        scope: &str,
        identifier: &str,
        max_attempts: i64,
        window: Duration,
        now: Instant,
    ) -> RateLimitDecision {
        self.windows
            .retain(|_, entry| entry.blocked_until.unwrap_or(entry.window_expires_at) > now);

        let key = (
            scope.trim().to_lowercase(),
            identifier.trim().to_lowercase(),
        );
        if !self.windows.contains_key(&key) && self.windows.len() >= MAX_TRACKED_IDENTIFIERS {
            return RateLimitDecision {
                allowed: false,
                retry_after_seconds: Some(duration_seconds_rounded_up(window)),
            };
        }

        let entry = self.windows.entry(key).or_insert(RateLimitWindow {
            attempt_count: 0,
            window_expires_at: now + window,
            blocked_until: None,
        });

        if let Some(blocked_until) = entry.blocked_until {
            if blocked_until > now {
                return RateLimitDecision {
                    allowed: false,
                    retry_after_seconds: Some(retry_after_seconds(blocked_until, now)),
                };
            }
        }

        if entry.window_expires_at <= now {
            entry.attempt_count = 0;
            entry.window_expires_at = now + window;
            entry.blocked_until = None;
        }

        entry.attempt_count += 1;
        if entry.attempt_count > max_attempts {
            let blocked_until = now + window;
            entry.attempt_count = max_attempts;
            entry.window_expires_at = blocked_until;
            entry.blocked_until = Some(blocked_until);
            RateLimitDecision {
                allowed: false,
                retry_after_seconds: Some(retry_after_seconds(blocked_until, now)),
            }
        } else {
            RateLimitDecision {
                allowed: true,
                retry_after_seconds: None,
            }
        }
    }
}

fn retry_after_seconds(blocked_until: Instant, now: Instant) -> i64 {
    duration_seconds_rounded_up(blocked_until.saturating_duration_since(now))
}

fn duration_seconds_rounded_up(duration: Duration) -> i64 {
    let rounded_up = duration
        .as_secs()
        .saturating_add(u64::from(duration.subsec_nanos() > 0))
        .max(1);
    i64::try_from(rounded_up).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_are_isolated_by_scope_and_identifier() {
        let mut limiter = InMemoryRateLimiter::default();
        let now = Instant::now();
        let window = Duration::from_secs(300);

        assert!(
            limiter
                .check_at("database_health", "admin-1", 2, window, now)
                .allowed
        );
        assert!(
            limiter
                .check_at("database_health", "admin-1", 2, window, now)
                .allowed
        );

        let blocked = limiter.check_at("database_health", "admin-1", 2, window, now);
        assert_eq!(
            blocked,
            RateLimitDecision {
                allowed: false,
                retry_after_seconds: Some(300),
            }
        );
        assert!(
            limiter
                .check_at("database_integrity", "admin-1", 2, window, now)
                .allowed
        );
        assert!(
            limiter
                .check_at("database_health", "admin-2", 2, window, now)
                .allowed
        );
    }

    #[test]
    fn expired_fallback_window_resets() {
        let mut limiter = InMemoryRateLimiter::default();
        let now = Instant::now();
        let window = Duration::from_secs(60);

        assert!(limiter.check_at("scope", "admin", 1, window, now).allowed);
        assert!(!limiter.check_at("scope", "admin", 1, window, now).allowed);
        assert!(
            limiter
                .check_at("scope", "admin", 1, window, now + window)
                .allowed
        );
    }

    #[test]
    fn identifier_storage_is_bounded_and_fails_closed_at_capacity() {
        let mut limiter = InMemoryRateLimiter::default();
        let now = Instant::now();
        let window = Duration::from_secs(60);

        for index in 0..MAX_TRACKED_IDENTIFIERS {
            assert!(
                limiter
                    .check_at("scope", &format!("admin-{index}"), 1, window, now)
                    .allowed
            );
        }

        assert_eq!(limiter.windows.len(), MAX_TRACKED_IDENTIFIERS);
        assert_eq!(
            limiter.check_at("scope", "overflow-admin", 1, window, now),
            RateLimitDecision {
                allowed: false,
                retry_after_seconds: Some(60),
            }
        );
        assert_eq!(limiter.windows.len(), MAX_TRACKED_IDENTIFIERS);
    }
}
