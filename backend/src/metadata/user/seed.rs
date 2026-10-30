//! Startup user bootstrap: the self-hosted admin, the cloud demo user and dev test users.

use crate::config::AppConfig;
use crate::metadata::pool::MetadataDb;
use crate::metadata::status::SubscriptionStatus;
use crate::metadata::types::locale_to_language;
use anyhow::Result;
use bdk_wallet::rusqlite::params;
use tracing::info;

impl MetadataDb {
    pub(in crate::metadata) async fn initialize_user_for_mode(
        &self,
        config: &AppConfig,
    ) -> Result<()> {
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
        let admin_password = config
            .get_self_hosted_admin_password()
            .map_err(anyhow::Error::msg)?;
        let password_hash = AuthService::hash_password_async(admin_password).await?;
        let system_locale = config.system_locale().clone();

        self.with_conn(move |conn| {
            // Check if self-hosted user already exists (keeping 'foss-user' ID for backwards compatibility)
            let self_hosted_user_exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM users WHERE id = 'foss-user')",
                [],
                |row| row.get(0),
            )?;

            if !self_hosted_user_exists {
                // Detect system locale for currency, fallback to USD
                let default_currency = system_locale
                    .currency_locale()
                    .map_or("USD", crate::exchange_rates::ExchangeRateService::locale_to_currency);

                // Create the hardcoded self-hosted user with locale-based currency and language
                let default_language = system_locale
                    .language_locale()
                    .map_or("en-US", locale_to_language);
                conn.execute(
                    "INSERT INTO users (id, email, password_hash, name, is_admin, is_demo, email_verified, subscription_tier, subscription_status, created_at, preferred_fiat_currency, preferred_language)
                     VALUES ('foss-user', 'admin@local', '', 'Admin', 1, 0, 1, 'team', 'active', datetime('now'), ?1, ?2)",
                    params![default_currency, default_language],
                )?;

                info!(email = "admin@local", currency = default_currency, "Created self-hosted user");
            }

            conn.execute(
                "UPDATE users SET password_hash = ?1, email_verified = 1, is_admin = 1, is_demo = 0 WHERE id = 'foss-user'",
                params![&password_hash],
            )?;

            Ok(())
        })
        .await
    }

    async fn ensure_demo_user(&self) -> Result<()> {
        use crate::auth::{AuthService, DEMO_USER_EMAIL, DEV_TEST_PASSWORD};

        self.with_conn(move |conn| {
            // Create a temporary auth service to hash passwords
            let password_hash = AuthService::hash_password(DEV_TEST_PASSWORD)?;

            // Check if demo user already exists
            let exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM users WHERE email = ?1)",
                [DEMO_USER_EMAIL],
                |row| row.get(0)
            )?;

            if exists {
                // If demo user exists, ensure subscription is active (for wallet syncing)
                conn.execute(
                    "UPDATE users SET subscription_status = 'active' WHERE email = ?1 AND subscription_status != 'active'",
                    [DEMO_USER_EMAIL],
                )?;
            } else {
                let user_id = uuid::Uuid::new_v4().to_string();
                conn.execute(
                    "INSERT INTO users (id, email, password_hash, name, is_admin, is_demo, email_verified, subscription_tier, subscription_status, created_at, preferred_fiat_currency, preferred_language)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, datetime('now'), ?10, ?11)",
                    params![&user_id, DEMO_USER_EMAIL, &password_hash, "Demo User", false, true, true, "team", SubscriptionStatus::Active, "USD", "en-US"],
                )?;

                info!(email = DEMO_USER_EMAIL, "Created demo user");
            }

            Ok(())
        })
        .await
    }

    async fn ensure_dev_test_users(&self) -> Result<()> {
        use crate::auth::{AuthService, DEV_TEST_EMAILS, DEV_TEST_PASSWORD};

        self.with_conn(move |conn| {
            // Create a temporary auth service to hash passwords
            let password_hash = AuthService::hash_password(DEV_TEST_PASSWORD)?;

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
                        params![&user_id, email, &password_hash, name, is_admin, false, true, tier, SubscriptionStatus::Pending, "USD", "en-US"],
                    )?;

                    info!(email = %email, is_admin, "Created dev test user");
                }
            }

            Ok(())
        }).await
    }
}
