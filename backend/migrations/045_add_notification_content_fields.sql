-- Per-delivery content controls. Defaults match the previous Standard level
-- for methods created by older application binaries.
ALTER TABLE contact_notification_methods
ADD COLUMN content_wallet_name BOOLEAN NOT NULL DEFAULT 1;

ALTER TABLE contact_notification_methods
ADD COLUMN content_event_type BOOLEAN NOT NULL DEFAULT 1;

ALTER TABLE contact_notification_methods
ADD COLUMN content_transaction_amount BOOLEAN NOT NULL DEFAULT 0;

ALTER TABLE contact_notification_methods
ADD COLUMN content_transaction_balance BOOLEAN NOT NULL DEFAULT 0;

ALTER TABLE contact_notification_methods
ADD COLUMN content_balance_alert_condition BOOLEAN NOT NULL DEFAULT 0;

ALTER TABLE contact_notification_methods
ADD COLUMN content_balance_alert_threshold BOOLEAN NOT NULL DEFAULT 0;

ALTER TABLE contact_notification_methods
ADD COLUMN content_balance_alert_balance BOOLEAN NOT NULL DEFAULT 0;

-- Translate methods written by v1.5.2 and the migration-044 release candidate.
-- Detailed transaction-balance disclosure followed the contact-wide setting.
UPDATE contact_notification_methods
SET content_wallet_name = CASE WHEN content_privacy_level = 'minimal' THEN 0 ELSE 1 END,
    content_event_type = CASE WHEN content_privacy_level = 'minimal' THEN 0 ELSE 1 END,
    content_transaction_amount = CASE WHEN content_privacy_level = 'detailed' THEN 1 ELSE 0 END,
    content_transaction_balance = CASE
        WHEN content_privacy_level = 'detailed'
        THEN COALESCE((
            SELECT include_wallet_balance_in_tx_notifications
            FROM contacts
            WHERE contacts.id = contact_notification_methods.contact_id
        ), 0)
        ELSE 0
    END,
    content_balance_alert_condition = CASE WHEN content_privacy_level = 'detailed' THEN 1 ELSE 0 END,
    content_balance_alert_threshold = CASE WHEN content_privacy_level = 'detailed' THEN 1 ELSE 0 END,
    content_balance_alert_balance = CASE WHEN content_privacy_level = 'detailed' THEN 1 ELSE 0 END;
