-- Add generic notification logs table for tracking delivery status across all providers
CREATE TABLE notification_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id INTEGER NOT NULL,
    contact_id INTEGER NOT NULL,
    provider_name TEXT NOT NULL,
    provider_message_id TEXT,         -- Twilio SID, ntfy response ID, etc.
    status TEXT NOT NULL CHECK (status IN ('sent', 'failed', 'delivered')),
    error_message TEXT,
    message_content TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (event_id) REFERENCES transaction_events (id),
    FOREIGN KEY (contact_id) REFERENCES contact_persons (id) ON DELETE CASCADE
);

-- Index for efficient queries by event and contact
CREATE INDEX idx_notification_logs_event_id ON notification_logs (event_id);
CREATE INDEX idx_notification_logs_contact_id ON notification_logs (contact_id);
CREATE INDEX idx_notification_logs_provider ON notification_logs (provider_name);