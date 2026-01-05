-- Migration 018: Add 'dropped' notification type
-- This allows logging notifications for transactions dropped from the mempool

-- SQLite doesn't support ALTER TABLE to modify CHECK constraints,
-- so we need to recreate the table

-- Drop temporary table if it exists (cleanup from failed migrations)
DROP TABLE IF EXISTS notification_logs_new;

-- Create new table with updated constraint and preserved column names/types
CREATE TABLE notification_logs_new (
    id TEXT PRIMARY KEY,
    transaction_txid TEXT, -- Nullable to support balance alerts if needed, but primarily for transactions
    transaction_wallet_checksum TEXT NOT NULL,
    notification_method_id TEXT,
    provider_name TEXT NOT NULL,
    provider_message_id TEXT,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'sent', 'failed')),
    error_message TEXT,
    message_content TEXT,
    notification_type TEXT NOT NULL DEFAULT 'pending' CHECK (notification_type IN ('pending', 'confirmed', 'balance_alert', 'dropped')),
    contact_name_snapshot TEXT,
    notification_target_snapshot TEXT,
    provider_type_snapshot TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    balance_alert_id TEXT, -- Reference to balance_alerts.id for balance alert notifications
    FOREIGN KEY (transaction_txid, transaction_wallet_checksum) REFERENCES transactions (txid, wallet_checksum) ON DELETE CASCADE,
    FOREIGN KEY (transaction_wallet_checksum) REFERENCES wallets(checksum) ON DELETE CASCADE,
    FOREIGN KEY (notification_method_id) REFERENCES contact_notification_methods (id) ON DELETE SET NULL
);

-- Copy existing data
INSERT INTO notification_logs_new (
    id, transaction_txid, transaction_wallet_checksum, notification_method_id, provider_name,
    provider_message_id, status, error_message, message_content,
    notification_type, contact_name_snapshot, notification_target_snapshot,
    provider_type_snapshot, created_at, balance_alert_id
)
SELECT
    id, transaction_txid, transaction_wallet_checksum, notification_method_id, provider_name,
    provider_message_id, status, error_message, message_content,
    notification_type, contact_name_snapshot, notification_target_snapshot,
    provider_type_snapshot, created_at, NULL
FROM notification_logs;

-- Drop old table and rename new one
DROP TABLE notification_logs;
ALTER TABLE notification_logs_new RENAME TO notification_logs;

-- Recreate indexes
CREATE INDEX IF NOT EXISTS idx_notification_logs_transaction ON notification_logs(transaction_txid, transaction_wallet_checksum);
CREATE INDEX IF NOT EXISTS idx_notification_logs_method ON notification_logs(notification_method_id);
CREATE INDEX IF NOT EXISTS idx_notification_logs_created_at ON notification_logs(created_at);
CREATE INDEX IF NOT EXISTS idx_notification_logs_balance_alert ON notification_logs(balance_alert_id);
