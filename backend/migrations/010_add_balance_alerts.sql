-- Migration 010: Add Balance Alerts System
-- This migration adds balance-based notification alerts for wallets

-- ============================
-- BALANCE ALERTS TABLE
-- ============================

-- Create balance_alerts table for user-configured balance thresholds
CREATE TABLE balance_alerts (
    id TEXT PRIMARY KEY, -- UUIDv4
    wallet_checksum TEXT NOT NULL,
    threshold_sats INTEGER NOT NULL, -- Balance threshold in satoshis
    alert_type TEXT NOT NULL CHECK (alert_type IN ('above', 'below', 'equals')),
    is_active BOOLEAN NOT NULL DEFAULT 1, -- Auto-disabled after firing, requires manual reactivation
    last_triggered_at INTEGER, -- Unix timestamp when alert was last triggered
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (wallet_checksum) REFERENCES wallets (checksum) ON DELETE CASCADE
);

-- Create indexes for efficient queries during sync
CREATE INDEX idx_balance_alerts_wallet_checksum ON balance_alerts(wallet_checksum);
CREATE INDEX idx_balance_alerts_active ON balance_alerts(is_active);
CREATE INDEX idx_balance_alerts_wallet_active ON balance_alerts(wallet_checksum, is_active);

-- ============================
-- BALANCE ALERT NOTIFICATIONS TABLE
-- ============================

-- Create balance_alert_notifications table for audit trail
-- Links balance alerts to the existing notification system
CREATE TABLE balance_alert_notifications (
    id TEXT PRIMARY KEY, -- UUIDv4
    balance_alert_id TEXT NOT NULL,
    wallet_checksum TEXT NOT NULL,
    threshold_sats INTEGER NOT NULL, -- Snapshot of threshold when triggered
    current_balance_sats INTEGER NOT NULL, -- Wallet balance when alert fired
    alert_type TEXT NOT NULL, -- Snapshot of alert type when triggered
    notification_sent_at INTEGER NOT NULL, -- Unix timestamp when notification was sent
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (balance_alert_id) REFERENCES balance_alerts (id) ON DELETE CASCADE,
    FOREIGN KEY (wallet_checksum) REFERENCES wallets (checksum) ON DELETE CASCADE
);

-- Create indexes for efficient audit queries
CREATE INDEX idx_balance_alert_notifications_alert_id ON balance_alert_notifications(balance_alert_id);
CREATE INDEX idx_balance_alert_notifications_wallet ON balance_alert_notifications(wallet_checksum);
CREATE INDEX idx_balance_alert_notifications_sent_at ON balance_alert_notifications(notification_sent_at);

-- ============================
-- SUMMARY
-- ============================
-- After this migration:
-- 1. Users can create balance alerts for their wallets
-- 2. Alerts automatically disable after firing (is_active = false)
-- 3. Users must manually reactivate alerts after they fire
-- 4. Complete audit trail of all balance alert notifications
-- 5. Efficient indexing for sync performance (only checked when changes=true)
-- 6. Supports above/below/equals threshold types
-- 7. Integrates with existing notification system for delivery