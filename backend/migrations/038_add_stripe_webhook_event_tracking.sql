-- Track the latest Stripe event applied to each user's entitlement.
ALTER TABLE users ADD COLUMN stripe_event_created INTEGER;
ALTER TABLE stripe_webhook_events ADD COLUMN delivery_status TEXT NOT NULL DEFAULT 'completed';
ALTER TABLE stripe_webhook_events ADD COLUMN processing_started_at DATETIME;
