-- Add wallet_type column to distinguish descriptor-based wallets from single-address watches
ALTER TABLE wallets ADD COLUMN wallet_type TEXT NOT NULL DEFAULT 'descriptor'
    CHECK (wallet_type IN ('descriptor', 'address'));
