use super::pool::MetadataDb;
use super::types::*;
use anyhow::{anyhow, Result};
use bdk_wallet::rusqlite::{params, OptionalExtension, Row, ToSql};
use tokio::task::spawn_blocking;

#[derive(Clone, Copy)]
struct WalletMetadataRowOptions {
    default_balance_total_to_zero: bool,
    default_contact_count_to_zero: bool,
    default_is_active_to_true: bool,
}

fn map_wallet_metadata_row(
    row: &Row<'_>,
    options: WalletMetadataRowOptions,
) -> bdk_wallet::rusqlite::Result<WalletMetadata> {
    // Expects the shared 13-column wallet SELECT shape used by the standard wallet queries:
    // checksum, name, descriptor, hex_color, created_at, balance_total, last_activity,
    // status, contact_count, user_id, is_active, wallet_type, last_synced_at.
    let balance_total = if options.default_balance_total_to_zero {
        Some(row.get(5).unwrap_or(0))
    } else {
        row.get(5).ok()
    };
    let contact_count = if options.default_contact_count_to_zero {
        row.get(8).unwrap_or(Some(0))
    } else {
        Some(row.get(8)?)
    };
    let is_active = if options.default_is_active_to_true {
        row.get::<_, i64>(10).unwrap_or(1) != 0
    } else {
        row.get::<_, i64>(10)? != 0
    };

    Ok(WalletMetadata {
        checksum: row.get(0)?,
        name: row.get(1)?,
        descriptor: row.get(2)?,
        hex_color: row.get(3)?,
        created_at: row.get(4)?,
        balance_total,
        last_activity: row
            .get::<_, Option<i64>>(6)
            .ok()
            .flatten()
            .map(|t| t.to_string()),
        status: row.get(7)?,
        contact_count,
        user_id: row.get(9)?,
        is_active,
        balance_fiat: None,
        fiat_currency: None,
        wallet_type: row
            .get::<_, String>(11)
            .unwrap_or_else(|_| "descriptor".to_string()),
        last_synced_at: row.get(12)?,
    })
}

impl MetadataDb {
    // ============================
    // WALLET CRUD OPERATIONS
    // ============================

