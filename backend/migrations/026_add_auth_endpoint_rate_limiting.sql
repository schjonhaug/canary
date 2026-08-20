-- Registration and forgot-password rate limiting
-- Stores per-endpoint, per-email throttling state without persisting IP addresses.

CREATE TABLE auth_rate_limits (
    scope TEXT NOT NULL,
    identifier TEXT NOT NULL,
    attempt_count INTEGER NOT NULL DEFAULT 1,
    first_attempt_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    blocked_until DATETIME DEFAULT NULL,
    PRIMARY KEY (scope, identifier)
);
