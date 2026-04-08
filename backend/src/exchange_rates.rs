use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use iso_currency::{Country, Currency};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::time::{interval, sleep, Duration};
use unic_langid::LanguageIdentifier;

#[cfg(test)]
use crate::config::{AppConfig, NetworkConfig, OperatingMode};
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

const EXCHANGE_RATE_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const EXCHANGE_RATE_MAX_ATTEMPTS: u32 = 3;
const EXCHANGE_RATE_RETRY_BASE_DELAY: Duration = Duration::from_secs(2);
const EXCHANGE_RATE_REFRESH_INTERVAL: Duration = Duration::from_secs(600);
const COINGECKO_API_BASE_URL: &str = "https://api.coingecko.com";

enum FetchRatesError {
    Retryable(anyhow::Error),
    NonRetryable(anyhow::Error),
}

pub struct ExchangeRateService {
    metadata_db: Arc<MetadataDb>,
    client: reqwest::Client,
    api_base_url: String,
    retry_base_delay: Duration,
}

impl ExchangeRateService {
    pub fn new(metadata_db: Arc<MetadataDb>) -> Result<Self> {
        Ok(Self {
            metadata_db,
            client: Self::build_http_client()?,
            api_base_url: COINGECKO_API_BASE_URL.to_string(),
            retry_base_delay: EXCHANGE_RATE_RETRY_BASE_DELAY,
        })
    }

    fn build_http_client() -> Result<reqwest::Client> {
        reqwest::Client::builder()
            .timeout(EXCHANGE_RATE_REQUEST_TIMEOUT)
            .build()
            .context("Failed to build exchange rate HTTP client")
    }

    #[cfg(test)]
    fn new_for_test(
        metadata_db: Arc<MetadataDb>,
        client: reqwest::Client,
        api_base_url: String,
        retry_base_delay: Duration,
    ) -> Self {
        Self {
            metadata_db,
            client,
            api_base_url,
            retry_base_delay,
        }
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
            "{}/api/v3/simple/price?ids=bitcoin&vs_currencies={}",
            self.api_base_url.trim_end_matches('/'),
            currencies,
        );

        let mut last_error = None;

        for attempt in 1..=EXCHANGE_RATE_MAX_ATTEMPTS {
            eprintln!(
                "Fetching exchange rates from CoinGecko (attempt {}/{})...",
                attempt, EXCHANGE_RATE_MAX_ATTEMPTS
            );

            match self.fetch_rates_once(&url).await {
                Ok(rates) => {
                    eprintln!("Fetched {} exchange rates", rates.len());
                    return Ok(rates);
                }
                Err(FetchRatesError::Retryable(error)) => {
                    last_error = Some(error);

                    if attempt < EXCHANGE_RATE_MAX_ATTEMPTS {
                        let delay = self.retry_delay(attempt);
                        if let Some(error) = &last_error {
                            eprintln!(
                                "Exchange rate fetch attempt {}/{} failed, retrying in {:.1}s: {}",
                                attempt,
                                EXCHANGE_RATE_MAX_ATTEMPTS,
                                delay.as_secs_f32(),
                                error
                            );
                        }
                        sleep(delay).await;
                    } else {
                        break;
                    }
                }
                Err(FetchRatesError::NonRetryable(error)) => {
                    last_error = Some(error);
                    break;
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Exchange rate fetch failed")))
    }

    /// Start background task to refresh exchange rates periodically
    pub fn start_refresh_task(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = interval(EXCHANGE_RATE_REFRESH_INTERVAL);

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

    async fn fetch_rates_once(
        &self,
        url: &str,
    ) -> std::result::Result<HashMap<String, ExchangeRate>, FetchRatesError> {
        let response = self
            .client
            .get(url)
            .header(
                "User-Agent",
                format!("Canary/{} (Bitcoin Wallet)", env!("CARGO_PKG_VERSION")),
            )
            .send()
            .await
            .map_err(|error| {
                Self::classify_reqwest_error(error, "Failed to fetch exchange rates")
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read response body".to_string());
            let message = format!(
                "Exchange rates API returned HTTP {}: {}",
                status.as_u16(),
                body.chars().take(500).collect::<String>()
            );

            if Self::is_retryable_status(status) {
                return Err(FetchRatesError::Retryable(anyhow::anyhow!(message)));
            }

            return Err(FetchRatesError::NonRetryable(anyhow::anyhow!(message)));
        }

        let body = response.text().await.map_err(|error| {
            Self::classify_reqwest_error(error, "Failed to read exchange rates response body")
        })?;

        let data: CoinGeckoResponse = serde_json::from_str(&body)
            .with_context(|| {
                format!(
                    "Failed to parse exchange rates response (HTTP {}): {}",
                    status.as_u16(),
                    body.chars().take(500).collect::<String>()
                )
            })
            .map_err(FetchRatesError::NonRetryable)?;

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

        Ok(rates)
    }

    fn is_retryable_status(status: reqwest::StatusCode) -> bool {
        status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
    }

    fn classify_reqwest_error(error: reqwest::Error, context: &'static str) -> FetchRatesError {
        let is_retryable = error.is_timeout() || error.is_connect() || error.is_request();
        let error = anyhow::Error::new(error).context(context);

        if is_retryable {
            FetchRatesError::Retryable(error)
        } else {
            FetchRatesError::NonRetryable(error)
        }
    }

    fn retry_delay(&self, attempt: u32) -> Duration {
        self.retry_base_delay
            .mul_f64(2_f64.powi((attempt.saturating_sub(1)) as i32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use tempfile::tempdir;

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

    async fn create_test_db() -> (Arc<MetadataDb>, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let test_config = AppConfig::new_for_test(
            NetworkConfig::Regtest,
            Some("tcp://127.0.0.1:50001".to_string()),
            "127.0.0.1:3000".to_string(),
            temp_dir.path().to_string_lossy().to_string(),
            OperatingMode::SelfHosted,
            None,
            None,
        );

        let db = MetadataDb::new(db_path.to_str().unwrap(), &test_config)
            .await
            .unwrap();

        (Arc::new(db), temp_dir)
    }

    #[tokio::test]
    async fn fetch_rates_retries_connect_failures_with_backoff() {
        let (metadata_db, _temp_dir) = create_test_db().await;
        let service = ExchangeRateService::new_for_test(
            metadata_db,
            ExchangeRateService::build_http_client().unwrap(),
            "http://127.0.0.1:9".to_string(),
            Duration::from_millis(25),
        );

        let start = Instant::now();
        let error = service.fetch_rates().await.unwrap_err().to_string();
        let elapsed = start.elapsed();

        assert!(error.contains("Failed to fetch exchange rates"));
        assert!(
            elapsed >= Duration::from_millis(70),
            "expected retries with backoff, got {:?}",
            elapsed
        );
    }

    #[test]
    fn retryable_status_includes_429_and_5xx_only() {
        assert!(ExchangeRateService::is_retryable_status(
            reqwest::StatusCode::TOO_MANY_REQUESTS
        ));
        assert!(ExchangeRateService::is_retryable_status(
            reqwest::StatusCode::BAD_GATEWAY
        ));
        assert!(!ExchangeRateService::is_retryable_status(
            reqwest::StatusCode::BAD_REQUEST
        ));
    }
}
