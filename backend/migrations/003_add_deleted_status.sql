-- Add support for 'deleted' status in wallet sync_status
-- Since SQLite doesn't support modifying CHECK constraints directly,
-- we need to recreate the table with the updated constraint

-- Create new table with updated constraint
CREATE TABLE wallets_new (
    checksum TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    descriptor TEXT NOT NULL UNIQUE,
    hex_color TEXT NOT NULL,
    balance_total INTEGER DEFAULT 0,
    last_activity DATETIME,
    last_synced_at DATETIME,
    sync_status TEXT DEFAULT 'pending' CHECK (sync_status IN ('pending', 'ready', 'deleted')),
    user_id TEXT NOT NULL,
    is_active BOOLEAN DEFAULT true,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

-- Copy data from old table
INSERT INTO wallets_new SELECT * FROM wallets;

-- Drop old table
DROP TABLE wallets;

-- Rename new table
ALTER TABLE wallets_new RENAME TO wallets;

-- Recreate indexes
CREATE INDEX idx_wallets_user ON wallets(user_id);