    pub async fn descriptor_exists(&self, descriptor: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let descriptor = descriptor.to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM wallets WHERE descriptor = ?1",
                params![descriptor],
                |row| row.get(0),
            )?;
            Ok(count > 0)
        })
        .await?
    }

    /// Check if a descriptor already exists for a specific user
    pub async fn descriptor_exists_for_user(
        &self,
        descriptor: &str,
        user_id: &str,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let descriptor = descriptor.to_string();
        let user_id = user_id.to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM wallets WHERE descriptor = ?1 AND user_id = ?2",
                params![descriptor, user_id],
                |row| row.get(0),
            )?;
            Ok(count > 0)
        })
        .await?
    }

    /// Extract checksum from a Bitcoin descriptor
    pub fn extract_checksum(&self, descriptor: &str) -> String {
        extract_checksum(descriptor)
    }

    pub async fn insert_wallet(
        &self,
        name: &str,
        descriptor: &str,
        user_id: &str,
    ) -> Result<String> {
        self.insert_wallet_with_type(name, descriptor, user_id, "descriptor")
            .await
    }

    pub async fn insert_wallet_with_type(
        &self,
        name: &str,
        descriptor: &str,
        user_id: &str,
        wallet_type: &str,
    ) -> Result<String> {
        self.insert_wallet_with_type_and_checksum(name, descriptor, user_id, wallet_type, None)
            .await
    }

    /// Insert a wallet with an explicit checksum override.
    /// When `checksum_override` is None, the checksum is extracted from the descriptor.
    /// When provided, the override value is used as the PK (for multi-user address watches).
    pub async fn insert_wallet_with_type_and_checksum(
        &self,
        name: &str,
        descriptor: &str,
        user_id: &str,
        wallet_type: &str,
        checksum_override: Option<&str>,
    ) -> Result<String> {
        let pool = self.pool.clone();
        let name = name.to_string();
        let descriptor = descriptor.to_string();
        let user_id = user_id.to_string();
        let wallet_type = wallet_type.to_string();
        let checksum_override = checksum_override.map(|s| s.to_string());

        spawn_blocking(move || -> Result<String> {
            let conn = pool.get()?;
            let hex_color = calculate_wallet_color(&descriptor);
            let current_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

            let checksum = match checksum_override {
                Some(c) => c,
                None => descriptor
                    .split('#')
                    .next_back()
                    .ok_or_else(|| {
                        anyhow::anyhow!("Invalid descriptor format: missing checksum")
                    })?
                    .to_string(),
            };

            conn.execute(
                "INSERT INTO wallets (checksum, name, descriptor, hex_color, balance_total, last_activity, status, user_id, wallet_type) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![&checksum, &name, &descriptor, &hex_color, "0", &current_time, "pending", user_id, wallet_type],
            )?;
            Ok(checksum)
        })
        .await?
    }

    pub async fn get_wallet_by_descriptor(
        &self,
        descriptor: &str,
    ) -> Result<Option<WalletMetadata>> {
        let pool = self.pool.clone();
        let descriptor = descriptor.to_string();

        spawn_blocking(move || -> Result<Option<WalletMetadata>> {
            let conn = pool.get()?;
            conn.query_row(
                "SELECT w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total,
                        (SELECT MAX(COALESCE(t.confirmed_at, t.first_seen_at)) FROM transactions t WHERE t.wallet_checksum = w.checksum) as last_activity,
                        w.status, COUNT(c.id) as contact_count, w.user_id, w.is_active, w.wallet_type, w.last_synced_at
                FROM wallets w
                 LEFT JOIN contacts c ON w.checksum = c.wallet_checksum
                 WHERE w.descriptor = ?1
                 GROUP BY w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total, w.status, w.user_id, w.is_active, w.wallet_type, w.last_synced_at",
                params![descriptor],
                |row| map_wallet_metadata_row(
                    row,
                    WalletMetadataRowOptions {
                        default_balance_total_to_zero: false,
                        default_contact_count_to_zero: false,
                        default_is_active_to_true: true,
                    },
                ),
            )
            .optional()
            .map_err(Into::into)
        })
        .await?
    }

    pub async fn get_wallet_by_checksum(&self, checksum: &str) -> Result<Option<WalletMetadata>> {
        let pool = self.pool.clone();
        let checksum = checksum.to_string();

        spawn_blocking(move || -> Result<Option<WalletMetadata>> {
            let conn = pool.get()?;
            conn.query_row(
                "SELECT w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total,
                        (SELECT MAX(COALESCE(t.confirmed_at, t.first_seen_at)) FROM transactions t WHERE t.wallet_checksum = w.checksum) as last_activity,
                        w.status, COUNT(c.id) as contact_count, w.user_id, w.is_active, w.wallet_type, w.last_synced_at
                FROM wallets w
                 LEFT JOIN contacts c ON w.checksum = c.wallet_checksum
                 WHERE w.checksum = ?1
                 GROUP BY w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total, w.status, w.user_id, w.is_active, w.wallet_type, w.last_synced_at",
                params![checksum],
                |row| map_wallet_metadata_row(
                    row,
                    WalletMetadataRowOptions {
                        default_balance_total_to_zero: false,
                        default_contact_count_to_zero: false,
                        default_is_active_to_true: true,
                    },
                ),
            )
            .optional()
            .map_err(Into::into)
        })
        .await?
    }

    pub async fn update_wallet_by_checksum(
        &self,
        wallet_checksum: &str,
        name: &str,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let name = name.to_string();
        let checksum = wallet_checksum.to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let changes = conn.execute(
                "UPDATE wallets SET name = ?1 WHERE checksum = ?2",
                params![&name, checksum],
            )?;
            Ok(changes > 0)
        })
        .await?
    }

    /// Mark a wallet as deleted (soft delete) - used for non-blocking deletion
    pub async fn mark_wallet_as_deleted(&self, checksum: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let checksum = checksum.to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let changes = conn.execute(
                "UPDATE wallets SET status = 'deleted' WHERE checksum = ?1",
                params![checksum],
            )?;
            Ok(changes > 0)
        })
        .await?
    }

    /// Hard delete wallet from database (used after soft delete cleanup)
    pub async fn hard_delete_wallet_by_checksum(&self, checksum: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let checksum = checksum.to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;

            // First delete notification_logs that reference this wallet's transactions
            // This prevents foreign key constraint failures when transactions are cascade deleted
            conn.execute(
                "DELETE FROM notification_logs WHERE transaction_wallet_checksum = ?1",
                params![checksum],
            )?;

            // Now delete the wallet - this will cascade delete transactions, contacts, etc.
            let changes =
                conn.execute("DELETE FROM wallets WHERE checksum = ?1", params![checksum])?;
            Ok(changes > 0)
        })
        .await?
    }

    // ============================
    // WALLET QUERY OPERATIONS
    // ============================

    pub async fn get_all_wallets(&self) -> Result<Vec<WalletMetadata>> {
        self.get_wallets_for_user(None).await
    }

    pub async fn get_wallets_for_user(&self, user_id: Option<&str>) -> Result<Vec<WalletMetadata>> {
        let pool = self.pool.clone();
        let user_id = user_id.map(|id| id.to_string());

        spawn_blocking(move || -> Result<Vec<WalletMetadata>> {
            let conn = pool.get()?;

            let query = match user_id {
                Some(_) => {
                    "SELECT w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total,
                            (SELECT MAX(COALESCE(t.confirmed_at, t.first_seen_at)) FROM transactions t WHERE t.wallet_checksum = w.checksum) as last_activity,
                            w.status, COUNT(c.id) as contact_count, w.user_id, w.is_active, w.wallet_type, w.last_synced_at
                     FROM wallets w
                     LEFT JOIN contacts c ON w.checksum = c.wallet_checksum
                     WHERE w.user_id = ?1 AND w.status != 'deleted'
                     GROUP BY w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total, w.status, w.user_id, w.is_active, w.wallet_type, w.last_synced_at
                     ORDER BY w.created_at DESC"
                }
                None => {
                    "SELECT w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total,
                            (SELECT MAX(COALESCE(t.confirmed_at, t.first_seen_at)) FROM transactions t WHERE t.wallet_checksum = w.checksum) as last_activity,
                            w.status, COUNT(c.id) as contact_count, w.user_id, w.is_active, w.wallet_type, w.last_synced_at
                     FROM wallets w
                     LEFT JOIN contacts c ON w.checksum = c.wallet_checksum
                     WHERE w.status != 'deleted'
                     GROUP BY w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total, w.status, w.user_id, w.is_active, w.wallet_type, w.last_synced_at
                     ORDER BY w.created_at DESC"
                }
            };

            let mut stmt = conn.prepare(query)?;

            let params: Vec<&dyn ToSql> = match user_id.as_ref() {
                Some(uid) => vec![uid],
                None => vec![],
            };

            let wallet_iter = stmt.query_map(&params[..], |row| {
                map_wallet_metadata_row(
                    row,
                    WalletMetadataRowOptions {
                        default_balance_total_to_zero: true,
                        default_contact_count_to_zero: false,
                        default_is_active_to_true: true,
                    },
                )
            })?;

            wallet_iter
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(Into::into)
        })
        .await?
    }

    /// Get wallets for a user ordered by creation time (oldest first) for subscription limits enforcement
    pub async fn get_wallets_for_user_oldest_first(
        &self,
        user_id: &str,
    ) -> Result<Vec<WalletMetadata>> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();

        spawn_blocking(move || -> Result<Vec<WalletMetadata>> {
            let conn = pool.get()?;

            let query = "SELECT w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total,
                               (SELECT MAX(COALESCE(t.confirmed_at, t.first_seen_at)) FROM transactions t WHERE t.wallet_checksum = w.checksum) as last_activity,
                               w.status, COUNT(c.id) as contact_count, w.user_id, w.is_active, w.wallet_type, w.last_synced_at
                        FROM wallets w
                        LEFT JOIN contacts c ON w.checksum = c.wallet_checksum
                        WHERE w.user_id = ?1 AND w.status != 'deleted'
                        GROUP BY w.checksum, w.name, w.descriptor, w.hex_color, w.created_at, w.balance_total, w.status, w.user_id, w.is_active, w.wallet_type, w.last_synced_at
                        ORDER BY w.created_at ASC"; // Oldest first for subscription limits

            let mut stmt = conn.prepare(query)?;

            let wallet_iter = stmt.query_map([&user_id], |row| {
                map_wallet_metadata_row(
                    row,
                    WalletMetadataRowOptions {
                        default_balance_total_to_zero: true,
                        default_contact_count_to_zero: false,
                        default_is_active_to_true: true,
                    },
                )
            })?;

            wallet_iter
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(Into::into)
        })
        .await?
    }

    pub async fn count_wallets_for_user(&self, user_id: &str) -> Result<usize> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();

        spawn_blocking(move || -> Result<usize> {
            let conn = pool.get()?;
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM wallets WHERE user_id = ?1 AND status != 'deleted'",
                params![user_id],
                |row| row.get(0),
            )?;
            Ok(count as usize)
        })
        .await?
    }

    pub async fn is_wallet_owned_by_user(
        &self,
        wallet_checksum: &str,
        user_id: &str,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();
        let user_id = user_id.to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let exists: bool = conn
                .prepare("SELECT 1 FROM wallets WHERE checksum = ?1 AND user_id = ?2")?
                .exists(params![checksum, user_id])?;
            Ok(exists)
        })
        .await?
    }

    /// Get all wallets for a specific tier that are due for sync
    pub async fn get_wallets_for_tier_sync(
        &self,
        tier: &crate::subscription::SubscriptionTier,
        network: &crate::config::NetworkConfig,
    ) -> Result<Vec<WalletMetadata>> {
        let pool = self.pool.clone();
        let network = network.clone();
        let tier_str = match tier {
            crate::subscription::SubscriptionTier::Personal => "personal",
            crate::subscription::SubscriptionTier::Team => "team",
        }
        .to_string();

        spawn_blocking(move || -> Result<Vec<WalletMetadata>> {
            let conn = pool.get()?;
            let tier_limits =
                crate::subscription::SubscriptionTier::from(tier_str.clone()).limits(&network);

            let mut stmt = conn.prepare(
                "SELECT w.checksum, w.name, w.descriptor, w.hex_color, w.balance_total,
                        w.last_activity, w.last_synced_at, w.status, w.user_id, w.created_at, w.wallet_type
                 FROM wallets w
                 JOIN users u ON w.user_id = u.id
                 WHERE w.is_active = 1 AND w.status IN ('ready', 'pending')
                   AND u.subscription_tier = ?1
                   AND (
                    -- Admin users bypass all subscription checks
                    u.is_admin = 1
                    OR
                    -- Regular users need valid subscription
                    (
                      -- Active subscriptions
                      u.subscription_status = 'active'
                      OR
                      -- Trial users within trial period
                      (u.subscription_status = 'trialing' AND datetime(u.trial_ends_at) > datetime('now'))
                      OR
                      -- Cancelled users still within their paid period
                      (u.subscription_status = 'canceled' AND u.subscription_ends_at IS NOT NULL AND datetime(u.subscription_ends_at) > datetime('now'))
                    )
                 )
                 AND (
                    -- Admin users bypass timing restrictions
                    u.is_admin = 1
                    OR
                    -- Regular users follow timing rules
                    (
                      -- Never synced before
                      w.last_synced_at IS NULL
                      OR
                      -- Or due for sync based on tier interval
                      datetime(w.last_synced_at) <= datetime('now', '-' || ?2 || ' seconds')
                    )
                 )
                 ORDER BY w.checksum",
            )?;

            let wallet_rows =
                stmt.query_map(params![tier_str, tier_limits.sync_interval_secs], |row| {
                    Ok(WalletMetadata {
                        checksum: row.get(0)?,
                        name: row.get(1)?,
                        descriptor: row.get(2)?,
                        hex_color: row.get(3)?,
                        balance_total: row.get(4)?,
                        last_activity: row.get(5)?,
                        last_synced_at: row.get(6)?,
                        status: row.get(7)?,
                        contact_count: None, // Not counting contacts in this query
                        user_id: row.get(8)?,
                        created_at: row.get(9)?,
                        is_active: true, // Query already filters for active wallets
                        balance_fiat: None,
                        fiat_currency: None,
                        wallet_type: row.get::<_, String>(10).unwrap_or_else(|_| "descriptor".to_string()),
                    })
                })?;

            wallet_rows
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(Into::into)
        })
        .await?
    }

    /// Get summary of wallets that are not being synced due to subscription issues
    pub async fn get_non_syncing_wallets_summary(&self) -> Result<NonSyncingWalletsSummary> {
        let pool = self.pool.clone();

        spawn_blocking(move || -> Result<NonSyncingWalletsSummary> {
            let conn = pool.get()?;

            // Count expired trials (trialing but past trial_ends_at)
            let expired_trials: usize = conn.query_row(
                "SELECT COUNT(DISTINCT w.checksum)
                 FROM wallets w
                 JOIN users u ON w.user_id = u.id
                 WHERE w.is_active = 1 AND w.status = 'ready'
                   AND u.is_admin = 0
                   AND u.subscription_status = 'trialing'
                   AND datetime(u.trial_ends_at) <= datetime('now')",
                [],
                |row| row.get(0),
            )?;

            // Count cancelled subscriptions (cancelled and past subscription_ends_at)
            let cancelled_subscriptions: usize = conn.query_row(
                "SELECT COUNT(DISTINCT w.checksum)
                 FROM wallets w
                 JOIN users u ON w.user_id = u.id
                 WHERE w.is_active = 1 AND w.status = 'ready'
                   AND u.is_admin = 0
                   AND u.subscription_status = 'canceled'
                   AND (u.subscription_ends_at IS NULL OR datetime(u.subscription_ends_at) <= datetime('now'))",
                [],
                |row| row.get(0),
            )?;

            // Count expired subscriptions
            let expired_subscriptions: usize = conn.query_row(
                "SELECT COUNT(DISTINCT w.checksum)
                 FROM wallets w
                 JOIN users u ON w.user_id = u.id
                 WHERE w.is_active = 1 AND w.status = 'ready'
                   AND u.is_admin = 0
                   AND u.subscription_status = 'expired'",
                [],
                |row| row.get(0),
            )?;

            // Count past_due subscriptions
            let past_due_subscriptions: usize = conn.query_row(
                "SELECT COUNT(DISTINCT w.checksum)
                 FROM wallets w
                 JOIN users u ON w.user_id = u.id
                 WHERE w.is_active = 1 AND w.status = 'ready'
                   AND u.is_admin = 0
                   AND u.subscription_status = 'past_due'",
                [],
                |row| row.get(0),
            )?;

            // Count inactive wallets (due to tier limits)
            let inactive_wallets: usize = conn.query_row(
                "SELECT COUNT(*)
                 FROM wallets w
                 WHERE w.is_active = 0 AND w.status = 'ready'",
                [],
                |row| row.get(0),
            )?;

            let total_non_syncing = expired_trials
                + cancelled_subscriptions
                + expired_subscriptions
                + past_due_subscriptions
                + inactive_wallets;

            Ok(NonSyncingWalletsSummary {
                expired_trials,
                cancelled_subscriptions,
                expired_subscriptions,
                past_due_subscriptions,
                inactive_wallets,
                total_non_syncing,
            })
        })
        .await?
    }

    pub async fn get_ready_wallets(&self) -> Result<Vec<WalletMetadata>> {
        let pool = self.pool.clone();

        spawn_blocking(move || -> Result<Vec<WalletMetadata>> {
            let conn = pool.get()?;

            let query = "SELECT w.checksum, w.name, w.descriptor, w.hex_color,
                                w.created_at, w.balance_total,
                                (SELECT MAX(COALESCE(t.confirmed_at, t.first_seen_at)) FROM transactions t
                                 WHERE t.wallet_checksum = w.checksum) as last_activity,
                                w.status, COUNT(c.id) as contact_count,
                                w.user_id, w.is_active, w.wallet_type, w.last_synced_at
                         FROM wallets w
                         LEFT JOIN contacts c ON w.checksum = c.wallet_checksum
                         WHERE w.status = 'ready'
                         GROUP BY w.checksum, w.name, w.descriptor, w.hex_color,
                                  w.created_at, w.balance_total, w.status,
                                  w.user_id, w.is_active, w.wallet_type, w.last_synced_at
                         ORDER BY w.created_at DESC";

            let mut stmt = conn.prepare(query)?;

            let wallet_iter = stmt.query_map([], |row| {
                map_wallet_metadata_row(
                    row,
                    WalletMetadataRowOptions {
                        default_balance_total_to_zero: true,
                        default_contact_count_to_zero: true,
                        default_is_active_to_true: false,
                    },
                )
            })?;

            wallet_iter
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| anyhow!("Failed to query ready wallets: {}", e))
        })
        .await?
    }

    /// Get all wallets marked as deleted
    pub async fn get_deleted_wallets(&self) -> Result<Vec<WalletMetadata>> {
        let pool = self.pool.clone();

        spawn_blocking(move || -> Result<Vec<WalletMetadata>> {
            let conn = pool.get()?;

            let query = "SELECT w.checksum, w.name, w.descriptor, w.hex_color,
                                w.created_at, w.balance_total,
                                (SELECT MAX(COALESCE(t.confirmed_at, t.first_seen_at)) FROM transactions t
                                 WHERE t.wallet_checksum = w.checksum) as last_activity,
                                w.status, COUNT(c.id) as contact_count,
                                w.user_id, w.is_active, w.wallet_type, w.last_synced_at
                         FROM wallets w
                         LEFT JOIN contacts c ON w.checksum = c.wallet_checksum
                         WHERE w.status = 'deleted'
                         GROUP BY w.checksum, w.name, w.descriptor, w.hex_color,
                                  w.created_at, w.balance_total, w.status,
                                  w.user_id, w.is_active, w.wallet_type, w.last_synced_at
                         ORDER BY w.created_at DESC";

            let mut stmt = conn.prepare(query)?;

            let wallet_iter = stmt.query_map([], |row| {
                map_wallet_metadata_row(
                    row,
                    WalletMetadataRowOptions {
                        default_balance_total_to_zero: true,
                        default_contact_count_to_zero: true,
                        default_is_active_to_true: false,
                    },
                )
            })?;

            wallet_iter
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| anyhow!("Failed to query deleted wallets: {}", e))
        })
        .await?
    }

    // ============================
    // WALLET STATUS & SYNC OPERATIONS
    // ============================

    pub async fn update_wallet_balance_by_checksum(
        &self,
        wallet_checksum: &str,
        balance_total: i64,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let checksum = wallet_checksum.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE wallets SET balance_total = ?1 WHERE checksum = ?2",
                params![balance_total, checksum],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn update_wallet_last_synced(&self, checksum: &str) -> Result<()> {
        let pool = self.pool.clone();
        let checksum = checksum.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            let current_time = chrono::Utc::now()
                .format("%Y-%m-%d %H:%M:%S%.3f")
                .to_string();
            conn.execute(
                "UPDATE wallets SET last_synced_at = ?1 WHERE checksum = ?2",
                params![current_time, checksum],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn update_wallet_status(&self, checksum: &str, status: &str) -> Result<()> {
        let pool = self.pool.clone();
        let checksum = checksum.to_string();
        let status = status.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE wallets SET status = ?1 WHERE checksum = ?2",
                params![status, checksum],
            )?;
            Ok(())
        })
        .await?
    }

    /// Atomically updates the wallet status, but only if the wallet has not
    /// already been marked as 'deleted'. This is used to prevent a race
    /// condition where a wallet is deleted while a background creation
    /// task is still running.
    ///
    /// Returns `Ok(true)` if the update was successful, `Ok(false)` if the
    /// wallet was not updated (because it was already deleted or not found),
    /// and `Err` if a database error occurred.
    pub async fn update_wallet_status_if_not_deleted(
        &self,
        checksum: &str,
        status: &str,
    ) -> Result<bool> {
        let pool = self.pool.clone();
        let checksum = checksum.to_string();
        let status = status.to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let changes = conn.execute(
                "UPDATE wallets SET status = ?1 WHERE checksum = ?2 AND status != 'deleted'",
                params![status, checksum],
            )?;
            Ok(changes > 0)
        })
        .await?
    }

    /// Get all address watch checksums grouped by their descriptor.
    /// Used by sync to deduplicate Electrum queries for the same address.
    pub async fn get_address_watches_grouped_by_descriptor(
        &self,
        wallets: &[WalletMetadata],
    ) -> std::collections::HashMap<String, Vec<WalletMetadata>> {
        let mut groups: std::collections::HashMap<String, Vec<WalletMetadata>> =
            std::collections::HashMap::new();
        for w in wallets {
            if w.wallet_type == "address" {
                groups
                    .entry(w.descriptor.clone())
                    .or_default()
                    .push(w.clone());
            }
        }
        groups
    }

    pub async fn update_wallet_active_status(&self, checksum: &str, is_active: bool) -> Result<()> {
        let pool = self.pool.clone();
        let checksum = checksum.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE wallets SET is_active = ? WHERE checksum = ?",
                params![is_active, checksum],
            )?;
            Ok(())
        })
        .await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bdk_wallet::rusqlite::Connection;

    fn map_test_row(
        balance_total_sql: &str,
        contact_count_sql: &str,
        is_active_sql: &str,
        options: WalletMetadataRowOptions,
    ) -> bdk_wallet::rusqlite::Result<WalletMetadata> {
        let conn = Connection::open_in_memory().unwrap();
        let query = format!(
            "SELECT \
                'checksum', 'name', 'descriptor', '#ff0000', '2026-01-01 00:00:00', \
                {balance_total_sql}, 123, 'ready', {contact_count_sql}, 'user-1', {is_active_sql}, \
                'descriptor', '2026-01-02 00:00:00'"
        );

        conn.query_row(&query, [], |row| map_wallet_metadata_row(row, options))
    }

    #[test]
    fn map_wallet_metadata_row_defaults_optional_values_to_zero_when_requested() {
        let wallet = map_test_row(
            "NULL",
            "'not-a-number'",
            "1",
            WalletMetadataRowOptions {
                default_balance_total_to_zero: true,
                default_contact_count_to_zero: true,
                default_is_active_to_true: false,
            },
        )
        .unwrap();

        assert_eq!(wallet.balance_total, Some(0));
        assert_eq!(wallet.contact_count, Some(0));
    }

    #[test]
    fn map_wallet_metadata_row_preserves_none_for_balance_total_without_zero_default() {
        let wallet = map_test_row(
            "NULL",
            "0",
            "1",
            WalletMetadataRowOptions {
                default_balance_total_to_zero: false,
                default_contact_count_to_zero: false,
                default_is_active_to_true: true,
            },
        )
        .unwrap();

        assert_eq!(wallet.balance_total, None);
        assert_eq!(wallet.contact_count, Some(0));
    }

    #[test]
    fn map_wallet_metadata_row_can_decode_is_active_strictly_or_with_fallback() {
        let strict_result = map_test_row(
            "0",
            "0",
            "NULL",
            WalletMetadataRowOptions {
                default_balance_total_to_zero: true,
                default_contact_count_to_zero: true,
                default_is_active_to_true: false,
            },
        );
        let fallback_result = map_test_row(
            "0",
            "0",
            "NULL",
            WalletMetadataRowOptions {
                default_balance_total_to_zero: true,
                default_contact_count_to_zero: true,
                default_is_active_to_true: true,
            },
        )
        .unwrap();

        assert!(strict_result.is_err());
        assert!(fallback_result.is_active);
    }
}
