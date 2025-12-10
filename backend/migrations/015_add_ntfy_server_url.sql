-- Migration 015: Add ntfy server URL preference
-- Allows self-hosted users to configure their own ntfy server

-- Add ntfy_server_url column to users table
-- NULL means use environment variable NTFY_SERVER_URL or default to https://ntfy.sh
ALTER TABLE users ADD COLUMN ntfy_server_url TEXT DEFAULT NULL;
