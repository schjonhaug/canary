-- Add support for explicit initial wallet creation/sync failures.
-- SQLite cannot alter CHECK constraints in place, so rebuild the wallets table
-- while preserving the current post-024 descriptor uniqueness rules.

PRAGMA foreign_keys = OFF;

BEGIN IMMEDIATE TRANSACTION;

CREATE TABLE wallets_new (
    checksum TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    descriptor TEXT NOT NULL,
    hex_color TEXT NOT NULL,
    balance_total INTEGER DEFAULT 0,
    last_activity DATETIME,
    last_synced_at DATETIME,
    status TEXT DEFAULT 'pending' CHECK (status IN ('pending', 'ready', 'failed', 'deleted')),
    user_id TEXT NOT NULL,
    is_active BOOLEAN DEFAULT true,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    wallet_type TEXT NOT NULL DEFAULT 'descriptor' CHECK (wallet_type IN ('descriptor', 'address')),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

INSERT INTO wallets_new (checksum, name, descriptor, hex_color, balance_total, last_activity, last_synced_at, status, user_id, is_active, created_at, wallet_type)
SELECT checksum, name, descriptor, hex_color, balance_total, last_activity, last_synced_at, status, user_id, is_active, created_at, wallet_type
FROM wallets;

DROP TABLE wallets;

ALTER TABLE wallets_new RENAME TO wallets;

CREATE INDEX idx_wallets_user ON wallets(user_id);

CREATE UNIQUE INDEX idx_wallets_descriptor_unique
    ON wallets(descriptor)
    WHERE wallet_type = 'descriptor';

CREATE UNIQUE INDEX idx_wallets_address_user_unique
    ON wallets(descriptor, user_id)
    WHERE wallet_type = 'address';

COMMIT;

PRAGMA foreign_keys = ON;
