-- Migration 009: Remove balance_after column from transactions
-- This migration removes the balance_after field to simplify sync logic
-- Balance calculations will be done on-the-fly in the frontend

-- SQLite doesn't support DROP COLUMN, so we need to recreate the table
-- First, preserve existing data

-- Rename current table
ALTER TABLE transactions RENAME TO transactions_old;

-- Create new transactions table without balance_after
CREATE TABLE transactions (
    txid TEXT NOT NULL,
    wallet_checksum TEXT NOT NULL,
    transaction_type TEXT NOT NULL CHECK (transaction_type IN ('send', 'receive')),
    amount_sats INTEGER NOT NULL,
    fee_sats INTEGER,
    block_height INTEGER,
    first_seen_at INTEGER NOT NULL,
    confirmed_at INTEGER,
    is_rbf BOOLEAN DEFAULT FALSE,
    is_cpfp BOOLEAN DEFAULT FALSE,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (txid, wallet_checksum),
    FOREIGN KEY (wallet_checksum) REFERENCES wallets (checksum) ON DELETE CASCADE
);

-- Copy data from old table (excluding balance_after)
INSERT INTO transactions (
    txid, wallet_checksum, transaction_type, amount_sats, fee_sats,
    block_height, first_seen_at, confirmed_at, is_rbf, is_cpfp, created_at
)
SELECT 
    txid, wallet_checksum, transaction_type, amount_sats, fee_sats,
    block_height, first_seen_at, confirmed_at, is_rbf, is_cpfp, created_at
FROM transactions_old;

-- Drop the old table
DROP TABLE transactions_old;

-- Recreate indexes
CREATE INDEX idx_transactions_wallet_checksum ON transactions(wallet_checksum);
CREATE INDEX idx_transactions_block_height ON transactions(block_height);
CREATE INDEX idx_transactions_first_seen_at ON transactions(first_seen_at);