-- Migration 017: Expand language support from 2 to 9 languages
-- Adds support for: English (en), Norwegian (no), Spanish (es), Portuguese (pt), German (de), French (fr), Japanese (ja), Danish (da), Swedish (sv)

-- SQLite doesn't support ALTER TABLE to modify CHECK constraints
-- We need to recreate the tables with updated constraints

-- Step 1: Recreate contacts table with expanded language constraint
DROP TABLE IF EXISTS contacts_new;
CREATE TABLE contacts_new (
    id TEXT PRIMARY KEY,
    wallet_checksum TEXT NOT NULL,
    name TEXT NOT NULL,
    language TEXT NOT NULL DEFAULT 'en' CHECK (language IN ('en', 'no', 'es', 'pt', 'de', 'fr', 'ja', 'da', 'sv')),
    is_active BOOLEAN DEFAULT true,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (wallet_checksum) REFERENCES wallets(checksum) ON DELETE CASCADE
);

-- Copy existing data
INSERT INTO contacts_new (id, wallet_checksum, name, language, is_active, created_at)
SELECT id, wallet_checksum, name, language, is_active, created_at FROM contacts;

-- Drop old table and rename new one
DROP TABLE contacts;
ALTER TABLE contacts_new RENAME TO contacts;

-- Recreate index on contacts table
CREATE INDEX idx_contacts_wallet_checksum ON contacts(wallet_checksum);

-- Step 2: Recreate pending_contact_verifications table with expanded language constraint
DROP TABLE IF EXISTS pending_contact_verifications_new;
CREATE TABLE pending_contact_verifications_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    wallet_checksum TEXT NOT NULL,
    provider_type TEXT NOT NULL CHECK (provider_type IN ('sms', 'email')),
    notification_target TEXT NOT NULL,
    contact_name TEXT NOT NULL,
    language TEXT NOT NULL DEFAULT 'en' CHECK (language IN ('en', 'no', 'es', 'pt', 'de', 'fr', 'ja', 'da', 'sv')),
    verification_code TEXT,
    expires_at DATETIME NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    verified_at DATETIME DEFAULT NULL,
    FOREIGN KEY (wallet_checksum) REFERENCES wallets(checksum) ON DELETE CASCADE
);

-- Copy existing data
INSERT INTO pending_contact_verifications_new (id, wallet_checksum, provider_type, notification_target, contact_name, language, verification_code, expires_at, created_at, verified_at)
SELECT id, wallet_checksum, provider_type, notification_target, contact_name, language, verification_code, expires_at, created_at, verified_at FROM pending_contact_verifications;

-- Drop old table and rename new one
DROP TABLE pending_contact_verifications;
ALTER TABLE pending_contact_verifications_new RENAME TO pending_contact_verifications;

-- Recreate indexes on pending_contact_verifications table
CREATE INDEX idx_pending_verifications_wallet ON pending_contact_verifications(wallet_checksum);
CREATE INDEX idx_pending_verifications_expires ON pending_contact_verifications(expires_at);
CREATE INDEX idx_pending_verifications_lookup ON pending_contact_verifications(wallet_checksum, notification_target, verified_at);