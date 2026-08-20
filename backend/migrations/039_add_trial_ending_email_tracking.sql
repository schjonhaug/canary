-- Persist completion of the trial-ending email independently from webhook completion.
ALTER TABLE stripe_webhook_events ADD COLUMN trial_ending_email_sent_at DATETIME;
