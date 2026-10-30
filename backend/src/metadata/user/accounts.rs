//! User account rows: creation, lookup by email, ID or Stripe customer, and profile updates.

use crate::metadata::pool::MetadataDb;
use crate::metadata::status::SubscriptionStatus;
use crate::metadata::types::UserRecord;
use crate::subscription::SubscriptionTier;
use anyhow::Result;
use bdk_wallet::rusqlite::{params, TransactionBehavior};
use uuid::Uuid;

impl MetadataDb {
    pub async fn create_user(
        &self,
        email: &str,
        password_hash: &str,
        name: Option<&str>,
        email_verified: bool,
        preferred_currency: Option<&str>,
        preferred_language: Option<&str>,
    ) -> Result<String> {
        let email = email.to_string();
        let password_hash = password_hash.to_string();
        let name = name.map(std::string::ToString::to_string);
        let preferred_currency = preferred_currency.map(std::string::ToString::to_string);
        let preferred_language = preferred_language.map(std::string::ToString::to_string);

        self.with_conn(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

            // Check if user already exists
            let existing: Option<String> = tx
                .prepare("SELECT id FROM users WHERE email = ?1")?
                .query_row(params![&email], |row| row.get(0))
                .ok();

            if let Some(_id) = existing {
                tx.rollback()?;
                return Err(anyhow::anyhow!("User with this email already exists"));
            }

            let user_name = name;

            // Generate UUID for new user
            let user_id = Uuid::new_v4().to_string();

            // Create new user
            tx.execute(
                "INSERT INTO users (id, email, password_hash, name, is_admin, is_demo, email_verified, subscription_tier, subscription_status, preferred_fiat_currency, preferred_language) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![&user_id, &email, &password_hash, user_name, false, false, email_verified, "team", SubscriptionStatus::Pending, preferred_currency.as_deref().unwrap_or("USD"), preferred_language.as_deref().unwrap_or("en-US")],
            )?;

            tx.commit()?;
            Ok(user_id)
        })
        .await
    }

    pub async fn get_user_by_email(&self, email: &str) -> Result<Option<UserRecord>> {
        let email = email.to_string();

        self.with_conn(move |conn| {
            let result = conn
                .prepare("SELECT id, email, password_hash, name, is_admin, is_demo, email_verified, subscription_tier, trial_ends_at, subscription_status, stripe_customer_id, stripe_subscription_id, subscription_started_at, subscription_ends_at, created_at, preferred_fiat_currency, preferred_language FROM users WHERE email = ?1")?
                .query_row(params![&email], |row| {
                    Ok(UserRecord {
                        id: row.get(0)?,
                        email: row.get(1)?,
                        password_hash: row.get(2)?,
                        name: row.get(3)?,
                        is_admin: row.get(4)?,
                        is_demo: row.get(5)?,
                        email_verified: row.get(6)?,
                        subscription_tier: SubscriptionTier::from(row.get::<_, String>(7)?),
                        trial_ends_at: row.get(8)?,
                        subscription_status: row.get(9)?,
                        stripe_customer_id: row.get(10)?,
                        stripe_subscription_id: row.get(11)?,
                        subscription_started_at: row.get(12)?,
                        subscription_ends_at: row.get(13)?,
                        created_at: row.get(14)?,
                        preferred_fiat_currency: row.get(15)?,
                        preferred_language: row.get(16)?,
                    })
                })
                .ok();
            Ok(result)
        }).await
    }

    pub async fn get_user_by_id(&self, user_id: &str) -> Result<Option<UserRecord>> {
        let user_id = user_id.to_string();

        self.with_conn(move |conn| {
            let result = conn
                .prepare("SELECT id, email, password_hash, name, is_admin, is_demo, email_verified, subscription_tier, trial_ends_at, subscription_status, stripe_customer_id, stripe_subscription_id, subscription_started_at, subscription_ends_at, created_at, preferred_fiat_currency, preferred_language FROM users WHERE id = ?1")?
                .query_row(params![user_id], |row| {
                    Ok(UserRecord {
                        id: row.get(0)?,
                        email: row.get(1)?,
                        password_hash: row.get(2)?,
                        name: row.get(3)?,
                        is_admin: row.get(4)?,
                        is_demo: row.get(5)?,
                        email_verified: row.get(6)?,
                        subscription_tier: SubscriptionTier::from(row.get::<_, String>(7)?),
                        trial_ends_at: row.get(8)?,
                        subscription_status: row.get(9)?,
                        stripe_customer_id: row.get(10)?,
                        stripe_subscription_id: row.get(11)?,
                        subscription_started_at: row.get(12)?,
                        subscription_ends_at: row.get(13)?,
                        created_at: row.get(14)?,
                        preferred_fiat_currency: row.get(15)?,
                        preferred_language: row.get(16)?,
                    })
                })
                .ok();
            Ok(result)
        }).await
    }

    pub async fn get_user_by_stripe_customer_id(
        &self,
        stripe_customer_id: &str,
    ) -> Result<Option<UserRecord>> {
        let stripe_customer_id = stripe_customer_id.to_string();

        self.with_conn(move |conn| {
            let result = conn
                .prepare("SELECT id, email, password_hash, name, is_admin, is_demo, email_verified, subscription_tier, trial_ends_at, subscription_status, stripe_customer_id, stripe_subscription_id, subscription_started_at, subscription_ends_at, created_at, preferred_fiat_currency, preferred_language FROM users WHERE stripe_customer_id = ?1")?
                .query_row(params![stripe_customer_id], |row| {
                    Ok(UserRecord {
                        id: row.get(0)?,
                        email: row.get(1)?,
                        password_hash: row.get(2)?,
                        name: row.get(3)?,
                        is_admin: row.get(4)?,
                        is_demo: row.get(5)?,
                        email_verified: row.get(6)?,
                        subscription_tier: SubscriptionTier::from(row.get::<_, String>(7)?),
                        trial_ends_at: row.get(8)?,
                        subscription_status: row.get(9)?,
                        stripe_customer_id: row.get(10)?,
                        stripe_subscription_id: row.get(11)?,
                        subscription_started_at: row.get(12)?,
                        subscription_ends_at: row.get(13)?,
                        created_at: row.get(14)?,
                        preferred_fiat_currency: row.get(15)?,
                        preferred_language: row.get(16)?,
                    })
                })
                .ok();
            Ok(result)
        }).await
    }

    pub async fn update_last_login(&self, user_id: &str) -> Result<()> {
        let user_id = user_id.to_string();

        self.with_conn(move |conn| {
            let current_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            conn.execute(
                "UPDATE users SET last_login = ?1 WHERE id = ?2",
                params![&current_time, user_id],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn get_all_users(&self) -> Result<Vec<UserRecord>> {
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, email, password_hash, name, is_admin, is_demo, email_verified, subscription_tier,
                        subscription_status, trial_ends_at, subscription_started_at,
                        stripe_customer_id, stripe_subscription_id, subscription_ends_at,
                        created_at, preferred_fiat_currency, preferred_language
                 FROM users"
            )?;

            let user_iter = stmt.query_map([], |row| {
                Ok(UserRecord {
                    id: row.get(0)?,
                    email: row.get(1)?,
                    password_hash: row.get(2)?,
                    name: row.get(3)?,
                    is_admin: row.get(4)?,
                    is_demo: row.get(5)?,
                    email_verified: row.get(6)?,
                    subscription_tier: SubscriptionTier::from(row.get::<_, String>(7)?),
                    subscription_status: row.get(8)?,
                    trial_ends_at: row.get(9)?,
                    subscription_started_at: row.get(10)?,
                    stripe_customer_id: row.get(11)?,
                    stripe_subscription_id: row.get(12)?,
                    subscription_ends_at: row.get(13)?,
                    created_at: row.get(14)?,
                    preferred_fiat_currency: row.get(15)?,
                    preferred_language: row.get(16)?,
                })
            })?;

            let mut users = Vec::new();
            for user in user_iter {
                users.push(user?);
            }

            Ok(users)
        })
        .await
    }

    pub async fn update_user_name(&self, user_id: &str, name: &str) -> Result<()> {
        let name = name.to_string();
        let user_id = user_id.to_string();

        self.with_conn(move |conn| {
            conn.execute(
                "UPDATE users SET name = ?1 WHERE id = ?2",
                params![&name, user_id],
            )?;
            Ok(())
        })
        .await
    }
}
