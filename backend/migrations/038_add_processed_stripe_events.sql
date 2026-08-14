-- Migration 038: Prevent concurrent duplicate Stripe webhook processing.

CREATE TABLE processed_stripe_events (
    event_id TEXT PRIMARY KEY,
    claim_token TEXT NOT NULL,
    claimed_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    processed_at DATETIME
);
