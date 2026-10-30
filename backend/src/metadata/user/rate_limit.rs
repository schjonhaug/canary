//! Endpoint rate limiting keyed by scope and identifier, stored in `auth_rate_limits`.

use crate::metadata::pool::MetadataDb;
use crate::metadata::types::RateLimitDecision;
use anyhow::Result;
use bdk_wallet::rusqlite::{params, OptionalExtension, TransactionBehavior};
use tokio::task::spawn_blocking;

impl MetadataDb {
    /// Check whether an endpoint identifier is currently blocked without recording an attempt.
    pub async fn is_endpoint_rate_limited(&self, scope: &str, identifier: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let scope = scope.to_string();
        let identifier = identifier.trim().to_lowercase();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let blocked: Option<Option<String>> = conn
                .query_row(
                    "SELECT blocked_until FROM auth_rate_limits WHERE scope = ?1 AND identifier = ?2",
                    params![scope, identifier],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?;
            Ok(blocked.flatten().is_some_and(|until| until > now))
        })
        .await?
    }

    /// Check and record a rate-limited endpoint attempt.
    /// Returns the allow/deny decision plus retry guidance when blocked.
    pub async fn check_endpoint_rate_limit(
        &self,
        scope: &str,
        identifier: &str,
        max_attempts: i64,
        window_minutes: i64,
    ) -> Result<RateLimitDecision> {
        let pool = self.pool.clone();
        let scope = scope.to_string();
        let identifier = identifier.trim().to_lowercase();
        let now = chrono::Utc::now();
        let now_naive = now.naive_utc();
        let now_str = now.format("%Y-%m-%d %H:%M:%S").to_string();
        let window_start = (now - chrono::Duration::minutes(window_minutes))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        let blocked_until = (now + chrono::Duration::minutes(window_minutes))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        spawn_blocking(move || -> Result<RateLimitDecision> {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

            let existing: Option<(i64, String, Option<String>)> = tx
                .prepare(
                    "SELECT attempt_count, first_attempt_at, blocked_until
                     FROM auth_rate_limits
                     WHERE scope = ?1 AND identifier = ?2",
                )?
                .query_row(params![&scope, &identifier], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
                .optional()?;

            let decision = match existing {
                Some((_, _, Some(blocked))) if blocked > now_str => {
                    let retry_after_seconds = chrono::NaiveDateTime::parse_from_str(
                        &blocked,
                        "%Y-%m-%d %H:%M:%S",
                    )
                    .map_or(window_minutes * 60, |blocked_until| (blocked_until - now_naive).num_seconds().max(1));

                    RateLimitDecision {
                        allowed: false,
                        retry_after_seconds: Some(retry_after_seconds),
                    }
                }
                Some((_, _, Some(_))) => {
                    tx.execute(
                        "UPDATE auth_rate_limits
                         SET attempt_count = 1, first_attempt_at = ?3, blocked_until = NULL
                         WHERE scope = ?1 AND identifier = ?2",
                        params![&scope, &identifier, &now_str],
                    )?;
                    RateLimitDecision {
                        allowed: true,
                        retry_after_seconds: None,
                    }
                }
                Some((attempt_count, first_attempt_at, _)) if first_attempt_at >= window_start => {
                    let next_attempt_count = attempt_count + 1;
                    if next_attempt_count > max_attempts {
                        tx.execute(
                            "UPDATE auth_rate_limits
                             SET attempt_count = ?3, first_attempt_at = ?4, blocked_until = ?5
                             WHERE scope = ?1 AND identifier = ?2",
                            params![
                                &scope,
                                &identifier,
                                max_attempts,
                                &now_str,
                                &blocked_until
                            ],
                        )?;
                        RateLimitDecision {
                            allowed: false,
                            retry_after_seconds: Some(window_minutes * 60),
                        }
                    } else {
                        tx.execute(
                            "UPDATE auth_rate_limits
                             SET attempt_count = ?3, blocked_until = NULL
                             WHERE scope = ?1 AND identifier = ?2",
                            params![&scope, &identifier, next_attempt_count],
                        )?;
                        RateLimitDecision {
                            allowed: true,
                            retry_after_seconds: None,
                        }
                    }
                }
                Some(_) => {
                    tx.execute(
                        "UPDATE auth_rate_limits
                         SET attempt_count = 1, first_attempt_at = ?3, blocked_until = NULL
                         WHERE scope = ?1 AND identifier = ?2",
                        params![&scope, &identifier, &now_str],
                    )?;
                    RateLimitDecision {
                        allowed: true,
                        retry_after_seconds: None,
                    }
                }
                None => {
                    tx.execute(
                        "INSERT INTO auth_rate_limits (scope, identifier, attempt_count, first_attempt_at)
                         VALUES (?1, ?2, 1, ?3)",
                        params![&scope, &identifier, &now_str],
                    )?;
                    RateLimitDecision {
                        allowed: true,
                        retry_after_seconds: None,
                    }
                }
            };

            tx.commit()?;
            Ok(decision)
        })
        .await?
    }

    /// Check and record a rate-limited auth endpoint attempt.
    /// Returns Ok(true) when the request should proceed and Ok(false) when blocked.
    pub async fn check_auth_rate_limit(
        &self,
        scope: &str,
        identifier: &str,
        max_attempts: i64,
        window_minutes: i64,
    ) -> Result<bool> {
        Ok(self
            .check_endpoint_rate_limit(scope, identifier, max_attempts, window_minutes)
            .await?
            .allowed)
    }
}
