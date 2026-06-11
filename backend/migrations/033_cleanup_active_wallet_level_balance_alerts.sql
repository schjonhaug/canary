BEGIN;

CREATE TEMP TABLE active_wallet_level_balance_alert_cleanup_ids (
    id TEXT PRIMARY KEY
);

CREATE TEMP TABLE active_wallet_level_balance_alert_history_ids (
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
  -- Inactive alerts are historical fired alerts. Keep them wallet-level so the UI
  -- can still show legacy standalone history instead of inventing per-contact rows.
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
  -- Match the copy step: only active wallet-level alerts are obsolete after fan-out.
  -- Rows with notification history are deactivated below instead of deleted so
  -- balance alert audit records keep a valid parent row.
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

INSERT INTO active_wallet_level_balance_alert_history_ids (id)
SELECT cleanup.id
FROM active_wallet_level_balance_alert_cleanup_ids cleanup
WHERE EXISTS (
    SELECT 1
    FROM balance_alert_notifications notification
    WHERE notification.balance_alert_id = cleanup.id
)
OR EXISTS (
    SELECT 1
    FROM balance_alert_notification_logs log
    WHERE log.balance_alert_id = cleanup.id
);

UPDATE balance_alerts
SET is_active = 0
WHERE id IN (
    SELECT id FROM active_wallet_level_balance_alert_history_ids
);

DELETE FROM active_wallet_level_balance_alert_cleanup_ids
WHERE id IN (
    SELECT id FROM active_wallet_level_balance_alert_history_ids
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

DROP TABLE active_wallet_level_balance_alert_history_ids;
DROP TABLE active_wallet_level_balance_alert_cleanup_ids;

COMMIT;
