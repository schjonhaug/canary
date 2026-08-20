-- Migration 029: Add preferred self-hosted transaction explorer selection
-- NULL means use the backend-provided default selection logic.
ALTER TABLE users ADD COLUMN preferred_tx_explorer_id TEXT DEFAULT NULL;
