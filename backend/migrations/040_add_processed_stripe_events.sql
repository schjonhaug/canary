-- Prevent concurrent duplicate Stripe webhook processing.

-- Pending email OTPs from before this migration are plaintext and cannot be
-- verified safely with the keyed digest format.
DELETE FROM pending_contact_verifications WHERE provider_type = 'email';

CREATE TABLE processed_stripe_events (
    event_id TEXT PRIMARY KEY,
    claim_token TEXT NOT NULL,
    claimed_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    processed_at DATETIME
);

CREATE INDEX idx_processed_stripe_events_retention
ON processed_stripe_events(processed_at)
WHERE processed_at IS NOT NULL;
