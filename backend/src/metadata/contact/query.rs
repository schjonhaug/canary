//! Contact reads: counts and contacts loaded together with their notification methods.

use crate::metadata::pool::MetadataDb;
use crate::metadata::types::{
    Contact, NotificationContentFields, NotificationMethod, ProviderType,
};
use anyhow::Result;
use bdk_wallet::rusqlite::{params, ToSql};
use phonenumber::PhoneNumber;
use std::str::FromStr;

impl MetadataDb {
    pub async fn count_contacts_for_wallet(&self, wallet_checksum: &str) -> Result<usize> {
        let checksum = wallet_checksum.to_string();

        self.with_conn(move |conn| {
            let count: usize = conn.query_row(
                "SELECT COUNT(*) FROM contacts WHERE wallet_checksum = ?1",
                params![checksum],
                |row| row.get(0),
            )?;
            Ok(count)
        })
        .await
    }

    pub async fn count_active_contacts_for_wallet(&self, wallet_checksum: &str) -> Result<usize> {
        let checksum = wallet_checksum.to_string();

        self.with_conn(move |conn| {
            let count: usize = conn.query_row(
                "SELECT COUNT(*) FROM contacts WHERE wallet_checksum = ?1 AND is_active = 1",
                params![checksum],
                |row| row.get(0),
            )?;
            Ok(count)
        })
        .await
    }

    pub async fn get_contacts_with_notification_methods(
        &self,
        wallet_checksum: &str,
    ) -> Result<Vec<Contact>> {
        self.get_contacts_with_notification_methods_filtered(wallet_checksum, false)
            .await
    }

    /// Get contacts for subscription limits ordered by creation time (oldest first)
    pub async fn get_contacts_oldest_first_for_limits(
        &self,
        wallet_checksum: &str,
    ) -> Result<Vec<Contact>> {
        let checksum = wallet_checksum.to_string();

        self.with_conn(move |conn| {
            // Get ALL contacts ordered by created_at ASC (oldest first) for limits enforcement
            let query = "SELECT id, wallet_checksum, name, created_at, is_active,
                                notify_sending, notify_sent, notify_receiving, notify_received,
                                notify_cpfp, notify_rbf, include_wallet_balance_in_tx_notifications
                         FROM contacts
                         WHERE wallet_checksum = ?1 ORDER BY created_at ASC";
            let mut stmt = conn.prepare(query)?;

            let contact_iter = stmt.query_map(params![checksum], |row| {
                Ok((
                    row.get::<_, String>(0)?, // id as UUIDv4
                    Contact {
                        id: Some(row.get(0)?),
                        wallet_checksum: row.get(1)?,
                        name: row.get(2)?,
                        notification_methods: Vec::new(), // Will be populated below
                        created_at: row.get(3)?,
                        is_active: row.get::<_, i64>(4).unwrap_or(1) != 0, // SQLite stores bool as int
                        notify_sending: row.get::<_, i64>(5).unwrap_or(1) != 0,
                        notify_sent: row.get::<_, i64>(6).unwrap_or(1) != 0,
                        notify_receiving: row.get::<_, i64>(7).unwrap_or(1) != 0,
                        notify_received: row.get::<_, i64>(8).unwrap_or(1) != 0,
                        notify_cpfp: row.get::<_, i64>(9).unwrap_or(1) != 0,
                        notify_rbf: row.get::<_, i64>(10).unwrap_or(1) != 0,
                        include_wallet_balance_in_tx_notifications: row
                            .get::<_, i64>(11)
                            .unwrap_or(0)
                            != 0,
                    },
                ))
            })?;

            let mut contacts: std::collections::HashMap<String, Contact> =
                std::collections::HashMap::new();
            for result in contact_iter {
                let (id, contact) = result?;
                contacts.insert(id, contact);
            }

            // Fetch all notification methods using IN clause (avoid N+1)
            // Chunk IDs to avoid hitting SQL parameter limits (SQLite default is 999)
            let contact_ids: Vec<String> = contacts.keys().cloned().collect();
            for ids_chunk in contact_ids.chunks(500) {
                let placeholders = ids_chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let methods_query = format!(
                    "SELECT id, contact_id, provider_type, notification_target, created_at, is_enabled,
                            content_wallet_name, content_event_type, content_transaction_amount,
                            content_transaction_balance, content_balance_alert_condition,
                            content_balance_alert_threshold, content_balance_alert_balance
                     FROM contact_notification_methods
                     WHERE contact_id IN ({placeholders}) ORDER BY contact_id"
                );

                let mut methods_stmt = conn.prepare(&methods_query)?;
                let method_params: Vec<&dyn ToSql> =
                    ids_chunk.iter().map(|id| id as &dyn ToSql).collect();

                let methods_iter = methods_stmt.query_map(method_params.as_slice(), |row| {
                    let provider_str: String = row.get(2)?;
                    Ok(NotificationMethod {
                        id: Some(row.get(0)?),
                        contact_id: row.get(1)?,
                        provider_type: ProviderType::from(provider_str.as_str()),
                        notification_target: row.get(3)?,
                        display_target: None,
                        created_at: row.get(4)?,
                        is_enabled: row.get::<_, i64>(5).unwrap_or(1) != 0,
                        content_fields: content_fields_from_row(row, 6)?,
                    })
                })?;

                for method_result in methods_iter {
                    let method = method_result?;
                    if let Some(contact) = contacts.get_mut(&method.contact_id) {
                        contact.notification_methods.push(method);
                    }
                }
            }

            let mut contacts: Vec<Contact> = contacts.into_values().collect();
            contacts.sort_by(|a, b| {
                a.name
                    .to_lowercase()
                    .cmp(&b.name.to_lowercase())
                    .then_with(|| a.created_at.cmp(&b.created_at))
            });
            Ok(contacts)
        })
        .await
    }

