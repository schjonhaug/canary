//! Subscription limit enforcement: which of a user's wallets and contacts stay
//! active for their tier and subscription state.

use super::manager::WalletManager;
use crate::metadata::{SubscriptionStatus, WalletStatus};

impl WalletManager {
    /// Apply subscription tier limits by setting `is_active` status on wallets and contacts
    pub async fn apply_subscription_limits(
        &self,
        user_id: &str,
        tier: &str,
        subscription_status: SubscriptionStatus,
        is_admin: bool,
        trial_ends_at: Option<String>,
        subscription_ends_at: Option<String>,
    ) -> Result<(), anyhow::Error> {
        // Check if subscription has expired or failed payment
        let is_subscription_active = crate::subscription::is_subscription_active(
            subscription_status,
            trial_ends_at.as_deref(),
            subscription_ends_at.as_deref(),
        );

        if is_admin {
            tracing::info!("Applying unlimited subscription limits for an admin account");
        } else if !is_subscription_active {
            tracing::info!("Deactivating wallets for an inactive subscription");
        } else {
            tracing::info!("Applying subscription tier limits");
        }

        // Get all wallets for this user ordered by creation time (oldest first)
        let wallets = self
            .metadata_db
            .get_wallets_for_user_oldest_first(user_id)
            .await?;

        // Determine wallet limit based on subscription status, tier, and admin status
        let wallet_limit = if is_admin {
            usize::MAX // Unlimited for admin
        } else if !is_subscription_active {
            0 // No active wallets for inactive subscriptions (expired, past_due, or canceled with no remaining access)
        } else {
            crate::subscription::SubscriptionTier::from(tier).max_wallets()
        };

        // Apply wallet limits. Failed wallets are recoverable records, not active
        // subscriptions slots, so keep them inactive and skip them when counting.
        let mut active_wallet_count = 0;
        let mut non_failed_wallet_count = 0;
        for wallet in &wallets {
            let (should_be_active, _wallet_position) =
                crate::subscription::wallet_active_limit_decision(
                    wallet.status,
                    wallet_limit,
                    &mut active_wallet_count,
                    &mut non_failed_wallet_count,
                );

            // Update is_active status only if it changed
            if wallet.is_active != should_be_active {
                if let Err(_error) = self
                    .metadata_db
                    .update_wallet_active_status(&wallet.checksum, should_be_active)
                    .await
                {
                    tracing::error!("Failed to update wallet active status");
                } else if wallet.status == WalletStatus::Failed {
                    tracing::info!("Updated a failed wallet active status");
                } else {
                    tracing::info!("Updated a wallet active status");
                }
            }
        }

        // For active wallets, apply contact limits
        let contact_limit = if is_admin {
            usize::MAX // Unlimited for admin
        } else if !is_subscription_active {
            0 // No active contacts for inactive subscriptions (expired, past_due, or canceled with no remaining access)
        } else {
            crate::subscription::SubscriptionTier::from(tier).max_contacts_per_wallet()
        };

        for wallet in &wallets {
            if wallet.is_active {
                // Get contacts ordered by creation time (oldest first)
                let contacts = self
                    .metadata_db
                    .get_contacts_oldest_first_for_limits(&wallet.checksum)
                    .await?;

                // Apply contact limits
                for (i, contact) in contacts.iter().enumerate() {
                    let should_be_active = i < contact_limit;

                    // Update is_active status only if it changed
                    if contact.is_active != should_be_active {
                        if let Some(contact_id) = &contact.id {
                            if let Err(_error) = self
                                .metadata_db
                                .update_contact_active_status(contact_id, should_be_active)
                                .await
                            {
                                tracing::error!("Failed to update contact active status");
                            } else {
                                tracing::info!("Updated a contact active status");
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
