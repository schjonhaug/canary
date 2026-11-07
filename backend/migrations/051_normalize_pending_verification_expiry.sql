-- Migration 051: Store pending contact verification expiry in SQLite's datetime format
--
-- Earlier versions stored expires_at as RFC 3339 text (2026-01-01T12:10:00.123+00:00), but the
-- expiry checks compare it as text with datetime('now') (2026-01-01 12:00:00). Rewrite existing
-- rows so those comparisons are correct, and drop rows whose expiry cannot be parsed.

DELETE FROM pending_contact_verifications WHERE datetime(expires_at) IS NULL;

UPDATE pending_contact_verifications
SET expires_at = datetime(expires_at)
WHERE expires_at <> datetime(expires_at);
