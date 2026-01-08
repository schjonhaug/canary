use super::pool::MetadataDb;
use super::types::*;
use crate::electrum::BlockHeader;
use crate::exchange_rates;
use anyhow::Result;
use bdk_wallet::rusqlite::{params, OptionalExtension, ToSql};
use phonenumber::PhoneNumber;
use std::str::FromStr;
use tokio::task::spawn_blocking;
use uuid::Uuid;

impl MetadataDb {
    // ============================
    // CONTACT OPERATIONS
    // ============================

    pub async fn count_contacts_for_wallet(&self, wallet_checksum: &str) -> Result<usize> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();

        spawn_blocking(move || -> Result<usize> {
            let conn = pool.get()?;
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM contacts WHERE wallet_checksum = ?1",
                params![checksum],
                |row| row.get(0),
            )?;
            Ok(count as usize)
        })
        .await?
    }

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
        let exclude_id = exclude_contact_id.map(|s| s.to_string());

        spawn_blocking(move || -> Result<Option<String>> {
            let conn = pool.get()?;

            // For emails, do case-insensitive comparison
            let existing_contact_name: Option<String> = if provider == "email" {
                if let Some(contact_id) = exclude_id {
                    conn.query_row(
                        "SELECT c.name FROM contacts c
                         JOIN contact_notification_methods cnm ON c.id = cnm.contact_id
                         WHERE cnm.wallet_checksum = ?1
                         AND cnm.provider_type = ?2
                         AND LOWER(cnm.notification_target) = LOWER(?3)
                         AND c.id != ?4
                         LIMIT 1",
                        params![&checksum, &provider, &target, &contact_id],
                        |row| row.get(0),
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
                        params![&checksum, &provider, &target],
                        |row| row.get(0),
                    )
                    .optional()?
                }
            } else if let Some(contact_id) = exclude_id {
                // For SMS and other types, exact match
                conn.query_row(
                    "SELECT c.name FROM contacts c
                     JOIN contact_notification_methods cnm ON c.id = cnm.contact_id
                     WHERE cnm.wallet_checksum = ?1
                     AND cnm.provider_type = ?2
                     AND cnm.notification_target = ?3
                     AND c.id != ?4
                     LIMIT 1",
                    params![&checksum, &provider, &target, &contact_id],
                    |row| row.get(0),
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
                    params![&checksum, &provider, &target],
                    |row| row.get(0),
                )
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
        let exclude_id = exclude_contact_id.map(|s| s.to_string());

        spawn_blocking(move || -> Result<Vec<String>> {
            let conn = pool.get()?;
            let mut duplicates = Vec::new();

            for (provider_type, notification_target) in &methods {
                // Skip ntfy as it's auto-generated and unique
                if provider_type == "ntfy" {
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
                    let provider_label = if provider_type == "email" {
                        "Email"
                    } else {
                        "Phone number"
                    };
                    duplicates.push(format!(
                        "{} '{}' is already used by contact '{}'",
                        provider_label, notification_target, existing_contact_name
                    ));
                }
            }

            Ok(duplicates)
        })
        .await?
    }

    // ============================
    // TRANSACTION OPERATIONS
    // ============================
    pub async fn insert_transaction(&self, transaction: &TransactionInsert) -> Result<String> {
        let pool = self.pool.clone();
        let transaction = transaction.clone();

        spawn_blocking(move || -> Result<String> {
            let conn = pool.get()?;
            conn.execute(
                "INSERT OR IGNORE INTO transactions (txid, wallet_checksum, transaction_type, amount_sats, fee_sats, block_height, first_seen_at, confirmed_at, parent_txid, transaction_status, replaced_by_txid, replaced_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    &transaction.txid,
                    &transaction.wallet_checksum,
                    transaction.transaction_type.as_str(),
                    transaction.amount_sats,
                    transaction.fee_sats,
                    transaction.block_height,
                    transaction.first_seen_at,
                    transaction.confirmed_at,
                    transaction.parent_txid.as_ref(),
                    &transaction.transaction_status,
                    transaction.replaced_by_txid.as_ref(),
                    transaction.replaced_at,
                ],
            )?;
            Ok(transaction.txid.clone())
        }).await?
    }

    pub async fn get_transaction_by_txid(
        &self,
        wallet_checksum: &str,
        txid: &str,
    ) -> Result<Option<Transaction>> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();
        let txid = txid.to_string();

        spawn_blocking(move || -> Result<Option<Transaction>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT txid, wallet_checksum, transaction_type, amount_sats, fee_sats, block_height, first_seen_at, confirmed_at, parent_txid, transaction_status, replaced_by_txid, replaced_at
                 FROM transactions
                 WHERE wallet_checksum = ?1 AND txid = ?2"
            )?;

            let mut rows = stmt.query_map([&checksum, &txid], |row| {
                Ok(Transaction {
                    txid: row.get(0)?,
                    wallet_checksum: row.get(1)?,
                    transaction_type: EventType::try_from(row.get::<_, String>(2)?.as_str())
                        .map_err(|e| bdk_wallet::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                    amount_sats: row.get(3)?,
                    fee_sats: row.get(4)?,
                    block_height: row.get(5)?,
                    first_seen_at: row.get(6)?,
                    confirmed_at: row.get(7)?,
                    parent_txid: row.get(8)?,
                    transaction_status: row.get(9)?,
                    replaced_by_txid: row.get(10)?,
                    replaced_at: row.get(11)?,
                    notification_status: vec![], // Will be populated by calling code if needed
                })
            })?;

            match rows.next() {
                Some(row) => Ok(Some(row?)),
                None => Ok(None),
            }
        }).await?
    }

    pub async fn update_transaction_confirmation(
        &self,
        wallet_checksum: &str,
        txid: &str,
        block_height: u32,
        confirmed_at: u64,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();
        let txid = txid.to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let changes = conn.execute(
                "UPDATE transactions SET block_height = ?1, confirmed_at = ?2, transaction_status = 'confirmed' WHERE wallet_checksum = ?3 AND txid = ?4",
                params![block_height, confirmed_at, &checksum, &txid],
            )?;
            Ok(changes > 0)
        }).await?
    }

    pub async fn get_transactions_by_wallet_checksum(
        &self,
        wallet_checksum: &str,
        limit: Option<usize>,
    ) -> Result<Vec<TransactionWithWallet>> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();
        let limit = limit.unwrap_or(10000); // Large default for sync operations

        spawn_blocking(move || -> Result<Vec<TransactionWithWallet>> {
            let conn = pool.get()?;

            // First get transactions
            let mut stmt = conn.prepare(
                "SELECT t.txid, t.wallet_checksum, w.name, t.transaction_type, t.amount_sats, t.fee_sats, t.block_height, t.first_seen_at, t.confirmed_at, t.parent_txid, t.transaction_status, t.replaced_by_txid, t.replaced_at
                 FROM transactions t
                 JOIN wallets w ON t.wallet_checksum = w.checksum
                 WHERE t.wallet_checksum = ?1
                 ORDER BY t.first_seen_at DESC, t.txid DESC
                 LIMIT ?2"
            )?;

            let transaction_iter = stmt.query_map([&checksum, &limit.to_string()], |row| {
                Ok(TransactionWithWallet {
                    txid: row.get(0)?,
                    wallet_checksum: row.get(1)?,
                    wallet_name: row.get(2)?,
                    transaction_type: EventType::try_from(row.get::<_, String>(3)?.as_str())
                        .map_err(|e| bdk_wallet::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                    amount_sats: row.get(4)?,
                    fee_sats: row.get(5)?,
                    block_height: row.get(6)?,
                    first_seen_at: row.get(7)?,
                    confirmed_at: row.get(8)?,
                    parent_txid: row.get(9)?,
                    transaction_status: row.get(10)?,
                    replaced_by_txid: row.get(11)?,
                    replaced_at: row.get(12)?,
                    notification_status: vec![], // Will be populated below
                })
            })?;

            let mut transactions = Vec::new();
            for transaction in transaction_iter {
                let mut tx = transaction?;

                // Get notification status for this transaction
                let mut notification_stmt = conn.prepare(
                    "SELECT nl.contact_name_snapshot, nl.provider_name, nl.status, nl.error_message,
                            nl.notification_target_snapshot, nl.provider_type_snapshot, nl.created_at, nl.message_content, nl.notification_type
                     FROM notification_logs nl
                     WHERE nl.transaction_txid = ?1 AND nl.transaction_wallet_checksum = ?2
                     ORDER BY nl.created_at ASC"
                )?;

                let notification_iter = notification_stmt.query_map([&tx.txid, &tx.wallet_checksum], |row| {
                    let notification_type: String = row.get(8)?;

                    Ok(NotificationStatus {
                        contact_name: row.get::<_, Option<String>>(0)?.unwrap_or("Unknown".to_string()),
                        provider_name: row.get(1)?,
                        status: row.get(2)?,
                        error_message: row.get(3)?,
                        notification_target: row.get(4)?,
                        provider_type: row.get(5)?,
                        created_at: row.get(6)?,
                        notification_type,
                    })
                })?;

                for notification in notification_iter {
                    tx.notification_status.push(notification?);
                }

                transactions.push(tx);
            }

            Ok(transactions)
        }).await?
    }

    // Normalized contact methods
    pub async fn insert_contact_with_notification_methods(
        &self,
        wallet_checksum: &str,
        name: &str,
        notification_methods: Vec<(ProviderType, String)>,
    ) -> Result<String> {
        let pool = self.pool.clone();
        let name = name.to_string();
        let checksum = wallet_checksum.to_string();

        spawn_blocking(move || -> Result<String> {
            let conn = pool.get()?;
            let tx = conn.unchecked_transaction()?;

            // Insert contact with UUID
            let contact_id = Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO contacts (id, wallet_checksum, name) VALUES (?1, ?2, ?3)",
                params![&contact_id, checksum, &name],
            )?;

            // Insert notification methods
            for (provider_type, notification_target) in notification_methods {
                let method_id = Uuid::new_v4().to_string();
                tx.execute(
                    "INSERT INTO contact_notification_methods (id, contact_id, provider_type, notification_target, wallet_checksum) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![&method_id, &contact_id, provider_type.as_str(), &notification_target, &checksum],
                )?;
            }

            tx.commit()?;
            Ok(contact_id)
        }).await?
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
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();

        spawn_blocking(move || -> Result<Vec<Contact>> {
            let conn = pool.get()?;

            // Get ALL contacts ordered by created_at ASC (oldest first) for limits enforcement
            let query = "SELECT id, wallet_checksum, name, created_at, is_active
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
                    },
                ))
            })?;

            let mut contacts: std::collections::HashMap<String, Contact> =
                std::collections::HashMap::new();
            for result in contact_iter {
                let (id, contact) = result?;
                contacts.insert(id, contact);
            }

            // Get notification methods for each contact
            for (contact_id, contact) in contacts.iter_mut() {
                let methods_query = "SELECT id, provider_type, notification_target, created_at
                                   FROM contact_notification_methods
                                   WHERE contact_id = ?1";
                let mut methods_stmt = conn.prepare(methods_query)?;

                let methods_iter = methods_stmt.query_map(params![contact_id], |row| {
                    let provider_str: String = row.get(1)?;
                    Ok(NotificationMethod {
                        id: Some(row.get(0)?),
                        contact_id: contact_id.clone(),
                        provider_type: ProviderType::from(provider_str.as_str()),
                        notification_target: row.get(2)?,
                        display_target: None,
                        created_at: row.get(3)?,
                    })
                })?;

                for method_result in methods_iter {
                    contact.notification_methods.push(method_result?);
                }
            }

            Ok(contacts.into_values().collect())
        })
        .await?
    }

    pub async fn get_contacts_with_notification_methods_filtered(
        &self,
        wallet_checksum: &str,
        include_inactive: bool,
    ) -> Result<Vec<Contact>> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();

        spawn_blocking(move || -> Result<Vec<Contact>> {
            let conn = pool.get()?;

            // Get contacts for the wallet (active only or all based on parameter)
            let query = if include_inactive {
                "SELECT id, wallet_checksum, name, created_at, is_active
                 FROM contacts
                 WHERE wallet_checksum = ?1 ORDER BY name, created_at"
            } else {
                "SELECT id, wallet_checksum, name, created_at, 1 as is_active
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
                    "SELECT id, contact_id, provider_type, notification_target, created_at
                     FROM contact_notification_methods
                     WHERE contact_id IN ({}) ORDER BY contact_id, provider_type",
                    placeholders
                );

                let mut method_stmt = conn.prepare(&query)?;
                let method_params: Vec<&dyn ToSql> =
                    contact_ids.iter().map(|id| id as &dyn ToSql).collect();

                let method_iter = method_stmt.query_map(method_params.as_slice(), |row| {
                    let provider_type_str: String = row.get(2)?;
                    let provider_type = ProviderType::from(provider_type_str.as_str());
                    let notification_target: String = row.get(3)?;

                    // Format phone numbers for display
                    let display_target = if provider_type == ProviderType::Sms {
                        PhoneNumber::from_str(&notification_target)
                            .ok()
                            .map(|phone| {
                                phone
                                    .format()
                                    .mode(phonenumber::Mode::International)
                                    .to_string()
                            })
                    } else {
                        None
                    };

                    Ok(NotificationMethod {
                        id: Some(row.get(0)?),
                        contact_id: row.get(1)?,
                        provider_type,
                        notification_target,
                        display_target,
                        created_at: row.get(4)?,
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

            Ok(contacts.into_values().collect())
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

    /// Get a single contact with its notification methods by ID and wallet checksum
    pub async fn get_single_contact_with_methods(
        &self,
        contact_id: &str,
        wallet_checksum: &str,
    ) -> Result<Option<Contact>> {
        let pool = self.pool.clone();
        let contact_id = contact_id.to_string();
        let checksum = wallet_checksum.to_string();

        spawn_blocking(move || -> Result<Option<Contact>> {
            let conn = pool.get()?;

            // Get the contact
            let query = "SELECT id, wallet_checksum, name, created_at, is_active
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
                })
            });

            let mut contact = match contact_result {
                Ok(contact) => contact,
                Err(bdk_wallet::rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
                Err(e) => return Err(e.into()),
            };

            // Get notification methods for this contact
            let methods_query = "SELECT id, provider_type, notification_target, created_at
                               FROM contact_notification_methods
                               WHERE contact_id = ?1";
            let mut methods_stmt = conn.prepare(methods_query)?;
            let methods_iter = methods_stmt.query_map(params![contact_id], |row| {
                let provider_type_str: String = row.get(1)?;
                let provider_type = ProviderType::from(provider_type_str.as_str());
                let notification_target: String = row.get(2)?;

                // Format phone numbers for display
                let display_target = if provider_type == ProviderType::Sms {
                    PhoneNumber::from_str(&notification_target)
                        .ok()
                        .map(|phone| {
                            phone
                                .format()
                                .mode(phonenumber::Mode::International)
                                .to_string()
                        })
                } else {
                    None
                };

                Ok(NotificationMethod {
                    id: Some(row.get(0)?),
                    contact_id: contact_id.clone(),
                    provider_type,
                    notification_target,
                    display_target,
                    created_at: row.get(3)?,
                })
            })?;

            for method_result in methods_iter {
                contact.notification_methods.push(method_result?);
            }

            Ok(Some(contact))
        })
        .await?
    }

    // ============================
    // BLOCKCHAIN OPERATIONS
    // ============================

    pub async fn upsert_current_block_header(&self, block_header: &BlockHeader) -> Result<()> {
        let pool = self.pool.clone();
        let block_header = block_header.clone();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "INSERT OR REPLACE INTO current_block_header (id, height, timestamp, updated_at)
                 VALUES (1, ?1, ?2, datetime('now'))",
                params![block_header.height, block_header.timestamp,],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn get_current_block_header(&self) -> Result<Option<BlockHeader>> {
        let pool = self.pool.clone();

        spawn_blocking(move || -> Result<Option<BlockHeader>> {
            let conn = pool.get()?;
            match conn.query_row(
                "SELECT height, timestamp FROM current_block_header WHERE id = 1",
                [],
                |row| {
                    Ok(BlockHeader {
                        height: row.get::<_, i64>(0)? as u32,
                        timestamp: row.get::<_, i64>(1)? as u64,
                    })
                },
            ) {
                Ok(block_header) => {
                    // Return None if this is the dummy row (height=0)
                    if block_header.height == 0 {
                        Ok(None)
                    } else {
                        Ok(Some(block_header))
                    }
                }
                Err(bdk_wallet::rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
        .await?
    }

    /// Insert notification log for transaction-based notifications (new schema)
    pub async fn insert_notification_log_for_transaction(
        &self,
        transaction_txid: &str,
        transaction_wallet_checksum: &str,
        notification_method_id: &str,
        provider_name: &str,
        provider_message_id: Option<&str>,
        status: &str,
        error_message: Option<&str>,
        message_content: &str,
        notification_type: &str,
    ) -> Result<String> {
        let pool = self.pool.clone();
        let transaction_txid = transaction_txid.to_string();
        let transaction_wallet_checksum = transaction_wallet_checksum.to_string();
        let notification_method_id = notification_method_id.to_string();
        let provider_name = provider_name.to_string();
        let provider_message_id = provider_message_id.map(|s| s.to_string());
        let status = status.to_string();
        let error_message = error_message.map(|s| s.to_string());
        let message_content = message_content.to_string();
        let notification_type = notification_type.to_string();

        spawn_blocking(move || -> Result<String> {
            let conn = pool.get()?;
            let log_id = uuid::Uuid::new_v4().to_string();

            // Get the contact info at the time of notification to preserve it
            let (contact_name, notification_target, provider_type): (String, String, String) = conn.query_row(
                "SELECT c.name, cnm.notification_target, cnm.provider_type
                 FROM contact_notification_methods cnm
                 JOIN contacts c ON cnm.contact_id = c.id
                 WHERE cnm.id = ?1",
                params![&notification_method_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            ).unwrap_or_else(|_| ("Unknown Contact".to_string(), "Unknown Target".to_string(), "unknown".to_string()));

            conn.execute(
                "INSERT INTO notification_logs (id, transaction_txid, transaction_wallet_checksum, notification_method_id, provider_name, provider_message_id, status, error_message, message_content, notification_type, contact_name_snapshot, notification_target_snapshot, provider_type_snapshot)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    &log_id,
                    &transaction_txid,
                    &transaction_wallet_checksum,
                    &notification_method_id,
                    &provider_name,
                    &provider_message_id,
                    &status,
                    &error_message,
                    &message_content,
                    &notification_type,
                    &contact_name,
                    &notification_target,
                    &provider_type,
                ],
            )?;
            Ok(log_id)
        }).await?
    }

    /// Insert notification log for balance alert notifications (separate from transactions)
    pub async fn insert_notification_log_for_balance_alert(
        &self,
        balance_alert_id: &str,
        wallet_checksum: &str,
        notification_method_id: &str,
        provider_name: &str,
        provider_message_id: Option<&str>,
        status: &str,
        error_message: Option<&str>,
        message_content: &str,
    ) -> Result<String> {
        let pool = self.pool.clone();
        let balance_alert_id = balance_alert_id.to_string();
        let wallet_checksum = wallet_checksum.to_string();
        let notification_method_id = notification_method_id.to_string();
        let provider_name = provider_name.to_string();
        let provider_message_id = provider_message_id.map(|s| s.to_string());
        let status = status.to_string();
        let error_message = error_message.map(|s| s.to_string());
        let message_content = message_content.to_string();

        spawn_blocking(move || -> Result<String> {
            let conn = pool.get()?;
            let log_id = uuid::Uuid::new_v4().to_string();

            // Get contact info for snapshot
            let (contact_name, notification_target, provider_type) = conn
                .prepare(
                    "SELECT c.name, cnm.notification_target, cnm.provider_type
                     FROM contact_notification_methods cnm
                     JOIN contacts c ON cnm.contact_id = c.id
                     WHERE cnm.id = ?1"
                )?
                .query_row(params![&notification_method_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                }).unwrap_or_else(|_| ("Unknown Contact".to_string(), "Unknown Target".to_string(), "unknown".to_string()));

            conn.execute(
                "INSERT INTO balance_alert_notification_logs (id, balance_alert_id, wallet_checksum, notification_method_id, provider_name, provider_message_id, status, error_message, message_content, contact_name_snapshot, notification_target_snapshot, provider_type_snapshot)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    &log_id,
                    &balance_alert_id,
                    &wallet_checksum,
                    &notification_method_id,
                    &provider_name,
                    &provider_message_id,
                    &status,
                    &error_message,
                    &message_content,
                    &contact_name,
                    &notification_target,
                    &provider_type,
                ],
            )?;
            Ok(log_id)
        }).await?
    }

    // Rate limiting for OTP (SMS contact verification only)
    pub async fn check_rate_limit(&self, phone_number: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let phone_number = phone_number.to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let current_time = chrono::Utc::now();
            let current_time_str = current_time.format("%Y-%m-%d %H:%M:%S").to_string();

            // Check if blocked
            let blocked: Option<String> = conn
                .prepare("SELECT blocked_until FROM otp_attempts WHERE phone_number = ?1")?
                .query_row(params![&phone_number], |row| row.get(0))
                .ok();

            if let Some(blocked_until) = blocked {
                if blocked_until > current_time_str {
                    return Ok(false); // Still blocked
                }
            }

            // Check recent attempts (last 15 minutes)
            let fifteen_minutes_ago = (current_time - chrono::Duration::minutes(15))
                .format("%Y-%m-%d %H:%M:%S").to_string();

            let recent_attempts: i32 = conn
                .prepare("SELECT attempt_count FROM otp_attempts WHERE phone_number = ?1 AND last_attempt > ?2")?
                .query_row(params![&phone_number, &fifteen_minutes_ago], |row| row.get(0))
                .unwrap_or(0);

            if recent_attempts >= 5 {
                // Block for 30 minutes
                let blocked_until = (current_time + chrono::Duration::minutes(30))
                    .format("%Y-%m-%d %H:%M:%S").to_string();

                conn.execute(
                    "UPDATE otp_attempts SET blocked_until = ?1 WHERE phone_number = ?2",
                    params![&blocked_until, &phone_number],
                )?;

                return Ok(false);
            }

            // Update attempt count
            let exists: bool = conn
                .prepare("SELECT 1 FROM otp_attempts WHERE phone_number = ?1")?
                .exists(params![&phone_number])?;

            if exists {
                conn.execute(
                    "UPDATE otp_attempts SET attempt_count = attempt_count + 1, last_attempt = ?1 WHERE phone_number = ?2",
                    params![&current_time_str, &phone_number],
                )?;
            } else {
                conn.execute(
                    "INSERT INTO otp_attempts (phone_number, attempt_count, last_attempt) VALUES (?1, 1, ?2)",
                    params![&phone_number, &current_time_str],
                )?;
            }

            Ok(true)
        }).await?
    }

    pub async fn clear_rate_limit(&self, phone_number: &str) -> Result<()> {
        let pool = self.pool.clone();
        let phone_number = phone_number.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "DELETE FROM otp_attempts WHERE phone_number = ?1",
                params![&phone_number],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn create_pending_contact_verification(
        &self,
        wallet_checksum: &str,
        provider_type: &str,
        notification_target: &str,
        contact_name: &str,
        verification_code: Option<&str>,
    ) -> Result<i64> {
        let pool = self.pool.clone();
        let wallet_checksum = wallet_checksum.to_string();
        let provider_type = provider_type.to_string();
        let notification_target = notification_target.to_string();
        let contact_name = contact_name.to_string();
        let verification_code = verification_code.map(|s| s.to_string());

        spawn_blocking(move || {
            let conn = pool.get()?;
            let expires_at = chrono::Utc::now() + chrono::Duration::minutes(10);

            conn.execute(
                "INSERT INTO pending_contact_verifications
                 (wallet_checksum, provider_type, notification_target, contact_name, verification_code, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    &wallet_checksum,
                    &provider_type,
                    &notification_target,
                    &contact_name,
                    &verification_code,
                    expires_at.to_rfc3339()
                ],
            )?;
            Ok(conn.last_insert_rowid())
        }).await?
    }

    pub async fn get_pending_verification(
        &self,
        wallet_checksum: &str,
        notification_target: &str,
    ) -> Result<Option<(i64, String, Option<String>)>> {
        let pool = self.pool.clone();
        let wallet_checksum = wallet_checksum.to_string();
        let notification_target = notification_target.to_string();

        spawn_blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, contact_name, verification_code
                 FROM pending_contact_verifications
                 WHERE wallet_checksum = ?1
                 AND notification_target = ?2
                 AND expires_at > datetime('now')
                 ORDER BY created_at DESC
                 LIMIT 1",
            )?;

            let result = stmt
                .query_row(params![&wallet_checksum, &notification_target], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
                .optional()?;

            Ok(result)
        })
        .await?
    }

    /// Mark a pending verification as completed (used for contact updates)
    /// This keeps the verification record for the PUT endpoint to find
    pub async fn mark_verification_completed(&self, verification_id: i64) -> Result<()> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE pending_contact_verifications
                 SET verified_at = CURRENT_TIMESTAMP
                 WHERE id = ?1",
                params![verification_id],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn cleanup_expired_verifications(&self) -> Result<u64> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            let conn = pool.get()?;
            let deleted_expired = conn.execute(
                "DELETE FROM pending_contact_verifications WHERE expires_at <= datetime('now')",
                [],
            )?;

            // Also clean up old completed verifications (older than 24 hours)
            let deleted_completed = conn.execute(
                "DELETE FROM pending_contact_verifications
                 WHERE verified_at IS NOT NULL
                 AND verified_at <= datetime('now', '-24 hours')",
                [],
            )?;

            Ok((deleted_expired + deleted_completed) as u64)
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

    /// Check if a phone number or email was recently verified for this wallet
    /// Used to ensure security when creating contacts with SMS/email methods
    pub async fn was_recently_verified(
        &self,
        wallet_checksum: &str,
        notification_target: &str,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();
        let target = notification_target.to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM pending_contact_verifications
                 WHERE wallet_checksum = ?1
                 AND notification_target = ?2
                 AND verified_at IS NOT NULL
                 AND verified_at > datetime('now', '-30 minutes')",
                params![checksum, target],
                |row| row.get(0),
            )?;
            Ok(count > 0)
        })
        .await?
    }

    /// Update contact with new methods using a transaction
    /// This ensures atomic updates - if any part fails, the old contact remains unchanged
    pub async fn update_contact_with_methods(
        &self,
        contact_id: &str,
        wallet_checksum: &str,
        name: &str,
        new_methods: Vec<(ProviderType, String)>,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let contact_id = contact_id.to_string();
        let checksum = wallet_checksum.to_string();
        let contact_name = name.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;

            // Start transaction
            conn.execute("BEGIN TRANSACTION", [])?;

            match (|| -> Result<()> {
                // Update contact basics
                conn.execute(
                    "UPDATE contacts SET name = ?1 WHERE id = ?2 AND wallet_checksum = ?3",
                    params![contact_name, contact_id, checksum],
                )?;

                // Check if contact was updated (exists and belongs to wallet)
                let affected: i64 = conn.query_row("SELECT changes()", [], |row| row.get(0))?;

                if affected == 0 {
                    return Err(anyhow::anyhow!("Contact not found or access denied"));
                }

                // Delete all old notification methods
                conn.execute(
                    "DELETE FROM contact_notification_methods WHERE contact_id = ?1",
                    params![contact_id],
                )?;

                // Insert new methods
                for (provider_type, target) in new_methods {
                    let method_id = uuid::Uuid::new_v4().to_string();
                    conn.execute(
                        "INSERT INTO contact_notification_methods
                         (id, contact_id, provider_type, notification_target, wallet_checksum)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![
                            method_id,
                            contact_id,
                            provider_type.as_str(),
                            target,
                            checksum
                        ],
                    )?;
                }

                Ok(())
            })() {
                Ok(()) => {
                    conn.execute("COMMIT", [])?;
                    Ok(())
                }
                Err(e) => {
                    conn.execute("ROLLBACK", [])?;
                    Err(e)
                }
            }
        })
        .await?
    }

    /// Mark a transaction as replaced by another transaction (RBF)
    pub async fn mark_transaction_replaced(
        &self,
        wallet_checksum: &str,
        original_txid: &str,
        replaced_by_txid: &str,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();
        let original_txid = original_txid.to_string();
        let replaced_by_txid = replaced_by_txid.to_string();
        let replaced_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let affected = conn.execute(
                "UPDATE transactions
                 SET transaction_status = 'replaced', replaced_by_txid = ?1, replaced_at = ?2
                 WHERE wallet_checksum = ?3 AND txid = ?4 AND transaction_status = 'pending'",
                params![&replaced_by_txid, replaced_at, &checksum, &original_txid],
            )?;
            Ok(affected > 0)
        })
        .await?
    }

    // Exchange rate methods
    pub async fn get_exchange_rates(
        &self,
    ) -> Result<std::collections::HashMap<String, exchange_rates::ExchangeRate>> {
        let pool = self.pool.clone();
        spawn_blocking(
            move || -> Result<std::collections::HashMap<String, exchange_rates::ExchangeRate>> {
                let conn = pool.get()?;
                let mut stmt = conn
                    .prepare("SELECT currency, rate_per_btc, last_updated FROM exchange_rates")?;
                let rate_iter = stmt.query_map(params![], |row| {
                    Ok(exchange_rates::ExchangeRate {
                        currency: row.get(0)?,
                        rate_per_btc: row.get(1)?,
                        last_updated: chrono::DateTime::parse_from_rfc3339(
                            &row.get::<_, String>(2)?,
                        )
                        .map_err(|e| {
                            bdk_wallet::rusqlite::Error::FromSqlConversionFailure(
                                2,
                                bdk_wallet::rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?
                        .with_timezone(&chrono::Utc),
                    })
                })?;

                let mut rates = std::collections::HashMap::new();
                for rate in rate_iter {
                    let rate = rate?;
                    rates.insert(rate.currency.clone(), rate);
                }
                Ok(rates)
            },
        )
        .await?
    }

    pub async fn store_exchange_rates(
        &self,
        rates: &std::collections::HashMap<String, exchange_rates::ExchangeRate>,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let rates = rates.clone();
        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            let tx = conn.unchecked_transaction()?;

            // Clear old rates
            tx.execute("DELETE FROM exchange_rates", params![])?;

            // Insert new rates
            for (currency, rate) in rates {
                tx.execute(
                    "INSERT INTO exchange_rates (currency, rate_per_btc, last_updated) VALUES (?1, ?2, ?3)",
                    params![currency, rate.rate_per_btc, rate.last_updated.to_rfc3339()],
                )?;
            }

            tx.commit()?;
            Ok(())
        })
        .await?
    }

    // ============================
    // BALANCE ALERTS CRUD METHODS
    // ============================

    pub async fn create_balance_alert(
        &self,
        wallet_checksum: &str,
        threshold_sats: i64,
        alert_type: BalanceAlertType,
        threshold_currency: Option<String>,
        threshold_fiat_amount: Option<f64>,
        current_balance_sats: Option<i64>,
    ) -> Result<BalanceAlert> {
        let pool = self.pool.clone();
        let wallet_checksum = wallet_checksum.to_string();
        let alert_id = Uuid::new_v4().to_string();
        let alert_type_str = alert_type.as_str().to_string();

        spawn_blocking(move || -> Result<BalanceAlert> {
            let conn = pool.get()?;
            let current_time = chrono::Utc::now().to_rfc3339();

            conn.execute(
                "INSERT INTO balance_alerts (id, wallet_checksum, threshold_sats, alert_type, is_active, created_at, threshold_currency, threshold_fiat_amount, last_checked_balance_sats)
                 VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7, ?8)",
                params![alert_id, wallet_checksum, threshold_sats, alert_type_str, current_time, threshold_currency, threshold_fiat_amount, current_balance_sats],
            )?;

            Ok(BalanceAlert {
                id: alert_id,
                wallet_checksum,
                threshold_sats,
                alert_type,
                is_active: true,
                last_triggered_at: None,
                created_at: current_time,
                threshold_currency,
                threshold_fiat_amount,
                last_checked_balance_sats: current_balance_sats,
            })
        })
        .await?
    }

    pub async fn get_active_balance_alerts_for_wallet(
        &self,
        wallet_checksum: &str,
    ) -> Result<Vec<BalanceAlert>> {
        let pool = self.pool.clone();
        let wallet_checksum = wallet_checksum.to_string();

        spawn_blocking(move || -> Result<Vec<BalanceAlert>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, wallet_checksum, threshold_sats, alert_type, is_active, last_triggered_at, created_at,
                        threshold_currency, threshold_fiat_amount, last_checked_balance_sats
                 FROM balance_alerts
                 WHERE wallet_checksum = ?1 AND is_active = 1"
            )?;

            let alert_iter = stmt.query_map(params![wallet_checksum], |row| {
                Ok(BalanceAlert {
                    id: row.get(0)?,
                    wallet_checksum: row.get(1)?,
                    threshold_sats: row.get(2)?,
                    alert_type: BalanceAlertType::try_from(row.get::<_, String>(3)?.as_str())
                        .map_err(|e| bdk_wallet::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                    is_active: row.get::<_, i64>(4)? != 0,
                    last_triggered_at: row.get::<_, Option<i64>>(5)?.map(|t| t as u64),
                    created_at: row.get(6)?,
                    threshold_currency: row.get(7)?,
                    threshold_fiat_amount: row.get(8)?,
                    last_checked_balance_sats: row.get(9)?,
                })
            })?;

            let mut alerts = Vec::new();
            for alert in alert_iter {
                alerts.push(alert?);
            }
            Ok(alerts)
        })
        .await?
    }

    pub async fn get_all_balance_alerts_for_wallet(
        &self,
        wallet_checksum: &str,
    ) -> Result<Vec<BalanceAlert>> {
        let pool = self.pool.clone();
        let wallet_checksum = wallet_checksum.to_string();

        spawn_blocking(move || -> Result<Vec<BalanceAlert>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, wallet_checksum, threshold_sats, alert_type, is_active, last_triggered_at, created_at,
                        threshold_currency, threshold_fiat_amount, last_checked_balance_sats
                 FROM balance_alerts
                 WHERE wallet_checksum = ?1
                 ORDER BY created_at ASC"
            )?;

            let alert_iter = stmt.query_map(params![wallet_checksum], |row| {
                Ok(BalanceAlert {
                    id: row.get(0)?,
                    wallet_checksum: row.get(1)?,
                    threshold_sats: row.get(2)?,
                    alert_type: BalanceAlertType::try_from(row.get::<_, String>(3)?.as_str())
                        .map_err(|e| bdk_wallet::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                    is_active: row.get::<_, i64>(4)? != 0,
                    last_triggered_at: row.get::<_, Option<i64>>(5)?.map(|t| t as u64),
                    created_at: row.get(6)?,
                    threshold_currency: row.get(7)?,
                    threshold_fiat_amount: row.get(8)?,
                    last_checked_balance_sats: row.get(9)?,
                })
            })?;

            let mut alerts = Vec::new();
            for alert in alert_iter {
                alerts.push(alert?);
            }
            Ok(alerts)
        })
        .await?
    }

    pub async fn get_balance_alert_by_id(&self, alert_id: &str) -> Result<Option<BalanceAlert>> {
        let pool = self.pool.clone();
        let alert_id = alert_id.to_string();

        spawn_blocking(move || -> Result<Option<BalanceAlert>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, wallet_checksum, threshold_sats, alert_type, is_active, last_triggered_at, created_at,
                        threshold_currency, threshold_fiat_amount, last_checked_balance_sats
                 FROM balance_alerts
                 WHERE id = ?1",
            )?;

            let alert = stmt
                .query_row(params![alert_id], |row| {
                    Ok(BalanceAlert {
                        id: row.get(0)?,
                        wallet_checksum: row.get(1)?,
                        threshold_sats: row.get(2)?,
                        alert_type: BalanceAlertType::try_from(row.get::<_, String>(3)?.as_str())
                            .map_err(|e| bdk_wallet::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                        is_active: row.get::<_, i64>(4)? != 0,
                        last_triggered_at: row.get::<_, Option<i64>>(5)?.map(|t| t as u64),
                        created_at: row.get(6)?,
                        threshold_currency: row.get(7)?,
                        threshold_fiat_amount: row.get(8)?,
                        last_checked_balance_sats: row.get(9)?,
                    })
                })
                .optional()?;

            Ok(alert)
        })
        .await?
    }

    #[allow(dead_code)] // Used in system tests
    pub async fn deactivate_balance_alert(&self, alert_id: &str) -> Result<()> {
        let pool = self.pool.clone();
        let alert_id = alert_id.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE balance_alerts
                 SET is_active = 0
                 WHERE id = ?1",
                params![alert_id],
            )?;
            Ok(())
        })
        .await?
    }

    /// Update the last checked balance for an alert (for threshold crossing detection)
    pub async fn update_alert_last_checked_balance(
        &self,
        alert_id: &str,
        balance_sats: i64,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let alert_id = alert_id.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE balance_alerts SET last_checked_balance_sats = ?1 WHERE id = ?2",
                params![balance_sats, alert_id],
            )?;
            Ok(())
        })
        .await?
    }

    /// Update the last triggered timestamp when an alert fires
    pub async fn update_balance_alert_triggered_timestamp(&self, alert_id: &str) -> Result<()> {
        let pool = self.pool.clone();
        let alert_id = alert_id.to_string();
        let triggered_at = chrono::Utc::now().timestamp() as u64;

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE balance_alerts SET last_triggered_at = ?1 WHERE id = ?2",
                params![triggered_at as i64, alert_id],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn delete_balance_alert(&self, alert_id: &str) -> Result<()> {
        let pool = self.pool.clone();
        let alert_id = alert_id.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "DELETE FROM balance_alerts WHERE id = ?1",
                params![alert_id],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn check_duplicate_balance_alert(
        &self,
        wallet_checksum: &str,
        threshold_sats: i64,
        alert_type: BalanceAlertType,
    ) -> Result<Option<BalanceAlert>> {
        let pool = self.pool.clone();
        let wallet_checksum = wallet_checksum.to_string();
        let alert_type_str = alert_type.as_str().to_string();

        spawn_blocking(move || -> Result<Option<BalanceAlert>> {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, wallet_checksum, threshold_sats, alert_type, is_active, last_triggered_at, created_at, threshold_currency, threshold_fiat_amount, last_checked_balance_sats
                 FROM balance_alerts
                 WHERE wallet_checksum = ?1 AND alert_type = ?2 AND threshold_sats = ?3
                 LIMIT 1"
            )?;

            let row = stmt.query_row(params![wallet_checksum, alert_type_str, threshold_sats], |row| {
                Ok(BalanceAlert {
                    id: row.get(0)?,
                    wallet_checksum: row.get(1)?,
                    threshold_sats: row.get(2)?,
                    alert_type: BalanceAlertType::try_from(row.get::<_, String>(3)?.as_str())
                        .map_err(|e| bdk_wallet::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                    is_active: row.get::<_, i64>(4)? != 0,
                    last_triggered_at: row.get::<_, Option<i64>>(5)?.map(|t| t as u64),
                    created_at: row.get(6)?,
                    threshold_currency: row.get(7)?,
                    threshold_fiat_amount: row.get(8)?,
                    last_checked_balance_sats: row.get(9)?,
                })
            });

            match row {
                Ok(alert) => Ok(Some(alert)),
                Err(bdk_wallet::rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
        .await?
    }

    pub async fn create_balance_alert_notification(
        &self,
        balance_alert_id: &str,
        wallet_checksum: &str,
        threshold_sats: i64,
        current_balance_sats: i64,
        alert_type: BalanceAlertType,
        threshold_currency: Option<String>,
        threshold_fiat_amount: Option<f64>,
        exchange_rate_snapshot: Option<f64>,
    ) -> Result<BalanceAlertNotification> {
        let pool = self.pool.clone();
        let notification_id = Uuid::new_v4().to_string();
        let balance_alert_id = balance_alert_id.to_string();
        let wallet_checksum = wallet_checksum.to_string();
        let alert_type_str = alert_type.as_str().to_string();
        let notification_sent_at = chrono::Utc::now().timestamp() as u64;

        spawn_blocking(move || -> Result<BalanceAlertNotification> {
            let conn = pool.get()?;
            let current_time = chrono::Utc::now().to_rfc3339();

            conn.execute(
                "INSERT INTO balance_alert_notifications
                 (id, balance_alert_id, wallet_checksum, threshold_sats, current_balance_sats, alert_type, notification_sent_at, created_at, threshold_currency, threshold_fiat_amount, exchange_rate_snapshot)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    notification_id,
                    balance_alert_id,
                    wallet_checksum,
                    threshold_sats,
                    current_balance_sats,
                    alert_type_str,
                    notification_sent_at as i64,
                    current_time,
                    threshold_currency,
                    threshold_fiat_amount,
                    exchange_rate_snapshot
                ],
            )?;

            Ok(BalanceAlertNotification {
                id: notification_id,
                balance_alert_id,
                wallet_checksum,
                threshold_sats,
                current_balance_sats,
                alert_type,
                notification_sent_at,
                created_at: current_time,
                threshold_currency,
                threshold_fiat_amount,
                exchange_rate_snapshot,
            })
        })
        .await?
    }
}
