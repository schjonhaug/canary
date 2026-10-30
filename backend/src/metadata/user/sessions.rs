//! Login sessions and administrator MFA state: session lifetime, role binding and TOTP replay protection.

use crate::metadata::pool::MetadataDb;
use anyhow::Result;
use bdk_wallet::rusqlite::{params, OptionalExtension};
use tokio::task::spawn_blocking;

impl MetadataDb {
    pub async fn create_session(
        &self,
        user_id: &str,
        token_hash: &str,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<String> {
        let pool = self.pool.clone();
        let token_hash = token_hash.to_string();
        let expires_at = expires_at.format("%Y-%m-%d %H:%M:%S").to_string();

        let user_id = user_id.to_string();
        spawn_blocking(move || -> Result<String> {
            let conn = pool.get()?;
            let session_id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO sessions (id, user_id, token_hash, expires_at) VALUES (?1, ?2, ?3, ?4)",
                params![&session_id, &user_id, &token_hash, &expires_at],
            )?;
            Ok(session_id)
        })
        .await?
    }

    pub async fn delete_session(&self, token_hash: &str) -> Result<()> {
        let pool = self.pool.clone();
        let token_hash = token_hash.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "DELETE FROM sessions WHERE token_hash = ?1",
                params![&token_hash],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn delete_user_sessions(&self, user_id: &str) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute("DELETE FROM sessions WHERE user_id = ?1", params![&user_id])?;
            Ok(())
        })
        .await?
    }

    pub async fn has_active_session(&self, token_hash: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let token_hash = token_hash.to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let current_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            let exists = conn.query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM sessions
                    WHERE token_hash = ?1 AND expires_at >= ?2
                )",
                params![&token_hash, &current_time],
                |row| row.get::<_, i64>(0),
            )?;
            Ok(exists != 0)
        })
        .await?
    }

    /// Bind a session to its subject and current role flags, not only its token hash.
    pub async fn has_active_session_for_user(
        &self,
        token_hash: &str,
        user_id: &str,
        is_admin: bool,
        is_demo: bool,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let token_hash = token_hash.to_string();
        let user_id = user_id.to_string();
        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            Ok(conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM sessions s JOIN users u ON u.id = s.user_id
                 WHERE s.token_hash = ?1 AND s.user_id = ?2 AND s.expires_at >= datetime('now')
                 AND u.is_admin = ?3 AND u.is_demo = ?4)",
                params![token_hash, user_id, is_admin, is_demo],
                |row| row.get(0),
            )?)
        })
        .await?
    }

    /// Atomically reject repeated or older TOTP steps, including parallel requests.
    pub async fn consume_admin_mfa_step(
        &self,
        user_id: &str,
        version: &str,
        step: i64,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let version = version.to_string();
        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let changed = conn.execute(
                "INSERT INTO admin_mfa_replay (user_id, key_version, last_step) VALUES (?1, ?2, ?3)
                 ON CONFLICT (user_id, key_version) DO UPDATE SET last_step = excluded.last_step
                 WHERE admin_mfa_replay.last_step < excluded.last_step",
                params![user_id, version, step],
            )?;
            Ok(changed == 1)
        })
        .await?
    }

    pub async fn verify_admin_session(
        &self,
        user_id: &str,
        token_hash: &str,
        version: &str,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let token_hash = token_hash.to_string();
        let version = version.to_string();
        spawn_blocking(move || -> Result<()> {
            let mut conn = pool.get()?;
            let tx = conn.transaction()?;
            let changed = tx.execute(
                "UPDATE sessions SET admin_mfa_verified_at = unixepoch(), admin_mfa_key_version = ?3
                 WHERE token_hash = ?1 AND user_id = ?2
                 AND EXISTS (SELECT 1 FROM users WHERE id = ?2 AND is_admin = 1 AND is_demo = 0)",
                params![token_hash, user_id, version],
            )?;
            if changed != 1 {
                return Err(anyhow::anyhow!("Administrator session unavailable"));
            }
            tx.execute(
                "UPDATE admin_support_grants SET revoked_at = unixepoch()
                 WHERE actor_user_id = ?1 AND revoked_at IS NULL AND expires_at > unixepoch()",
                params![user_id],
            )?;
            tx.execute(
                "INSERT INTO admin_audit_log (id, actor_user_id, operation, target, details_json)
                 VALUES (?1, ?2, 'admin_mfa_login', 'own_session',
                 ?3)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    user_id,
                    serde_json::json!({"reason":"administrator sign-in", "result":"verified"})
                        .to_string()
                ],
            )?;
            tx.commit()?;
            Ok(())
        })
        .await?
    }

    pub async fn remaining_admin_mfa_seconds(
        &self,
        user_id: &str,
        max_age: i64,
    ) -> Result<Option<i64>> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        spawn_blocking(move || -> Result<Option<i64>> {
            let conn = pool.get()?;
            conn.query_row(
                "SELECT admin_mfa_verified_at + ?2 - unixepoch()
                 FROM sessions
                 WHERE user_id = ?1
                   AND admin_mfa_verified_at BETWEEN unixepoch() - ?2 AND unixepoch()
                 ORDER BY admin_mfa_verified_at DESC LIMIT 1",
                params![user_id, max_age],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
        })
        .await?
    }

    pub async fn has_recent_admin_mfa_session(
        &self,
        token_hash: &str,
        version: &str,
        max_age: i64,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let token_hash = token_hash.to_string();
        let version = version.to_string();
        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            Ok(conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM sessions WHERE token_hash = ?1
                 AND admin_mfa_key_version = ?2
                 AND admin_mfa_verified_at BETWEEN unixepoch() - ?3 AND unixepoch())",
                params![token_hash, version, max_age],
                |row| row.get(0),
            )?)
        })
        .await?
    }

    pub async fn cleanup_expired_sessions(&self) -> Result<u64> {
        let pool = self.pool.clone();

        spawn_blocking(move || -> Result<u64> {
            let conn = pool.get()?;
            let current_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            let rows_deleted = conn.execute(
                "DELETE FROM sessions WHERE expires_at < ?1",
                params![&current_time],
            )?;
            Ok(rows_deleted as u64)
        })
        .await?
    }
}
