-- Rename sync_status column to status for better clarity
-- The field represents the wallet's overall status (pending, ready, deleted)
-- not specifically sync status since syncing happens periodically

ALTER TABLE wallets RENAME COLUMN sync_status TO status;