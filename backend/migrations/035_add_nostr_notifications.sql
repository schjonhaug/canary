-- Migration 035: Nostr notification provider support

PRAGMA foreign_keys = OFF;

BEGIN TRANSACTION;

DROP TABLE IF EXISTS contact_notification_methods_new;
CREATE TABLE contact_notification_methods_new (
    id TEXT PRIMARY KEY,
    contact_id TEXT NOT NULL,
    provider_type TEXT NOT NULL CHECK (provider_type IN ('sms', 'ntfy', 'email', 'nostr')),
    notification_target TEXT NOT NULL,
    wallet_checksum TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    is_enabled BOOLEAN NOT NULL DEFAULT 1,
    FOREIGN KEY (contact_id) REFERENCES contacts (id) ON DELETE CASCADE,
    UNIQUE(contact_id, provider_type, notification_target)
);

INSERT INTO contact_notification_methods_new (
    id,
    contact_id,
    provider_type,
    notification_target,
    wallet_checksum,
    created_at,
    is_enabled
)
SELECT
    id,
    contact_id,
    provider_type,
    notification_target,
    wallet_checksum,
    created_at,
    is_enabled
FROM contact_notification_methods;

DROP TABLE contact_notification_methods;
ALTER TABLE contact_notification_methods_new RENAME TO contact_notification_methods;

CREATE INDEX IF NOT EXISTS idx_contact_notification_methods_contact_id ON contact_notification_methods (contact_id);
CREATE INDEX IF NOT EXISTS idx_contact_notification_methods_provider_type ON contact_notification_methods (provider_type);
CREATE INDEX IF NOT EXISTS idx_contact_notification_methods_wallet_provider_target
    ON contact_notification_methods (wallet_checksum, provider_type, notification_target);

CREATE UNIQUE INDEX IF NOT EXISTS idx_unique_wallet_notification_target
    ON contact_notification_methods (wallet_checksum, provider_type, notification_target)
    WHERE provider_type IN ('email', 'sms', 'nostr');

CREATE TABLE IF NOT EXISTS instance_secrets (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

COMMIT;

PRAGMA foreign_keys = ON;
