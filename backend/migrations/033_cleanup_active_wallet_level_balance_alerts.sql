BEGIN;

CREATE TEMP TABLE active_wallet_level_balance_alert_cleanup_ids (
    id TEXT PRIMARY KEY
);

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
        AND (
            existing_ba.threshold_currency = ba.threshold_currency
            OR (existing_ba.threshold_currency IS NULL AND ba.threshold_currency IS NULL)
        )
        AND (
            existing_ba.threshold_fiat_amount = ba.threshold_fiat_amount
            OR (existing_ba.threshold_fiat_amount IS NULL AND ba.threshold_fiat_amount IS NULL)
        )
  );

INSERT INTO active_wallet_level_balance_alert_cleanup_ids (id)
SELECT ba.id
FROM balance_alerts ba
WHERE ba.contact_id IS NULL
  AND ba.is_active = 1
  AND EXISTS (
      SELECT 1
      FROM contacts c
      WHERE c.wallet_checksum = ba.wallet_checksum
        AND c.is_active = 1
  )
  AND EXISTS (
      SELECT 1
      FROM balance_alerts contact_ba
      WHERE contact_ba.wallet_checksum = ba.wallet_checksum
        AND contact_ba.contact_id IS NOT NULL
        AND contact_ba.threshold_sats = ba.threshold_sats
        AND contact_ba.alert_type = ba.alert_type
        AND (
            contact_ba.threshold_currency = ba.threshold_currency
            OR (contact_ba.threshold_currency IS NULL AND ba.threshold_currency IS NULL)
        )
        AND (
            contact_ba.threshold_fiat_amount = ba.threshold_fiat_amount
            OR (contact_ba.threshold_fiat_amount IS NULL AND ba.threshold_fiat_amount IS NULL)
        )
  );

DELETE FROM balance_alert_notification_logs
WHERE balance_alert_id IN (
    SELECT id FROM active_wallet_level_balance_alert_cleanup_ids
);

DELETE FROM balance_alert_notifications
WHERE balance_alert_id IN (
    SELECT id FROM active_wallet_level_balance_alert_cleanup_ids
);

DELETE FROM balance_alerts
WHERE id IN (
    SELECT id FROM active_wallet_level_balance_alert_cleanup_ids
);

DROP TABLE active_wallet_level_balance_alert_cleanup_ids;

COMMIT;
