BEGIN;

-- Wallet-level alerts are no longer part of the current configuration or
-- delivery model. Keep unmatched v1.5.2 rows as inactive audit/recovery data
-- instead of deleting them (and cascading away their notification history).
-- Migration 031/033 already copied every deliverable active alert to each
-- active contact, so disabling the remaining parents cannot lose delivery.
UPDATE balance_alerts
SET is_active = 0
WHERE contact_id IS NULL;

COMMIT;
