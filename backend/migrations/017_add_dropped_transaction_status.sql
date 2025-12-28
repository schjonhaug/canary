-- Migration 017: Add 'dropped' transaction status
-- This migration adds support for detecting transactions that have been dropped from the mempool
-- (due to low fees, node restarts, or exceeding mempool expiry time)

-- Note: SQLite doesn't support altering CHECK constraints, so we recreate the table

-- Create new table with updated constraint and dropped_at column
CREATE TABLE transactions_new (
    txid TEXT NOT NULL,
    wallet_checksum TEXT NOT NULL,
    transaction_type TEXT NOT NULL CHECK (transaction_type IN ('send', 'receive')),
    amount_sats INTEGER NOT NULL,
    fee_sats INTEGER,
    block_height INTEGER,
    first_seen_at INTEGER NOT NULL,
    confirmed_at INTEGER,
    parent_txid TEXT,
    transaction_status TEXT DEFAULT 'pending' CHECK (transaction_status IN ('pending', 'confirmed', 'replaced', 'dropped')),
    replaced_by_txid TEXT,
    replaced_at INTEGER,
    dropped_at INTEGER, -- Unix timestamp when transaction was detected as dropped from mempool
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (txid, wallet_checksum),
    FOREIGN KEY (wallet_checksum) REFERENCES wallets (checksum) ON DELETE CASCADE
);

-- Copy existing data (dropped_at will be NULL for existing transactions)
INSERT INTO transactions_new (
    txid, wallet_checksum, transaction_type, amount_sats, fee_sats, block_height,
    first_seen_at, confirmed_at, parent_txid, transaction_status, replaced_by_txid,
    replaced_at, created_at
)
SELECT
    txid, wallet_checksum, transaction_type, amount_sats, fee_sats, block_height,
    first_seen_at, confirmed_at, parent_txid, transaction_status, replaced_by_txid,
    replaced_at, created_at
FROM transactions;

-- Drop old table and rename new one
DROP TABLE transactions;
ALTER TABLE transactions_new RENAME TO transactions;

-- Recreate indexes (matching the indexes from migration 008)
CREATE INDEX idx_transactions_wallet_checksum ON transactions(wallet_checksum);
CREATE INDEX idx_transactions_block_height ON transactions(block_height);
CREATE INDEX idx_transactions_first_seen_at ON transactions(first_seen_at);
CREATE INDEX idx_transactions_status ON transactions(transaction_status);
CREATE INDEX idx_transactions_replaced_by ON transactions(replaced_by_txid);
CREATE INDEX idx_transactions_parent_txid ON transactions(parent_txid);
