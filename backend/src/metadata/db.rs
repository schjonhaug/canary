use super::pool::MetadataDb;
use crate::electrum::BlockHeader;
use crate::exchange_rates;
use anyhow::Result;
use bdk_wallet::rusqlite::params;
use tokio::task::spawn_blocking;

impl MetadataDb {
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
