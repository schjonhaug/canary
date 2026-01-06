use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use iso_currency::{Country, Currency};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::time::interval;

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
    pub fn locale_to_currency(locale: &str) -> &'static str {
        // Normalize locale: convert underscores to hyphens, lowercase
        let normalized = locale.to_lowercase().replace('_', "-");

        // Split into parts: "fr-FR" -> ["fr", "FR"], "fr" -> ["fr"]
        let parts: Vec<&str> = normalized.split('-').collect();

        let country_code = if parts.len() >= 2 {
            // Has country code: use it directly (uppercase)
            parts[1].to_uppercase()
        } else {
            // Bare language code: map to default country
            match Self::language_to_default_country(parts[0]) {
                Some(code) => code.to_string(),
                None => return "USD",
            }
        };

        // Try to parse country code and get currency
        match Country::from_str(&country_code) {
            Ok(country) => Self::country_to_currency(country),
            Err(_) => "USD",
        }
    }

    /// Fetch exchange rates from CoinGecko API
    pub async fn fetch_rates(&self) -> Result<HashMap<String, ExchangeRate>> {
        let currencies = SUPPORTED_CURRENCIES.join(",").to_lowercase();
        let url = format!(
            "https://api.coingecko.com/api/v3/simple/price?ids=bitcoin&vs_currencies={}",
            currencies
        );

        eprintln!("Fetching exchange rates from CoinGecko...");

        let response = reqwest::get(&url)
            .await
            .context("Failed to fetch exchange rates")?;

        let data: CoinGeckoResponse = response
            .json()
            .await
            .context("Failed to parse exchange rates response")?;

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
