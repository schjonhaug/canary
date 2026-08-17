-- Persist trial-ending emails until delivery succeeds after webhook completion.
CREATE TABLE stripe_trial_ending_notifications (
    event_id TEXT NOT NULL,
    customer_id TEXT NOT NULL,
    trial_end_timestamp INTEGER NOT NULL,
    sent_at DATETIME,
    PRIMARY KEY (event_id, customer_id)
);
