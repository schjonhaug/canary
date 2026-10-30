//! Contact writes: inserting, updating, deactivating and deleting contacts and their notification methods.

use super::ContactNotificationSettings;
use crate::metadata::pool::MetadataDb;
use crate::metadata::types::{ContentPrivacyLevel, NotificationContentFields, ProviderType};
use anyhow::Result;
use bdk_wallet::rusqlite::{params, OptionalExtension, ToSql};
use tokio::task::spawn_blocking;
use uuid::Uuid;

impl MetadataDb {
    pub async fn insert_contact_with_notification_methods(
        &self,
        wallet_checksum: &str,
        name: &str,
        notification_methods: Vec<(ProviderType, String)>,
    ) -> Result<String> {
        self.insert_contact_with_notification_settings(
            wallet_checksum,
            name,
            notification_methods
                .into_iter()
                .map(|(provider, target)| {
                    (
                        provider,
                        target,
                        true,
                        NotificationContentFields::standard(),
                    )
                })
                .collect(),
            ContactNotificationSettings::defaults_for_new_contact(),
        )
        .await
    }

    pub async fn insert_contact_with_notification_settings(
        &self,
        wallet_checksum: &str,
        name: &str,
        notification_methods: Vec<(ProviderType, String, bool, NotificationContentFields)>,
        notification_settings: ContactNotificationSettings,
    ) -> Result<String> {
        let pool = self.pool.clone();
        let name = name.to_string();
        let checksum = wallet_checksum.to_string();

        spawn_blocking(move || -> Result<String> {
            let conn = pool.get()?;
            let tx = conn.unchecked_transaction()?;

            // Insert contact with UUID
            let contact_id = Uuid::new_v4().to_string();
            let legacy_contact_balance =
                legacy_contact_balance_for_methods(&notification_methods);
            tx.execute(
                "INSERT INTO contacts (
                    id, wallet_checksum, name, notify_sending, notify_sent, notify_receiving,
                    notify_received, notify_cpfp, notify_rbf, include_wallet_balance_in_tx_notifications
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    &contact_id,
                    checksum,
                    &name,
                    notification_settings.notify_sending,
                    notification_settings.notify_sent,
                    notification_settings.notify_receiving,
                    notification_settings.notify_received,
                    notification_settings.notify_cpfp,
                    notification_settings.notify_rbf,
                    legacy_contact_balance,
                ],
            )?;

            // Insert notification methods
            for (provider_type, notification_target, is_enabled, content_fields) in
                notification_methods
            {
                let method_id = Uuid::new_v4().to_string();
                let legacy_level = content_fields.legacy_privacy_level();
                tx.execute(
                    "INSERT INTO contact_notification_methods (
                        id, contact_id, provider_type, notification_target, wallet_checksum,
                        is_enabled, content_privacy_level, content_wallet_name,
                        content_event_type, content_transaction_amount,
                        content_transaction_balance, content_balance_alert_condition,
                        content_balance_alert_threshold, content_balance_alert_balance
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                    params![
                        &method_id,
                        &contact_id,
                        provider_type.as_str(),
                        &notification_target,
                        &checksum,
                        is_enabled,
                        legacy_level.as_str(),
                        content_fields.wallet_name,
                        content_fields.event_type,
                        content_fields.transaction_amount,
                        content_fields.transaction_balance,
                        content_fields.balance_alert_condition,
                        content_fields.balance_alert_threshold,
                        content_fields.balance_alert_balance,
                    ],
                )?;
            }

            tx.commit()?;
            Ok(contact_id)
        })
        .await?
    }

    pub async fn delete_wallet_contact(
        &self,
        wallet_checksum: &str,
        contact_id: &str,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();
        let contact_id = contact_id.to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let rows_affected = conn.execute(
                "DELETE FROM contacts WHERE id = ?1 AND wallet_checksum = ?2",
                params![contact_id, checksum],
            )?;
            Ok(rows_affected > 0)
        })
        .await?
    }

    /// Update contact active status for subscription tier limits
    pub async fn update_contact_active_status(
        &self,
        contact_id: &str,
        is_active: bool,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let contact_id = contact_id.to_string();

        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;

            conn.execute(
                "UPDATE contacts SET is_active = ? WHERE id = ?",
                params![is_active, contact_id],
            )?;

            Ok::<(), anyhow::Error>(())
        })
        .await??;

        Ok(())
    }

    /// Update contact with new methods using a transaction
    /// This ensures atomic updates - if any part fails, the old contact remains unchanged
    pub async fn update_contact_with_methods(
        &self,
        contact_id: &str,
        wallet_checksum: &str,
        name: &str,
        new_methods: Vec<(ProviderType, String, bool, NotificationContentFields)>,
        notification_settings: ContactNotificationSettings,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let contact_id = contact_id.to_string();
        let checksum = wallet_checksum.to_string();
        let contact_name = name.to_string();

        spawn_blocking(move || -> Result<()> {
            let mut conn = pool.get()?;

            // Use rusqlite's Transaction API for automatic rollback on drop
            let tx = conn.transaction()?;

            let legacy_contact_balance = legacy_contact_balance_for_methods(&new_methods);

            // Update contact basics and the conservative legacy balance mirror.
            tx.execute(
                "UPDATE contacts
                 SET name = ?1,
                     notify_sending = ?4,
                     notify_sent = ?5,
                     notify_receiving = ?6,
                     notify_received = ?7,
                     notify_cpfp = ?8,
                     notify_rbf = ?9,
                     include_wallet_balance_in_tx_notifications = ?10
                 WHERE id = ?2 AND wallet_checksum = ?3",
                params![
                    contact_name,
                    contact_id,
                    checksum,
                    notification_settings.notify_sending,
                    notification_settings.notify_sent,
                    notification_settings.notify_receiving,
                    notification_settings.notify_received,
                    notification_settings.notify_cpfp,
                    notification_settings.notify_rbf,
                    legacy_contact_balance,
                ],
            )?;

            // Check if contact was updated (exists and belongs to wallet)
            let affected: i64 = tx.query_row("SELECT changes()", [], |row| row.get(0))?;

            if affected == 0 {
                return Err(anyhow::anyhow!("Contact not found or access denied"));
            }

            let mut retained_method_ids = Vec::new();
            for (provider_type, target, is_enabled, content_fields) in new_methods {
                let provider = provider_type.as_str();
                let legacy_level = content_fields.legacy_privacy_level();
                let existing_method_id = tx
                    .query_row(
                        "SELECT id FROM contact_notification_methods
                         WHERE contact_id = ?1 AND provider_type = ?2 AND notification_target = ?3
                         LIMIT 1",
                        params![contact_id, provider, target],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?;

                let method_id = if let Some(method_id) = existing_method_id {
                    tx.execute(
                        "UPDATE contact_notification_methods
                         SET wallet_checksum = ?1, is_enabled = ?2, content_privacy_level = ?3,
                             content_wallet_name = ?4, content_event_type = ?5,
                             content_transaction_amount = ?6, content_transaction_balance = ?7,
                             content_balance_alert_condition = ?8,
                             content_balance_alert_threshold = ?9,
                             content_balance_alert_balance = ?10
                         WHERE id = ?11",
                        params![
                            checksum,
                            is_enabled,
                            legacy_level.as_str(),
                            content_fields.wallet_name,
                            content_fields.event_type,
                            content_fields.transaction_amount,
                            content_fields.transaction_balance,
                            content_fields.balance_alert_condition,
                            content_fields.balance_alert_threshold,
                            content_fields.balance_alert_balance,
                            method_id,
                        ],
                    )?;
                    method_id
                } else {
                    let method_id = uuid::Uuid::new_v4().to_string();
                    tx.execute(
                        "INSERT INTO contact_notification_methods
                         (id, contact_id, provider_type, notification_target, wallet_checksum,
                          is_enabled, content_privacy_level, content_wallet_name,
                          content_event_type, content_transaction_amount,
                          content_transaction_balance, content_balance_alert_condition,
                          content_balance_alert_threshold, content_balance_alert_balance)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                        params![
                            method_id,
                            contact_id,
                            provider,
                            target,
                            checksum,
                            is_enabled,
                            legacy_level.as_str(),
                            content_fields.wallet_name,
                            content_fields.event_type,
                            content_fields.transaction_amount,
                            content_fields.transaction_balance,
                            content_fields.balance_alert_condition,
                            content_fields.balance_alert_threshold,
                            content_fields.balance_alert_balance,
                        ],
                    )?;
                    method_id
                };

                retained_method_ids.push(method_id);
            }

            if retained_method_ids.is_empty() {
                tx.execute(
                    "DELETE FROM contact_notification_methods WHERE contact_id = ?1",
                    params![contact_id],
                )?;
            } else {
                let placeholders = retained_method_ids
                    .iter()
                    .map(|_| "?")
                    .collect::<Vec<_>>()
                    .join(",");
                let delete_query = format!(
                    "DELETE FROM contact_notification_methods
                     WHERE contact_id = ? AND id NOT IN ({placeholders})"
                );
                let mut delete_params: Vec<&dyn ToSql> =
                    Vec::with_capacity(retained_method_ids.len() + 1);
                delete_params.push(&contact_id);
                for method_id in &retained_method_ids {
                    delete_params.push(method_id);
                }
                tx.execute(&delete_query, delete_params.as_slice())?;
            }

            tx.commit()?;
            Ok(())
        })
        .await?
    }
}

fn legacy_contact_balance_for_methods(
    methods: &[(ProviderType, String, bool, NotificationContentFields)],
) -> bool {
    let mut has_detailed_method = false;
    for (_, _, _, fields) in methods {
        if fields.legacy_privacy_level() == ContentPrivacyLevel::Detailed {
            has_detailed_method = true;
            if !fields.transaction_balance {
                return false;
            }
        }
    }
    has_detailed_method
}
