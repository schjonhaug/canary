-- Add last_activity column to wallets table
-- This tracks when the wallet balance was last updated

ALTER TABLE wallets ADD COLUMN last_activity DATETIME;

-- Set initial last_activity to created_at for existing wallets
UPDATE wallets SET last_activity = created_at WHERE last_activity IS NULL;