    pub async fn get_contacts_with_notification_methods_filtered(
        &self,
        wallet_checksum: &str,
        include_inactive: bool,
    ) -> Result<Vec<Contact>> {
        let checksum = wallet_checksum.to_string();

        self.with_conn(move |conn| {
            // Get contacts for the wallet (active only or all based on parameter)
            let query = if include_inactive {
                "SELECT id, wallet_checksum, name, created_at, is_active,
                        notify_sending, notify_sent, notify_receiving, notify_received,
                        notify_cpfp, notify_rbf, include_wallet_balance_in_tx_notifications
                 FROM contacts
                 WHERE wallet_checksum = ?1 ORDER BY name, created_at"
            } else {
                "SELECT id, wallet_checksum, name, created_at, 1 as is_active,
                        notify_sending, notify_sent, notify_receiving, notify_received,
                        notify_cpfp, notify_rbf, include_wallet_balance_in_tx_notifications
                 FROM contacts
                 WHERE wallet_checksum = ?1 AND is_active = 1 ORDER BY name, created_at"
            };
            let mut stmt = conn.prepare(query)?;

            let contact_iter = stmt.query_map(params![checksum], |row| {
                Ok((
                    row.get::<_, String>(0)?, // id as UUIDv4
                    Contact {
                        id: Some(row.get(0)?),
                        wallet_checksum: row.get(1)?,
                        name: row.get(2)?,
                        notification_methods: Vec::new(), // Will be populated below
                        created_at: row.get(3)?,
                        is_active: row.get::<_, i64>(4).unwrap_or(1) != 0, // SQLite stores bool as int
                        notify_sending: row.get::<_, i64>(5).unwrap_or(1) != 0,
                        notify_sent: row.get::<_, i64>(6).unwrap_or(1) != 0,
                        notify_receiving: row.get::<_, i64>(7).unwrap_or(1) != 0,
                        notify_received: row.get::<_, i64>(8).unwrap_or(1) != 0,
                        notify_cpfp: row.get::<_, i64>(9).unwrap_or(1) != 0,
                        notify_rbf: row.get::<_, i64>(10).unwrap_or(1) != 0,
                        include_wallet_balance_in_tx_notifications: row
                            .get::<_, i64>(11)
                            .unwrap_or(0)
                            != 0,
                    },
                ))
            })?;

            let mut contacts: std::collections::HashMap<String, Contact> =
                std::collections::HashMap::new();
            for contact_result in contact_iter {
                let (contact_id, contact) = contact_result?;
                contacts.insert(contact_id, contact);
            }

            // Now get all notification methods for these contacts
            let contact_ids: Vec<String> = contacts.keys().cloned().collect();
            if !contact_ids.is_empty() {
                let placeholders = contact_ids
                    .iter()
                    .map(|_| "?")
                    .collect::<Vec<_>>()
                    .join(",");
                let query = format!(
                    "SELECT id, contact_id, provider_type, notification_target, created_at, is_enabled,
                            content_wallet_name, content_event_type, content_transaction_amount,
                            content_transaction_balance, content_balance_alert_condition,
                            content_balance_alert_threshold, content_balance_alert_balance
                     FROM contact_notification_methods
                     WHERE contact_id IN ({placeholders}) ORDER BY contact_id, provider_type"
                );

                let mut method_stmt = conn.prepare(&query)?;
                let method_params: Vec<&dyn ToSql> =
                    contact_ids.iter().map(|id| id as &dyn ToSql).collect();

                let method_iter = method_stmt.query_map(method_params.as_slice(), |row| {
                    let provider_type_str: String = row.get(2)?;
                    let provider_type = ProviderType::from(provider_type_str.as_str());
                    let notification_target: String = row.get(3)?;

                    let display_target =
                        display_target_for_method(&provider_type, &notification_target);

                    Ok(NotificationMethod {
                        id: Some(row.get(0)?),
                        contact_id: row.get(1)?,
                        provider_type,
                        notification_target,
                        display_target,
                        created_at: row.get(4)?,
                        is_enabled: row.get::<_, i64>(5).unwrap_or(1) != 0,
                        content_fields: content_fields_from_row(row, 6)?,
                    })
                })?;

                // Add notification methods to their corresponding contacts
                for method_result in method_iter {
                    let method = method_result?;
                    if let Some(contact) = contacts.get_mut(&method.contact_id) {
                        contact.notification_methods.push(method);
                    }
                }
            }

            let mut contacts: Vec<Contact> = contacts.into_values().collect();
            contacts.sort_by(|a, b| {
                a.name
                    .to_lowercase()
                    .cmp(&b.name.to_lowercase())
                    .then_with(|| a.created_at.cmp(&b.created_at))
            });
            Ok(contacts)
        })
        .await
    }

