//! `BTCPay` subscription events: deduplication, ordering, checkout correlation and applying them to the user.

use crate::metadata::pool::MetadataDb;
use crate::metadata::status::SubscriptionStatus;
use crate::metadata::types::{BtcPayEventApplyResult, BtcPaySubscriptionEventParams};
use anyhow::Result;
use bdk_wallet::rusqlite::{self, OptionalExtension, Transaction, TransactionBehavior, params};

impl MetadataDb {
    /// Atomically deduplicate, order, correlate, and apply a `BTCPay` subscription event.
    pub async fn apply_btcpay_subscription_event(
        &self,
        event: &BtcPaySubscriptionEventParams<'_>,
    ) -> Result<BtcPayEventApplyResult> {
        let event = OwnedEvent::from(event);
        self.with_conn(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            // Every outcome, including duplicates and stale events, commits so the
            // delivery stays recorded; errors roll back when `tx` is dropped.
            let result = apply_event(&tx, &event)?;
            tx.commit()?;
            Ok(result)
        })
        .await
    }
}

/// Owned copy of the event so it can move into the blocking database closure.
struct OwnedEvent {
    delivery_id: String,
    event_type: String,
    event_timestamp: i64,
    event_priority: i64,
    checkout_token: String,
    customer_id: String,
    plan_id: String,
    subscription_tier: String,
    subscription_status: SubscriptionStatus,
    subscription_started_at: Option<String>,
    subscription_ends_at: Option<String>,
}

impl From<&BtcPaySubscriptionEventParams<'_>> for OwnedEvent {
    fn from(event: &BtcPaySubscriptionEventParams<'_>) -> Self {
        Self {
            delivery_id: event.delivery_id.to_string(),
            event_type: event.event_type.to_string(),
            event_timestamp: event.event_timestamp,
            event_priority: event.event_priority,
            checkout_token: event.checkout_token.to_string(),
            customer_id: event.customer_id.to_string(),
            plan_id: event.plan_id.to_string(),
            subscription_tier: event.subscription_tier.to_string(),
            subscription_status: event.subscription_status,
            subscription_started_at: event.subscription_started_at.map(ToOwned::to_owned),
            subscription_ends_at: event.subscription_ends_at.map(ToOwned::to_owned),
        }
    }
}

/// The pending checkout an event refers to.
struct Checkout {
    user_id: String,
    provider: String,
    subscription_tier: String,
    /// Row id, which orders checkouts by creation.
    sequence: i64,
}

/// The user's current `BTCPay` subscription link.
struct CurrentLink {
    checkout_token: String,
    customer_id: String,
    last_event_timestamp: i64,
    last_event_priority: i64,
    /// Creation order of the checkout the link belongs to.
    checkout_sequence: i64,
}

/// Runs the whole event inside `tx`. The caller commits.
fn apply_event(
    tx: &Transaction<'_>,
    event: &OwnedEvent,
) -> rusqlite::Result<BtcPayEventApplyResult> {
    if !record_delivery(tx, event)? {
        return Ok(BtcPayEventApplyResult::Duplicate);
    }

    let Some(checkout) = find_checkout(tx, &event.checkout_token)? else {
        return Ok(BtcPayEventApplyResult::CheckoutNotFound);
    };
    if checkout.provider != "btcpay" || checkout.subscription_tier != event.subscription_tier {
        return Ok(BtcPayEventApplyResult::CheckoutNotFound);
    }

    let current = current_link(tx, &checkout.user_id)?;
    let same_subscription = current.as_ref().is_some_and(|link| {
        link.checkout_token == event.checkout_token && link.customer_id == event.customer_id
    });

    if let Some(link) = &current {
        if same_subscription {
            if event.event_timestamp < link.last_event_timestamp
                || (event.event_timestamp == link.last_event_timestamp
                    && event.event_priority <= link.last_event_priority)
            {
                return Ok(BtcPayEventApplyResult::Stale);
            }
        } else if event.subscription_status != SubscriptionStatus::Active
            || checkout.sequence <= link.checkout_sequence
        {
            // Across subscriptions, checkout creation order is authoritative:
            // provider timestamps can be skewed between separate subscribers.
            return Ok(BtcPayEventApplyResult::Superseded);
        }
    }

    upsert_link(tx, &checkout.user_id, event)?;
    update_user_subscription(tx, &checkout.user_id, event, same_subscription)?;
    Ok(BtcPayEventApplyResult::Applied)
}

