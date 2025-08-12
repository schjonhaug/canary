-- Canary Bitcoin Wallet - Initial Database Schema with UUIDs
-- This is the complete initial schema for the application
-- Uses UUIDs for security-critical IDs instead of sequential integers

-- Core data tables

-- Current block header tracking (singleton table)
CREATE TABLE current_block_header (
    id INTEGER PRIMARY KEY DEFAULT 1,
    height INTEGER NOT NULL,
    timestamp INTEGER NOT NULL,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    CHECK (id = 1)
);

-- Users table: UUID primary key (critical for JWT security)
CREATE TABLE users (
    id TEXT PRIMARY KEY, -- UUIDv4
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    name TEXT,
    is_admin BOOLEAN NOT NULL DEFAULT FALSE,
    email_verified BOOLEAN NOT NULL DEFAULT FALSE,
    -- Subscription fields (extensible for future Business tier)
    subscription_tier TEXT DEFAULT 'pro' CHECK (subscription_tier IN ('personal', 'pro', 'business')),
    trial_started_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    trial_ends_at DATETIME DEFAULT (datetime('now', '+30 days')),
    subscription_status TEXT DEFAULT 'trial' CHECK (subscription_status IN ('trial', 'active', 'expired', 'cancelled')),
    -- Stripe integration
    stripe_customer_id TEXT UNIQUE,
    stripe_subscription_id TEXT,
    subscription_started_at DATETIME,
    subscription_ends_at DATETIME,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    last_login DATETIME
);

