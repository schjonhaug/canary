-- Migration 022: Remove IP address logging from login attempts
--
-- Privacy improvement: IP addresses combined with wallet data (xpubs) create
-- a liability. We keep email-based rate limiting (5 attempts → 15 min lockout)
-- but remove IP tracking entirely.

-- Drop the IP-based index
DROP INDEX IF EXISTS idx_login_attempts_ip_time;

-- Create new table without ip_address column
CREATE TABLE login_attempts_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    email TEXT NOT NULL,
    attempt_time DATETIME DEFAULT CURRENT_TIMESTAMP,
    success BOOLEAN NOT NULL DEFAULT FALSE
);

-- Copy data (excluding ip_address)
INSERT INTO login_attempts_new (id, email, attempt_time, success)
SELECT id, email, attempt_time, success FROM login_attempts;

-- Drop old table and rename new one
DROP TABLE login_attempts;
ALTER TABLE login_attempts_new RENAME TO login_attempts;

-- Recreate email index
CREATE INDEX idx_login_attempts_email_time ON login_attempts (email, attempt_time);
