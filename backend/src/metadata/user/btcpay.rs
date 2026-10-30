//! `BTCPay` subscription events: deduplication, ordering, checkout correlation and applying them to the user.

use crate::metadata::pool::MetadataDb;
use crate::metadata::status::SubscriptionStatus;
use crate::metadata::types::{BtcPayEventApplyResult, BtcPaySubscriptionEventParams};
use anyhow::Result;
use bdk_wallet::rusqlite::{params, OptionalExtension, TransactionBehavior};

impl MetadataDb {
    /// Atomically deduplicate, order, correlate, and apply a `BTCPay` subscription event.
    pub async fn apply_btcpay_subscription_event(
        &self,
        event: &BtcPaySubscriptionEventParams<'_>,
    ) -> Result<BtcPayEventApplyResult> {
        let delivery_id = event.delivery_id.to_string();
        let event_type = event.event_type.to_string();
        let event_timestamp = event.event_timestamp;
        let event_priority = event.event_priority;
        let checkout_token = event.checkout_token.to_string();
        let customer_id = event.customer_id.to_string();
        let plan_id = event.plan_id.to_string();
        let subscription_tier = event.subscription_tier.to_string();
        let subscription_status = event.subscription_status;
        let subscription_started_at = event.subscription_started_at.map(ToOwned::to_owned);
        let subscription_ends_at = event.subscription_ends_at.map(ToOwned::to_owned);

        self.with_conn(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

            if tx.execute(
                "INSERT OR IGNORE INTO processed_btcpay_events
                    (delivery_id, event_type, event_timestamp)
                 VALUES (?1, ?2, ?3)",
                params![delivery_id, event_type, event_timestamp],
            )? == 0
            {
                tx.commit()?;
                return Ok(BtcPayEventApplyResult::Duplicate);
            }

            let checkout = tx
                .query_row(
                    "SELECT user_id, provider, subscription_tier, id
                     FROM pending_billing_checkouts
                     WHERE token = ?1",
                    params![checkout_token],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, i64>(3)?,
                        ))
                    },
                )
                .optional()?;
            let Some((user_id, provider, checkout_tier, checkout_sequence)) = checkout else {
                tx.commit()?;
                return Ok(BtcPayEventApplyResult::CheckoutNotFound);
            };
            if provider != "btcpay" || checkout_tier != subscription_tier {
                tx.commit()?;
                return Ok(BtcPayEventApplyResult::CheckoutNotFound);
            }

            let current = tx
                .query_row(
                    "SELECT links.checkout_token, links.customer_id, links.status,
                            links.last_event_timestamp, links.last_event_priority,
                            checkouts.id
                     FROM btcpay_subscription_links links
                     JOIN pending_billing_checkouts checkouts
                       ON checkouts.token = links.checkout_token
                     WHERE links.user_id = ?1",
                    params![user_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, i64>(5)?,
                        ))
                    },
                )
                .optional()?;

            let same_subscription =
                current
                    .as_ref()
                    .is_some_and(|(current_token, current_customer, _, _, _, _)| {
                        current_token == &checkout_token && current_customer == &customer_id
                    });

            if let Some((
                _current_token,
                _current_customer,
                _current_status,
                last_timestamp,
                last_priority,
                current_sequence,
            )) = current.as_ref()
            {
                if same_subscription {
                    if event_timestamp < *last_timestamp
                        || (event_timestamp == *last_timestamp && event_priority <= *last_priority)
                    {
                        tx.commit()?;
                        return Ok(BtcPayEventApplyResult::Stale);
                    }
                } else {
                    // Across subscriptions, checkout creation order is authoritative:
                    // provider timestamps can be skewed between separate subscribers.
                    if subscription_status != SubscriptionStatus::Active
                        || checkout_sequence <= *current_sequence
                    {
                        tx.commit()?;
                        return Ok(BtcPayEventApplyResult::Superseded);
                    }
                }
            }

            tx.execute(
                "INSERT INTO btcpay_subscription_links (
                    user_id, checkout_token, customer_id, plan_id, status,
                    last_event_timestamp, last_event_priority, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, CURRENT_TIMESTAMP)
                 ON CONFLICT(user_id) DO UPDATE SET
                    checkout_token = excluded.checkout_token,
                    customer_id = excluded.customer_id,
                    plan_id = excluded.plan_id,
                    status = excluded.status,
                    last_event_timestamp = excluded.last_event_timestamp,
                    last_event_priority = excluded.last_event_priority,
                    updated_at = CURRENT_TIMESTAMP",
                params![
                    user_id,
                    checkout_token,
                    customer_id,
                    plan_id,
                    subscription_status,
                    event_timestamp,
                    event_priority,
                ],
            )?;

            if subscription_status == SubscriptionStatus::Active {
                tx.execute(
                    "UPDATE users SET
                        subscription_tier = ?1,
                        subscription_status = 'active',
                        stripe_customer_id = NULL,
                        stripe_subscription_id = NULL,
                        subscription_started_at = CASE
                            WHEN ?2 THEN subscription_started_at
                            ELSE ?3
                        END,
                        subscription_ends_at = ?4
                     WHERE id = ?5",
                    params![
                        subscription_tier,
                        same_subscription,
                        subscription_started_at,
                        subscription_ends_at,
                        user_id,
                    ],
                )?;
                tx.execute(
                    "UPDATE pending_billing_checkouts
                     SET completed_at = COALESCE(completed_at, CURRENT_TIMESTAMP)
                     WHERE token = ?1",
                    params![checkout_token],
                )?;
            } else if subscription_status == SubscriptionStatus::Expired {
                tx.execute(
                    "UPDATE users SET
                        subscription_status = 'expired',
                        stripe_customer_id = NULL,
                        stripe_subscription_id = NULL,
                        subscription_ends_at = COALESCE(?1, subscription_ends_at)
                     WHERE id = ?2",
                    params![subscription_ends_at, user_id],
                )?;
            } else {
                tx.execute(
                    "UPDATE users SET
                        subscription_status = ?1,
                        subscription_ends_at = COALESCE(?2, subscription_ends_at)
                     WHERE id = ?3",
                    params![subscription_status, subscription_ends_at, user_id],
                )?;
            }

            tx.commit()?;
            Ok(BtcPayEventApplyResult::Applied)
        })
        .await
    }
}
