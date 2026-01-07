-- Migration 019: Update locale codes to include country/region variants
-- Changes: en -> en-US, no -> nb, es -> es-419, pt -> pt-BR, de -> de-DE, fr -> fr-FR
-- Japanese (ja), Danish (da), Swedish (sv) remain unchanged (single standard variants)
--
-- Note: The language column was removed from contacts and pending_contact_verifications
-- in migration 018. Only users.preferred_language needs to be updated.

-- Update users.preferred_language to use new locale codes
UPDATE users SET preferred_language = 'en-US' WHERE preferred_language = 'en';
UPDATE users SET preferred_language = 'nb' WHERE preferred_language = 'no';
UPDATE users SET preferred_language = 'es-419' WHERE preferred_language = 'es';
UPDATE users SET preferred_language = 'pt-BR' WHERE preferred_language = 'pt';
UPDATE users SET preferred_language = 'de-DE' WHERE preferred_language = 'de';
UPDATE users SET preferred_language = 'fr-FR' WHERE preferred_language = 'fr';
