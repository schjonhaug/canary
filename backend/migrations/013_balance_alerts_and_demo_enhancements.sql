-- Migration 013: Balance Alerts Enhancements and Demo Account Support
-- This migration combines multiple features that enhance the balance alerts system
-- and adds support for read-only demo accounts

-- ============================
-- PART 1: FIAT CURRENCY THRESHOLD SUPPORT
-- ============================

-- Add fiat threshold fields to balance_alerts table
-- When threshold_currency is NULL, use threshold_sats (backward compatible)
-- When threshold_currency is set, use threshold_fiat_amount
ALTER TABLE balance_alerts ADD COLUMN threshold_currency TEXT; -- e.g., 'USD', 'EUR', NULL for BTC
ALTER TABLE balance_alerts ADD COLUMN threshold_fiat_amount REAL; -- Fiat amount when currency is set

-- Add fiat threshold snapshot fields to balance_alert_notifications for audit trail
-- These capture the exact threshold configuration and exchange rate at trigger time
ALTER TABLE balance_alert_notifications ADD COLUMN threshold_currency TEXT;
ALTER TABLE balance_alert_notifications ADD COLUMN threshold_fiat_amount REAL;
ALTER TABLE balance_alert_notifications ADD COLUMN exchange_rate_snapshot REAL; -- BTC/fiat rate at trigger time

-- ============================
-- PART 2: THRESHOLD CROSSING DETECTION
-- ============================

-- Add last_checked_balance_sats column to track balance state for crossing detection
ALTER TABLE balance_alerts ADD COLUMN last_checked_balance_sats INTEGER;

-- Initialize with NULL for existing alerts (will be set to current balance on first check)
-- This prevents existing alerts from firing immediately after migration

-- ============================
-- PART 3: DEMO ACCOUNT SUPPORT
-- ============================

-- Add is_demo column to users table for read-only demo accounts
ALTER TABLE users ADD COLUMN is_demo BOOLEAN NOT NULL DEFAULT FALSE;

-- Create index for efficient demo user queries
CREATE INDEX idx_users_is_demo ON users(is_demo);

-- ============================
-- VALIDATION CONSTRAINTS
-- ============================

-- Note: SQLite doesn't support CHECK constraints on ALTER TABLE
-- Validation must be enforced in application code:
-- 1. Exactly one threshold type must be provided (BTC OR fiat, not both or neither)
-- 2. If fiat: threshold_currency must be in SUPPORTED_CURRENCIES
-- 3. If fiat: threshold_fiat_amount must be positive
-- 4. If BTC: threshold_sats must be positive

-- ============================
-- SUMMARY
-- ============================
-- After this migration:
--
-- Balance Alerts Enhancements:
-- 1. Support both BTC and fiat currency thresholds
-- 2. Use real-time exchange rates for fiat comparisons
-- 3. Capture exchange rate snapshots in notification audit trail
-- 4. Track last checked balance for threshold crossing detection
-- 5. Smart crossing detection prevents spam:
--    - "Below" alerts only fire when crossing from above to below
--    - "Above" alerts only fire when crossing from below to above
--    - "Equals" alerts fire when crossing to equals, auto-reactivate when crossing away
-- 6. Backward compatible with existing BTC-only alerts
--
-- Demo Account Support:
-- 1. Users can be marked as demo accounts (is_demo = TRUE)
-- 2. Demo accounts have read-only access (enforced in API layer)
-- 3. Indexed for efficient demo user queries
