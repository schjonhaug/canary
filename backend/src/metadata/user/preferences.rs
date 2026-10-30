//! Per-user preference columns: fiat currency, language, transaction explorer and ntfy server settings.

use crate::metadata::pool::MetadataDb;
use crate::metadata::types::Language;
use anyhow::Result;
use bdk_wallet::rusqlite::{params, OptionalExtension};

impl MetadataDb {
    pub async fn get_user_preferred_currency(&self, user_id: &str) -> Result<String> {
        let user_id = user_id.to_string();
        self.with_conn(move |conn| {
            let currency: Option<String> = conn
                .prepare("SELECT preferred_fiat_currency FROM users WHERE id = ?1")?
                .query_row(params![user_id], |row| row.get(0))
                .optional()?;
            Ok(currency.unwrap_or_else(|| "USD".to_string()))
        })
        .await
    }

    pub async fn update_user_preferred_currency(
        &self,
        user_id: &str,
        currency: &str,
    ) -> Result<()> {
        let user_id = user_id.to_string();
        let currency = currency.to_string();
        self.with_conn(move |conn| {
            conn.execute(
                "UPDATE users SET preferred_fiat_currency = ?1 WHERE id = ?2",
                params![currency, user_id],
            )?;
            Ok(())
        })
        .await
    }

    /// Get user's preferred language for notifications
    pub async fn get_user_preferred_language(&self, user_id: &str) -> Result<Language> {
        let user_id = user_id.to_string();
        self.with_conn(move |conn| {
            let language: Option<Option<String>> = conn
                .prepare("SELECT preferred_language FROM users WHERE id = ?1")?
                .query_row(params![user_id], |row| row.get(0))
                .optional()?;
            Ok(language
                .flatten()
                .map_or(Language::English, |l: String| Language::from(l.as_str())))
        })
        .await
    }

    /// Update user's preferred language
    pub async fn update_user_preferred_language(
        &self,
        user_id: &str,
        language: &str,
    ) -> Result<()> {
        let user_id = user_id.to_string();
        let language = language.to_string();
        self.with_conn(move |conn| {
            conn.execute(
                "UPDATE users SET preferred_language = ?1 WHERE id = ?2",
                params![language, user_id],
            )?;
            Ok(())
        })
        .await
    }

    /// Get user's preferred transaction explorer ID
    pub async fn get_user_preferred_tx_explorer_id(&self, user_id: &str) -> Result<Option<String>> {
        let user_id = user_id.to_string();
        self.with_conn(move |conn| {
            let explorer_id: Option<Option<String>> = conn
                .prepare("SELECT preferred_tx_explorer_id FROM users WHERE id = ?1")?
                .query_row(params![user_id], |row| row.get(0))
                .optional()?;
            Ok(explorer_id.flatten())
        })
        .await
    }

    /// Update user's preferred transaction explorer ID
    pub async fn update_user_preferred_tx_explorer_id(
        &self,
        user_id: &str,
        explorer_id: Option<&str>,
    ) -> Result<()> {
        let user_id = user_id.to_string();
        let explorer_id = explorer_id.map(std::string::ToString::to_string);
        self.with_conn(move |conn| {
            conn.execute(
                "UPDATE users SET preferred_tx_explorer_id = ?1 WHERE id = ?2",
                params![explorer_id, user_id],
            )?;
            Ok(())
        })
        .await
    }

    /// Get user's preferred ntfy server URL (None means use env var or default)
    pub async fn get_user_ntfy_server_url(&self, user_id: &str) -> Result<Option<String>> {
        let user_id = user_id.to_string();
        self.with_conn(move |conn| {
            let url: Option<Option<String>> = conn
                .prepare("SELECT ntfy_server_url FROM users WHERE id = ?1")?
                .query_row(params![user_id], |row| row.get(0))
                .optional()?;
            Ok(url.flatten())
        })
        .await
    }

    /// Update user's preferred ntfy server URL (None to use default)
    pub async fn update_user_ntfy_server_url(
        &self,
        user_id: &str,
        ntfy_server_url: Option<&str>,
    ) -> Result<()> {
        let user_id = user_id.to_string();
        let ntfy_server_url = ntfy_server_url.map(std::string::ToString::to_string);
        self.with_conn(move |conn| {
            conn.execute(
                "UPDATE users SET ntfy_server_url = ?1 WHERE id = ?2",
                params![ntfy_server_url, user_id],
            )?;
            Ok(())
        })
        .await
    }

    /// Get user's ntfy authentication credentials
    /// Returns (`access_token`, username, password)
    pub async fn get_user_ntfy_auth(
        &self,
        user_id: &str,
    ) -> Result<(Option<String>, Option<String>, Option<String>)> {
        let user_id = user_id.to_string();
        self.with_conn(move |conn| {
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
        .await
    }

    /// Update user's ntfy authentication credentials
    /// Set `access_token` for Bearer token auth, or username+password for Basic auth
    /// Setting `access_token` will clear username/password and vice versa
    pub async fn update_user_ntfy_auth(
        &self,
        user_id: &str,
        access_token: Option<&str>,
        username: Option<&str>,
        password: Option<&str>,
    ) -> Result<()> {
        let user_id = user_id.to_string();
        let access_token = access_token.map(std::string::ToString::to_string);
        let username = username.map(std::string::ToString::to_string);
        let password = password.map(std::string::ToString::to_string);
        self.with_conn(move |conn| {
            conn.execute(
                "UPDATE users SET ntfy_access_token = ?1, ntfy_username = ?2, ntfy_password = ?3 WHERE id = ?4",
                params![access_token, username, password, user_id],
            )?;
            Ok(())
        })
        .await
    }
}
