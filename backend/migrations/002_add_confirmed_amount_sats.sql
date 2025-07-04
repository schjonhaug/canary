-- Add confirmed_amount_sats column to transaction_events
-- This was the previous implementation

ALTER TABLE transaction_events ADD COLUMN confirmed_amount_sats INTEGER;