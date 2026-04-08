-- Migration 026: Add expression index for transaction ordering and last_activity queries
CREATE INDEX IF NOT EXISTS idx_transactions_wallet_ordering
    ON transactions(
        wallet_checksum,
        COALESCE(confirmed_at, first_seen_at) DESC,
        txid DESC
    );
