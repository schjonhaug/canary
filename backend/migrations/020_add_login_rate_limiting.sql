-- Login rate limiting and account lockout
-- Tracks failed login attempts per email to prevent brute-force attacks

CREATE TABLE login_attempts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    email TEXT NOT NULL,
    ip_address TEXT,  -- Optional: for IP-based rate limiting
    attempt_time DATETIME DEFAULT CURRENT_TIMESTAMP,
    success BOOLEAN NOT NULL DEFAULT FALSE
);

-- Index for efficient lookups by email and time
CREATE INDEX idx_login_attempts_email_time ON login_attempts (email, attempt_time);
CREATE INDEX idx_login_attempts_ip_time ON login_attempts (ip_address, attempt_time);

-- Add lockout fields to users table
ALTER TABLE users ADD COLUMN failed_login_attempts INTEGER DEFAULT 0;
ALTER TABLE users ADD COLUMN locked_until DATETIME DEFAULT NULL;