-- Sessions table: JWT/session management
CREATE TABLE sessions (
    id TEXT PRIMARY KEY, -- UUIDv4
    user_id TEXT NOT NULL, -- UUID reference
    token_hash TEXT NOT NULL UNIQUE,
    expires_at DATETIME NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

-- Email verification tokens
CREATE TABLE email_verification_tokens (
    id INTEGER PRIMARY KEY AUTOINCREMENT, -- Keep integer (internal only)
    user_id TEXT NOT NULL, -- UUID reference
    token TEXT NOT NULL UNIQUE,
    expires_at DATETIME NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

-- Password reset tokens  
CREATE TABLE password_reset_tokens (
    id INTEGER PRIMARY KEY AUTOINCREMENT, -- Keep integer (internal only)
    user_id TEXT NOT NULL, -- UUID reference
    token TEXT NOT NULL UNIQUE,
    expires_at DATETIME NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

-- OTP attempts tracking (keep integer - internal only)
CREATE TABLE otp_attempts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    phone_number TEXT NOT NULL,
    attempt_count INTEGER NOT NULL DEFAULT 1,
    last_attempt DATETIME DEFAULT CURRENT_TIMESTAMP,
    blocked_until DATETIME
);

-- Wallets table: Core wallet metadata
CREATE TABLE wallets (
    checksum TEXT PRIMARY KEY, -- Already secure (crypto hash)
    name TEXT NOT NULL,
    descriptor TEXT NOT NULL UNIQUE,
    hex_color TEXT NOT NULL,
    balance_total INTEGER DEFAULT 0,
    last_activity DATETIME,
    last_synced_at DATETIME,
    user_id TEXT NOT NULL, -- UUID reference
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

-- Transaction events table: UUID primary key (exposed in API)
CREATE TABLE transaction_events (
    id TEXT PRIMARY KEY, -- UUIDv4
    wallet_checksum TEXT NOT NULL,
    event_type TEXT NOT NULL CHECK (event_type IN ('send', 'receive')),
    amount_sats INTEGER NOT NULL,
    is_confirmed BOOLEAN DEFAULT FALSE,
    is_rbf BOOLEAN DEFAULT FALSE,
    is_cpfp BOOLEAN DEFAULT FALSE,
    balance_total INTEGER,
    transaction_time INTEGER NOT NULL,
    FOREIGN KEY (wallet_checksum) REFERENCES wallets (checksum) ON DELETE CASCADE
);

-- Contacts table: UUID primary key (exposed in API URLs)  
CREATE TABLE contacts (
    id TEXT PRIMARY KEY, -- UUIDv4
    wallet_checksum TEXT NOT NULL,
    name TEXT NOT NULL,
    language TEXT NOT NULL DEFAULT 'en' CHECK (language IN ('en', 'no')),
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (wallet_checksum) REFERENCES wallets (checksum) ON DELETE CASCADE
);

-- Contact notification methods: UUID primary key
CREATE TABLE contact_notification_methods (
    id TEXT PRIMARY KEY, -- UUIDv4
    contact_id TEXT NOT NULL, -- UUID reference
    provider_type TEXT NOT NULL CHECK (provider_type IN ('sms', 'ntfy', 'email')),
    notification_target TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (contact_id) REFERENCES contacts (id) ON DELETE CASCADE,
    UNIQUE(contact_id, provider_type, notification_target)
);

-- Pending contact verifications
CREATE TABLE pending_contact_verifications (
    id INTEGER PRIMARY KEY AUTOINCREMENT, -- Keep integer (temporary internal)
    wallet_checksum TEXT NOT NULL,
    provider_type TEXT NOT NULL CHECK (provider_type IN ('sms', 'email')),
    notification_target TEXT NOT NULL,
    contact_name TEXT NOT NULL,
    language TEXT NOT NULL DEFAULT 'en' CHECK (language IN ('en', 'no')),
    verification_code TEXT,
    expires_at DATETIME NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (wallet_checksum) REFERENCES wallets (checksum) ON DELETE CASCADE
);

-- Notification logs: UUID references
CREATE TABLE notification_logs (
    id TEXT PRIMARY KEY, -- UUIDv4
    event_id TEXT NOT NULL, -- UUID reference
    notification_method_id TEXT NOT NULL, -- UUID reference  
    provider_name TEXT NOT NULL,
    provider_message_id TEXT,
    status TEXT NOT NULL CHECK (status IN ('sent', 'failed', 'delivered')),
    error_message TEXT,
    message_content TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (event_id) REFERENCES transaction_events (id),
    FOREIGN KEY (notification_method_id) REFERENCES contact_notification_methods (id) ON DELETE CASCADE
);

-- Stripe webhook events for idempotency
CREATE TABLE stripe_webhook_events (
    id TEXT PRIMARY KEY, -- Stripe event ID
    event_type TEXT NOT NULL,
    processed_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    user_id TEXT, -- UUID reference (optional, some events may not be user-specific)
    subscription_id TEXT, -- Stripe subscription ID
    customer_id TEXT, -- Stripe customer ID
    metadata TEXT, -- JSON metadata from the event
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE SET NULL
);

-- Performance indexes
CREATE INDEX idx_sessions_token ON sessions(token_hash);
CREATE INDEX idx_sessions_expires ON sessions(expires_at);
CREATE INDEX idx_sessions_user_id ON sessions(user_id);
CREATE INDEX idx_wallets_user ON wallets(user_id);
CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_email_verification_tokens_token ON email_verification_tokens(token);
CREATE INDEX idx_email_verification_tokens_expires ON email_verification_tokens(expires_at);
CREATE INDEX idx_email_verification_tokens_user_id ON email_verification_tokens(user_id);
CREATE INDEX idx_password_reset_tokens_token ON password_reset_tokens(token);
CREATE INDEX idx_password_reset_tokens_expires ON password_reset_tokens(expires_at);
CREATE INDEX idx_password_reset_tokens_user_id ON password_reset_tokens(user_id);
CREATE INDEX idx_otp_attempts_phone ON otp_attempts(phone_number);
CREATE INDEX idx_notification_logs_event_id ON notification_logs (event_id);
CREATE INDEX idx_notification_logs_notification_method_id ON notification_logs (notification_method_id);
CREATE INDEX idx_notification_logs_provider ON notification_logs (provider_name);
CREATE INDEX idx_transaction_events_wallet_checksum ON transaction_events (wallet_checksum);
CREATE INDEX idx_contacts_wallet_checksum ON contacts (wallet_checksum);
CREATE INDEX idx_contact_notification_methods_contact_id ON contact_notification_methods (contact_id);
CREATE INDEX idx_contact_notification_methods_provider_type ON contact_notification_methods (provider_type);
CREATE INDEX idx_pending_verifications_wallet ON pending_contact_verifications (wallet_checksum);
CREATE INDEX idx_pending_verifications_expires ON pending_contact_verifications (expires_at);
CREATE INDEX idx_stripe_webhook_events_user_id ON stripe_webhook_events (user_id);
CREATE INDEX idx_stripe_webhook_events_customer_id ON stripe_webhook_events (customer_id);