//! Pending contact verifications and checks that a notification target has been verified.

use crate::metadata::pool::MetadataDb;
use anyhow::Result;
use bdk_wallet::rusqlite::{params, OptionalExtension};
use tokio::task::spawn_blocking;

impl MetadataDb {
    pub async fn create_pending_contact_verification(
        &self,
        wallet_checksum: &str,
        provider_type: &str,
        notification_target: &str,
        contact_name: &str,
        verification_code: Option<&str>,
    ) -> Result<i64> {
        let pool = self.pool.clone();
        let wallet_checksum = wallet_checksum.to_string();
        let provider_type = provider_type.to_string();
        let notification_target = notification_target.to_string();
        let contact_name = contact_name.to_string();
        let verification_code = verification_code.map(std::string::ToString::to_string);

        spawn_blocking(move || {
            let conn = pool.get()?;
            let expires_at = chrono::Utc::now() + chrono::Duration::minutes(10);

            conn.execute(
                "INSERT INTO pending_contact_verifications
                 (wallet_checksum, provider_type, notification_target, contact_name, verification_code, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    &wallet_checksum,
                    &provider_type,
                    &notification_target,
                    &contact_name,
                    &verification_code,
                    expires_at.to_rfc3339()
                ],
            )?;
            Ok(conn.last_insert_rowid())
        })
        .await?
    }

    pub async fn get_pending_verification(
        &self,
        wallet_checksum: &str,
        notification_target: &str,
    ) -> Result<Option<(i64, String, Option<String>)>> {
        let pool = self.pool.clone();
        let wallet_checksum = wallet_checksum.to_string();
        let notification_target = notification_target.to_string();

        spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, contact_name, verification_code
                 FROM pending_contact_verifications
                 WHERE wallet_checksum = ?1
                 AND notification_target = ?2
                 AND expires_at > datetime('now')
                 ORDER BY created_at DESC
                 LIMIT 1",
            )?;

            let result = stmt
                .query_row(params![&wallet_checksum, &notification_target], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
                .optional()?;

            Ok(result)
        })
        .await?
    }

    /// Mark a pending verification as completed (used for contact updates)
    /// This keeps the verification record for the PUT endpoint to find
    pub async fn mark_verification_completed(&self, verification_id: i64) -> Result<()> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE pending_contact_verifications
                 SET verified_at = CURRENT_TIMESTAMP
                 WHERE id = ?1",
                params![verification_id],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn cleanup_expired_verifications(&self) -> Result<u64> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            let conn = pool.get()?;
            let deleted_expired = conn.execute(
                "DELETE FROM pending_contact_verifications WHERE expires_at <= datetime('now')",
                [],
            )?;

            // Also clean up old completed verifications (older than 24 hours)
            let deleted_completed = conn.execute(
                "DELETE FROM pending_contact_verifications
                 WHERE verified_at IS NOT NULL
                 AND verified_at <= datetime('now', '-24 hours')",
                [],
            )?;

            Ok((deleted_expired + deleted_completed) as u64)
        })
        .await?
    }

    /// Check if a phone number or email was recently verified for this wallet
    /// Used to ensure security when creating contacts with SMS/email methods
    pub async fn was_recently_verified(
        &self,
        wallet_checksum: &str,
        notification_target: &str,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();
        let target = notification_target.to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM pending_contact_verifications
                 WHERE wallet_checksum = ?1
                 AND notification_target = ?2
                 AND verified_at IS NOT NULL
                 AND verified_at > datetime('now', '-30 minutes')",
                params![checksum, target],
                |row| row.get(0),
            )?;
            Ok(count > 0)
        })
        .await?
    }

    /// Check if a notification target (email or phone) is already used by any wallet owned by this user
    /// This allows skipping OTP verification for cross-wallet contact reuse
    pub async fn is_notification_target_verified_for_user(
        &self,
        user_id: &str,
        provider_type: &str,
        notification_target: &str,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let provider = provider_type.to_string();
        let target = notification_target.to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;

            // Check if this exact notification target exists for any wallet owned by this user
            let count: i64 = if provider == "email" {
                // Case-insensitive comparison for email
                conn.query_row(
                    "SELECT COUNT(*) FROM contact_notification_methods cnm
                     JOIN wallets w ON cnm.wallet_checksum = w.checksum
                     WHERE w.user_id = ?1
                     AND cnm.provider_type = ?2
                     AND LOWER(cnm.notification_target) = LOWER(?3)",
                    params![user_id, provider, target],
                    |row| row.get(0),
                )?
            } else {
                // Exact match for SMS (E.164 format)
                conn.query_row(
                    "SELECT COUNT(*) FROM contact_notification_methods cnm
                     JOIN wallets w ON cnm.wallet_checksum = w.checksum
                     WHERE w.user_id = ?1
                     AND cnm.provider_type = ?2
                     AND cnm.notification_target = ?3",
                    params![user_id, provider, target],
                    |row| row.get(0),
                )?
            };

            Ok(count > 0)
        })
        .await?
    }
}
