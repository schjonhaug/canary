-- Migration 011: Add balance_alert notification type
-- This migration extends the notification_logs table to support balance alert notifications

-- Drop the existing CHECK constraint that only allows 'pending' and 'confirmed'
-- Note: SQLite doesn't support dropping constraints directly, so we need to recreate the table

-- Create new table with updated constraint (matching current schema from migration 008)
CREATE TABLE notification_logs_new (
    id TEXT PRIMARY KEY,
    transaction_txid TEXT NOT NULL,
    transaction_wallet_checksum TEXT NOT NULL,
    notification_method_id TEXT,
    provider_name TEXT NOT NULL,
    provider_message_id TEXT,
    status TEXT NOT NULL CHECK (status IN ('sent', 'failed', 'delivered')),
    error_message TEXT,
    message_content TEXT NOT NULL,
    notification_type TEXT NOT NULL DEFAULT 'pending' CHECK (notification_type IN ('pending', 'confirmed', 'balance_alert')),
    contact_name_snapshot TEXT,
    notification_target_snapshot TEXT,
    provider_type_snapshot TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (transaction_txid, transaction_wallet_checksum) REFERENCES transactions (txid, wallet_checksum) ON DELETE CASCADE,
    FOREIGN KEY (notification_method_id) REFERENCES contact_notification_methods (id) ON DELETE SET NULL
);

-- Copy existing data
INSERT INTO notification_logs_new
SELECT * FROM notification_logs;

-- Drop old table and rename new one
DROP TABLE notification_logs;
ALTER TABLE notification_logs_new RENAME TO notification_logs;

-- Recreate indexes (matching the indexes from migration 008)
CREATE INDEX idx_notification_logs_transaction ON notification_logs(transaction_txid, transaction_wallet_checksum);
CREATE INDEX idx_notification_logs_method ON notification_logs(notification_method_id);
CREATE INDEX idx_notification_logs_created_at ON notification_logs(created_at);