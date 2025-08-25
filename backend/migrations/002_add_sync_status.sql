-- Add sync_status column to wallets table
-- This migration adds the sync_status field that was introduced after initial deployment
-- SKIP: This column already exists in the current schema, this migration is effectively a no-op

-- Note: This migration exists for historical compatibility but sync_status column
-- is already present in the initial schema. No action needed.