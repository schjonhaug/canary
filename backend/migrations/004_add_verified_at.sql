-- Add verified_at column to track completed verifications
-- This allows us to keep verification records for the PUT endpoint to find
-- while marking them as completed to prevent reuse

ALTER TABLE pending_contact_verifications 
ADD COLUMN verified_at DATETIME DEFAULT NULL;

-- Add index for efficient lookup of recent verifications
CREATE INDEX idx_pending_verifications_lookup 
ON pending_contact_verifications(wallet_checksum, notification_target, verified_at);