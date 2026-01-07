-- Migration 018: Remove language column from contacts and pending_contact_verifications
-- Language is now determined by the user's preferred_language setting (in users table)
-- instead of being stored per-contact

-- SQLite doesn't support ALTER TABLE DROP COLUMN in older versions
-- We need to recreate the tables without the language column

-- IMPORTANT: Disable foreign keys to prevent CASCADE DELETE when dropping contacts table
-- (contact_notification_methods has ON DELETE CASCADE referencing contacts)
PRAGMA foreign_keys = OFF;

-- Step 1: Recreate contacts table without language column
DROP TABLE IF EXISTS contacts_new;
CREATE TABLE contacts_new (
    id TEXT PRIMARY KEY,
    wallet_checksum TEXT NOT NULL,
    name TEXT NOT NULL,
    is_active BOOLEAN DEFAULT true,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (wallet_checksum) REFERENCES wallets(checksum) ON DELETE CASCADE
);

-- Copy existing data (excluding language column)
INSERT INTO contacts_new (id, wallet_checksum, name, is_active, created_at)
SELECT id, wallet_checksum, name, is_active, created_at FROM contacts;

-- Drop old table and rename new one
DROP TABLE contacts;
ALTER TABLE contacts_new RENAME TO contacts;

-- Recreate index on contacts table
CREATE INDEX idx_contacts_wallet_checksum ON contacts(wallet_checksum);

-- Step 2: Recreate pending_contact_verifications table without language column
DROP TABLE IF EXISTS pending_contact_verifications_new;
CREATE TABLE pending_contact_verifications_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    wallet_checksum TEXT NOT NULL,
    provider_type TEXT NOT NULL CHECK (provider_type IN ('sms', 'email')),
    notification_target TEXT NOT NULL,
    contact_name TEXT NOT NULL,
    verification_code TEXT,
    expires_at DATETIME NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    verified_at DATETIME DEFAULT NULL,
    FOREIGN KEY (wallet_checksum) REFERENCES wallets(checksum) ON DELETE CASCADE
);

-- Copy existing data (excluding language column)
INSERT INTO pending_contact_verifications_new (id, wallet_checksum, provider_type, notification_target, contact_name, verification_code, expires_at, created_at, verified_at)
SELECT id, wallet_checksum, provider_type, notification_target, contact_name, verification_code, expires_at, created_at, verified_at FROM pending_contact_verifications;

-- Drop old table and rename new one
DROP TABLE pending_contact_verifications;
ALTER TABLE pending_contact_verifications_new RENAME TO pending_contact_verifications;

-- Recreate indexes on pending_contact_verifications table
CREATE INDEX idx_pending_verifications_wallet ON pending_contact_verifications(wallet_checksum);
CREATE INDEX idx_pending_verifications_expires ON pending_contact_verifications(expires_at);
CREATE INDEX idx_pending_verifications_lookup ON pending_contact_verifications(wallet_checksum, notification_target, verified_at);

-- Re-enable foreign keys
PRAGMA foreign_keys = ON;
