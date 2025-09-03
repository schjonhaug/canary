-- Migration 008: Transaction-Based Architecture Refactor
-- This migration transitions from balance-based to transaction-based tracking
-- Preserves user accounts but provides clean slate for wallet data

-- ============================
-- PRESERVE: Keep all data in these tables
-- ============================
-- users - User accounts, subscriptions, Stripe data
-- sessions - Active login sessions  
-- email_verification_tokens - Auth tokens
-- password_reset_tokens - Password reset tokens
-- stripe_webhook_events - Billing history
-- current_block_header - Blockchain state (not wallet-specific)

-- ============================
-- RESET: Delete data, keep table structure
-- ============================

-- Delete wallet-dependent data (cascading deletes will handle foreign keys)
DELETE FROM notification_logs;
DELETE FROM pending_contact_verifications; 
DELETE FROM contact_notification_methods;
DELETE FROM contacts;
DELETE FROM wallets;
DELETE FROM otp_attempts;

-- Note: transaction_events will be dropped and recreated below

-- ============================
-- RECREATE: New transaction-based schema
-- ============================

-- Drop old transaction_events table
DROP TABLE transaction_events;

-- Create new transactions table with txid as primary key
CREATE TABLE transactions (
    txid TEXT PRIMARY KEY, -- Bitcoin transaction ID (hash) - globally unique
    wallet_checksum TEXT NOT NULL,
    transaction_type TEXT NOT NULL CHECK (transaction_type IN ('send', 'receive')),
    amount_sats INTEGER NOT NULL, -- Amount for this specific transaction
    fee_sats INTEGER, -- Transaction fee (for send transactions)
    block_height INTEGER, -- NULL = mempool, >0 = confirmed at this height
    first_seen_at INTEGER NOT NULL, -- Unix timestamp when we first detected this transaction
    confirmed_at INTEGER, -- Unix timestamp when transaction was confirmed (from block)
    is_rbf BOOLEAN DEFAULT FALSE, -- Replace-by-fee
    is_cpfp BOOLEAN DEFAULT FALSE, -- Child-pays-for-parent
    balance_after INTEGER, -- Wallet balance after this transaction
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    -- Note: No unique constraint needed since txid is globally unique
    FOREIGN KEY (wallet_checksum) REFERENCES wallets (checksum) ON DELETE CASCADE
);

-- Create indexes for efficient queries
CREATE INDEX idx_transactions_wallet_checksum ON transactions(wallet_checksum);
CREATE INDEX idx_transactions_block_height ON transactions(block_height);
CREATE INDEX idx_transactions_first_seen_at ON transactions(first_seen_at);

-- ============================
-- UPDATE: notification_logs to reference new transactions table
-- ============================

-- SQLite doesn't support dropping foreign keys, so we need to recreate the table
-- Drop the old index first
DROP INDEX IF EXISTS idx_notification_logs_event_id;
DROP INDEX IF EXISTS idx_notification_logs_transaction_id;

-- Rename the old table
ALTER TABLE notification_logs RENAME TO notification_logs_old;

-- Create new notification_logs table with correct foreign key
CREATE TABLE notification_logs (
    id TEXT PRIMARY KEY,
    transaction_id TEXT NOT NULL, -- Now references transactions.txid
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
    FOREIGN KEY (transaction_id) REFERENCES transactions (txid),
    FOREIGN KEY (notification_method_id) REFERENCES contact_notification_methods (id) ON DELETE SET NULL
);

-- No need to copy data since we deleted all notification logs above
-- Starting fresh with clean notification audit trail

-- Drop the old table
DROP TABLE notification_logs_old;

-- Create indexes for the new table
CREATE INDEX idx_notification_logs_transaction_id ON notification_logs(transaction_id);
CREATE INDEX idx_notification_logs_notification_method_id ON notification_logs (notification_method_id);
CREATE INDEX idx_notification_logs_provider ON notification_logs (provider_name);

-- ============================
-- SUMMARY
-- ============================
-- After this migration:
-- 1. Users keep their accounts, subscriptions, and login sessions
-- 2. All wallet data is reset - users need to re-add wallets and contacts
-- 3. New transaction-based tracking with proper lifecycle management
-- 4. Notification logs are linked to individual transactions with clear audit trail
-- 5. System is ready for transaction-based sync logic