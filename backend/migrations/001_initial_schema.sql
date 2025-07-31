-- Canary Bitcoin Wallet Management Database Schema
-- This is the complete initial schema for the application

-- Wallets table: Core wallet metadata with balance tracking
CREATE TABLE wallets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    descriptor TEXT NOT NULL UNIQUE,
    wallet_filename TEXT NOT NULL,
    hex_color TEXT NOT NULL,
    balance_total INTEGER DEFAULT 0,
    last_activity DATETIME,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Transaction events table: Bitcoin transaction tracking with comprehensive metadata
CREATE TABLE transaction_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    wallet_id INTEGER NOT NULL,
    event_type TEXT NOT NULL CHECK (event_type IN ('send', 'receive')),
    amount_sats INTEGER NOT NULL,
    is_confirmed BOOLEAN DEFAULT FALSE,
    is_rbf BOOLEAN DEFAULT FALSE,
    is_cpfp BOOLEAN DEFAULT FALSE,
    balance_total INTEGER,
    transaction_time INTEGER NOT NULL,
    FOREIGN KEY (wallet_id) REFERENCES wallets (id) ON DELETE CASCADE
);

-- Contacts table: Basic contact info
CREATE TABLE contacts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    wallet_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    language TEXT NOT NULL DEFAULT 'en' CHECK (language IN ('en', 'no')),
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (wallet_id) REFERENCES wallets (id) ON DELETE CASCADE
);

-- Contact notification methods: Multiple notification methods per contact  
CREATE TABLE contact_notification_methods (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    contact_id INTEGER NOT NULL,
    provider_type TEXT NOT NULL CHECK (provider_type IN ('sms', 'ntfy')),
    notification_target TEXT NOT NULL,  -- phone number or ntfy topic
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (contact_id) REFERENCES contacts (id) ON DELETE CASCADE,
    UNIQUE(contact_id, provider_type, notification_target)
);

-- Notification logs table: Generic notification tracking for all providers
CREATE TABLE notification_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id INTEGER NOT NULL,
    notification_method_id INTEGER NOT NULL,
    provider_name TEXT NOT NULL,
    provider_message_id TEXT,         -- Twilio SID, ntfy response ID, etc.
    status TEXT NOT NULL CHECK (status IN ('sent', 'failed', 'delivered')),
    error_message TEXT,
    message_content TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (event_id) REFERENCES transaction_events (id),
    FOREIGN KEY (notification_method_id) REFERENCES contact_notification_methods (id) ON DELETE CASCADE
);

-- Current block header table: Blockchain state tracking (singleton)
CREATE TABLE current_block_header (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    height INTEGER NOT NULL,
    timestamp INTEGER NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Initialize the singleton block header row
INSERT INTO current_block_header (id, height, timestamp) 
VALUES (1, 0, 0);

-- Indexes for performance
CREATE INDEX idx_notification_logs_event_id ON notification_logs (event_id);
CREATE INDEX idx_notification_logs_notification_method_id ON notification_logs (notification_method_id);
CREATE INDEX idx_notification_logs_provider ON notification_logs (provider_name);
CREATE INDEX idx_transaction_events_wallet_id ON transaction_events (wallet_id);
CREATE INDEX idx_contacts_wallet_id ON contacts (wallet_id);
CREATE INDEX idx_contact_notification_methods_contact_id ON contact_notification_methods (contact_id);
CREATE INDEX idx_contact_notification_methods_provider_type ON contact_notification_methods (provider_type);