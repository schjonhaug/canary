-- Migration 031: Per-contact notification settings
-- Moves notification configuration toward a per-contact model while keeping
-- existing wallet-level rows available for historical audit references.

BEGIN TRANSACTION;

ALTER TABLE contacts ADD COLUMN notify_sending BOOLEAN NOT NULL DEFAULT 1;
ALTER TABLE contacts ADD COLUMN notify_sent BOOLEAN NOT NULL DEFAULT 1;
ALTER TABLE contacts ADD COLUMN notify_receiving BOOLEAN NOT NULL DEFAULT 1;
ALTER TABLE contacts ADD COLUMN notify_received BOOLEAN NOT NULL DEFAULT 1;
ALTER TABLE contacts ADD COLUMN notify_cpfp BOOLEAN NOT NULL DEFAULT 1;
ALTER TABLE contacts ADD COLUMN notify_rbf BOOLEAN NOT NULL DEFAULT 1;
ALTER TABLE contacts ADD COLUMN include_wallet_balance_in_tx_notifications BOOLEAN NOT NULL DEFAULT 0;

-- v1.5.2 had no separate RBF event toggle: replacement activity was covered by
-- the ordinary pending/confirmed notifications. Disable the newly introduced
-- RBF-specific delivery for contacts that already exist during this migration
-- so upgrades do not gain an extra message. Contacts created after migration
-- continue to use the column default above.
UPDATE contacts SET notify_rbf = 0;

ALTER TABLE contact_notification_methods ADD COLUMN is_enabled BOOLEAN NOT NULL DEFAULT 1;

ALTER TABLE balance_alerts ADD COLUMN contact_id TEXT REFERENCES contacts(id) ON DELETE CASCADE;
ALTER TABLE balance_alert_notifications ADD COLUMN contact_id TEXT REFERENCES contacts(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_balance_alerts_contact_id ON balance_alerts(contact_id);
CREATE INDEX IF NOT EXISTS idx_balance_alerts_wallet_contact_active ON balance_alerts(wallet_checksum, contact_id, is_active);

-- Fan out existing wallet-level active alerts to each current contact so every
-- contact keeps receiving the same balance notifications after settings become
-- contact-specific. Wallets with no contacts keep their wallet-level alerts
-- active and visible until the user deletes or recreates them. The retry
-- dedup key mirrors migration 032's cleanup key. created_at is copied verbatim
-- into the per-contact rows and identifies rows created by this migration.
INSERT INTO balance_alerts (
    id,
    wallet_checksum,
    threshold_sats,
    alert_type,
    is_active,
    last_triggered_at,
    created_at,
    threshold_currency,
    threshold_fiat_amount,
    last_checked_balance_sats,
    contact_id
)
SELECT
    lower(hex(randomblob(4))) || '-' ||
    lower(hex(randomblob(2))) || '-' ||
    lower(hex(randomblob(2))) || '-' ||
    lower(hex(randomblob(2))) || '-' ||
    lower(hex(randomblob(6))),
    ba.wallet_checksum,
    ba.threshold_sats,
    ba.alert_type,
    ba.is_active,
    ba.last_triggered_at,
    ba.created_at,
    ba.threshold_currency,
    ba.threshold_fiat_amount,
    ba.last_checked_balance_sats,
    c.id
FROM balance_alerts ba
JOIN contacts c ON c.wallet_checksum = ba.wallet_checksum
WHERE ba.contact_id IS NULL
  AND ba.is_active = 1
  AND c.is_active = 1
  AND NOT EXISTS (
      SELECT 1
      FROM balance_alerts existing_ba
      WHERE existing_ba.wallet_checksum = ba.wallet_checksum
        AND existing_ba.contact_id = c.id
        AND existing_ba.threshold_sats = ba.threshold_sats
        AND existing_ba.alert_type = ba.alert_type
        AND existing_ba.created_at = ba.created_at
        AND (
            existing_ba.threshold_currency = ba.threshold_currency
            OR (existing_ba.threshold_currency IS NULL AND ba.threshold_currency IS NULL)
        )
        AND (
            existing_ba.threshold_fiat_amount = ba.threshold_fiat_amount
            OR (existing_ba.threshold_fiat_amount IS NULL AND ba.threshold_fiat_amount IS NULL)
        )
  );

UPDATE balance_alerts
SET is_active = 0
WHERE contact_id IS NULL
  AND is_active = 1
  AND EXISTS (
      SELECT 1
      FROM contacts c
      WHERE c.wallet_checksum = balance_alerts.wallet_checksum
        AND c.is_active = 1
  );

-- New transactional runs cannot leave notification_logs dropped without a
-- rename. Only databases that partially applied the pre-hardened 031 in the
-- old drop/rename window need manual repair because notification_logs is
-- already absent.
DROP TABLE IF EXISTS notification_logs_new;
CREATE TABLE notification_logs_new (
    id TEXT PRIMARY KEY,
    transaction_txid TEXT NOT NULL,
    transaction_wallet_checksum TEXT NOT NULL,
    notification_method_id TEXT,
    provider_name TEXT NOT NULL,
    provider_message_id TEXT,
    status TEXT NOT NULL CHECK (status IN ('sent', 'failed', 'delivered')),
    error_message TEXT,
    message_content TEXT NOT NULL,
    notification_type TEXT NOT NULL DEFAULT 'pending' CHECK (notification_type IN ('pending', 'confirmed', 'balance_alert', 'sending', 'sent', 'receiving', 'received', 'cpfp', 'rbf')),
    contact_name_snapshot TEXT,
    notification_target_snapshot TEXT,
    provider_type_snapshot TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (transaction_txid, transaction_wallet_checksum) REFERENCES transactions (txid, wallet_checksum) ON DELETE CASCADE,
    FOREIGN KEY (notification_method_id) REFERENCES contact_notification_methods (id) ON DELETE SET NULL
);

INSERT INTO notification_logs_new (
    id,
    transaction_txid,
    transaction_wallet_checksum,
    notification_method_id,
    provider_name,
    provider_message_id,
    status,
    error_message,
    message_content,
    notification_type,
    contact_name_snapshot,
    notification_target_snapshot,
    provider_type_snapshot,
    created_at
)
SELECT
    id,
    transaction_txid,
    transaction_wallet_checksum,
    notification_method_id,
    provider_name,
    provider_message_id,
    status,
    error_message,
    message_content,
    notification_type,
    contact_name_snapshot,
    notification_target_snapshot,
    provider_type_snapshot,
    created_at
FROM notification_logs;

DROP TABLE notification_logs;
ALTER TABLE notification_logs_new RENAME TO notification_logs;

CREATE INDEX IF NOT EXISTS idx_notification_logs_transaction ON notification_logs(transaction_txid, transaction_wallet_checksum);
CREATE INDEX IF NOT EXISTS idx_notification_logs_method ON notification_logs(notification_method_id);
CREATE INDEX IF NOT EXISTS idx_notification_logs_created_at ON notification_logs(created_at);
CREATE INDEX IF NOT EXISTS idx_notification_logs_wallet_checksum ON notification_logs(transaction_wallet_checksum);

COMMIT;
