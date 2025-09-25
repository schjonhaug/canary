-- Migration 012: Create balance_alert_notification_logs table
-- This migration creates a separate table for balance alert notification logs
-- to avoid foreign key constraint issues with the transaction-based notification_logs table

-- Create balance alert notification logs table
CREATE TABLE balance_alert_notification_logs (
    id TEXT PRIMARY KEY,
    balance_alert_id TEXT NOT NULL,
    wallet_checksum TEXT NOT NULL,
    notification_method_id TEXT,
    provider_name TEXT NOT NULL,
    provider_message_id TEXT,
    status TEXT NOT NULL CHECK (status IN ('sent', 'failed', 'delivered')),
    error_message TEXT,
    message_content TEXT NOT NULL,
    contact_name_snapshot TEXT,
    notification_target_snapshot TEXT,
    provider_type_snapshot TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (balance_alert_id) REFERENCES balance_alerts (id) ON DELETE CASCADE,
    FOREIGN KEY (notification_method_id) REFERENCES contact_notification_methods (id) ON DELETE SET NULL
);

-- Create indexes for performance
CREATE INDEX idx_balance_alert_notification_logs_alert ON balance_alert_notification_logs(balance_alert_id);
CREATE INDEX idx_balance_alert_notification_logs_method ON balance_alert_notification_logs(notification_method_id);
CREATE INDEX idx_balance_alert_notification_logs_created_at ON balance_alert_notification_logs(created_at);