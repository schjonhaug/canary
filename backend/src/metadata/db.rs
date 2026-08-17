use super::pool::MetadataDb;
use crate::electrum::BlockHeader;
use crate::exchange_rates;
use crate::stripe_billing::SubscriptionUpdate;
use anyhow::Result;
use bdk_wallet::rusqlite::{params, OptionalExtension};
use std::sync::atomic::{AtomicI64, Ordering};
use tokio::task::spawn_blocking;

static LAST_STRIPE_EVENT_CLEANUP: AtomicI64 = AtomicI64::new(0);
pub enum StripeEventClaim {
    Claimed(String),
    Active,
    Processed,
}

impl MetadataDb {
    pub async fn claim_stripe_event(&self, event_id: &str) -> Result<StripeEventClaim> {
        let pool = self.pool.clone();
        let event_id = event_id.to_string();
        let claim_token = uuid::Uuid::new_v4().to_string();

        spawn_blocking(move || {
            let conn = pool.get()?;
            let now = chrono::Utc::now().timestamp();
            let last_cleanup = LAST_STRIPE_EVENT_CLEANUP.load(Ordering::Relaxed);
            if now - last_cleanup >= 24 * 60 * 60
                && LAST_STRIPE_EVENT_CLEANUP
                    .compare_exchange(last_cleanup, now, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
            {
                // Stripe retries deliveries for at most three days; retain a wider replay window.
                conn.execute(
                    "DELETE FROM stripe_trial_ending_notifications
                     WHERE event_id IN (
                         SELECT event_id FROM processed_stripe_events
                         WHERE processed_at <= datetime('now', '-30 days')
                     )",
                    [],
                )?;
                conn.execute(
                    "DELETE FROM processed_stripe_events
                     WHERE processed_at <= datetime('now', '-30 days')",
                    [],
                )?;
            }
            let claimed = conn.execute(
                "INSERT INTO processed_stripe_events (event_id, claim_token, claimed_at)
                 VALUES (?1, ?2, CURRENT_TIMESTAMP)
                 ON CONFLICT(event_id) DO UPDATE SET claim_token = ?2, claimed_at = CURRENT_TIMESTAMP
                 WHERE processed_at IS NULL
                 AND claimed_at <= datetime('now', '-5 minutes')",
                params![&event_id, &claim_token],
            )? == 1;
            if claimed {
                return Ok(StripeEventClaim::Claimed(claim_token));
            }

            let processed: Option<bool> = conn
                .query_row(
                "SELECT processed_at IS NOT NULL FROM processed_stripe_events WHERE event_id = ?1",
                params![event_id],
                |row| row.get(0),
            )
                .optional()?;
            Ok(if processed == Some(true) {
                StripeEventClaim::Processed
            } else {
                StripeEventClaim::Active
            })
        })
        .await?
    }

    /// Atomically persist subscription changes, queue notification delivery, and complete the event.
    pub async fn complete_stripe_event_with_subscription_updates(
        &self,
        event_id: &str,
        claim_token: &str,
        updates: &[SubscriptionUpdate],
        trial_ending_notifications: &[crate::stripe_billing::TrialEndingNotification],
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let event_id = event_id.to_string();
        let claim_token = claim_token.to_string();
        let updates = updates.to_vec();
        let trial_ending_notifications = trial_ending_notifications.to_vec();

        spawn_blocking(move || {
            let mut conn = pool.get()?;
            let transaction = conn.transaction()?;

            for update in updates {
                if update.subscription_tier == "keep_current" {
                    if update.subscription_status == "expired"
                        && update.stripe_subscription_id.is_none()
                    {
                        transaction.execute(
                            "UPDATE users SET subscription_status = 'expired', stripe_subscription_id = NULL WHERE id = ?1",
                            params![update.user_id],
                        )?;
                    } else {
                        transaction.execute(
                            "UPDATE users SET subscription_status = ?1,
                                stripe_subscription_id = COALESCE(?2, stripe_subscription_id)
                             WHERE id = ?3",
                            params![update.subscription_status, update.stripe_subscription_id, update.user_id],
                        )?;
                    }
                } else {
                    transaction.execute(
                        "UPDATE users SET
                            subscription_tier = ?1,
                            subscription_status = ?2,
                            stripe_subscription_id = ?3,
                            subscription_started_at = ?4,
                            subscription_ends_at = ?5,
                            trial_ends_at = COALESCE(?6, trial_ends_at)
                         WHERE id = ?7",
                        params![
                            update.subscription_tier,
                            update.subscription_status,
                            update.stripe_subscription_id,
                            update.subscription_started_at,
                            update.subscription_ends_at,
                            update.trial_ends_at,
                            update.user_id,
                        ],
                    )?;
                }

                let (is_admin, subscription_tier, subscription_status, trial_ends_at, subscription_ends_at):
                    (bool, String, String, Option<String>, Option<String>) = transaction.query_row(
                    "SELECT is_admin, subscription_tier, subscription_status, trial_ends_at, subscription_ends_at
                     FROM users WHERE id = ?1",
                    params![update.user_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
                )?;
                let limit = if is_admin {
                    i64::MAX
                } else if crate::subscription::is_subscription_active(
                    &subscription_status,
                    trial_ends_at.as_deref(),
                    subscription_ends_at.as_deref(),
                ) {
                    match subscription_tier.as_str() {
                        "team" => 5,
                        _ => 1,
                    }
                } else {
                    0
                };

                transaction.execute(
                    "WITH ranked_wallets AS (
                        SELECT checksum, ROW_NUMBER() OVER (ORDER BY created_at) AS position
                        FROM wallets WHERE user_id = ?1 AND status NOT IN ('failed', 'deleted')
                     )
                     UPDATE wallets SET is_active = CASE
                        WHEN status IN ('failed', 'deleted') THEN 0
                        WHEN checksum IN (SELECT checksum FROM ranked_wallets WHERE position <= ?2) THEN 1
                        ELSE 0 END
                     WHERE user_id = ?1",
                    params![update.user_id, limit],
                )?;
                transaction.execute(
                    "WITH ranked_contacts AS (
                        SELECT c.id, ROW_NUMBER() OVER (
                            PARTITION BY c.wallet_checksum ORDER BY c.created_at
                        ) AS position
                        FROM contacts c JOIN wallets w ON w.checksum = c.wallet_checksum
                        WHERE w.user_id = ?1
                     )
                     UPDATE contacts SET is_active = CASE
                        WHEN id IN (SELECT id FROM ranked_contacts WHERE position <= ?2) THEN 1
                        ELSE 0 END
                     WHERE wallet_checksum IN (SELECT checksum FROM wallets WHERE user_id = ?1)",
                    params![update.user_id, limit],
                )?;
            }

            for notification in trial_ending_notifications {
                transaction.execute(
                    "INSERT OR IGNORE INTO stripe_trial_ending_notifications
                     (event_id, customer_id, trial_end_timestamp)
                     VALUES (?1, ?2, ?3)",
                    params![event_id, notification.customer_id, notification.trial_end_timestamp],
                )?;
            }

            let completed = transaction.execute(
                "UPDATE processed_stripe_events
                 SET processed_at = CURRENT_TIMESTAMP
                 WHERE event_id = ?1 AND claim_token = ?2 AND processed_at IS NULL",
                params![event_id, claim_token],
            )? == 1;
            if completed {
                transaction.commit()?;
            } else {
                transaction.rollback()?;
            }
            Ok(completed)
        })
        .await?
    }

