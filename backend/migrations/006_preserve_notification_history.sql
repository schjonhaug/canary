-- Migration 006: Preserve notification history when contacts are deleted
-- This fixes the issue where deleting a contact would cascade delete all notification logs,
-- losing historical data about who was notified for past transactions.

-- Step 1: Add contact_name_snapshot column to preserve contact name at notification time
ALTER TABLE notification_logs ADD COLUMN contact_name_snapshot TEXT;

-- Step 2: Backfill existing notification logs with current contact names
UPDATE notification_logs 
SET contact_name_snapshot = (
    SELECT c.name
    FROM contact_notification_methods cnm
    JOIN contacts c ON cnm.contact_id = c.id
    WHERE cnm.id = notification_logs.notification_method_id
)
WHERE contact_name_snapshot IS NULL;

-- Step 3: Drop the existing foreign key constraint (SQLite requires recreating the table)
-- First, create the new table structure
CREATE TABLE notification_logs_new (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL,
    notification_method_id TEXT, -- Changed from NOT NULL to allow NULLs
    provider_name TEXT NOT NULL,
    provider_message_id TEXT,
    status TEXT NOT NULL CHECK (status IN ('sent', 'failed', 'delivered')),
    error_message TEXT,
    message_content TEXT NOT NULL,
    contact_name_snapshot TEXT, -- New field to preserve contact name
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (event_id) REFERENCES transaction_events (id),
    FOREIGN KEY (notification_method_id) REFERENCES contact_notification_methods (id) ON DELETE SET NULL
);

-- Copy all data to the new table
INSERT INTO notification_logs_new 
SELECT id, event_id, notification_method_id, provider_name, provider_message_id, 
       status, error_message, message_content, contact_name_snapshot, created_at
FROM notification_logs;

-- Drop the old table and rename the new one
DROP TABLE notification_logs;
ALTER TABLE notification_logs_new RENAME TO notification_logs;

-- Recreate the indexes
CREATE INDEX idx_notification_logs_event_id ON notification_logs (event_id);
CREATE INDEX idx_notification_logs_notification_method_id ON notification_logs (notification_method_id);
CREATE INDEX idx_notification_logs_provider ON notification_logs (provider_name);