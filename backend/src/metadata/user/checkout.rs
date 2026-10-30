//! Pending billing checkouts that correlate a provider checkout with the user who started it.

use crate::metadata::pool::MetadataDb;
use crate::metadata::types::PendingBillingCheckout;
use anyhow::Result;
use bdk_wallet::rusqlite::{params, OptionalExtension};

impl MetadataDb {
    pub async fn create_pending_billing_checkout(
        &self,
        token: &str,
        user_id: &str,
        provider: &str,
        subscription_tier: &str,
        billing_period: &str,
    ) -> Result<()> {
        let token = token.to_string();
        let user_id = user_id.to_string();
        let provider = provider.to_string();
        let subscription_tier = subscription_tier.to_string();
        let billing_period = billing_period.to_string();

        self.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO pending_billing_checkouts (
                    token, user_id, provider, subscription_tier, billing_period, expires_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, datetime('now', '+1 day'))",
                params![token, user_id, provider, subscription_tier, billing_period],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn get_pending_billing_checkout(
        &self,
        token: &str,
    ) -> Result<Option<PendingBillingCheckout>> {
        let token = token.to_string();

        self.with_conn(move |conn| {
            let checkout = conn
                .query_row(
                    "SELECT token, user_id, provider, subscription_tier, billing_period, completed_at
                     FROM pending_billing_checkouts
                     WHERE token = ?1
                       AND (expires_at > datetime('now') OR completed_at IS NOT NULL)",
                    params![token],
                    |row| {
                        Ok(PendingBillingCheckout {
                            token: row.get(0)?,
                            user_id: row.get(1)?,
                            provider: row.get(2)?,
                            subscription_tier: row.get(3)?,
                            billing_period: row.get(4)?,
                            completed_at: row.get(5)?,
                        })
                    },
                )
                .optional()?;

            Ok(checkout)
        })
        .await
    }

    /// Look up a checkout correlation for a verified provider webhook.
    /// Completed and expired rows remain valid correlations for later renewal events.
    pub async fn get_billing_checkout_by_token(
        &self,
        token: &str,
    ) -> Result<Option<PendingBillingCheckout>> {
        let token = token.to_string();

        self.with_conn(move |conn| {
            let checkout = conn
                .query_row(
                    "SELECT token, user_id, provider, subscription_tier, billing_period, completed_at
                     FROM pending_billing_checkouts
                     WHERE token = ?1",
                    params![token],
                    |row| {
                        Ok(PendingBillingCheckout {
                            token: row.get(0)?,
                            user_id: row.get(1)?,
                            provider: row.get(2)?,
                            subscription_tier: row.get(3)?,
                            billing_period: row.get(4)?,
                            completed_at: row.get(5)?,
                        })
                    },
                )
                .optional()?;

            Ok(checkout)
        })
        .await
    }

    pub async fn mark_pending_billing_checkout_completed(&self, token: &str) -> Result<()> {
        let token = token.to_string();

        self.with_conn(move |conn| {
            conn.execute(
                "UPDATE pending_billing_checkouts
                 SET completed_at = CURRENT_TIMESTAMP
                 WHERE token = ?1 AND completed_at IS NULL",
                params![token],
            )?;
            Ok(())
        })
        .await
    }
}
