BEGIN;

CREATE TEMP TABLE wallet_level_balance_alert_cleanup_ids (
    id TEXT PRIMARY KEY
);

INSERT INTO wallet_level_balance_alert_cleanup_ids (id)
SELECT id
FROM balance_alerts
WHERE contact_id IS NULL;

DELETE FROM balance_alert_notification_logs
WHERE balance_alert_id IN (
    SELECT id FROM wallet_level_balance_alert_cleanup_ids
);

DELETE FROM balance_alert_notifications
WHERE balance_alert_id IN (
    SELECT id FROM wallet_level_balance_alert_cleanup_ids
);

DELETE FROM balance_alerts
WHERE id IN (
    SELECT id FROM wallet_level_balance_alert_cleanup_ids
);

DROP TABLE wallet_level_balance_alert_cleanup_ids;

COMMIT;
