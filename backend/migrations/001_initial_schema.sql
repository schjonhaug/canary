-- Initial schema for Kanari wallet management system
-- Creates all base tables

CREATE TABLE IF NOT EXISTS wallets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    descriptor TEXT NOT NULL UNIQUE,
    wallet_filename TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS transaction_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    wallet_id INTEGER NOT NULL,
    event_type TEXT NOT NULL CHECK (event_type IN ('send', 'receive')),
    amount_sats INTEGER NOT NULL,
    is_confirmed BOOLEAN DEFAULT FALSE,
    is_rbf BOOLEAN DEFAULT FALSE,
    is_cpfp BOOLEAN DEFAULT FALSE,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (wallet_id) REFERENCES wallets (id)
);

CREATE TABLE IF NOT EXISTS contact_persons (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    phone_number TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS wallet_contacts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    wallet_id INTEGER NOT NULL,
    contact_id INTEGER NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (wallet_id) REFERENCES wallets (id),
    FOREIGN KEY (contact_id) REFERENCES contact_persons (id),
    UNIQUE(wallet_id, contact_id)
);

CREATE TABLE IF NOT EXISTS twilio_config (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    account_sid TEXT NOT NULL,
    auth_token TEXT NOT NULL,
    messaging_service_sid TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS sms_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id INTEGER NOT NULL,
    contact_id INTEGER NOT NULL,
    twilio_sid TEXT,
    status TEXT NOT NULL CHECK (status IN ('sent', 'failed', 'pending')),
    error_message TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (event_id) REFERENCES transaction_events (id),
    FOREIGN KEY (contact_id) REFERENCES contact_persons (id)
);