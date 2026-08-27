BEGIN;

CREATE TEMP TABLE migrated_balance_alert_cleanup_ids (
    id TEXT PRIMARY KEY
);

CREATE TEMP TABLE migrated_balance_alert_history_ids (
    id TEXT PRIMARY KEY
);

INSERT INTO migrated_balance_alert_cleanup_ids (id)
SELECT ba.id
FROM balance_alerts ba
WHERE ba.contact_id IS NULL
  AND ba.is_active = 0
  AND EXISTS (
      SELECT 1
      FROM balance_alerts contact_ba
      WHERE contact_ba.wallet_checksum = ba.wallet_checksum
        AND contact_ba.contact_id IS NOT NULL
        AND contact_ba.threshold_sats = ba.threshold_sats
        AND contact_ba.alert_type = ba.alert_type
        -- Migration 031 copied ba.created_at verbatim into each per-contact row.
        AND contact_ba.created_at = ba.created_at
        AND (
            contact_ba.threshold_currency = ba.threshold_currency
            OR (contact_ba.threshold_currency IS NULL AND ba.threshold_currency IS NULL)
        )
        AND (
            contact_ba.threshold_fiat_amount = ba.threshold_fiat_amount
            OR (contact_ba.threshold_fiat_amount IS NULL AND ba.threshold_fiat_amount IS NULL)
        )
  );

-- Keep the inactive wallet-level parent when legacy audit rows still refer to
-- it. The parent stays outside current per-contact delivery, but preserving it
-- prevents ON DELETE CASCADE from erasing the v1.5.2 notification history.
INSERT INTO migrated_balance_alert_history_ids (id)
SELECT cleanup.id
FROM migrated_balance_alert_cleanup_ids cleanup
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

DELETE FROM migrated_balance_alert_cleanup_ids
WHERE id IN (
    SELECT id FROM migrated_balance_alert_history_ids
);

DELETE FROM balance_alert_notification_logs
WHERE balance_alert_id IN (
    SELECT id FROM migrated_balance_alert_cleanup_ids
);

DELETE FROM balance_alert_notifications
WHERE balance_alert_id IN (
    SELECT id FROM migrated_balance_alert_cleanup_ids
);

DELETE FROM balance_alerts
WHERE id IN (
    SELECT id FROM migrated_balance_alert_cleanup_ids
);

DROP TABLE migrated_balance_alert_history_ids;
DROP TABLE migrated_balance_alert_cleanup_ids;

COMMIT;
