-- Add wallet_checksum column to contact_notification_methods table for direct constraint
-- This prevents duplicate email/phone numbers within the same wallet while allowing
-- the same email/phone to be used across different wallets

-- Check if wallet_checksum column already exists using pragma_table_info
-- If it doesn't exist, we'll recreate the table with the new column
-- This is the safest approach for SQLite migrations

-- Create new table with desired schema including wallet_checksum
CREATE TABLE contact_notification_methods_new (
    id TEXT PRIMARY KEY,
    contact_id TEXT NOT NULL,
    provider_type TEXT NOT NULL CHECK (provider_type IN ('sms', 'ntfy', 'email')),
    notification_target TEXT NOT NULL,
    wallet_checksum TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (contact_id) REFERENCES contacts (id) ON DELETE CASCADE,
    UNIQUE(contact_id, provider_type, notification_target)
);

-- Copy existing data, setting wallet_checksum from contacts table
INSERT INTO contact_notification_methods_new 
SELECT 
    cnm.id, 
    cnm.contact_id, 
    cnm.provider_type, 
    cnm.notification_target,
    c.wallet_checksum,
    cnm.created_at 
FROM contact_notification_methods cnm
JOIN contacts c ON c.id = cnm.contact_id;

-- Drop old table and rename new one
DROP TABLE contact_notification_methods;
ALTER TABLE contact_notification_methods_new RENAME TO contact_notification_methods;

-- Recreate indexes
CREATE INDEX idx_contact_notification_methods_contact_id ON contact_notification_methods (contact_id);
CREATE INDEX idx_contact_notification_methods_provider_type ON contact_notification_methods (provider_type);

-- Create unique constraint for email and SMS notifications within same wallet
-- Note: ntfy is excluded as topics are auto-generated and guaranteed unique
CREATE UNIQUE INDEX idx_unique_wallet_notification_target 
ON contact_notification_methods (wallet_checksum, provider_type, notification_target) 
WHERE provider_type IN ('email', 'sms');