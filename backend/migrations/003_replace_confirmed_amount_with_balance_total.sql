-- Replace confirmed_amount_sats with balance_total
-- This stores the total wallet balance at the time of each transaction event

-- Create a temporary table with the new schema (without confirmed_amount_sats, with balance_total)
CREATE TABLE transaction_events_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    wallet_id INTEGER NOT NULL,
    event_type TEXT NOT NULL CHECK (event_type IN ('send', 'receive')),
    amount_sats INTEGER NOT NULL,
    is_confirmed BOOLEAN DEFAULT FALSE,
    is_rbf BOOLEAN DEFAULT FALSE,
    is_cpfp BOOLEAN DEFAULT FALSE,
    balance_total INTEGER,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (wallet_id) REFERENCES wallets (id)
);

-- Copy data from old table to new table (excluding confirmed_amount_sats, setting balance_total to NULL)
INSERT INTO transaction_events_new (id, wallet_id, event_type, amount_sats, is_confirmed, is_rbf, is_cpfp, balance_total, created_at)
SELECT id, wallet_id, event_type, amount_sats, is_confirmed, is_rbf, is_cpfp, NULL, created_at
FROM transaction_events;

-- Drop the old table
DROP TABLE transaction_events;

-- Rename the new table to the original name
ALTER TABLE transaction_events_new RENAME TO transaction_events;