//! OTP rate limits: how often a code may be sent to a phone number and how many wrong codes a target may submit.

use crate::metadata::pool::MetadataDb;
use anyhow::Result;
use bdk_wallet::rusqlite::params;

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

    /// Check and increment verification attempt counter.
    /// Returns Ok(true) if allowed to proceed, Ok(false) if rate limited.
    /// Limits: 5 failed attempts per 15 minutes, then blocked for 30 minutes.
    pub async fn check_verification_rate_limit(&self, notification_target: &str) -> Result<bool> {
        let target = notification_target.to_string();

        self.with_conn(move |conn| {
            let current_time = chrono::Utc::now();
            let current_time_str = current_time.format("%Y-%m-%d %H:%M:%S").to_string();

            let tx = conn.transaction()?;

            // Check if currently blocked
            let blocked: Option<String> = tx
                .prepare(
                    "SELECT blocked_until FROM otp_verification_attempts WHERE notification_target = ?1",
                )?
                .query_row(params![&target], |row| row.get(0))
                .ok();

            if let Some(blocked_until) = blocked
                && blocked_until > current_time_str {
                    tx.commit()?;
                    return Ok(false); // Still blocked
                }

            // Check recent failed attempts (last 15 minutes)
            let fifteen_minutes_ago = (current_time - chrono::Duration::minutes(15))
                .format("%Y-%m-%d %H:%M:%S")
                .to_string();

            let recent_attempts: i32 = tx
                .prepare(
                    "SELECT attempt_count FROM otp_verification_attempts
                     WHERE notification_target = ?1 AND last_attempt > ?2",
                )?
                .query_row(params![&target, &fifteen_minutes_ago], |row| row.get(0))
                .unwrap_or(0);

            if recent_attempts >= 5 {
                // Already at limit, block for 30 minutes
                let blocked_until = (current_time + chrono::Duration::minutes(30))
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string();

                tx.execute(
                    "UPDATE otp_verification_attempts SET blocked_until = ?1
                     WHERE notification_target = ?2",
                    params![&blocked_until, &target],
                )?;

                tx.commit()?;
                return Ok(false);
            }

            tx.commit()?;
            Ok(true)
        })
        .await
    }

    /// Record a failed OTP verification attempt.
    pub async fn record_failed_verification_attempt(
        &self,
        notification_target: &str,
    ) -> Result<()> {
        let target = notification_target.to_string();

        self.with_conn(move |conn| {
            let current_time = chrono::Utc::now();
            let current_time_str = current_time.format("%Y-%m-%d %H:%M:%S").to_string();
            let fifteen_minutes_ago = (current_time - chrono::Duration::minutes(15))
                .format("%Y-%m-%d %H:%M:%S")
                .to_string();

            let tx = conn.transaction()?;

            let exists: bool = tx
                .prepare("SELECT 1 FROM otp_verification_attempts WHERE notification_target = ?1")?
                .exists(params![&target])?;

            if exists {
                // Check if last attempt was within the window
                let recent_attempts: i32 = tx
                    .prepare(
                        "SELECT attempt_count FROM otp_verification_attempts
                         WHERE notification_target = ?1 AND last_attempt > ?2",
                    )?
                    .query_row(params![&target, &fifteen_minutes_ago], |row| row.get(0))
                    .unwrap_or(0);

                if recent_attempts > 0 {
                    // Within window, increment
                    tx.execute(
                        "UPDATE otp_verification_attempts
                         SET attempt_count = attempt_count + 1, last_attempt = ?1
                         WHERE notification_target = ?2",
                        params![&current_time_str, &target],
                    )?;
                } else {
                    // Outside window, reset counter
                    tx.execute(
                        "UPDATE otp_verification_attempts
                         SET attempt_count = 1, last_attempt = ?1, blocked_until = NULL
                         WHERE notification_target = ?2",
                        params![&current_time_str, &target],
                    )?;
                }
            } else {
                tx.execute(
                    "INSERT INTO otp_verification_attempts
                     (notification_target, attempt_count, last_attempt)
                     VALUES (?1, 1, ?2)",
                    params![&target, &current_time_str],
                )?;
            }

            tx.commit()?;
            Ok(())
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
