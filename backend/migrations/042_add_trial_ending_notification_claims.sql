-- Prevent concurrent webhook deliveries from sending the same email twice.
ALTER TABLE stripe_trial_ending_notifications ADD COLUMN claim_token TEXT;
ALTER TABLE stripe_trial_ending_notifications ADD COLUMN claimed_at DATETIME;
