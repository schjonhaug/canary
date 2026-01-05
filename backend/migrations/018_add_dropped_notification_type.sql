-- Migration 018: Add 'dropped' notification type
-- This allows logging notifications for transactions dropped from the mempool

-- SQLite doesn't support ALTER TABLE to modify CHECK constraints,
-- so we need to recreate the table

-- Drop temporary table if it exists (cleanup from failed migrations)
DROP TABLE IF EXISTS notification_logs_new;

-- Create new table with updated constraint
CREATE TABLE notification_logs_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    txid TEXT, -- Can be NULL for balance alert notifications
    wallet_checksum TEXT NOT NULL,
    notification_method_id INTEGER NOT NULL,
    provider TEXT NOT NULL,
    provider_message_id TEXT, -- Provider-specific message ID for delivery tracking
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'sent', 'failed')),
    error_message TEXT,
    message_content TEXT, -- Snapshot of actual message sent
    notification_type TEXT NOT NULL DEFAULT 'pending' CHECK (notification_type IN ('pending', 'confirmed', 'balance_alert', 'dropped')),
    contact_name_snapshot TEXT,
    notification_target_snapshot TEXT,
    provider_type_snapshot TEXT,
    sent_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    balance_alert_id TEXT, -- Reference to balance_alerts.id for balance alert notifications
    FOREIGN KEY (wallet_checksum) REFERENCES wallets(checksum) ON DELETE CASCADE
);

-- Copy existing data
INSERT INTO notification_logs_new (
    txid, wallet_checksum, notification_method_id, provider,
    provider_message_id, status, error_message, message_content,
    notification_type, contact_name_snapshot, notification_target_snapshot,
    provider_type_snapshot, sent_at, balance_alert_id
)
SELECT
    transaction_txid,            -- Map transaction_txid -> txid
    transaction_wallet_checksum, -- Map transaction_wallet_checksum -> wallet_checksum
    notification_method_id,      -- Keep as is
    provider_name,               -- Map provider_name -> provider
    provider_message_id,
    status, 
    error_message, 
    message_content,
    notification_type, 
    contact_name_snapshot,
    notification_target_snapshot,
    provider_type_snapshot,
    created_at,                  -- Map created_at -> sent_at
    NULL                         -- New column balance_alert_id
FROM notification_logs;

-- Drop old table and rename new one
DROP TABLE notification_logs;
ALTER TABLE notification_logs_new RENAME TO notification_logs;

-- Recreate indexes
CREATE INDEX IF NOT EXISTS idx_notification_logs_txid ON notification_logs(txid);
CREATE INDEX IF NOT EXISTS idx_notification_logs_wallet ON notification_logs(wallet_checksum);
CREATE INDEX IF NOT EXISTS idx_notification_logs_balance_alert ON notification_logs(balance_alert_id);
