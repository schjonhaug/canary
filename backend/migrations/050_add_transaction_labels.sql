-- Store the user-editable BIP-329 label alongside each wallet transaction.
ALTER TABLE transactions ADD COLUMN label TEXT;

CREATE INDEX IF NOT EXISTS idx_transactions_wallet_label
    ON transactions(wallet_checksum, label);
