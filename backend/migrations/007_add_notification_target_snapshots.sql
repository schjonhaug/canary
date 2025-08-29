-- Migration 007: Add notification target and provider type snapshots
-- This enhancement preserves the complete notification method information
-- (target address and provider type) when contacts are deleted, providing
-- full audit trail of where notifications were actually sent.

-- Step 1: Add snapshot columns for notification target and provider type
ALTER TABLE notification_logs ADD COLUMN notification_target_snapshot TEXT;
ALTER TABLE notification_logs ADD COLUMN provider_type_snapshot TEXT;

-- Step 2: Backfill existing notification logs with current target and provider info
UPDATE notification_logs 
SET notification_target_snapshot = (
    SELECT cnm.notification_target
    FROM contact_notification_methods cnm
    WHERE cnm.id = notification_logs.notification_method_id
),
provider_type_snapshot = (
    SELECT cnm.provider_type
    FROM contact_notification_methods cnm
    WHERE cnm.id = notification_logs.notification_method_id
)
WHERE notification_method_id IS NOT NULL;