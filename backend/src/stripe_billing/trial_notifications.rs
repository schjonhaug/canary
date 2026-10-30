//! Trial ending emails. Each is delivered only after the webhook event that
//! announced it has been stored, so a retried event cannot send it twice.

use anyhow::{Result, bail};

use super::{StripeBilling, TrialEndingNotification};

impl StripeBilling {
    async fn send_trial_ending_notification(
        &self,
        notification: &TrialEndingNotification,
        event_id: &str,
    ) -> Result<bool> {
        match self
            .metadata_db
            .get_user_by_stripe_customer_id(&notification.customer_id)
            .await
        {
            Ok(Some(user)) if user.email_verified => {
                let trial_ends_at =
                    chrono::DateTime::from_timestamp(notification.trial_end_timestamp, 0)
                        .map_or_else(
                            || "soon".to_string(),
                            |dt| dt.format("%B %d, %Y").to_string(),
                        );
                let user_name = user.name.as_deref().unwrap_or(&user.email);
                let user_language = user.preferred_language.as_deref().unwrap_or("en-US");

                let email_service = self
                    .email_service
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Email service not configured"))?;
                email_service
                    .send_trial_ending_notification(
                        &user.email,
                        user_name,
                        &trial_ends_at,
                        user_language,
                        &format!("stripe-trial-will-end-{event_id}"),
                    )
                    .await?;
                tracing::info!("Trial ending notification sent");
                Ok(true)
            }
            Ok(Some(_user)) => {
                tracing::info!("Trial notification skipped: email unverified");
                Ok(false)
            }
            Ok(None) => {
                tracing::warn!("Trial notification skipped: customer mapping unavailable");
                Ok(false)
            }
            Err(e) => Err(e),
        }
    }

    pub async fn deliver_pending_trial_ending_notifications(&self, event_id: &str) -> Result<()> {
        for notification in self
            .metadata_db
            .get_pending_trial_ending_notifications(event_id)
            .await?
        {
            let Some(claim_token) = self
                .metadata_db
                .claim_trial_ending_notification(event_id, &notification.customer_id)
                .await?
            else {
                bail!("Trial ending notification is already being delivered");
            };

            let sent = match self
                .send_trial_ending_notification(&notification, event_id)
                .await
            {
                Ok(sent) => sent,
                Err(error) => {
                    let _ = self
                        .metadata_db
                        .release_trial_ending_notification(
                            event_id,
                            &notification.customer_id,
                            &claim_token,
                        )
                        .await;
                    return Err(error);
                }
            };
            if !sent {
                tracing::info!("Discarding trial notification without a customer mapping");
            }
            let marked_sent = self
                .metadata_db
                .mark_trial_ending_notification_sent(
                    event_id,
                    &notification.customer_id,
                    &claim_token,
                )
                .await?;
            if !marked_sent {
                bail!("Trial ending notification claim was lost before completion");
            }
        }
        Ok(())
    }
}
