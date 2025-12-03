//! Notification provider failure tracking with throttled admin alerts.
//!
//! Tracks consecutive delivery failures for SMS (Twilio) and Email (Resend) providers,
//! sending admin notifications after a threshold is reached. Follows the pattern
//! established by `ElectrumClientManager` for failure tracking and alerting.

use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Number of consecutive failures before sending an admin alert
const ALERT_FAILURE_THRESHOLD: u32 = 3;

/// Duration after which failure counter resets if no new failures occur (1 hour)
const FAILURE_RESET_DURATION: Duration = Duration::from_secs(3600);

/// Categories of notification provider errors for actionable admin alerts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderErrorCategory {
    /// 401 - Bad credentials or invalid API key
    Authentication,
    /// 402 - Account needs funding (common with Twilio)
    InsufficientFunds,
    /// 429 - Too many requests, rate limited
    RateLimit,
    /// Bad email/phone format or undeliverable address
    InvalidRecipient,
    /// 5xx from provider
    ServerError,
    /// Connection/timeout/DNS errors
    NetworkError,
    /// Unrecognized error pattern
    Unknown,
}

impl ProviderErrorCategory {
    /// Parse an error message to determine its category.
    pub fn from_error(error_msg: &str) -> Self {
        let msg_lower = error_msg.to_lowercase();

        // Authentication errors (401, invalid credentials)
        if msg_lower.contains("401")
            || msg_lower.contains("authenticate")
            || msg_lower.contains("invalid api key")
            || msg_lower.contains("unauthorized")
            || msg_lower.contains("authentication")
            || msg_lower.contains("credential")
        {
            return Self::Authentication;
        }

        // Insufficient funds (402, balance issues)
        if msg_lower.contains("402")
            || msg_lower.contains("insufficient")
            || msg_lower.contains("balance")
            || msg_lower.contains("payment required")
        {
            return Self::InsufficientFunds;
        }

        // Rate limiting (429)
        if msg_lower.contains("429")
            || msg_lower.contains("rate limit")
            || msg_lower.contains("too many")
            || msg_lower.contains("throttl")
        {
            return Self::RateLimit;
        }

        // Invalid recipient (400, format issues)
        if msg_lower.contains("undeliverable")
            || msg_lower.contains("not valid")
            || msg_lower.contains("bad request")
            || msg_lower.contains("400")
        {
            return Self::InvalidRecipient;
        }

        // "invalid" can mean many things - check if it's for recipient
        if msg_lower.contains("invalid") {
            // Distinguish from auth errors that also contain "invalid"
            if !msg_lower.contains("api key") && !msg_lower.contains("credential") {
                return Self::InvalidRecipient;
            }
        }

        // Server errors (5xx)
        if msg_lower.contains("500")
            || msg_lower.contains("502")
            || msg_lower.contains("503")
            || msg_lower.contains("504")
            || msg_lower.contains("server error")
            || msg_lower.contains("internal error")
        {
            return Self::ServerError;
        }

        // Network errors
        if msg_lower.contains("timeout")
            || msg_lower.contains("connection")
            || msg_lower.contains("network")
            || msg_lower.contains("dns")
            || msg_lower.contains("unreachable")
        {
            return Self::NetworkError;
        }

        Self::Unknown
    }

    /// Get a suggested action for resolving this error category.
    pub fn suggested_action(&self) -> &'static str {
        match self {
            Self::Authentication => "Check credentials in environment variables",
            Self::InsufficientFunds => "Add funds to your provider account",
            Self::RateLimit => "Reduce notification frequency or upgrade plan",
            Self::InvalidRecipient => "Check contact phone/email format",
            Self::ServerError => "Provider experiencing issues, retry later",
            Self::NetworkError => "Check network connectivity",
            Self::Unknown => "Review error logs for details",
        }
    }
}

