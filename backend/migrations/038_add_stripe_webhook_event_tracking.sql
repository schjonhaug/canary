-- Track the latest Stripe event applied to each user's entitlement.
ALTER TABLE users ADD COLUMN stripe_event_created INTEGER;
