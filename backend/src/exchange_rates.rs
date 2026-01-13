use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use iso_currency::{Country, Currency};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::time::interval;
use unic_langid::LanguageIdentifier;

use crate::metadata::MetadataDb;

// All supported fiat currencies from CoinGecko
pub const SUPPORTED_CURRENCIES: &[&str] = &[
    "USD", "AED", "ARS", "AUD", "BDT", "BHD", "BMD", "BRL", "CAD", "CHF", "CLP", "CNY", "CZK",
    "DKK", "EUR", "GBP", "GEL", "HKD", "HUF", "IDR", "ILS", "INR", "JPY", "KRW", "KWD", "LKR",
    "MMK", "MXN", "MYR", "NGN", "NOK", "NZD", "PHP", "PKR", "PLN", "RUB", "SAR", "SEK", "SGD",
    "THB", "TRY", "TWD", "UAH", "VEF", "VND", "ZAR",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeRate {
    pub currency: String,
    pub rate_per_btc: f64,
    pub last_updated: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct CoinGeckoResponse {
    bitcoin: HashMap<String, f64>,
}

pub struct ExchangeRateService {
    metadata_db: Arc<MetadataDb>,
}

impl ExchangeRateService {
    pub fn new(metadata_db: Arc<MetadataDb>) -> Self {
        Self { metadata_db }
    }

    /// Map a bare language code to its default country code
    /// Only needed for languages without a country in the locale
    fn language_to_default_country(lang: &str) -> Option<&'static str> {
        match lang {
            "en" => Some("US"),
            "fr" => Some("FR"),
            "de" => Some("DE"),
            "es" => Some("ES"),
            "pt" => Some("PT"),
            "it" => Some("IT"),
            "nl" => Some("NL"),
            "ja" => Some("JP"),
            "ko" => Some("KR"),
            "zh" => Some("CN"),
            "ru" => Some("RU"),
            "ar" => Some("SA"),
            "he" => Some("IL"),
            "tr" => Some("TR"),
            "pl" => Some("PL"),
            "cs" => Some("CZ"),
            "hu" => Some("HU"),
            "el" => Some("GR"),
            "fi" => Some("FI"),
            "sv" => Some("SE"),
            "da" => Some("DK"),
            "no" | "nb" | "nn" => Some("NO"),
            "th" => Some("TH"),
            "vi" => Some("VN"),
            "id" => Some("ID"),
            "ms" => Some("MY"),
            "hi" => Some("IN"),
            "uk" => Some("UA"),
            "af" => Some("ZA"),
            _ => None,
        }
    }

    /// Get currency code from a Country using iso_currency
    fn country_to_currency(country: Country) -> &'static str {
        let currencies = Currency::from_country(country);
        if let Some(currency) = currencies.first() {
            currency.code()
        } else {
            "USD"
        }
    }

    /// Map browser locale to appropriate fiat currency
    ///
    /// Handles various locale formats:
    /// - Simple: "en", "fr", "de"
    /// - With region: "en-US", "fr-FR", "de-DE"
    /// - With underscore: "en_US", "de_DE"
    /// - With encoding: "de_DE.UTF-8", "en_US.UTF-8"
    /// - With script: "zh-Hant-TW", "zh-Hans-CN"
    /// - With quality: "fr;q=0.8", "en-US;q=0.9"
    pub fn locale_to_currency(locale: &str) -> &'static str {
        // Strip quality value if present: "fr;q=0.8" -> "fr"
        let locale_clean = locale.split(';').next().unwrap_or(locale);

        // Strip encoding suffix if present: "de_DE.UTF-8" -> "de_DE"
        let locale_no_encoding = locale_clean.split('.').next().unwrap_or(locale_clean);

        // Normalize underscore to hyphen for unic-langid: "de_DE" -> "de-DE"
        let normalized = locale_no_encoding.replace('_', "-");

        // Parse with unic-langid (handles script tags like zh-Hant-TW correctly)
        if let Ok(lang_id) = normalized.parse::<LanguageIdentifier>() {
            // Try region first if available
            if let Some(region) = lang_id.region {
                if let Ok(country) = Country::from_str(&region.to_string()) {
                    return Self::country_to_currency(country);
                }
            }

            // Fall back to language -> default country mapping
            let lang_str = lang_id.language.to_string();
            if let Some(country_code) = Self::language_to_default_country(&lang_str) {
                if let Ok(country) = Country::from_str(country_code) {
                    return Self::country_to_currency(country);
                }
            }
        }

        "USD"
    }

    /// Fetch exchange rates from CoinGecko API
    pub async fn fetch_rates(&self) -> Result<HashMap<String, ExchangeRate>> {
        let currencies = SUPPORTED_CURRENCIES.join(",").to_lowercase();
        let url = format!(
            "https://api.coingecko.com/api/v3/simple/price?ids=bitcoin&vs_currencies={}",
            currencies
        );

        eprintln!("Fetching exchange rates from CoinGecko...");

        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .header(
                "User-Agent",
                format!("Canary/{} (Bitcoin Wallet)", env!("CARGO_PKG_VERSION")),
            )
            .send()
            .await
            .context("Failed to fetch exchange rates")?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read response body".to_string());
            anyhow::bail!(
                "Exchange rates API returned HTTP {}: {}",
                status.as_u16(),
                body.chars().take(500).collect::<String>()
            );
        }

        let body = response
            .text()
            .await
            .context("Failed to read exchange rates response body")?;

        let data: CoinGeckoResponse = serde_json::from_str(&body).with_context(|| {
            format!(
                "Failed to parse exchange rates response (HTTP {}): {}",
                status.as_u16(),
                body.chars().take(500).collect::<String>()
            )
        })?;

        let now = Utc::now();
        let mut rates = HashMap::new();

        for (currency, rate) in data.bitcoin {
            let currency_upper = currency.to_uppercase();
            rates.insert(
                currency_upper.clone(),
                ExchangeRate {
                    currency: currency_upper,
                    rate_per_btc: rate,
                    last_updated: now,
                },
            );
        }

        eprintln!("Fetched {} exchange rates", rates.len());
        Ok(rates)
    }

    /// Start background task to refresh exchange rates periodically
    pub fn start_refresh_task(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = interval(std::time::Duration::from_secs(600)); // 10 minutes

            loop {
                interval.tick().await;

                if let Err(e) = self.fetch_and_store_rates().await {
                    eprintln!("Failed to refresh exchange rates: {}", e);
                }
            }
        });
    }

    async fn fetch_and_store_rates(&self) -> Result<()> {
        let rates = self.fetch_rates().await?;
        self.metadata_db.store_exchange_rates(&rates).await?;
        eprintln!("Exchange rates refreshed successfully");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_locale_to_currency_simple_language() {
        // Simple language codes should map to default country's currency
        assert_eq!(ExchangeRateService::locale_to_currency("en"), "USD");
        assert_eq!(ExchangeRateService::locale_to_currency("de"), "EUR");
        assert_eq!(ExchangeRateService::locale_to_currency("fr"), "EUR");
        assert_eq!(ExchangeRateService::locale_to_currency("ja"), "JPY");
        assert_eq!(ExchangeRateService::locale_to_currency("no"), "NOK");
        assert_eq!(ExchangeRateService::locale_to_currency("sv"), "SEK");
        assert_eq!(ExchangeRateService::locale_to_currency("da"), "DKK");
    }

    #[test]
    fn test_locale_to_currency_with_region() {
        // Language with region should use region's currency
        assert_eq!(ExchangeRateService::locale_to_currency("en-US"), "USD");
        assert_eq!(ExchangeRateService::locale_to_currency("en-GB"), "GBP");
        assert_eq!(ExchangeRateService::locale_to_currency("de-DE"), "EUR");
        assert_eq!(ExchangeRateService::locale_to_currency("de-AT"), "EUR");
        assert_eq!(ExchangeRateService::locale_to_currency("fr-FR"), "EUR");
        assert_eq!(ExchangeRateService::locale_to_currency("fr-CA"), "CAD");
        assert_eq!(ExchangeRateService::locale_to_currency("pt-BR"), "BRL");
        assert_eq!(ExchangeRateService::locale_to_currency("pt-PT"), "EUR");
    }

    #[test]
    fn test_locale_to_currency_with_underscore() {
        // Underscore separator should work the same as hyphen
        assert_eq!(ExchangeRateService::locale_to_currency("en_US"), "USD");
        assert_eq!(ExchangeRateService::locale_to_currency("de_DE"), "EUR");
        assert_eq!(ExchangeRateService::locale_to_currency("fr_FR"), "EUR");
        assert_eq!(ExchangeRateService::locale_to_currency("no_NO"), "NOK");
    }

    #[test]
    fn test_locale_to_currency_with_encoding() {
        // Encoding suffixes like .UTF-8 should be stripped
        assert_eq!(
            ExchangeRateService::locale_to_currency("de_DE.UTF-8"),
            "EUR"
        );
        assert_eq!(
            ExchangeRateService::locale_to_currency("en_US.UTF-8"),
            "USD"
        );
        assert_eq!(
            ExchangeRateService::locale_to_currency("fr_FR.ISO-8859-1"),
            "EUR"
        );
        assert_eq!(
            ExchangeRateService::locale_to_currency("ja_JP.UTF-8"),
            "JPY"
        );
    }

    #[test]
    fn test_locale_to_currency_with_script() {
        // Script tags should be handled correctly (zh-Hant-TW -> TW -> TWD)
        assert_eq!(ExchangeRateService::locale_to_currency("zh-Hant-TW"), "TWD");
        assert_eq!(ExchangeRateService::locale_to_currency("zh-Hans-CN"), "CNY");
        assert_eq!(ExchangeRateService::locale_to_currency("sr-Latn-RS"), "RSD");
    }

    #[test]
    fn test_locale_to_currency_with_quality() {
        // Quality values should be stripped
        assert_eq!(ExchangeRateService::locale_to_currency("fr;q=0.8"), "EUR");
        assert_eq!(
            ExchangeRateService::locale_to_currency("en-US;q=0.9"),
            "USD"
        );
        assert_eq!(
            ExchangeRateService::locale_to_currency("de-DE;q=0.7"),
            "EUR"
        );
    }

    #[test]
    fn test_locale_to_currency_unknown_falls_back_to_usd() {
        // Unknown locales should fall back to USD
        assert_eq!(ExchangeRateService::locale_to_currency("xx"), "USD");
        assert_eq!(ExchangeRateService::locale_to_currency(""), "USD");
        assert_eq!(
            ExchangeRateService::locale_to_currency("invalid-locale"),
            "USD"
        );
    }

    #[test]
    fn test_locale_to_currency_case_insensitive() {
        // Should handle different cases
        assert_eq!(ExchangeRateService::locale_to_currency("EN-US"), "USD");
        assert_eq!(ExchangeRateService::locale_to_currency("en-us"), "USD");
        assert_eq!(ExchangeRateService::locale_to_currency("DE-de"), "EUR");
    }
}