/// Records the delivery id. Returns `false` if it was already processed.
fn record_delivery(tx: &Transaction<'_>, event: &OwnedEvent) -> rusqlite::Result<bool> {
    let inserted = tx.execute(
        "INSERT OR IGNORE INTO processed_btcpay_events
            (delivery_id, event_type, event_timestamp)
         VALUES (?1, ?2, ?3)",
        params![event.delivery_id, event.event_type, event.event_timestamp],
    )?;
    Ok(inserted > 0)
}

fn find_checkout(tx: &Transaction<'_>, checkout_token: &str) -> rusqlite::Result<Option<Checkout>> {
    tx.query_row(
        "SELECT user_id, provider, subscription_tier, id
         FROM pending_billing_checkouts
         WHERE token = ?1",
        params![checkout_token],
        |row| {
            Ok(Checkout {
                user_id: row.get(0)?,
                provider: row.get(1)?,
                subscription_tier: row.get(2)?,
                sequence: row.get(3)?,
            })
        },
    )
    .optional()
}

fn current_link(tx: &Transaction<'_>, user_id: &str) -> rusqlite::Result<Option<CurrentLink>> {
    tx.query_row(
        "SELECT links.checkout_token, links.customer_id,
                links.last_event_timestamp, links.last_event_priority,
                checkouts.id
         FROM btcpay_subscription_links links
         JOIN pending_billing_checkouts checkouts
           ON checkouts.token = links.checkout_token
         WHERE links.user_id = ?1",
        params![user_id],
        |row| {
            Ok(CurrentLink {
                checkout_token: row.get(0)?,
                customer_id: row.get(1)?,
                last_event_timestamp: row.get(2)?,
                last_event_priority: row.get(3)?,
                checkout_sequence: row.get(4)?,
            })
        },
    )
    .optional()
}

/// Points the user's subscription link at this event's checkout and customer.
fn upsert_link(tx: &Transaction<'_>, user_id: &str, event: &OwnedEvent) -> rusqlite::Result<()> {
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
            event.checkout_token,
            event.customer_id,
            event.plan_id,
            event.subscription_status,
            event.event_timestamp,
            event.event_priority,
        ],
    )?;
    Ok(())
}

/// Applies the event's status to the user row. An active event also clears any
/// Stripe subscription and completes the checkout.
fn update_user_subscription(
    tx: &Transaction<'_>,
    user_id: &str,
    event: &OwnedEvent,
    same_subscription: bool,
) -> rusqlite::Result<()> {
    match event.subscription_status {
        SubscriptionStatus::Active => {
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
                    event.subscription_tier,
                    same_subscription,
                    event.subscription_started_at,
                    event.subscription_ends_at,
                    user_id,
                ],
            )?;
            tx.execute(
                "UPDATE pending_billing_checkouts
                 SET completed_at = COALESCE(completed_at, CURRENT_TIMESTAMP)
                 WHERE token = ?1",
                params![event.checkout_token],
            )?;
        }
        SubscriptionStatus::Expired => {
            tx.execute(
                "UPDATE users SET
                    subscription_status = 'expired',
                    stripe_customer_id = NULL,
                    stripe_subscription_id = NULL,
                    subscription_ends_at = COALESCE(?1, subscription_ends_at)
                 WHERE id = ?2",
                params![event.subscription_ends_at, user_id],
            )?;
        }
        _ => {
            tx.execute(
                "UPDATE users SET
                    subscription_status = ?1,
                    subscription_ends_at = COALESCE(?2, subscription_ends_at)
                 WHERE id = ?3",
                params![
                    event.subscription_status,
                    event.subscription_ends_at,
                    user_id
                ],
            )?;
        }
    }
    Ok(())
}
