-- Add user preferences for fiat currency display
-- Default to USD for existing users, new users will get smart defaults based on browser locale
ALTER TABLE users ADD COLUMN preferred_fiat_currency TEXT DEFAULT 'USD';

-- Create table for caching exchange rates
-- Supports all CoinGecko fiat currencies (46 total)
CREATE TABLE IF NOT EXISTS exchange_rates (
    currency TEXT PRIMARY KEY,
    rate_per_btc REAL NOT NULL,
    last_updated DATETIME NOT NULL
);

-- Create index for efficient cache expiry checks
CREATE INDEX IF NOT EXISTS idx_exchange_rates_last_updated ON exchange_rates(last_updated);