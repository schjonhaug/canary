-- Make contacts wallet-specific instead of global
-- This migration restructures the contacts system to make each contact belong to a specific wallet

-- First, create the new wallet-specific contacts table
CREATE TABLE contact_persons_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    wallet_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    phone_number TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (wallet_id) REFERENCES wallets (id) ON DELETE CASCADE
);

-- Migrate existing data - each contact that was linked to wallets becomes wallet-specific
INSERT INTO contact_persons_new (wallet_id, name, phone_number, created_at)
SELECT wc.wallet_id, cp.name, cp.phone_number, cp.created_at
FROM contact_persons cp
JOIN wallet_contacts wc ON cp.id = wc.contact_id;

-- Update foreign key references in sms_logs to point to the new contact structure
-- Create a mapping table to track old_contact_id -> new_contact_id per wallet
CREATE TEMPORARY TABLE contact_mapping AS
SELECT 
    old_cp.id as old_contact_id,
    new_cp.id as new_contact_id,
    wc.wallet_id
FROM contact_persons old_cp
JOIN wallet_contacts wc ON old_cp.id = wc.contact_id
JOIN contact_persons_new new_cp ON (
    new_cp.wallet_id = wc.wallet_id AND 
    new_cp.name = old_cp.name AND 
    new_cp.phone_number = old_cp.phone_number
);

-- Create new sms_logs table with updated foreign key
CREATE TABLE sms_logs_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id INTEGER NOT NULL,
    contact_id INTEGER NOT NULL,
    twilio_sid TEXT,
    status TEXT NOT NULL CHECK (status IN ('sent', 'failed', 'pending')),
    error_message TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (event_id) REFERENCES transaction_events (id),
    FOREIGN KEY (contact_id) REFERENCES contact_persons_new (id) ON DELETE CASCADE
);

-- Migrate sms_logs data using the mapping
INSERT INTO sms_logs_new (event_id, contact_id, twilio_sid, status, error_message, created_at)
SELECT sl.event_id, cm.new_contact_id, sl.twilio_sid, sl.status, sl.error_message, sl.created_at
FROM sms_logs sl
JOIN contact_mapping cm ON sl.contact_id = cm.old_contact_id;

-- Drop old tables
DROP TABLE sms_logs;
DROP TABLE wallet_contacts;
DROP TABLE contact_persons;

-- Rename new tables to final names
ALTER TABLE contact_persons_new RENAME TO contact_persons;
ALTER TABLE sms_logs_new RENAME TO sms_logs;

-- Drop the temporary mapping table
DROP TABLE contact_mapping;