-- Migration 013: Add preferred language field to users table
-- This field stores the user's language preference for emails and UI
-- Initially set from browser_locale at registration, can be changed by user later

ALTER TABLE users ADD COLUMN preferred_language TEXT DEFAULT 'en';