    /// Get a single contact with its notification methods by ID and wallet checksum
    pub async fn get_single_contact_with_methods(
        &self,
        contact_id: &str,
        wallet_checksum: &str,
    ) -> Result<Option<Contact>> {
        let contact_id = contact_id.to_string();
        let checksum = wallet_checksum.to_string();

        self.with_conn(move |conn| {
            // Get the contact
            let query = "SELECT id, wallet_checksum, name, created_at, is_active,
                                notify_sending, notify_sent, notify_receiving, notify_received,
                                notify_cpfp, notify_rbf, include_wallet_balance_in_tx_notifications
                         FROM contacts
                         WHERE id = ?1 AND wallet_checksum = ?2";
            let mut stmt = conn.prepare(query)?;
            let contact_result = stmt.query_row(params![contact_id, checksum], |row| {
                Ok(Contact {
                    id: Some(row.get(0)?),
                    wallet_checksum: row.get(1)?,
                    name: row.get(2)?,
                    notification_methods: Vec::new(), // Will be populated below
                    created_at: row.get(3)?,
                    is_active: row.get::<_, i64>(4).unwrap_or(1) != 0, // SQLite stores bool as int
                    notify_sending: row.get::<_, i64>(5).unwrap_or(1) != 0,
                    notify_sent: row.get::<_, i64>(6).unwrap_or(1) != 0,
                    notify_receiving: row.get::<_, i64>(7).unwrap_or(1) != 0,
                    notify_received: row.get::<_, i64>(8).unwrap_or(1) != 0,
                    notify_cpfp: row.get::<_, i64>(9).unwrap_or(1) != 0,
                    notify_rbf: row.get::<_, i64>(10).unwrap_or(1) != 0,
                    include_wallet_balance_in_tx_notifications: row.get::<_, i64>(11).unwrap_or(0)
                        != 0,
                })
            });

            let mut contact = match contact_result {
                Ok(contact) => contact,
                Err(bdk_wallet::rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
                Err(e) => return Err(e.into()),
            };

            // Get notification methods for this contact
            let methods_query =
                "SELECT id, provider_type, notification_target, created_at, is_enabled,
                        content_wallet_name, content_event_type, content_transaction_amount,
                        content_transaction_balance, content_balance_alert_condition,
                        content_balance_alert_threshold, content_balance_alert_balance
                               FROM contact_notification_methods
                               WHERE contact_id = ?1";
            let mut methods_stmt = conn.prepare(methods_query)?;
            let methods_iter = methods_stmt.query_map(params![contact_id], |row| {
                let provider_type_str: String = row.get(1)?;
                let provider_type = ProviderType::from(provider_type_str.as_str());
                let notification_target: String = row.get(2)?;

                let display_target =
                    display_target_for_method(&provider_type, &notification_target);

                Ok(NotificationMethod {
                    id: Some(row.get(0)?),
                    contact_id: contact_id.clone(),
                    provider_type,
                    notification_target,
                    display_target,
                    created_at: row.get(3)?,
                    is_enabled: row.get::<_, i64>(4).unwrap_or(1) != 0,
                    content_fields: content_fields_from_row(row, 5)?,
                })
            })?;

            for method_result in methods_iter {
                contact.notification_methods.push(method_result?);
            }

            Ok(Some(contact))
        })
        .await
    }
}

fn display_target_for_method(
    provider_type: &ProviderType,
    notification_target: &str,
) -> Option<String> {
    match provider_type {
        ProviderType::Sms => PhoneNumber::from_str(notification_target)
            .ok()
            .map(|phone| {
                phone
                    .format()
                    .mode(phonenumber::Mode::International)
                    .to_string()
            }),
        ProviderType::Nostr => crate::nostr_provider::nostr_display_target(notification_target),
        ProviderType::Webhook => Some(crate::webhook_provider::redact_webhook_url(
            notification_target,
        )),
        _ => None,
    }
}

fn content_fields_from_row(
    row: &bdk_wallet::rusqlite::Row<'_>,
    start: usize,
) -> bdk_wallet::rusqlite::Result<NotificationContentFields> {
    Ok(NotificationContentFields {
        wallet_name: row.get::<_, i64>(start)? != 0,
        event_type: row.get::<_, i64>(start + 1)? != 0,
        transaction_amount: row.get::<_, i64>(start + 2)? != 0,
        transaction_balance: row.get::<_, i64>(start + 3)? != 0,
        balance_alert_condition: row.get::<_, i64>(start + 4)? != 0,
        balance_alert_threshold: row.get::<_, i64>(start + 5)? != 0,
        balance_alert_balance: row.get::<_, i64>(start + 6)? != 0,
    })
}
