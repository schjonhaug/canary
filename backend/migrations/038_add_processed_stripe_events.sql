-- Migration 038: Prevent concurrent duplicate Stripe webhook processing.

CREATE TABLE processed_stripe_events (
    event_id TEXT PRIMARY KEY,
    processed_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
