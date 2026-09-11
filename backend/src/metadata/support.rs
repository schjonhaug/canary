use super::pool::MetadataDb;
use super::types::AdminSupportGrant;
use anyhow::Result;
use bdk_wallet::rusqlite::{params, OptionalExtension};
use serde_json::json;
use tokio::task::spawn_blocking;
use uuid::Uuid;

impl MetadataDb {
    pub async fn grant_admin_support_access(
        &self,
        actor_user_id: &str,
        target_user_id: &str,
        reason: &str,
        ttl_seconds: i64,
    ) -> Result<AdminSupportGrant> {
        let pool = self.pool.clone();
        let actor_user_id = actor_user_id.to_string();
        let target_user_id = target_user_id.to_string();
        let reason = reason.to_string();
        spawn_blocking(move || -> Result<AdminSupportGrant> {
            let mut conn = pool.get()?;
            let tx = conn.transaction()?;
            tx.execute(
                "UPDATE admin_support_grants SET revoked_at = unixepoch()
                 WHERE actor_user_id = ?1 AND revoked_at IS NULL AND expires_at > unixepoch()",
                params![&actor_user_id],
            )?;
            let id = Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO admin_support_grants
                 (id, actor_user_id, target_user_id, reason, created_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, unixepoch(), unixepoch() + ?5)",
                params![&id, &actor_user_id, &target_user_id, &reason, ttl_seconds],
            )?;
            let grant = tx.query_row(
                "SELECT id, actor_user_id, target_user_id, reason, created_at, expires_at
                 FROM admin_support_grants WHERE id = ?1",
                params![&id],
                |row| {
                    Ok(AdminSupportGrant {
                        id: row.get(0)?,
                        actor_user_id: row.get(1)?,
                        target_user_id: row.get(2)?,
                        reason: row.get(3)?,
                        created_at: row.get(4)?,
                        expires_at: row.get(5)?,
                    })
                },
            )?;
            tx.execute(
                "INSERT INTO admin_audit_log (id, actor_user_id, operation, target, details_json)
                 VALUES (?1, ?2, 'admin_support_access', ?3, ?4)",
                params![
                    Uuid::new_v4().to_string(),
                    actor_user_id,
                    target_user_id,
                    json!({
                        "reason": reason,
                        "result": "granted",
                        "expires_at": grant.expires_at
                    })
                    .to_string()
                ],
            )?;
            tx.commit()?;
            Ok(grant)
        })
        .await?
    }

    pub async fn active_admin_support_grant(
        &self,
        actor_user_id: &str,
    ) -> Result<Option<AdminSupportGrant>> {
        let pool = self.pool.clone();
        let actor_user_id = actor_user_id.to_string();
        spawn_blocking(move || -> Result<Option<AdminSupportGrant>> {
            let conn = pool.get()?;
            conn.query_row(
                "SELECT id, actor_user_id, target_user_id, reason, created_at, expires_at
                 FROM admin_support_grants
                 WHERE actor_user_id = ?1 AND revoked_at IS NULL AND expires_at > unixepoch()
                 ORDER BY created_at DESC LIMIT 1",
                params![actor_user_id],
                |row| {
                    Ok(AdminSupportGrant {
                        id: row.get(0)?,
                        actor_user_id: row.get(1)?,
                        target_user_id: row.get(2)?,
                        reason: row.get(3)?,
                        created_at: row.get(4)?,
                        expires_at: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
        })
        .await?
    }

    pub async fn has_active_admin_support_grant(
        &self,
        actor_user_id: &str,
        target_user_id: &str,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let actor_user_id = actor_user_id.to_string();
        let target_user_id = target_user_id.to_string();
        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            conn.query_row(
                "SELECT 1 FROM admin_support_grants
                 WHERE actor_user_id = ?1 AND target_user_id = ?2
                   AND revoked_at IS NULL AND expires_at > unixepoch()
                 LIMIT 1",
                params![actor_user_id, target_user_id],
                |_| Ok(true),
            )
            .optional()
            .map(|row| row.unwrap_or(false))
            .map_err(Into::into)
        })
        .await?
    }

    pub async fn revoke_admin_support_grants(&self, actor_user_id: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let actor_user_id = actor_user_id.to_string();
        spawn_blocking(move || -> Result<bool> {
            let mut conn = pool.get()?;
            let tx = conn.transaction()?;
            let changed = tx.execute(
                "UPDATE admin_support_grants SET revoked_at = unixepoch()
                 WHERE actor_user_id = ?1 AND revoked_at IS NULL AND expires_at > unixepoch()",
                params![&actor_user_id],
            )?;
            if changed > 0 {
                tx.execute(
                    "INSERT INTO admin_audit_log (id, actor_user_id, operation, target, details_json)
                     VALUES (?1, ?2, 'admin_support_access', 'own_session', ?3)",
                    params![
                        Uuid::new_v4().to_string(),
                        actor_user_id,
                        json!({"result": "revoked"}).to_string()
                    ],
                )?;
            }
            tx.commit()?;
            Ok(changed > 0)
        })
        .await?
    }
}
