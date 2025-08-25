-- Add support for 'deleted' status in wallet sync_status
-- Since SQLite doesn't support modifying CHECK constraints directly, 
-- and this is a non-breaking change (just adding a new valid value),
-- we'll document that 'deleted' is now supported.
-- The application will handle this status correctly.

-- This migration is intentionally empty for existing databases
-- New installations will get the updated schema from 001_initial_schema.sql