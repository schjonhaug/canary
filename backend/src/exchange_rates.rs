use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

    /// Map browser locale to appropriate fiat currency
    pub fn locale_to_currency(locale: &str) -> &'static str {
        // Normalize locale format: convert underscores to hyphens (e.g., "no_NO" -> "no-no")
        let locale_lower = locale.to_lowercase().replace('_', "-");

        // Direct locale mappings
        match locale_lower.as_str() {
            // Americas
            l if l.starts_with("en-us") => "USD",
            l if l.starts_with("en-ca") || l.starts_with("fr-ca") => "CAD",
            l if l.starts_with("es-mx") => "MXN",
            l if l.starts_with("pt-br") => "BRL",
            l if l.starts_with("es-ar") => "ARS",
            l if l.starts_with("es-cl") => "CLP",

            // Europe
            l if l.starts_with("en-gb") => "GBP",
            l if l.starts_with("de-ch") => "CHF",
            l if l.starts_with("fr-ch") => "CHF",
            l if l.starts_with("it-ch") => "CHF",
            l if l.starts_with("nb-no") || l.starts_with("nn-no") || l.starts_with("no-") => "NOK",
            l if l.starts_with("sv-se") => "SEK",
            l if l.starts_with("da-dk") => "DKK",
            l if l.starts_with("cs-cz") => "CZK",
            l if l.starts_with("hu-hu") => "HUF",
            l if l.starts_with("pl-pl") => "PLN",
            l if l.starts_with("ru-ru") => "RUB",
            l if l.starts_with("uk-ua") => "UAH",
            l if l.starts_with("tr-tr") => "TRY",

            // Eurozone countries
            l if l.starts_with("de-de") || l.starts_with("de-at") => "EUR",
            l if l.starts_with("fr-fr") || l.starts_with("fr-be") => "EUR",
            l if l.starts_with("es-es") => "EUR",
            l if l.starts_with("it-it") => "EUR",
            l if l.starts_with("nl-nl") || l.starts_with("nl-be") => "EUR",
            l if l.starts_with("pt-pt") => "EUR",
            l if l.starts_with("el-gr") => "EUR",
            l if l.starts_with("fi-fi") => "EUR",

            // Asia-Pacific
            l if l.starts_with("zh-cn") => "CNY",
            l if l.starts_with("zh-tw") => "TWD",
            l if l.starts_with("zh-hk") => "HKD",
            l if l.starts_with("zh-sg") => "SGD",
            l if l.starts_with("ja-jp") => "JPY",
            l if l.starts_with("ko-kr") => "KRW",
            l if l.starts_with("en-in") || l.starts_with("hi-in") => "INR",
            l if l.starts_with("en-au") => "AUD",
            l if l.starts_with("en-nz") => "NZD",
            l if l.starts_with("en-sg") => "SGD",
            l if l.starts_with("en-hk") => "HKD",
            l if l.starts_with("th-th") => "THB",
            l if l.starts_with("vi-vn") => "VND",
            l if l.starts_with("id-id") => "IDR",
            l if l.starts_with("ms-my") => "MYR",
            l if l.starts_with("en-ph") || l.starts_with("fil-ph") => "PHP",
            l if l.starts_with("ur-pk") || l.starts_with("en-pk") => "PKR",
            l if l.starts_with("bn-bd") || l.starts_with("en-bd") => "BDT",
            l if l.starts_with("si-lk") || l.starts_with("en-lk") => "LKR",
            l if l.starts_with("my-mm") => "MMK",

            // Middle East
            l if l.starts_with("ar-sa") => "SAR",
            l if l.starts_with("ar-ae") => "AED",
            l if l.starts_with("ar-kw") => "KWD",
            l if l.starts_with("ar-bh") => "BHD",
            l if l.starts_with("he-il") => "ILS",
            l if l.starts_with("ka-ge") => "GEL",

            // Africa
            l if l.starts_with("en-za") || l.starts_with("af-za") => "ZAR",
            l if l.starts_with("en-ng") => "NGN",

            // Default to USD for unknown locales
            _ => "USD",
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