    pub async fn get_pending_trial_ending_notifications(
        &self,
        event_id: &str,
    ) -> Result<Vec<crate::stripe_billing::TrialEndingNotification>> {
        let pool = self.pool.clone();
        let event_id = event_id.to_string();
        spawn_blocking(move || {
            let conn = pool.get()?;
            let mut statement = conn.prepare(
                "SELECT customer_id, trial_end_timestamp
                 FROM stripe_trial_ending_notifications
                 WHERE event_id = ?1 AND sent_at IS NULL",
            )?;
            let notifications = statement
                .query_map(params![event_id], |row| {
                    Ok(crate::stripe_billing::TrialEndingNotification {
                        customer_id: row.get(0)?,
                        trial_end_timestamp: row.get(1)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(notifications)
        })
        .await?
    }

    pub async fn mark_trial_ending_notification_sent(
        &self,
        event_id: &str,
        customer_id: &str,
        claim_token: &str,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let event_id = event_id.to_string();
        let customer_id = customer_id.to_string();
        let claim_token = claim_token.to_string();
        spawn_blocking(move || {
            let conn = pool.get()?;
            Ok(conn.execute(
                "UPDATE stripe_trial_ending_notifications
                 SET sent_at = CURRENT_TIMESTAMP, claim_token = NULL, claimed_at = NULL
                 WHERE event_id = ?1 AND customer_id = ?2 AND claim_token = ?3 AND sent_at IS NULL",
                params![event_id, customer_id, claim_token],
            )? == 1)
        })
        .await?
    }

    /// Claim a pending email delivery before performing the external send.
    pub async fn claim_trial_ending_notification(
        &self,
        event_id: &str,
        customer_id: &str,
    ) -> Result<Option<String>> {
        let pool = self.pool.clone();
        let event_id = event_id.to_string();
        let customer_id = customer_id.to_string();
        let claim_token = uuid::Uuid::new_v4().to_string();
        spawn_blocking(move || {
            let conn = pool.get()?;
            let claimed = conn.execute(
                "UPDATE stripe_trial_ending_notifications
                 SET claim_token = ?3, claimed_at = CURRENT_TIMESTAMP
                 WHERE event_id = ?1 AND customer_id = ?2 AND sent_at IS NULL
                   AND (claimed_at IS NULL OR claimed_at <= datetime('now', '-5 minutes'))",
                params![event_id, customer_id, claim_token],
            )? == 1;
            Ok(claimed.then_some(claim_token))
        })
        .await?
    }

    pub async fn release_trial_ending_notification(
        &self,
        event_id: &str,
        customer_id: &str,
        claim_token: &str,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let event_id = event_id.to_string();
        let customer_id = customer_id.to_string();
        let claim_token = claim_token.to_string();
        spawn_blocking(move || {
            let conn = pool.get()?;
            Ok(conn.execute(
                "UPDATE stripe_trial_ending_notifications
                 SET claim_token = NULL, claimed_at = NULL
                 WHERE event_id = ?1 AND customer_id = ?2 AND claim_token = ?3 AND sent_at IS NULL",
                params![event_id, customer_id, claim_token],
            )? == 1)
        })
        .await?
    }

    pub async fn refresh_stripe_event_claim(
        &self,
        event_id: &str,
        claim_token: &str,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let event_id = event_id.to_string();
        let claim_token = claim_token.to_string();
        spawn_blocking(move || {
            let conn = pool.get()?;
            Ok(conn.execute(
                "UPDATE processed_stripe_events SET claimed_at = CURRENT_TIMESTAMP
                 WHERE event_id = ?1 AND claim_token = ?2 AND processed_at IS NULL",
                params![event_id, claim_token],
            )? == 1)
        })
        .await?
    }

    pub async fn release_stripe_event(&self, event_id: &str, claim_token: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let event_id = event_id.to_string();
        let claim_token = claim_token.to_string();

        spawn_blocking(move || {
            let conn = pool.get()?;
            Ok(conn.execute(
                "DELETE FROM processed_stripe_events WHERE event_id = ?1 AND claim_token = ?2 AND processed_at IS NULL",
                params![event_id, claim_token],
            )? == 1)
        })
        .await?
    }

    pub async fn get_instance_secret(&self, key: &str) -> Result<Option<String>> {
        let pool = self.pool.clone();
        let key = key.to_string();

        spawn_blocking(move || -> Result<Option<String>> {
            let conn = pool.get()?;
            let value = conn
                .query_row(
                    "SELECT value FROM instance_secrets WHERE key = ?1",
                    params![key],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            Ok(value)
        })
        .await?
    }

    pub async fn set_instance_secret_if_absent(&self, key: &str, value: &str) -> Result<()> {
        let pool = self.pool.clone();
        let key = key.to_string();
        let value = value.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "INSERT OR IGNORE INTO instance_secrets (key, value) VALUES (?1, ?2)",
                params![key, value],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn get_instance_setting(&self, key: &str) -> Result<Option<String>> {
        let pool = self.pool.clone();
        let key = key.to_string();

        spawn_blocking(move || -> Result<Option<String>> {
            let conn = pool.get()?;
            let value = conn
                .query_row(
                    "SELECT value FROM instance_settings WHERE key = ?1",
                    params![key],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            Ok(value)
        })
        .await?
    }

    pub async fn set_instance_setting(&self, key: &str, value: &str) -> Result<()> {
        let pool = self.pool.clone();
        let key = key.to_string();
        let value = value.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO instance_settings (key, value, updated_at)
                 VALUES (?1, ?2, CURRENT_TIMESTAMP)
                 ON CONFLICT(key) DO UPDATE SET
                    value = excluded.value,
                    updated_at = CURRENT_TIMESTAMP",
                params![key, value],
            )?;
            Ok(())
        })
        .await?
    }

    // ============================
    // BLOCKCHAIN OPERATIONS
    // ============================

    pub async fn upsert_current_block_header(&self, block_header: &BlockHeader) -> Result<()> {
        let pool = self.pool.clone();
        let block_header = block_header.clone();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "INSERT OR REPLACE INTO current_block_header (id, height, timestamp, updated_at)
                 VALUES (1, ?1, ?2, datetime('now'))",
                params![block_header.height, block_header.timestamp,],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn get_current_block_header(&self) -> Result<Option<BlockHeader>> {
        let pool = self.pool.clone();

        spawn_blocking(move || -> Result<Option<BlockHeader>> {
            let conn = pool.get()?;
            match conn.query_row(
                "SELECT height, timestamp FROM current_block_header WHERE id = 1",
                [],
                |row| {
                    Ok(BlockHeader {
                        height: row.get::<_, i64>(0)? as u32,
                        timestamp: row.get::<_, i64>(1)? as u64,
                    })
                },
            ) {
                Ok(block_header) => {
                    // Return None if this is the dummy row (height=0)
                    if block_header.height == 0 {
                        Ok(None)
                    } else {
                        Ok(Some(block_header))
                    }
                }
                Err(bdk_wallet::rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
        .await?
    }

    // ============================
    // EXCHANGE RATE OPERATIONS
    // ============================

    pub async fn get_exchange_rates(
        &self,
    ) -> Result<std::collections::HashMap<String, exchange_rates::ExchangeRate>> {
        let pool = self.pool.clone();
        spawn_blocking(
            move || -> Result<std::collections::HashMap<String, exchange_rates::ExchangeRate>> {
                let conn = pool.get()?;
                let mut stmt = conn
                    .prepare("SELECT currency, rate_per_btc, last_updated FROM exchange_rates")?;
                let rate_iter = stmt.query_map(params![], |row| {
                    Ok(exchange_rates::ExchangeRate {
                        currency: row.get(0)?,
                        rate_per_btc: row.get(1)?,
                        last_updated: chrono::DateTime::parse_from_rfc3339(
                            &row.get::<_, String>(2)?,
                        )
                        .map_err(|e| {
                            bdk_wallet::rusqlite::Error::FromSqlConversionFailure(
                                2,
                                bdk_wallet::rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?
                        .with_timezone(&chrono::Utc),
                    })
                })?;

                let mut rates = std::collections::HashMap::new();
                for rate in rate_iter {
                    let rate = rate?;
                    rates.insert(rate.currency.clone(), rate);
                }
                Ok(rates)
            },
        )
        .await?
    }

    pub async fn store_exchange_rates(
        &self,
        rates: &std::collections::HashMap<String, exchange_rates::ExchangeRate>,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let rates = rates.clone();
        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            let tx = conn.unchecked_transaction()?;

            // Clear old rates
            tx.execute("DELETE FROM exchange_rates", params![])?;

            // Insert new rates
            for (currency, rate) in rates {
                tx.execute(
                    "INSERT INTO exchange_rates (currency, rate_per_btc, last_updated) VALUES (?1, ?2, ?3)",
                    params![currency, rate.rate_per_btc, rate.last_updated.to_rfc3339()],
                )?;
            }

            tx.commit()?;
            Ok(())
        })
        .await?
    }
}
