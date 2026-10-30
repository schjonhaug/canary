//! Stripe webhook idempotency and event-ordered subscription updates.
//!
//! Each webhook event is claimed with a token before it is processed, so a retry
//! only reprocesses events that failed or whose claim went stale. Subscription
//! updates carry the event's `created` timestamp and ID and are skipped when a
//! newer event has already been applied.

use crate::metadata::pool::MetadataDb;
use crate::metadata::status::SubscriptionStatus;
use crate::metadata::types::SubscriptionUpdateParams;
use anyhow::Result;
use bdk_wallet::rusqlite::params;

impl MetadataDb {
    pub async fn claim_stripe_webhook_event(
        &self,
        event_id: &str,
        event_created: i64,
        event_type: &str,
    ) -> Result<Option<String>> {
        let event_id = event_id.to_string();
        let event_type = event_type.to_string();
        let claim_token = uuid::Uuid::new_v4().to_string();

        self.with_conn(move |conn| {
            let claimed = conn.execute(
                "INSERT INTO stripe_webhook_events (id, event_type, metadata, delivery_status, processing_started_at, claim_token)
                 VALUES (?1, ?2, ?3, 'processing', CURRENT_TIMESTAMP, ?4)
                 ON CONFLICT(id) DO UPDATE SET delivery_status = 'processing', processing_started_at = CURRENT_TIMESTAMP, claim_token = ?4
                 WHERE stripe_webhook_events.delivery_status = 'failed'
                    OR (stripe_webhook_events.delivery_status = 'processing'
                        AND stripe_webhook_events.processing_started_at < datetime('now', '-5 minutes'))",
                params![event_id, event_type, format!("{{\"created\":{event_created}}}"), claim_token],
            )? > 0;
            Ok(claimed.then_some(claim_token))
        })
        .await
    }

    pub async fn complete_stripe_webhook_event(
        &self,
        event_id: &str,
        claim_token: &str,
    ) -> Result<bool> {
        let event_id = event_id.to_string();
        let claim_token = claim_token.to_string();

        self.with_conn(move |conn| {
            Ok(conn.execute(
                "UPDATE stripe_webhook_events SET delivery_status = 'completed', processed_at = CURRENT_TIMESTAMP
                 WHERE id = ?1 AND claim_token = ?2",
                params![event_id, claim_token],
            )? > 0)
        })
        .await
    }

    pub async fn refresh_stripe_webhook_claim(
        &self,
        event_id: &str,
        claim_token: &str,
    ) -> Result<bool> {
        let event_id = event_id.to_string();
        let claim_token = claim_token.to_string();

        self.with_conn(move |conn| {
            Ok(conn.execute(
                "UPDATE stripe_webhook_events SET processing_started_at = CURRENT_TIMESTAMP
                 WHERE id = ?1 AND claim_token = ?2 AND delivery_status = 'processing'",
                params![event_id, claim_token],
            )? > 0)
        })
        .await
    }

