use super::pool::MetadataDb;
use super::types::*;
use crate::config::AppConfig;
use crate::subscription::SubscriptionTier;
use anyhow::Result;
use bdk_wallet::rusqlite::{params, OptionalExtension, TransactionBehavior};
use tokio::task::spawn_blocking;
use uuid::Uuid;

impl MetadataDb {
    // ============================
    // USER INITIALIZATION METHODS
    // ============================

    pub(super) async fn initialize_user_for_mode(&self, config: &AppConfig) -> Result<()> {
        if config.is_self_hosted_mode() {
            // Self-hosted mode: Create hardcoded self-hosted user admin
            self.ensure_self_hosted_user(config).await?;
        } else {
            // Cloud mode: Always create demo user (available on all networks)
            self.ensure_demo_user().await?;

            // Cloud mode in dev/regtest: Create test users for development
            if cfg!(debug_assertions) {
                self.ensure_dev_test_users().await?;
            }
        }
        // Cloud mode in production: Regular users created via registration

        Ok(())
    }

    async fn ensure_self_hosted_user(&self, config: &AppConfig) -> Result<()> {
        use crate::auth::AuthService;

        let pool = self.pool.clone();
        let auth_service = AuthService::new("self-hosted-bootstrap".to_string(), None);
        let admin_password = config
            .get_self_hosted_admin_password()
            .map_err(anyhow::Error::msg)?;
        let password_hash = auth_service.hash_password(admin_password)?;

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;

            // Check if self-hosted user already exists (keeping 'foss-user' ID for backwards compatibility)
            let self_hosted_user_exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM users WHERE id = 'foss-user')",
                [],
                |row| row.get(0),
            )?;

            if !self_hosted_user_exists {
                // Detect system locale for currency, fallback to USD
                let default_currency = std::env::var("LANG")
                    .or_else(|_| std::env::var("LC_ALL"))
                    .map(|locale| crate::exchange_rates::ExchangeRateService::locale_to_currency(&locale))
                    .unwrap_or("USD");

                // Create the hardcoded self-hosted user with locale-based currency and language
                let default_language = std::env::var("LANG")
                    .ok()
                    .map(|locale| locale_to_language(&locale))
                    .unwrap_or("en-US");
                conn.execute(
                    "INSERT INTO users (id, email, password_hash, name, is_admin, is_demo, email_verified, subscription_tier, subscription_status, created_at, preferred_fiat_currency, preferred_language)
                     VALUES ('foss-user', 'admin@local', '', 'Admin', 1, 0, 1, 'team', 'active', datetime('now'), ?1, ?2)",
                    params![default_currency, default_language],
                )?;

                println!("✅ Created self-hosted user: admin@local with currency: {}", default_currency);
            }

            conn.execute(
                "UPDATE users SET password_hash = ?1, email_verified = 1, is_admin = 1, is_demo = 0 WHERE id = 'foss-user'",
                params![&password_hash],
            )?;

            Ok(())
        })
        .await?
    }

    async fn ensure_demo_user(&self) -> Result<()> {
        use crate::auth::{AuthService, DEMO_USER_EMAIL, DEV_TEST_PASSWORD};

        let pool = self.pool.clone();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;

            // Create a temporary auth service to hash passwords
            let auth_service = AuthService::new("temp".to_string(), None);
            let password_hash = auth_service.hash_password(DEV_TEST_PASSWORD)?;

            // Check if demo user already exists
            let exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM users WHERE email = ?1)",
                [DEMO_USER_EMAIL],
                |row| row.get(0)
            )?;

            if !exists {
                let user_id = uuid::Uuid::new_v4().to_string();
                conn.execute(
                    "INSERT INTO users (id, email, password_hash, name, is_admin, is_demo, email_verified, subscription_tier, subscription_status, created_at, preferred_fiat_currency, preferred_language)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, datetime('now'), ?10, ?11)",
                    params![&user_id, DEMO_USER_EMAIL, &password_hash, "Demo User", false, true, true, "team", "active", "USD", "en-US"],
                )?;

                println!("[CLOUD MODE] Created demo user: {}", DEMO_USER_EMAIL);
            } else {
                // If demo user exists, ensure subscription is active (for wallet syncing)
                conn.execute(
                    "UPDATE users SET subscription_status = 'active' WHERE email = ?1 AND subscription_status != 'active'",
                    [DEMO_USER_EMAIL],
                )?;
            }

            Ok(())
        }).await??;

        Ok(())
    }

    async fn ensure_dev_test_users(&self) -> Result<()> {
        use crate::auth::{AuthService, DEV_TEST_EMAILS, DEV_TEST_PASSWORD};

        let pool = self.pool.clone();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;

            // Create a temporary auth service to hash passwords
            let auth_service = AuthService::new("temp".to_string(), None);
            let password_hash = auth_service.hash_password(DEV_TEST_PASSWORD)?;

            for (index, email) in DEV_TEST_EMAILS.iter().enumerate() {
                // Check if user already exists
                let exists: bool = conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM users WHERE email = ?1)",
                    [email],
                    |row| row.get(0)
                )?;

                if !exists {
                    let (name, tier) = match *email {
                        "delivered+admin@resend.dev" => ("Admin", "team"),
                        "delivered+alice@resend.dev" => ("Alice", "personal"),
                        "delivered+bob@resend.dev" => ("Bob", "team"),
                        "delivered+charlie@resend.dev" => ("Charlie", "team"),
                        _ => ("Test User", "personal"),
                    };

                    // First user (admin) becomes admin
                    let is_admin = index == 0;

                    let user_id = uuid::Uuid::new_v4().to_string();
                    conn.execute(
                        "INSERT INTO users (id, email, password_hash, name, is_admin, is_demo, email_verified, subscription_tier, subscription_status, created_at, preferred_fiat_currency, preferred_language)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, datetime('now'), ?10, ?11)",
                        params![&user_id, email, &password_hash, name, is_admin, false, true, tier, "pending", "USD", "en-US"],
                    )?;

                    println!("[DEV MODE] Created test user: {} (admin: {})", email, is_admin);
                }
            }

            Ok(())
        }).await?
    }

    // ============================
    // USER CRUD METHODS
    // ============================

    pub async fn create_user(
        &self,
        email: &str,
        password_hash: &str,
        name: Option<&str>,
        email_verified: bool,
        preferred_currency: Option<&str>,
        preferred_language: Option<&str>,
    ) -> Result<String> {
        let pool = self.pool.clone();
        let email = email.to_string();
        let password_hash = password_hash.to_string();
        let name = name.map(|n| n.to_string());
        let preferred_currency = preferred_currency.map(|c| c.to_string());
        let preferred_language = preferred_language.map(|l| l.to_string());

        let result = spawn_blocking(move || -> Result<(String, bool)> {
            let conn = pool.get()?;
            let tx = conn.unchecked_transaction()?;

            // Check if user already exists
            let existing: Option<String> = tx
                .prepare("SELECT id FROM users WHERE email = ?1")?
                .query_row(params![&email], |row| row.get(0))
                .ok();

            if let Some(_id) = existing {
                tx.rollback()?;
                return Err(anyhow::anyhow!("User with this email already exists"));
            }

            // Determine if this user should be admin (first user becomes admin)
            let admin_count: i64 = tx.query_row(
                "SELECT COUNT(*) FROM users WHERE is_admin = 1",
                [],
                |row| row.get(0)
            )?;

            let final_is_admin = admin_count == 0;

            if final_is_admin {
                println!("Creating first admin user: {}", email);
            } else {
                println!("Creating regular user: {} (existing admins: {})", email, admin_count);
            }

            let user_name = name;

            // Generate UUID for new user
            let user_id = Uuid::new_v4().to_string();

            println!("DEBUG: Creating user {} with name {:?}, is_admin={}", email, user_name, final_is_admin);

            // Create new user
            tx.execute(
                "INSERT INTO users (id, email, password_hash, name, is_admin, is_demo, email_verified, subscription_tier, subscription_status, preferred_fiat_currency, preferred_language) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![&user_id, &email, &password_hash, user_name, final_is_admin, false, email_verified, "team", "pending", preferred_currency.as_deref().unwrap_or("USD"), preferred_language.as_deref().unwrap_or("en-US")],
            )?;

            tx.commit()?;
            Ok((user_id, final_is_admin))
        }).await??; // First ? for JoinError, second ? for inner Result

        let (user_id, _was_admin) = result;

        Ok(user_id)
    }

    pub async fn get_user_by_email(&self, email: &str) -> Result<Option<UserRecord>> {
        let pool = self.pool.clone();
        let email = email.to_string();

        spawn_blocking(move || -> Result<Option<UserRecord>> {
            let conn = pool.get()?;
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
        }).await?
    }

    pub async fn get_user_by_id(&self, user_id: &str) -> Result<Option<UserRecord>> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();

        spawn_blocking(move || -> Result<Option<UserRecord>> {
            let conn = pool.get()?;
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
        }).await?
    }

    pub async fn get_user_by_stripe_customer_id(
        &self,
        stripe_customer_id: &str,
    ) -> Result<Option<UserRecord>> {
        let pool = self.pool.clone();
        let stripe_customer_id = stripe_customer_id.to_string();

        spawn_blocking(move || -> Result<Option<UserRecord>> {
            let conn = pool.get()?;
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
        }).await?
    }

    pub async fn update_last_login(&self, user_id: &str) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            let current_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            conn.execute(
                "UPDATE users SET last_login = ?1 WHERE id = ?2",
                params![&current_time, user_id],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn get_all_users(&self) -> Result<Vec<UserRecord>> {
        let pool = self.pool.clone();

        spawn_blocking(move || -> Result<Vec<UserRecord>> {
            let conn = pool.get()?;
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
        .await?
    }

    pub async fn update_user_name(&self, user_id: &str, name: &str) -> Result<()> {
        let pool = self.pool.clone();
        let name = name.to_string();
        let user_id = user_id.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE users SET name = ?1 WHERE id = ?2",
                params![&name, user_id],
            )?;
            Ok(())
        })
        .await?
    }

    // ============================
    // STRIPE/SUBSCRIPTION MANAGEMENT
    // ============================

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
        subscription_status: &str,
        trial_ends_at: Option<String>,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let subscription_status = subscription_status.to_string();

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
        subscription_status: &str,
        stripe_subscription_id: Option<&str>,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let subscription_status = subscription_status.to_string();
        let stripe_subscription_id = stripe_subscription_id.map(|s| s.to_string());

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

    pub async fn update_user_subscription(
        &self,
        user_id: &str,
        params: &SubscriptionUpdateParams<'_>,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let subscription_tier = params.subscription_tier.to_string();
        let subscription_status = params.subscription_status.to_string();
        let stripe_subscription_id = params.stripe_subscription_id.map(|s| s.to_string());
        let subscription_started_at = params.subscription_started_at.map(|s| s.to_string());
        let subscription_ends_at = params.subscription_ends_at.map(|s| s.to_string());
        let trial_ends_at = params.trial_ends_at.map(|s| s.to_string());

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

    // ============================
    // SESSION MANAGEMENT
    // ============================

    pub async fn create_session(
        &self,
        user_id: &str,
        token_hash: &str,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<String> {
        let pool = self.pool.clone();
        let token_hash = token_hash.to_string();
        let expires_at = expires_at.format("%Y-%m-%d %H:%M:%S").to_string();

        let user_id = user_id.to_string();
        spawn_blocking(move || -> Result<String> {
            let conn = pool.get()?;
            let session_id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO sessions (id, user_id, token_hash, expires_at) VALUES (?1, ?2, ?3, ?4)",
                params![&session_id, &user_id, &token_hash, &expires_at],
            )?;
            Ok(session_id)
        })
        .await?
    }

    pub async fn delete_session(&self, token_hash: &str) -> Result<()> {
        let pool = self.pool.clone();
        let token_hash = token_hash.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "DELETE FROM sessions WHERE token_hash = ?1",
                params![&token_hash],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn delete_user_sessions(&self, user_id: &str) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute("DELETE FROM sessions WHERE user_id = ?1", params![&user_id])?;
            Ok(())
        })
        .await?
    }

    pub async fn has_active_session(&self, token_hash: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let token_hash = token_hash.to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            let current_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            let exists = conn.query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM sessions
                    WHERE token_hash = ?1 AND expires_at >= ?2
                )",
                params![&token_hash, &current_time],
                |row| row.get::<_, i64>(0),
            )?;
            Ok(exists != 0)
        })
        .await?
    }

    pub async fn cleanup_expired_sessions(&self) -> Result<u64> {
        let pool = self.pool.clone();

        spawn_blocking(move || -> Result<u64> {
            let conn = pool.get()?;
            let current_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            let rows_deleted = conn.execute(
                "DELETE FROM sessions WHERE expires_at < ?1",
                params![&current_time],
            )?;
            Ok(rows_deleted as u64)
        })
        .await?
    }

    // ============================
    // EMAIL VERIFICATION TOKEN MANAGEMENT
    // ============================

    /// Creates an email verification token. The token should be pre-hashed by the caller
    /// using AuthService::hash_token() for security.
    pub async fn create_email_verification_token(&self, user_id: &str, token: &str) -> Result<()> {
        let pool = self.pool.clone();
        let token = token.to_string();
        let expires_at = (chrono::Utc::now() + chrono::Duration::hours(24))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        let user_id = user_id.to_string();
        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO email_verification_tokens (user_id, token, expires_at) VALUES (?1, ?2, ?3)",
                params![&user_id, &token, &expires_at],
            )?;
            Ok(())
        }).await?
    }

    /// Verifies an email verification token. The token should be pre-hashed by the caller
    /// using AuthService::hash_token() for security.
    pub async fn verify_email_token(&self, token: &str) -> Result<Option<String>> {
        let pool = self.pool.clone();
        let token = token.to_string();
        let current_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        tracing::debug!("Verifying email token: {} at time: {}", token, current_time);

        spawn_blocking(move || -> Result<Option<String>> {
            let conn = pool.get()?;
            let tx = conn.unchecked_transaction()?;

            // Get user_id for valid, non-expired token
            let user_id: Option<String> = tx
                .prepare("SELECT user_id FROM email_verification_tokens WHERE token = ?1 AND expires_at > ?2")?
                .query_row(params![&token, &current_time], |row| row.get(0))
                .ok();

            tracing::debug!("Token query result: {:?}", user_id);

            if let Some(user_id) = user_id {
                // Mark user as verified
                tx.execute(
                    "UPDATE users SET email_verified = TRUE WHERE id = ?1",
                    params![user_id],
                )?;

                // Delete the token
                tx.execute(
                    "DELETE FROM email_verification_tokens WHERE token = ?1",
                    params![&token],
                )?;

                tx.commit()?;
                Ok(Some(user_id))
            } else {
                tx.rollback()?;
                Ok(None)
            }
        }).await?
    }

    // ============================
    // PASSWORD RESET TOKEN MANAGEMENT
    // ============================

    pub async fn create_password_reset_token(&self, user_id: &str, token: &str) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let token = token.to_string();
        let expires_at = (chrono::Utc::now() + chrono::Duration::hours(1))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;

            // Delete any existing tokens for this user
            conn.execute(
                "DELETE FROM password_reset_tokens WHERE user_id = ?1",
                params![&user_id],
            )?;

            // Create new token
            conn.execute(
                "INSERT INTO password_reset_tokens (user_id, token, expires_at) VALUES (?1, ?2, ?3)",
                params![&user_id, &token, &expires_at],
            )?;
            Ok(())
        }).await?
    }

    pub async fn verify_password_reset_token(&self, token: &str) -> Result<Option<String>> {
        let pool = self.pool.clone();
        let token = token.to_string();
        let current_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        spawn_blocking(move || -> Result<Option<String>> {
            let conn = pool.get()?;
            let user_id: Option<String> = conn
                .prepare("SELECT user_id FROM password_reset_tokens WHERE token = ?1 AND expires_at > ?2")?
                .query_row(params![&token, &current_time], |row| row.get(0))
                .ok();
            Ok(user_id)
        }).await?
    }

    pub async fn update_user_password(&self, user_id: &str, password_hash: &str) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let password_hash = password_hash.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            let tx = conn.unchecked_transaction()?;

            // Update password
            tx.execute(
                "UPDATE users SET password_hash = ?1 WHERE id = ?2",
                params![&password_hash, &user_id],
            )?;

            // Delete all password reset tokens for this user
            tx.execute(
                "DELETE FROM password_reset_tokens WHERE user_id = ?1",
                params![&user_id],
            )?;

            tx.commit()?;
            Ok(())
        })
        .await?
    }

    pub async fn update_user_password_and_revoke_sessions(
        &self,
        user_id: &str,
        password_hash: &str,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let password_hash = password_hash.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            let tx = conn.unchecked_transaction()?;

            tx.execute(
                "UPDATE users SET password_hash = ?1 WHERE id = ?2",
                params![&password_hash, &user_id],
            )?;
            tx.execute(
                "DELETE FROM password_reset_tokens WHERE user_id = ?1",
                params![&user_id],
            )?;
            tx.execute("DELETE FROM sessions WHERE user_id = ?1", params![&user_id])?;

            tx.commit()?;
            Ok(())
        })
        .await?
    }

    // ============================
    // USER PREFERENCES
    // ============================

    pub async fn get_user_preferred_currency(&self, user_id: &str) -> Result<String> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        spawn_blocking(move || -> Result<String> {
            let conn = pool.get()?;
            let currency: Option<String> = conn
                .prepare("SELECT preferred_fiat_currency FROM users WHERE id = ?1")?
                .query_row(params![user_id], |row| row.get(0))
                .optional()?;
            Ok(currency.unwrap_or_else(|| "USD".to_string()))
        })
        .await?
    }

    pub async fn update_user_preferred_currency(
        &self,
        user_id: &str,
        currency: &str,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let currency = currency.to_string();
        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE users SET preferred_fiat_currency = ?1 WHERE id = ?2",
                params![currency, user_id],
            )?;
            Ok(())
        })
        .await?
    }

    /// Get user's preferred language for notifications
    pub async fn get_user_preferred_language(&self, user_id: &str) -> Result<Language> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        spawn_blocking(move || -> Result<Language> {
            let conn = pool.get()?;
            let language: Option<Option<String>> = conn
                .prepare("SELECT preferred_language FROM users WHERE id = ?1")?
                .query_row(params![user_id], |row| row.get(0))
                .optional()?;
            Ok(language
                .flatten()
                .map(|l: String| Language::from(l.as_str()))
                .unwrap_or(Language::English))
        })
        .await?
    }

    /// Update user's preferred language
    pub async fn update_user_preferred_language(
        &self,
        user_id: &str,
        language: &str,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let language = language.to_string();
        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE users SET preferred_language = ?1 WHERE id = ?2",
                params![language, user_id],
            )?;
            Ok(())
        })
        .await?
    }

    /// Get user's preferred ntfy server URL (None means use env var or default)
    pub async fn get_user_ntfy_server_url(&self, user_id: &str) -> Result<Option<String>> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        spawn_blocking(move || -> Result<Option<String>> {
            let conn = pool.get()?;
            let url: Option<Option<String>> = conn
                .prepare("SELECT ntfy_server_url FROM users WHERE id = ?1")?
                .query_row(params![user_id], |row| row.get(0))
                .optional()?;
            Ok(url.flatten())
        })
        .await?
    }

    /// Update user's preferred ntfy server URL (None to use default)
    pub async fn update_user_ntfy_server_url(
        &self,
        user_id: &str,
        ntfy_server_url: Option<&str>,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let ntfy_server_url = ntfy_server_url.map(|u| u.to_string());
        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE users SET ntfy_server_url = ?1 WHERE id = ?2",
                params![ntfy_server_url, user_id],
            )?;
            Ok(())
        })
        .await?
    }

    /// Get user's ntfy authentication credentials
    /// Returns (access_token, username, password)
    pub async fn get_user_ntfy_auth(
        &self,
        user_id: &str,
    ) -> Result<(Option<String>, Option<String>, Option<String>)> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        spawn_blocking(move || -> Result<(Option<String>, Option<String>, Option<String>)> {
            let conn = pool.get()?;
            let result: Option<(Option<String>, Option<String>, Option<String>)> = conn
                .prepare(
                    "SELECT ntfy_access_token, ntfy_username, ntfy_password FROM users WHERE id = ?1",
                )?
                .query_row(params![user_id], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
                .optional()?;
            Ok(result.unwrap_or((None, None, None)))
        })
        .await?
    }

    /// Update user's ntfy authentication credentials
    /// Set access_token for Bearer token auth, or username+password for Basic auth
    /// Setting access_token will clear username/password and vice versa
    pub async fn update_user_ntfy_auth(
        &self,
        user_id: &str,
        access_token: Option<&str>,
        username: Option<&str>,
        password: Option<&str>,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let user_id = user_id.to_string();
        let access_token = access_token.map(|s| s.to_string());
        let username = username.map(|s| s.to_string());
        let password = password.map(|s| s.to_string());
        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE users SET ntfy_access_token = ?1, ntfy_username = ?2, ntfy_password = ?3 WHERE id = ?4",
                params![access_token, username, password, user_id],
            )?;
            Ok(())
        })
        .await?
    }

    // ============================
    // ENDPOINT RATE LIMITING
    // ============================

    /// Check and record a rate-limited endpoint attempt.
    /// Returns the allow/deny decision plus retry guidance when blocked.
    pub async fn check_endpoint_rate_limit(
        &self,
        scope: &str,
        identifier: &str,
        max_attempts: i64,
        window_minutes: i64,
    ) -> Result<RateLimitDecision> {
        let pool = self.pool.clone();
        let scope = scope.to_string();
        let identifier = identifier.trim().to_lowercase();
        let now = chrono::Utc::now();
        let now_naive = now.naive_utc();
        let now_str = now.format("%Y-%m-%d %H:%M:%S").to_string();
        let window_start = (now - chrono::Duration::minutes(window_minutes))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        let blocked_until = (now + chrono::Duration::minutes(window_minutes))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        spawn_blocking(move || -> Result<RateLimitDecision> {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

            let existing: Option<(i64, String, Option<String>)> = tx
                .prepare(
                    "SELECT attempt_count, first_attempt_at, blocked_until
                     FROM auth_rate_limits
                     WHERE scope = ?1 AND identifier = ?2",
                )?
                .query_row(params![&scope, &identifier], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
                .optional()?;

            let decision = match existing {
                Some((_, _, Some(blocked))) if blocked > now_str => {
                    let retry_after_seconds = chrono::NaiveDateTime::parse_from_str(
                        &blocked,
                        "%Y-%m-%d %H:%M:%S",
                    )
                    .map(|blocked_until| (blocked_until - now_naive).num_seconds().max(1))
                    .unwrap_or(window_minutes * 60);

                    RateLimitDecision {
                        allowed: false,
                        retry_after_seconds: Some(retry_after_seconds),
                    }
                }
                Some((_, _, Some(_))) => {
                    tx.execute(
                        "UPDATE auth_rate_limits
                         SET attempt_count = 1, first_attempt_at = ?3, blocked_until = NULL
                         WHERE scope = ?1 AND identifier = ?2",
                        params![&scope, &identifier, &now_str],
                    )?;
                    RateLimitDecision {
                        allowed: true,
                        retry_after_seconds: None,
                    }
                }
                Some((attempt_count, first_attempt_at, _)) if first_attempt_at >= window_start => {
                    let next_attempt_count = attempt_count + 1;
                    if next_attempt_count > max_attempts {
                        tx.execute(
                            "UPDATE auth_rate_limits
                             SET attempt_count = ?3, first_attempt_at = ?4, blocked_until = ?5
                             WHERE scope = ?1 AND identifier = ?2",
                            params![
                                &scope,
                                &identifier,
                                max_attempts,
                                &now_str,
                                &blocked_until
                            ],
                        )?;
                        RateLimitDecision {
                            allowed: false,
                            retry_after_seconds: Some(window_minutes * 60),
                        }
                    } else {
                        tx.execute(
                            "UPDATE auth_rate_limits
                             SET attempt_count = ?3, blocked_until = NULL
                             WHERE scope = ?1 AND identifier = ?2",
                            params![&scope, &identifier, next_attempt_count],
                        )?;
                        RateLimitDecision {
                            allowed: true,
                            retry_after_seconds: None,
                        }
                    }
                }
                Some(_) => {
                    tx.execute(
                        "UPDATE auth_rate_limits
                         SET attempt_count = 1, first_attempt_at = ?3, blocked_until = NULL
                         WHERE scope = ?1 AND identifier = ?2",
                        params![&scope, &identifier, &now_str],
                    )?;
                    RateLimitDecision {
                        allowed: true,
                        retry_after_seconds: None,
                    }
                }
                None => {
                    tx.execute(
                        "INSERT INTO auth_rate_limits (scope, identifier, attempt_count, first_attempt_at)
                         VALUES (?1, ?2, 1, ?3)",
                        params![&scope, &identifier, &now_str],
                    )?;
                    RateLimitDecision {
                        allowed: true,
                        retry_after_seconds: None,
                    }
                }
            };

            tx.commit()?;
            Ok(decision)
        })
        .await?
    }

    /// Check and record a rate-limited auth endpoint attempt.
    /// Returns Ok(true) when the request should proceed and Ok(false) when blocked.
    pub async fn check_auth_rate_limit(
        &self,
        scope: &str,
        identifier: &str,
        max_attempts: i64,
        window_minutes: i64,
    ) -> Result<bool> {
        Ok(self
            .check_endpoint_rate_limit(scope, identifier, max_attempts, window_minutes)
            .await?
            .allowed)
    }

    /// Check if an account is currently locked due to too many failed login attempts
    /// Returns the lockout end time if locked, None if not locked
    pub async fn check_account_lockout(&self, email: &str) -> Result<Option<String>> {
        let pool = self.pool.clone();
        let email = email.to_string();
        let current_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        spawn_blocking(move || -> Result<Option<String>> {
            let conn = pool.get()?;
            let locked_until: Option<String> = conn
                .prepare("SELECT locked_until FROM users WHERE email = ?1 AND locked_until > ?2")?
                .query_row(params![&email, &current_time], |row| row.get(0))
                .optional()?;
            Ok(locked_until)
        })
        .await?
    }

    /// Check if lockout has expired and clear it if so
    /// Returns true if an expired lockout was cleared, false otherwise
    pub async fn clear_expired_lockout(&self, email: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let email = email.to_string();
        let current_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        spawn_blocking(move || -> Result<bool> {
            let conn = pool.get()?;
            // Check if there's an expired lockout (locked_until is set but in the past)
            let has_expired_lockout: bool = conn
                .prepare(
                    "SELECT 1 FROM users WHERE email = ?1 AND locked_until IS NOT NULL AND locked_until <= ?2",
                )?
                .query_row(params![&email, &current_time], |_| Ok(true))
                .optional()?
                .unwrap_or(false);

            if has_expired_lockout {
                // Clear the expired lockout and reset counter
                conn.execute(
                    "UPDATE users SET failed_login_attempts = 0, locked_until = NULL WHERE email = ?1",
                    params![&email],
                )?;
                Ok(true)
            } else {
                Ok(false)
            }
        })
        .await?
    }

    /// Record a login attempt (successful or failed)
    pub async fn record_login_attempt(&self, email: &str, success: bool) -> Result<()> {
        let pool = self.pool.clone();
        let email = email.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO login_attempts (email, success) VALUES (?1, ?2)",
                params![&email, success],
            )?;
            Ok(())
        })
        .await?
    }

    /// Get the count of failed login attempts in the last N minutes
    pub async fn get_failed_login_count(&self, email: &str, window_minutes: i64) -> Result<i64> {
        let pool = self.pool.clone();
        let email = email.to_string();
        let window_start = (chrono::Utc::now() - chrono::Duration::minutes(window_minutes))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        spawn_blocking(move || -> Result<i64> {
            let conn = pool.get()?;
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM login_attempts WHERE email = ?1 AND attempt_time > ?2 AND success = FALSE",
                params![&email, &window_start],
                |row| row.get(0),
            )?;
            Ok(count)
        })
        .await?
    }

    /// Lock a user account for a specified duration
    pub async fn lock_account(&self, email: &str, lock_duration_minutes: i64) -> Result<()> {
        let pool = self.pool.clone();
        let email = email.to_string();
        let locked_until = (chrono::Utc::now() + chrono::Duration::minutes(lock_duration_minutes))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE users SET locked_until = ?1 WHERE email = ?2",
                params![&locked_until, &email],
            )?;
            Ok(())
        })
        .await?
    }

    /// Increment failed login counter for a user
    pub async fn increment_failed_login_count(&self, email: &str) -> Result<i64> {
        let pool = self.pool.clone();
        let email = email.to_string();

        spawn_blocking(move || -> Result<i64> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE users SET failed_login_attempts = COALESCE(failed_login_attempts, 0) + 1 WHERE email = ?1",
                params![&email],
            )?;
            let count: i64 = conn.query_row(
                "SELECT COALESCE(failed_login_attempts, 0) FROM users WHERE email = ?1",
                params![&email],
                |row| row.get(0),
            )?;
            Ok(count)
        })
        .await?
    }

    /// Reset failed login counter on successful login
    pub async fn reset_failed_login_count(&self, email: &str) -> Result<()> {
        let pool = self.pool.clone();
        let email = email.to_string();

        spawn_blocking(move || -> Result<()> {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE users SET failed_login_attempts = 0, locked_until = NULL WHERE email = ?1",
                params![&email],
            )?;
            Ok(())
        })
        .await?
    }

    /// Clean up old login attempts (older than N days)
    pub async fn cleanup_old_login_attempts(&self, days: i64) -> Result<u64> {
        let pool = self.pool.clone();
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        spawn_blocking(move || -> Result<u64> {
            let conn = pool.get()?;
            let deleted = conn.execute(
                "DELETE FROM login_attempts WHERE attempt_time < ?1",
                params![&cutoff],
            )?;
            Ok(deleted as u64)
        })
        .await?
    }
}
