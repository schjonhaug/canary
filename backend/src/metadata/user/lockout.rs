//! Login attempt history and account lockout after repeated failed sign-ins.

use crate::metadata::pool::MetadataDb;
use anyhow::Result;
use bdk_wallet::rusqlite::{params, OptionalExtension};

impl MetadataDb {
    /// Check if an account is currently locked due to too many failed login attempts
    /// Returns the lockout end time if locked, None if not locked
    pub async fn check_account_lockout(&self, email: &str) -> Result<Option<String>> {
        let email = email.to_string();
        let current_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        self.with_conn(move |conn| {
            let locked_until: Option<String> = conn
                .prepare("SELECT locked_until FROM users WHERE email = ?1 AND locked_until > ?2")?
                .query_row(params![&email, &current_time], |row| row.get(0))
                .optional()?;
            Ok(locked_until)
        })
        .await
    }

    /// Check if lockout has expired and clear it if so
    /// Returns true if an expired lockout was cleared, false otherwise
    pub async fn clear_expired_lockout(&self, email: &str) -> Result<bool> {
        let email = email.to_string();
        let current_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        self.with_conn(move |conn| {
            // Check if there's an expired lockout (locked_until is set but in the past)
            let has_expired_lockout: bool = conn
                .prepare(
                    "SELECT 1 FROM users WHERE email = ?1 AND locked_until IS NOT NULL AND locked_until <= ?2",
                )?
                .query_row(params![&email, &current_time], |_| Ok(true))
                .optional()?
                .unwrap_or(false);

            if has_expired_lockout {
                // Clear the expired lockout and reset counter
                conn.execute(
                    "UPDATE users SET failed_login_attempts = 0, locked_until = NULL WHERE email = ?1",
                    params![&email],
                )?;
                Ok(true)
            } else {
                Ok(false)
            }
        })
        .await
    }

    /// Record a login attempt (successful or failed)
    pub async fn record_login_attempt(&self, email: &str, success: bool) -> Result<()> {
        let email = email.to_string();

        self.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO login_attempts (email, success) VALUES (?1, ?2)",
                params![&email, success],
            )?;
            Ok(())
        })
        .await
    }

    /// Get the count of failed login attempts in the last N minutes
    pub async fn get_failed_login_count(&self, email: &str, window_minutes: i64) -> Result<i64> {
        let email = email.to_string();
        let window_start = (chrono::Utc::now() - chrono::Duration::minutes(window_minutes))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        self.with_conn(move |conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM login_attempts WHERE email = ?1 AND attempt_time > ?2 AND success = FALSE",
                params![&email, &window_start],
                |row| row.get(0),
            )?;
            Ok(count)
        })
        .await
    }

    /// Lock a user account for a specified duration
    pub async fn lock_account(&self, email: &str, lock_duration_minutes: i64) -> Result<()> {
        let email = email.to_string();
        let locked_until = (chrono::Utc::now() + chrono::Duration::minutes(lock_duration_minutes))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        self.with_conn(move |conn| {
            conn.execute(
                "UPDATE users SET locked_until = ?1 WHERE email = ?2",
                params![&locked_until, &email],
            )?;
            Ok(())
        })
        .await
    }

    /// Increment failed login counter for a user
    pub async fn increment_failed_login_count(&self, email: &str) -> Result<i64> {
        let email = email.to_string();

        self.with_conn(move |conn| {
            conn.execute(
                "UPDATE users SET failed_login_attempts = COALESCE(failed_login_attempts, 0) + 1 WHERE email = ?1",
                params![&email],
            )?;
            let count: i64 = conn.query_row(
                "SELECT COALESCE(failed_login_attempts, 0) FROM users WHERE email = ?1",
                params![&email],
                |row| row.get(0),
            )?;
            Ok(count)
        })
        .await
    }

    /// Reset failed login counter on successful login
    pub async fn reset_failed_login_count(&self, email: &str) -> Result<()> {
        let email = email.to_string();

        self.with_conn(move |conn| {
            conn.execute(
                "UPDATE users SET failed_login_attempts = 0, locked_until = NULL WHERE email = ?1",
                params![&email],
            )?;
            Ok(())
        })
        .await
    }

    /// Clean up old login attempts (older than N days)
    pub async fn cleanup_old_login_attempts(&self, days: i64) -> Result<u64> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        self.with_conn(move |conn| {
            let deleted = conn.execute(
                "DELETE FROM login_attempts WHERE attempt_time < ?1",
                params![&cutoff],
            )?;
            Ok(deleted as u64)
        })
        .await
    }
}
