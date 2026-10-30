//! Email verification and password reset tokens, and the password updates that consume them.

use crate::metadata::pool::MetadataDb;
use anyhow::Result;
use bdk_wallet::rusqlite::params;
use tokio::task::spawn_blocking;

impl MetadataDb {
    /// Creates an email verification token. The token should be pre-hashed by the caller
    /// using `AuthService::hash_token()` for security.
    pub async fn create_email_verification_token(&self, user_id: &str, token: &str) -> Result<()> {
        let pool = self.pool.clone();
        let token = token.to_string();
        let expires_at = (chrono::Utc::now() + chrono::Duration::hours(24))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        let user_id = user_id.to_string();
        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO email_verification_tokens (user_id, token, expires_at) VALUES (?1, ?2, ?3)",
                params![&user_id, &token, &expires_at],
            )?;
            Ok(())
        }).await?
    }

    /// Verifies an email verification token. The token should be pre-hashed by the caller
    /// using `AuthService::hash_token()` for security.
    pub async fn verify_email_token(&self, token: &str) -> Result<Option<String>> {
        let pool = self.pool.clone();
        let token = token.to_string();
        let current_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        tracing::debug!("Verifying email token");

        spawn_blocking(move || -> Result<Option<String>> {
            let conn = pool.get()?;
            let tx = conn.unchecked_transaction()?;

            // Get user_id for valid, non-expired token
            let user_id: Option<String> = tx
                .prepare("SELECT user_id FROM email_verification_tokens WHERE token = ?1 AND expires_at > ?2")?
                .query_row(params![&token, &current_time], |row| row.get(0))
                .ok();

            tracing::debug!(matched = user_id.is_some(), "Email verification token lookup completed");

            if let Some(user_id) = user_id {
                // Mark user as verified
                tx.execute(
                    "UPDATE users SET email_verified = TRUE WHERE id = ?1",
                    params![user_id],
                )?;

                // Delete the token
                tx.execute(
                    "DELETE FROM email_verification_tokens WHERE token = ?1",
                    params![&token],
                )?;

                tx.commit()?;
                Ok(Some(user_id))
            } else {
                tx.rollback()?;
                Ok(None)
            }
        }).await?
    }

    pub async fn create_password_reset_token(&self, user_id: &str, token: &str) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let token = token.to_string();
        let expires_at = (chrono::Utc::now() + chrono::Duration::hours(1))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;

            // Delete any existing tokens for this user
            conn.execute(
                "DELETE FROM password_reset_tokens WHERE user_id = ?1",
                params![&user_id],
            )?;

            // Create new token
            conn.execute(
                "INSERT INTO password_reset_tokens (user_id, token, expires_at) VALUES (?1, ?2, ?3)",
                params![&user_id, &token, &expires_at],
            )?;
            Ok(())
        }).await?
    }

    pub async fn verify_password_reset_token(&self, token: &str) -> Result<Option<String>> {
        let pool = self.pool.clone();
        let token = token.to_string();
        let current_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        spawn_blocking(move || -> Result<Option<String>> {
            let conn = pool.get()?;
            let user_id: Option<String> = conn
                .prepare("SELECT user_id FROM password_reset_tokens WHERE token = ?1 AND expires_at > ?2")?
                .query_row(params![&token, &current_time], |row| row.get(0))
                .ok();
            Ok(user_id)
        }).await?
    }

    pub async fn update_user_password(&self, user_id: &str, password_hash: &str) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let password_hash = password_hash.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            let tx = conn.unchecked_transaction()?;

            // Update password
            tx.execute(
                "UPDATE users SET password_hash = ?1 WHERE id = ?2",
                params![&password_hash, &user_id],
            )?;

            // Delete all password reset tokens for this user
            tx.execute(
                "DELETE FROM password_reset_tokens WHERE user_id = ?1",
                params![&user_id],
            )?;

            tx.commit()?;
            Ok(())
        })
        .await?
    }

    pub async fn update_user_password_and_revoke_sessions(
        &self,
        user_id: &str,
        password_hash: &str,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let password_hash = password_hash.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            let tx = conn.unchecked_transaction()?;

            tx.execute(
                "UPDATE users SET password_hash = ?1 WHERE id = ?2",
                params![&password_hash, &user_id],
            )?;
            tx.execute(
                "DELETE FROM password_reset_tokens WHERE user_id = ?1",
                params![&user_id],
            )?;
            tx.execute("DELETE FROM sessions WHERE user_id = ?1", params![&user_id])?;

            tx.commit()?;
            Ok(())
        })
        .await?
    }
}
