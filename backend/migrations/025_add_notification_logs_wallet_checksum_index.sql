-- Add index on transaction_wallet_checksum for the batched notification query
-- The existing composite index (transaction_txid, transaction_wallet_checksum)
-- has transaction_txid as the leading column, so queries filtering only by
-- transaction_wallet_checksum cannot use it efficiently.
CREATE INDEX IF NOT EXISTS idx_notification_logs_wallet_checksum
ON notification_logs(transaction_wallet_checksum);