    pub async fn is_stripe_webhook_event_complete(&self, event_id: &str) -> Result<bool> {
        let event_id = event_id.to_string();

        self.with_conn(move |conn| {
            Ok(conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM stripe_webhook_events WHERE id = ?1 AND delivery_status = 'completed')",
                params![event_id],
                |row| row.get(0),
            )?)
        })
        .await
    }

    pub async fn fail_stripe_webhook_event(
        &self,
        event_id: &str,
        claim_token: &str,
    ) -> Result<bool> {
        let event_id = event_id.to_string();
        let claim_token = claim_token.to_string();

        self.with_conn(move |conn| {
            Ok(conn.execute(
                "UPDATE stripe_webhook_events SET delivery_status = 'failed' WHERE id = ?1 AND claim_token = ?2",
                params![event_id, claim_token],
            )? > 0)
        })
        .await
    }

    pub async fn trial_ending_email_was_sent(&self, event_id: &str) -> Result<bool> {
        let event_id = event_id.to_string();

        self.with_conn(move |conn| {
            Ok(conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM stripe_webhook_events WHERE id = ?1 AND trial_ending_email_sent_at IS NOT NULL)",
                params![event_id],
                |row| row.get(0),
            )?)
        })
        .await
    }

    pub async fn mark_trial_ending_email_sent(&self, event_id: &str) -> Result<bool> {
        let event_id = event_id.to_string();

        self.with_conn(move |conn| {
            Ok(conn.execute(
                "UPDATE stripe_webhook_events SET trial_ending_email_sent_at = CURRENT_TIMESTAMP
                 WHERE id = ?1 AND trial_ending_email_sent_at IS NULL",
                params![event_id],
            )? > 0)
        })
        .await
    }

    pub async fn update_user_subscription_status_for_stripe_event(
        &self,
        user_id: &str,
        subscription_status: SubscriptionStatus,
        stripe_subscription_id: Option<&str>,
        event_created: i64,
        event_id: &str,
        authoritative_same_timestamp: bool,
    ) -> Result<bool> {
        let user_id = user_id.to_string();
        let stripe_subscription_id = stripe_subscription_id.map(str::to_string);
        let event_id = event_id.to_string();

        self.with_conn(move |conn| {
            Ok(conn.execute(
                "UPDATE users SET subscription_status = ?1,
                    stripe_subscription_id = COALESCE(?2, stripe_subscription_id),
                    stripe_event_created = ?3, stripe_event_id = ?4
                  WHERE id = ?5 AND (stripe_event_created IS NULL OR stripe_event_created < ?3
                    OR (stripe_event_created = ?3 AND (stripe_event_id = ?4 OR ?6)))",
                params![
                    subscription_status,
                    stripe_subscription_id,
                    event_created,
                    event_id,
                    user_id,
                    authoritative_same_timestamp
                ],
            )? > 0)
        })
        .await
    }

    pub async fn expire_user_subscription_for_stripe_event(
        &self,
        user_id: &str,
        event_created: i64,
        event_id: &str,
        authoritative_same_timestamp: bool,
    ) -> Result<bool> {
        let user_id = user_id.to_string();
        let event_id = event_id.to_string();

        self.with_conn(move |conn| {
            Ok(conn.execute(
                "UPDATE users SET subscription_status = 'expired', stripe_subscription_id = NULL,
                    stripe_event_created = ?1, stripe_event_id = ?2
                 WHERE id = ?3 AND (stripe_event_created IS NULL OR stripe_event_created < ?1
                    OR (stripe_event_created = ?1 AND (stripe_event_id = ?2 OR ?4)))",
                params![
                    event_created,
                    event_id,
                    user_id,
                    authoritative_same_timestamp
                ],
            )? > 0)
        })
        .await
    }

    pub async fn update_user_subscription_for_stripe_event(
        &self,
        user_id: &str,
        params: &SubscriptionUpdateParams<'_>,
        event_created: i64,
        event_id: &str,
        authoritative_same_timestamp: bool,
    ) -> Result<bool> {
        let user_id = user_id.to_string();
        let subscription_tier = params.subscription_tier.to_string();
        let subscription_status = params.subscription_status;
        let stripe_subscription_id = params.stripe_subscription_id.map(str::to_string);
        let subscription_started_at = params.subscription_started_at.map(str::to_string);
        let subscription_ends_at = params.subscription_ends_at.map(str::to_string);
        let trial_ends_at = params.trial_ends_at.map(str::to_string);
        let event_id = event_id.to_string();

        self.with_conn(move |conn| {
            Ok(conn.execute(
                "UPDATE users SET subscription_tier = ?1, subscription_status = ?2,
                    stripe_subscription_id = ?3, subscription_started_at = ?4,
                    subscription_ends_at = ?5, trial_ends_at = COALESCE(?6, trial_ends_at),
                    stripe_event_created = ?7, stripe_event_id = ?8
                  WHERE id = ?9 AND (stripe_event_created IS NULL OR stripe_event_created < ?7
                    OR (stripe_event_created = ?7 AND (stripe_event_id = ?8 OR ?10)))",
                params![
                    subscription_tier,
                    subscription_status,
                    stripe_subscription_id,
                    subscription_started_at,
                    subscription_ends_at,
                    trial_ends_at,
                    event_created,
                    event_id,
                    user_id,
                    authoritative_same_timestamp
                ],
            )? > 0)
        })
        .await
    }
}
