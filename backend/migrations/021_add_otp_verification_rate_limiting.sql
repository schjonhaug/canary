-- OTP verification rate limiting
-- Tracks failed OTP code verification attempts to prevent brute-force attacks
-- This is separate from otp_attempts which tracks OTP sending attempts

CREATE TABLE otp_verification_attempts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    notification_target TEXT NOT NULL UNIQUE,  -- Phone number or email address (unique to prevent duplicates)
    attempt_count INTEGER NOT NULL DEFAULT 1,
    last_attempt DATETIME DEFAULT CURRENT_TIMESTAMP,
    blocked_until DATETIME
);

-- Index for efficient lookups by notification target (UNIQUE already creates an index, but explicit for clarity)
CREATE INDEX idx_otp_verification_attempts_target ON otp_verification_attempts(notification_target);
