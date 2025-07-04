-- Add balance_total column to wallets table
-- This stores the current total balance of each wallet

ALTER TABLE wallets ADD COLUMN balance_total INTEGER DEFAULT 0;