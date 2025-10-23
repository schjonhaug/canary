-- Migration 013: Add threshold crossing detection for balance alerts
-- This migration adds balance tracking to enable smart crossing detection

-- Add last_checked_balance_sats column to track balance state for crossing detection
ALTER TABLE balance_alerts ADD COLUMN last_checked_balance_sats INTEGER;

-- Initialize with NULL for existing alerts (will be set to current balance on first check)
-- This prevents existing alerts from firing immediately after migration

-- ============================
-- SUMMARY
-- ============================
-- After this migration:
-- 1. Alerts track the last checked balance to detect threshold crossings
-- 2. "Below" alerts only fire when crossing from above to below threshold
-- 3. "Above" alerts only fire when crossing from below to above threshold
-- 4. "Equals" alerts fire when crossing to equals, auto-reactivate when crossing away
-- 5. Alerts remain active after firing (no manual reactivation needed)
-- 6. Natural spam prevention through crossing detection logic
