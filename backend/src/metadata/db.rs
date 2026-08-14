use super::pool::MetadataDb;
use crate::electrum::BlockHeader;
use crate::exchange_rates;
use anyhow::Result;
use bdk_wallet::rusqlite::{params, OptionalExtension};
use tokio::task::spawn_blocking;

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

            let processed: bool = conn.query_row(
                "SELECT processed_at IS NOT NULL FROM processed_stripe_events WHERE event_id = ?1",
                params![event_id],
                |row| row.get(0),
            )?;
            Ok(if processed {
                StripeEventClaim::Processed
            } else {
                StripeEventClaim::Active
            })
        })
        .await?
    }

    pub async fn complete_stripe_event(&self, event_id: &str, claim_token: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let event_id = event_id.to_string();
        let claim_token = claim_token.to_string();

        spawn_blocking(move || {
            let conn = pool.get()?;
            Ok(conn.execute(
                "UPDATE processed_stripe_events
                 SET processed_at = CURRENT_TIMESTAMP
                 WHERE event_id = ?1 AND claim_token = ?2 AND processed_at IS NULL",
                params![event_id, claim_token],
            )? == 1)
        })
        .await?
    }

    pub async fn renew_stripe_event_claim(
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
                "UPDATE processed_stripe_events
                 SET claimed_at = CURRENT_TIMESTAMP
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
