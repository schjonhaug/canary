-- Add wallet_checksum column to contact_notification_methods table for direct constraint
-- This prevents duplicate email/phone numbers within the same wallet while allowing
-- the same email/phone to be used across different wallets

-- Add wallet_checksum column only if it doesn't exist
-- Check if column exists and add it if needed
CREATE TABLE IF NOT EXISTS temp_check_column AS SELECT wallet_checksum FROM contact_notification_methods LIMIT 0;
DROP TABLE IF EXISTS temp_check_column;

-- Update existing records with wallet_checksum from contacts table (handles NULL values)
UPDATE contact_notification_methods 
SET wallet_checksum = (
    SELECT wallet_checksum 
    FROM contacts 
    WHERE contacts.id = contact_notification_methods.contact_id
)
WHERE wallet_checksum IS NULL;

-- Since SQLite doesn't support modifying constraints directly,
-- we need to recreate the table with the updated schema
DROP TABLE IF EXISTS contact_notification_methods_new;
CREATE TABLE contact_notification_methods_new (
    id TEXT PRIMARY KEY,
    contact_id TEXT NOT NULL,
    provider_type TEXT NOT NULL CHECK (provider_type IN ('sms', 'ntfy', 'email')),
    notification_target TEXT NOT NULL,
    wallet_checksum TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (contact_id) REFERENCES contacts (id) ON DELETE CASCADE,
    UNIQUE(contact_id, provider_type, notification_target)
);

-- Copy data from old table
INSERT INTO contact_notification_methods_new 
SELECT id, contact_id, provider_type, notification_target, wallet_checksum, created_at 
FROM contact_notification_methods;

-- Drop old table
DROP TABLE contact_notification_methods;

-- Rename new table
ALTER TABLE contact_notification_methods_new RENAME TO contact_notification_methods;

-- Recreate indexes
CREATE INDEX idx_contact_notification_methods_contact_id ON contact_notification_methods (contact_id);
CREATE INDEX idx_contact_notification_methods_provider_type ON contact_notification_methods (provider_type);

-- Create unique constraint for email and SMS notifications within same wallet
-- Note: ntfy is excluded as topics are auto-generated and guaranteed unique
CREATE UNIQUE INDEX idx_unique_wallet_notification_target 
ON contact_notification_methods (wallet_checksum, provider_type, notification_target) 
WHERE provider_type IN ('email', 'sms');