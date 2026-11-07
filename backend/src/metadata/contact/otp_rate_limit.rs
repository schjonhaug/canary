//! OTP rate limits: how often a code may be sent to a phone number and how many wrong codes a target may submit.

use crate::metadata::pool::MetadataDb;
use anyhow::Result;
use bdk_wallet::rusqlite::{OptionalExtension, TransactionBehavior, params};

/// Verification attempts allowed per target in a 15-minute window before it is blocked.
const MAX_VERIFICATION_ATTEMPTS: i64 = 5;

impl MetadataDb {
    pub async fn check_rate_limit(&self, phone_number: &str) -> Result<bool> {
        let phone_number = phone_number.to_string();

        self.with_conn(move |conn| {
            let current_time = chrono::Utc::now();
            let current_time_str = current_time.format("%Y-%m-%d %H:%M:%S").to_string();

            // Use transaction to prevent race conditions between reads and writes
            let tx = conn.transaction()?;

            // Check if blocked
            let blocked: Option<String> = tx
                .prepare("SELECT blocked_until FROM otp_attempts WHERE phone_number = ?1")?
                .query_row(params![&phone_number], |row| row.get(0))
                .ok();

            if let Some(blocked_until) = blocked
                && blocked_until > current_time_str {
                    return Ok(false); // Still blocked
                }

            // Check recent attempts (last 15 minutes)
            let fifteen_minutes_ago = (current_time - chrono::Duration::minutes(15))
                .format("%Y-%m-%d %H:%M:%S")
                .to_string();

            let recent_attempts: i32 = tx
                .prepare(
                    "SELECT attempt_count FROM otp_attempts WHERE phone_number = ?1 AND last_attempt > ?2",
                )?
                .query_row(params![&phone_number, &fifteen_minutes_ago], |row| {
                    row.get(0)
                })
                .unwrap_or(0);

            if recent_attempts >= 5 {
                // Block for 30 minutes
                let blocked_until = (current_time + chrono::Duration::minutes(30))
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string();

                tx.execute(
                    "UPDATE otp_attempts SET blocked_until = ?1 WHERE phone_number = ?2",
                    params![&blocked_until, &phone_number],
                )?;

                tx.commit()?;
                return Ok(false);
            }

            // Update attempt count
            let exists: bool = tx
                .prepare("SELECT 1 FROM otp_attempts WHERE phone_number = ?1")?
                .exists(params![&phone_number])?;

            if exists {
                // If the last attempt was not recent (outside 15-min window), reset the counter
                // Otherwise, increment it
                if recent_attempts > 0 {
                    tx.execute(
                        "UPDATE otp_attempts SET attempt_count = attempt_count + 1, last_attempt = ?1 WHERE phone_number = ?2",
                        params![&current_time_str, &phone_number],
                    )?;
                } else {
                    tx.execute(
                        "UPDATE otp_attempts SET attempt_count = 1, last_attempt = ?1, blocked_until = NULL WHERE phone_number = ?2",
                        params![&current_time_str, &phone_number],
                    )?;
                }
            } else {
                tx.execute(
                    "INSERT INTO otp_attempts (phone_number, attempt_count, last_attempt) VALUES (?1, 1, ?2)",
                    params![&phone_number, &current_time_str],
                )?;
            }

            tx.commit()?;
            Ok(true)
        })
        .await
    }

    pub async fn clear_rate_limit(&self, phone_number: &str) -> Result<()> {
        let phone_number = phone_number.to_string();

        self.with_conn(move |conn| {
            conn.execute(
                "DELETE FROM otp_attempts WHERE phone_number = ?1",
                params![&phone_number],
            )?;
            Ok(())
        })
        .await
    }

    /// Charges one code-verification attempt against `notification_target` and returns whether
    /// the attempt may go ahead.
    ///
    /// The attempt is recorded before the code is checked, in one `IMMEDIATE` transaction, so
    /// concurrent requests cannot all pass the limit before any failure is counted. A successful
    /// verification clears the counter with [`Self::clear_verification_rate_limit`].
    /// Limits: five attempts per 15 minutes, then blocked for 30 minutes.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot be read or written. Callers must treat that as
    /// "not allowed".
    pub async fn reserve_verification_attempt(&self, notification_target: &str) -> Result<bool> {
        let target = notification_target.to_string();

        self.with_conn(move |conn| {
            let now = chrono::Utc::now();
            let now_str = now.format("%Y-%m-%d %H:%M:%S").to_string();
            let window_start = (now - chrono::Duration::minutes(15))
                .format("%Y-%m-%d %H:%M:%S")
                .to_string();

            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

            let existing: Option<(i64, Option<String>, Option<String>)> = tx
                .query_row(
                    "SELECT attempt_count, last_attempt, blocked_until
                     FROM otp_verification_attempts
                     WHERE notification_target = ?1",
                    params![&target],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;

            let allowed = match existing {
                Some((_, _, Some(blocked_until))) if blocked_until > now_str => false,
                Some((attempt_count, Some(last_attempt), _)) if last_attempt > window_start => {
                    if attempt_count >= MAX_VERIFICATION_ATTEMPTS {
                        let blocked_until = (now + chrono::Duration::minutes(30))
                            .format("%Y-%m-%d %H:%M:%S")
                            .to_string();
                        tx.execute(
                            "UPDATE otp_verification_attempts SET blocked_until = ?1
                             WHERE notification_target = ?2",
                            params![&blocked_until, &target],
                        )?;
                        false
                    } else {
                        tx.execute(
                            "UPDATE otp_verification_attempts
                             SET attempt_count = attempt_count + 1, last_attempt = ?1
                             WHERE notification_target = ?2",
                            params![&now_str, &target],
                        )?;
                        true
                    }
                }
                Some(_) => {
                    tx.execute(
                        "UPDATE otp_verification_attempts
                         SET attempt_count = 1, last_attempt = ?1, blocked_until = NULL
                         WHERE notification_target = ?2",
                        params![&now_str, &target],
                    )?;
                    true
                }
                None => {
                    tx.execute(
                        "INSERT INTO otp_verification_attempts
                         (notification_target, attempt_count, last_attempt)
                         VALUES (?1, 1, ?2)",
                        params![&target, &now_str],
                    )?;
                    true
                }
            };

            tx.commit()?;
            Ok(allowed)
        })
        .await
    }

    /// Clear verification rate limit on successful verification.
    pub async fn clear_verification_rate_limit(&self, notification_target: &str) -> Result<()> {
        let target = notification_target.to_string();

        self.with_conn(move |conn| {
            conn.execute(
                "DELETE FROM otp_verification_attempts WHERE notification_target = ?1",
                params![&target],
            )?;
            Ok(())
        })
        .await
    }
}
