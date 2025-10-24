-- Migration 013: Add is_demo flag for read-only demo accounts
-- This flag allows creating demo accounts that can view but not modify data

-- Add is_demo column to users table
ALTER TABLE users ADD COLUMN is_demo BOOLEAN NOT NULL DEFAULT FALSE;

-- Create index for efficient demo user queries
CREATE INDEX idx_users_is_demo ON users(is_demo);
