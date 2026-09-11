//! Cloud administrator second-factor validation. Enrollment is operator-controlled.
use crate::auth::AuthService;
use crate::metadata::MetadataDb;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use totp_rs::{Builder, Secret, Totp};

pub const MAX_ADMIN_SESSION_AGE_SECONDS: i64 = 15 * 60;

fn secret_path() -> PathBuf {
    std::env::var_os("CANARY_ADMIN_MFA_SECRETS_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/run/secrets/canary-admin-mfa.json"))
}

fn load_factor(path: &Path, user_id: &str) -> Result<(Totp, String)> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() > 64 * 1024 {
        return Err(anyhow!("Invalid administrator MFA configuration"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // 0600 for a single-owner runtime, or 0640 so root can own the file while
        // the backend group reads it. World access is never allowed.
        let mode = metadata.permissions().mode() & 0o777;
        if mode != 0o600 && mode != 0o640 {
            return Err(anyhow!("Administrator MFA configuration must be private"));
        }
    }
    let entries: HashMap<String, String> = serde_json::from_slice(&std::fs::read(path)?)?;
    let encoded = entries
        .get(user_id)
        .ok_or_else(|| anyhow!("Administrator not enrolled"))?;
    let secret = Secret::try_from_base32(encoded)
        .map_err(|_| anyhow!("Invalid administrator MFA configuration"))?;
    if secret.as_bytes().len() < 20 {
        return Err(anyhow!("Administrator MFA secret is too short"));
    }
    // High-entropy key fingerprint binds sessions and replay counters to enrollment.
    let version = AuthService::hash_token(&hex::encode(secret.as_bytes()));
    let factor = Builder::new()
        .with_secret(secret)
        .with_skew(1)
        .build()
        .map_err(|_| anyhow!("Invalid administrator MFA configuration"))?;
    Ok((factor, version))
}

async fn enrolled_factor(user_id: &str) -> Result<(Totp, String)> {
    let user_id = user_id.to_string();
    let path = secret_path();
    tokio::task::spawn_blocking(move || load_factor(&path, &user_id)).await?
}

pub async fn verify(db: &MetadataDb, user_id: &str, code: &str) -> Result<Option<String>> {
    if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Ok(None);
    }
    let (factor, version) = enrolled_factor(user_id).await?;
    let now = u64::try_from(chrono::Utc::now().timestamp())?;
    let Some(step) = factor.check(code, now) else {
        return Ok(None);
    };
    if db
        .consume_admin_mfa_step(user_id, &version, i64::try_from(step)?)
        .await?
    {
        Ok(Some(version))
    } else {
        Ok(None)
    }
}

pub async fn session_is_recent(db: &MetadataDb, user_id: &str, token_hash: &str) -> Result<bool> {
    let (_, version) = enrolled_factor(user_id).await?;
    db.has_recent_admin_mfa_session(token_hash, &version, MAX_ADMIN_SESSION_AGE_SECONDS)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rfc_vector_and_secret_file_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mfa.json");
        let secret = Secret::new(Box::from(*b"12345678901234567890"));
        std::fs::write(
            &path,
            serde_json::json!({"synthetic":secret.to_base32()}).to_string(),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let (factor, _) = load_factor(&path, "synthetic").unwrap();
        assert_eq!(factor.generate(59).to_string(), "287082");
        assert!(factor.check("287082", 59).is_some());
        assert!(factor.check("287082", 359).is_none());
        assert!(load_factor(&path, "unenrolled").is_err());
        let link = directory.path().join("link.json");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(load_factor(&link, "synthetic").is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        assert!(load_factor(&path, "synthetic").is_ok());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(load_factor(&path, "synthetic").is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::write(&path, "x".repeat(65 * 1024)).unwrap();
        assert!(load_factor(&path, "synthetic").is_err());
    }
}
