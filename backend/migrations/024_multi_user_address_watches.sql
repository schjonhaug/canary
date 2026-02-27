-- Support multi-user address watches by replacing global UNIQUE on descriptor
-- with conditional unique indexes:
--   - Descriptor wallets: globally unique (one BDK wallet per descriptor)
--   - Address watches: unique per user (same user can't watch same address twice,
--     but different users CAN watch the same address independently)

-- Disable foreign keys during table rebuild to prevent CASCADE deletes on DROP TABLE
PRAGMA foreign_keys = OFF;

-- Create new table without global UNIQUE on descriptor
CREATE TABLE wallets_new (
    checksum TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    descriptor TEXT NOT NULL,
    hex_color TEXT NOT NULL,
    balance_total INTEGER DEFAULT 0,
    last_activity DATETIME,
    last_synced_at DATETIME,
    status TEXT DEFAULT 'pending' CHECK (status IN ('pending', 'ready', 'deleted')),
    user_id TEXT NOT NULL,
    is_active BOOLEAN DEFAULT true,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    wallet_type TEXT NOT NULL DEFAULT 'descriptor' CHECK (wallet_type IN ('descriptor', 'address')),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

-- Copy data from old table
INSERT INTO wallets_new (checksum, name, descriptor, hex_color, balance_total, last_activity, last_synced_at, status, user_id, is_active, created_at, wallet_type)
SELECT checksum, name, descriptor, hex_color, balance_total, last_activity, last_synced_at, status, user_id, is_active, created_at, wallet_type
FROM wallets;

-- Drop old table
DROP TABLE wallets;

-- Rename new table
ALTER TABLE wallets_new RENAME TO wallets;

PRAGMA foreign_keys = ON;

-- Recreate base index
CREATE INDEX idx_wallets_user ON wallets(user_id);

-- Descriptor wallets must be globally unique (one BDK wallet per descriptor)
CREATE UNIQUE INDEX idx_wallets_descriptor_unique
    ON wallets(descriptor)
    WHERE wallet_type = 'descriptor';

-- Address watches must be unique per user (same user can't watch same address twice)
CREATE UNIQUE INDEX idx_wallets_address_user_unique
    ON wallets(descriptor, user_id)
    WHERE wallet_type = 'address';
