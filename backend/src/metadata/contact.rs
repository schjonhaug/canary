use super::pool::MetadataDb;
use super::types::*;
use anyhow::Result;
use bdk_wallet::rusqlite::{params, OptionalExtension, ToSql};
use phonenumber::PhoneNumber;
use std::str::FromStr;
use tokio::task::spawn_blocking;
use uuid::Uuid;

impl MetadataDb {
    // ============================
    // CONTACT CRUD OPERATIONS
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
                let query = format!("{} AND c.id != ?4 LIMIT 1", base_query);
                conn.query_row(
                    &query,
                    params![&checksum, &provider, &target, &contact_id],
                    |row| row.get(0),
                )
                .optional()?
            } else {
                let query = format!("{} LIMIT 1", base_query);
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
        })
        .await?
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

            // Fetch all notification methods using IN clause (avoid N+1)
            // Chunk IDs to avoid hitting SQL parameter limits (SQLite default is 999)
            let contact_ids: Vec<String> = contacts.keys().cloned().collect();
            for ids_chunk in contact_ids.chunks(500) {
                let placeholders = ids_chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let methods_query = format!(
                    "SELECT id, contact_id, provider_type, notification_target, created_at
                     FROM contact_notification_methods
                     WHERE contact_id IN ({}) ORDER BY contact_id",
                    placeholders
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
                    })
                })?;

                for method_result in methods_iter {
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
        new_methods: Vec<(ProviderType, String)>,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let contact_id = contact_id.to_string();
        let checksum = wallet_checksum.to_string();
        let contact_name = name.to_string();

        spawn_blocking(move || -> Result<()> {
            let mut conn = pool.get()?;

            // Use rusqlite's Transaction API for automatic rollback on drop
            let tx = conn.transaction()?;

            // Update contact basics
            tx.execute(
                "UPDATE contacts SET name = ?1 WHERE id = ?2 AND wallet_checksum = ?3",
                params![contact_name, contact_id, checksum],
            )?;

            // Check if contact was updated (exists and belongs to wallet)
            let affected: i64 = tx.query_row("SELECT changes()", [], |row| row.get(0))?;

            if affected == 0 {
                return Err(anyhow::anyhow!("Contact not found or access denied"));
            }

            // Delete all old notification methods
            tx.execute(
                "DELETE FROM contact_notification_methods WHERE contact_id = ?1",
                params![contact_id],
            )?;

            // Insert new methods
            for (provider_type, target) in new_methods {
                let method_id = uuid::Uuid::new_v4().to_string();
                tx.execute(
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

            tx.commit()?;
            Ok(())
        })
        .await?
    }

    // ============================
    // CONTACT VERIFICATION OPERATIONS
    // ============================

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
        })
        .await?
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

    // ============================
    // OTP RATE LIMITING OPERATIONS
    // ============================

    pub async fn check_rate_limit(&self, phone_number: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let phone_number = phone_number.to_string();

        spawn_blocking(move || -> Result<bool> {
            let mut conn = pool.get()?;
            let current_time = chrono::Utc::now();
            let current_time_str = current_time.format("%Y-%m-%d %H:%M:%S").to_string();

            // Use transaction to prevent race conditions between reads and writes
            let tx = conn.transaction()?;

            // Check if blocked
            let blocked: Option<String> = tx
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
                .format("%Y-%m-%d %H:%M:%S")
                .to_string();

            let recent_attempts: i32 = tx
                .prepare(
                    "SELECT attempt_count FROM otp_attempts WHERE phone_number = ?1 AND last_attempt > ?2",
                )?
                .query_row(params![&phone_number, &fifteen_minutes_ago], |row| {
                    row.get(0)
                })
                .unwrap_or(0);

            if recent_attempts >= 5 {
                // Block for 30 minutes
                let blocked_until = (current_time + chrono::Duration::minutes(30))
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string();

                tx.execute(
                    "UPDATE otp_attempts SET blocked_until = ?1 WHERE phone_number = ?2",
                    params![&blocked_until, &phone_number],
                )?;

                tx.commit()?;
                return Ok(false);
            }

            // Update attempt count
            let exists: bool = tx
                .prepare("SELECT 1 FROM otp_attempts WHERE phone_number = ?1")?
                .exists(params![&phone_number])?;

            if exists {
                // If the last attempt was not recent (outside 15-min window), reset the counter
                // Otherwise, increment it
                if recent_attempts > 0 {
                    tx.execute(
                        "UPDATE otp_attempts SET attempt_count = attempt_count + 1, last_attempt = ?1 WHERE phone_number = ?2",
                        params![&current_time_str, &phone_number],
                    )?;
                } else {
                    tx.execute(
                        "UPDATE otp_attempts SET attempt_count = 1, last_attempt = ?1, blocked_until = NULL WHERE phone_number = ?2",
                        params![&current_time_str, &phone_number],
                    )?;
                }
            } else {
                tx.execute(
                    "INSERT INTO otp_attempts (phone_number, attempt_count, last_attempt) VALUES (?1, 1, ?2)",
                    params![&phone_number, &current_time_str],
                )?;
            }

            tx.commit()?;
            Ok(true)
        })
        .await?
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
}
