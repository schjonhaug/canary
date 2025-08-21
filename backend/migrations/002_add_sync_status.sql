-- Add sync_status column to wallets table
-- This migration adds the sync_status field that was introduced after initial deployment

ALTER TABLE wallets ADD COLUMN sync_status TEXT DEFAULT 'pending' CHECK (sync_status IN ('pending', 'ready'));

-- Update existing wallets to 'ready' status since they were already synced
-- Note: This UPDATE is for databases that already had wallets when this migration runs
UPDATE wallets SET sync_status = 'ready' WHERE sync_status = 'pending' AND last_synced_at IS NOT NULL;