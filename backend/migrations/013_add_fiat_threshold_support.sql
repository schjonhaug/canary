-- Migration 013: Add Fiat Currency Threshold Support for Balance Alerts
-- This migration extends the balance alerts system to support fiat currency thresholds
-- in addition to the existing Bitcoin (satoshi) thresholds

-- ============================
-- BALANCE ALERTS TABLE UPDATES
-- ============================

-- Add fiat threshold fields to balance_alerts table
-- When threshold_currency is NULL, use threshold_sats (backward compatible)
-- When threshold_currency is set, use threshold_fiat_amount
ALTER TABLE balance_alerts ADD COLUMN threshold_currency TEXT; -- e.g., 'USD', 'EUR', NULL for BTC
ALTER TABLE balance_alerts ADD COLUMN threshold_fiat_amount REAL; -- Fiat amount when currency is set

-- ============================
-- BALANCE ALERT NOTIFICATIONS TABLE UPDATES
-- ============================

-- Add fiat threshold snapshot fields for audit trail
-- These capture the exact threshold configuration and exchange rate at trigger time
ALTER TABLE balance_alert_notifications ADD COLUMN threshold_currency TEXT;
ALTER TABLE balance_alert_notifications ADD COLUMN threshold_fiat_amount REAL;
ALTER TABLE balance_alert_notifications ADD COLUMN exchange_rate_snapshot REAL; -- BTC/fiat rate at trigger time

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
-- BACKWARD COMPATIBILITY
-- ============================

-- Existing alerts remain unchanged:
-- - threshold_currency = NULL
-- - threshold_fiat_amount = NULL
-- - Alert checking logic prioritizes threshold_sats when currency is NULL

-- ============================
-- SUMMARY
-- ============================
-- After this migration:
-- 1. Balance alerts support both BTC and fiat currency thresholds
-- 2. Fiat thresholds use real-time exchange rates for comparison
-- 3. Exchange rate snapshots captured in notification audit trail
-- 4. Backward compatible with existing BTC-only alerts
-- 5. Application code enforces validation (one threshold type per alert)
-- 6. Users can set alerts like "$1,000" or "€5,000" instead of calculating BTC amounts
