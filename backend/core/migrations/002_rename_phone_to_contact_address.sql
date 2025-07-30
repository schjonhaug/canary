-- Migration to rename phone_number to contact_address
-- This makes the field more generic for different notification providers

ALTER TABLE contact_persons RENAME COLUMN phone_number TO contact_address;