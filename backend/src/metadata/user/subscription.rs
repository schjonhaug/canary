//! Direct writes to a user's subscription columns: tier, status, trial end and Stripe IDs.

use crate::metadata::pool::MetadataDb;
use crate::metadata::status::SubscriptionStatus;
use crate::metadata::types::SubscriptionUpdateParams;
use anyhow::Result;
use bdk_wallet::rusqlite::params;
use tokio::task::spawn_blocking;

impl MetadataDb {
    pub async fn update_user_stripe_customer(
        &self,
        user_id: &str,
        stripe_customer_id: &str,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let stripe_customer_id = stripe_customer_id.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE users SET stripe_customer_id = ?1 WHERE id = ?2",
                params![stripe_customer_id, user_id],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn update_user_trial_status(
        &self,
        user_id: &str,
        subscription_status: SubscriptionStatus,
        trial_ends_at: Option<String>,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE users SET
                    subscription_status = ?1,
                    trial_ends_at = ?2
                WHERE id = ?3",
                params![subscription_status, trial_ends_at, user_id],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn update_user_subscription_status(
        &self,
        user_id: &str,
        subscription_status: SubscriptionStatus,
        stripe_subscription_id: Option<&str>,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let stripe_subscription_id = stripe_subscription_id.map(std::string::ToString::to_string);

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE users SET
                    subscription_status = ?1,
                    stripe_subscription_id = COALESCE(?2, stripe_subscription_id)
                WHERE id = ?3",
                params![subscription_status, stripe_subscription_id, user_id,],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn expire_user_subscription(&self, user_id: &str) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE users SET subscription_status = 'expired', stripe_subscription_id = NULL WHERE id = ?1",
                params![user_id],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn update_user_subscription(
        &self,
        user_id: &str,
        params: &SubscriptionUpdateParams<'_>,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let subscription_tier = params.subscription_tier.to_string();
        let subscription_status = params.subscription_status;
        let stripe_subscription_id = params
            .stripe_subscription_id
            .map(std::string::ToString::to_string);
        let subscription_started_at = params
            .subscription_started_at
            .map(std::string::ToString::to_string);
        let subscription_ends_at = params
            .subscription_ends_at
            .map(std::string::ToString::to_string);
        let trial_ends_at = params.trial_ends_at.map(std::string::ToString::to_string);

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE users SET
                    subscription_tier = ?1,
                    subscription_status = ?2,
                    stripe_subscription_id = ?3,
                    subscription_started_at = ?4,
                    subscription_ends_at = ?5,
                    trial_ends_at = COALESCE(?6, trial_ends_at)
                WHERE id = ?7",
                params![
                    subscription_tier,
                    subscription_status,
                    stripe_subscription_id,
                    subscription_started_at,
                    subscription_ends_at,
                    trial_ends_at,
                    user_id
                ],
            )?;
            Ok(())
        })
        .await?
    }
}
