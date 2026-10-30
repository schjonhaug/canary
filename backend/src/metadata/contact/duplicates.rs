//! Checks that a notification target is not already used by another contact of the same wallet.

use crate::metadata::pool::MetadataDb;
use anyhow::Result;
use bdk_wallet::rusqlite::{params, OptionalExtension};
use tokio::task::spawn_blocking;

impl MetadataDb {
    /// Check if a notification target (email or phone) is already used by another contact in the same wallet
    pub async fn check_duplicate_notification_target(
        &self,
        wallet_checksum: &str,
        provider_type: &str,
        notification_target: &str,
        exclude_contact_id: Option<&str>,
    ) -> Result<Option<String>> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();
        let provider = provider_type.to_string();
        let target = notification_target.to_string();
        let exclude_id = exclude_contact_id.map(std::string::ToString::to_string);

        spawn_blocking(move || -> Result<Option<String>> {
            let conn = pool.get()?;

            // Build query dynamically to reduce code duplication
            let mut base_query = "SELECT c.name FROM contacts c \
                JOIN contact_notification_methods cnm ON c.id = cnm.contact_id \
                WHERE cnm.wallet_checksum = ?1 AND cnm.provider_type = ?2"
                .to_string();

            // For emails, do case-insensitive comparison
            if provider == "email" {
                base_query.push_str(" AND LOWER(cnm.notification_target) = LOWER(?3)");
            } else {
                base_query.push_str(" AND cnm.notification_target = ?3");
            }

            let existing_contact_name: Option<String> = if let Some(contact_id) = exclude_id {
                let query = format!("{base_query} AND c.id != ?4 LIMIT 1");
                conn.query_row(
                    &query,
                    params![&checksum, &provider, &target, &contact_id],
                    |row| row.get(0),
                )
                .optional()?
            } else {
                let query = format!("{base_query} LIMIT 1");
                conn.query_row(&query, params![&checksum, &provider, &target], |row| {
                    row.get(0)
                })
                .optional()?
            };

            Ok(existing_contact_name)
        })
        .await?
    }

    /// Check for duplicate notification targets across multiple targets for batch validation
    pub async fn check_duplicate_notification_targets(
        &self,
        wallet_checksum: &str,
        notification_methods: &[(String, String)], // (provider_type, notification_target)
        exclude_contact_id: Option<&str>,
    ) -> Result<Vec<String>> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();
        let methods = notification_methods.to_vec();
        let exclude_id = exclude_contact_id.map(std::string::ToString::to_string);

        spawn_blocking(move || -> Result<Vec<String>> {
            let conn = pool.get()?;
            let mut duplicates = Vec::new();

            for (provider_type, notification_target) in &methods {
                // ntfy topics, webhook endpoints, and Telegram chats may be shared across contacts.
                if provider_type == "ntfy"
                    || provider_type == "webhook"
                    || provider_type == "telegram"
                {
                    continue;
                }

                // For emails, do case-insensitive comparison
                let existing_contact_name: Option<String> = if provider_type == "email" {
                    if let Some(contact_id) = &exclude_id {
                        conn.query_row(
                            "SELECT c.name FROM contacts c
                             JOIN contact_notification_methods cnm ON c.id = cnm.contact_id
                             WHERE cnm.wallet_checksum = ?1
                             AND cnm.provider_type = ?2
                             AND LOWER(cnm.notification_target) = LOWER(?3)
                             AND c.id != ?4
                             LIMIT 1",
                            params![&checksum, provider_type, notification_target, contact_id],
                            |row| row.get::<_, String>(0),
                        )
                        .optional()?
                    } else {
                        conn.query_row(
                            "SELECT c.name FROM contacts c
                             JOIN contact_notification_methods cnm ON c.id = cnm.contact_id
                             WHERE cnm.wallet_checksum = ?1
                             AND cnm.provider_type = ?2
                             AND LOWER(cnm.notification_target) = LOWER(?3)
                             LIMIT 1",
                            params![&checksum, provider_type, notification_target],
                            |row| row.get::<_, String>(0),
                        )
                        .optional()?
                    }
                } else if let Some(contact_id) = &exclude_id {
                    // For SMS and other types, exact match
                    conn.query_row(
                        "SELECT c.name FROM contacts c
                         JOIN contact_notification_methods cnm ON c.id = cnm.contact_id
                         WHERE cnm.wallet_checksum = ?1
                         AND cnm.provider_type = ?2
                         AND cnm.notification_target = ?3
                         AND c.id != ?4
                         LIMIT 1",
                        params![&checksum, provider_type, notification_target, contact_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?
                } else {
                    conn.query_row(
                        "SELECT c.name FROM contacts c
                         JOIN contact_notification_methods cnm ON c.id = cnm.contact_id
                         WHERE cnm.wallet_checksum = ?1
                         AND cnm.provider_type = ?2
                         AND cnm.notification_target = ?3
                         LIMIT 1",
                        params![&checksum, provider_type, notification_target],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?
                };

                if let Some(existing_contact_name) = existing_contact_name {
                    let provider_label = match provider_type.as_str() {
                        "email" => "Email",
                        "sms" => "Phone number",
                        "nostr" => "Nostr recipient",
                        "webhook" => "Webhook URL",
                        "telegram" => "Telegram chat",
                        _ => "Notification target",
                    };
                    duplicates.push(format!(
                        "{provider_label} '{notification_target}' is already used by contact '{existing_contact_name}'"
                    ));
                }
            }

            Ok(duplicates)
        })
        .await?
    }
}