impl fmt::Display for ProviderErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Authentication => write!(f, "Authentication"),
            Self::InsufficientFunds => write!(f, "Insufficient Funds"),
            Self::RateLimit => write!(f, "Rate Limit"),
            Self::InvalidRecipient => write!(f, "Invalid Recipient"),
            Self::ServerError => write!(f, "Server Error"),
            Self::NetworkError => write!(f, "Network Error"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Tracks consecutive notification delivery failures for a single provider.
///
/// This tracker implements throttled alerting:
/// - Alerts are sent only after `ALERT_FAILURE_THRESHOLD` consecutive failures
/// - Only one alert is sent per outage (until recovery)
/// - Recovery alerts are sent when notifications succeed after a failure period
/// - Failure counter resets after `FAILURE_RESET_DURATION` of inactivity
pub struct NotificationFailureTracker {
    /// Provider identifier for logging/display
    #[allow(dead_code)]
    provider_name: String,
    /// Counter for consecutive failures
    consecutive_failures: AtomicU32,
    /// Flag to track if an alert has been sent for the current outage
    alert_sent: AtomicBool,
    /// Last error message for diagnostics
    last_error: RwLock<Option<String>>,
    /// Timestamp of last failure for time-based reset
    last_failure_time: RwLock<Option<Instant>>,
}

impl NotificationFailureTracker {
    /// Create a new tracker for the specified provider.
    pub fn new(provider_name: &str) -> Self {
        Self {
            provider_name: provider_name.to_string(),
            consecutive_failures: AtomicU32::new(0),
            alert_sent: AtomicBool::new(false),
            last_error: RwLock::new(None),
            last_failure_time: RwLock::new(None),
        }
    }

    /// Get the provider name.
    #[allow(dead_code)]
    pub fn provider_name(&self) -> &str {
        &self.provider_name
    }

    /// Record a failed notification delivery.
    ///
    /// Returns a tuple of:
    /// - `should_alert`: true if an admin alert should be sent
    /// - `failure_count`: current number of consecutive failures
    /// - `category`: categorized error type for the alert
    pub async fn record_failure(
        &self,
        error_msg: Option<&str>,
    ) -> (bool, u32, ProviderErrorCategory) {
        // Check for time-based reset
        let should_reset = {
            let last_time = self.last_failure_time.read().await;
            if let Some(last) = *last_time {
                last.elapsed() > FAILURE_RESET_DURATION
            } else {
                false
            }
        };

        if should_reset {
            self.consecutive_failures.store(0, Ordering::SeqCst);
            self.alert_sent.store(false, Ordering::SeqCst);
        }

        // Increment failure counter
        let failures = self.consecutive_failures.fetch_add(1, Ordering::SeqCst) + 1;

        // Update last error and timestamp
        *self.last_error.write().await = error_msg.map(|s| s.to_string());
        *self.last_failure_time.write().await = Some(Instant::now());

        // Categorize the error
        let category = error_msg
            .map(ProviderErrorCategory::from_error)
            .unwrap_or(ProviderErrorCategory::Unknown);

        // Determine if we should alert
        let should_alert =
            failures >= ALERT_FAILURE_THRESHOLD && !self.alert_sent.load(Ordering::SeqCst);

        (should_alert, failures, category)
    }

    /// Record a successful notification delivery.
    ///
    /// Returns true if a recovery alert should be sent (i.e., we had previous failures
    /// and an alert was sent).
    pub fn record_success(&self) -> bool {
        // Reset failure counter
        self.consecutive_failures.store(0, Ordering::SeqCst);

        // Atomically check if alert was sent and reset it
        // Returns true if an alert was previously sent (meaning we should send recovery)
        self.alert_sent
            .compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    /// Mark that an alert has been sent for the current outage.
    pub fn mark_alert_sent(&self) {
        self.alert_sent.store(true, Ordering::SeqCst);
    }

    /// Get the last error message.
    #[allow(dead_code)]
    pub async fn last_error(&self) -> Option<String> {
        self.last_error.read().await.clone()
    }

    /// Get the current consecutive failure count.
    #[allow(dead_code)]
    pub fn get_consecutive_failures(&self) -> u32 {
        self.consecutive_failures.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_categorization() {
        assert_eq!(
            ProviderErrorCategory::from_error("HTTP 401: Unauthorized"),
            ProviderErrorCategory::Authentication
        );
        assert_eq!(
            ProviderErrorCategory::from_error(
                "HTTP 401: {\"code\": 20003, \"message\": \"Authenticate\"}"
            ),
            ProviderErrorCategory::Authentication
        );
        assert_eq!(
            ProviderErrorCategory::from_error("Invalid API key"),
            ProviderErrorCategory::Authentication
        );
        assert_eq!(
            ProviderErrorCategory::from_error("HTTP 402: Insufficient funds"),
            ProviderErrorCategory::InsufficientFunds
        );
        assert_eq!(
            ProviderErrorCategory::from_error("HTTP 429: Rate limit exceeded"),
            ProviderErrorCategory::RateLimit
        );
        assert_eq!(
            ProviderErrorCategory::from_error("HTTP 400: Phone number is not valid"),
            ProviderErrorCategory::InvalidRecipient
        );
        assert_eq!(
            ProviderErrorCategory::from_error("HTTP 500: Internal server error"),
            ProviderErrorCategory::ServerError
        );
        assert_eq!(
            ProviderErrorCategory::from_error("Connection timeout"),
            ProviderErrorCategory::NetworkError
        );
        assert_eq!(
            ProviderErrorCategory::from_error("Some unknown error"),
            ProviderErrorCategory::Unknown
        );
    }

    #[test]
    fn test_suggested_actions() {
        assert!(ProviderErrorCategory::Authentication
            .suggested_action()
            .contains("credentials"));
        assert!(ProviderErrorCategory::InsufficientFunds
            .suggested_action()
            .contains("funds"));
        assert!(ProviderErrorCategory::RateLimit
            .suggested_action()
            .contains("frequency"));
    }

    #[tokio::test]
    async fn test_failure_tracking() {
        let tracker = NotificationFailureTracker::new("test");

        // First two failures should not trigger alert
        let (should_alert, count, _) = tracker.record_failure(Some("HTTP 401")).await;
        assert!(!should_alert);
        assert_eq!(count, 1);

        let (should_alert, count, _) = tracker.record_failure(Some("HTTP 401")).await;
        assert!(!should_alert);
        assert_eq!(count, 2);

        // Third failure should trigger alert
        let (should_alert, count, category) = tracker.record_failure(Some("HTTP 401")).await;
        assert!(should_alert);
        assert_eq!(count, 3);
        assert_eq!(category, ProviderErrorCategory::Authentication);

        // Mark alert sent
        tracker.mark_alert_sent();

        // Fourth failure should NOT trigger alert (already sent)
        let (should_alert, count, _) = tracker.record_failure(Some("HTTP 401")).await;
        assert!(!should_alert);
        assert_eq!(count, 4);
    }

    #[tokio::test]
    async fn test_recovery_notification() {
        let tracker = NotificationFailureTracker::new("test");

        // Simulate failures and alert
        for _ in 0..3 {
            tracker.record_failure(Some("error")).await;
        }
        tracker.mark_alert_sent();

        // Success should trigger recovery notification
        assert!(tracker.record_success());

        // Second success should not trigger recovery (already sent)
        assert!(!tracker.record_success());
    }

    #[test]
    fn test_display_formatting() {
        assert_eq!(
            format!("{}", ProviderErrorCategory::Authentication),
            "Authentication"
        );
        assert_eq!(
            format!("{}", ProviderErrorCategory::InsufficientFunds),
            "Insufficient Funds"
        );
    }
}
