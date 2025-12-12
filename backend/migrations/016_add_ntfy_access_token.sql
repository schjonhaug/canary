-- Migration 016: Add ntfy authentication for self-hosted servers
-- Supports both access token and username/password authentication

-- Add ntfy_access_token column for Bearer token auth
-- NULL means no token auth
ALTER TABLE users ADD COLUMN ntfy_access_token TEXT DEFAULT NULL;

-- Add ntfy_username and ntfy_password for Basic auth
-- NULL means no basic auth
ALTER TABLE users ADD COLUMN ntfy_username TEXT DEFAULT NULL;
ALTER TABLE users ADD COLUMN ntfy_password TEXT DEFAULT NULL;
