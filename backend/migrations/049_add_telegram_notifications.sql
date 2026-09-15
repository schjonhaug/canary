-- Migration 049: Telegram Bot notification provider support

PRAGMA foreign_keys = OFF;

BEGIN TRANSACTION;

DROP TABLE IF EXISTS contact_notification_methods_new;
CREATE TABLE contact_notification_methods_new (
    id TEXT PRIMARY KEY,
    contact_id TEXT NOT NULL,
    provider_type TEXT NOT NULL CHECK (provider_type IN ('sms', 'ntfy', 'email', 'nostr', 'webhook', 'telegram')),
    notification_target TEXT NOT NULL,
    wallet_checksum TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    is_enabled BOOLEAN NOT NULL DEFAULT 1,
    content_privacy_level TEXT NOT NULL DEFAULT 'detailed'
        CHECK (content_privacy_level IN ('minimal', 'standard', 'detailed')),
    content_wallet_name BOOLEAN NOT NULL DEFAULT 1,
    content_event_type BOOLEAN NOT NULL DEFAULT 1,
    content_transaction_amount BOOLEAN NOT NULL DEFAULT 0,
    content_transaction_balance BOOLEAN NOT NULL DEFAULT 0,
    content_balance_alert_condition BOOLEAN NOT NULL DEFAULT 0,
    content_balance_alert_threshold BOOLEAN NOT NULL DEFAULT 0,
    content_balance_alert_balance BOOLEAN NOT NULL DEFAULT 0,
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
    is_enabled,
    content_privacy_level,
    content_wallet_name,
    content_event_type,
    content_transaction_amount,
    content_transaction_balance,
    content_balance_alert_condition,
    content_balance_alert_threshold,
    content_balance_alert_balance
)
SELECT
    id,
    contact_id,
    provider_type,
    notification_target,
    wallet_checksum,
    created_at,
    is_enabled,
    content_privacy_level,
    content_wallet_name,
    content_event_type,
    content_transaction_amount,
    content_transaction_balance,
    content_balance_alert_condition,
    content_balance_alert_threshold,
    content_balance_alert_balance
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

COMMIT;

PRAGMA foreign_keys = ON;
