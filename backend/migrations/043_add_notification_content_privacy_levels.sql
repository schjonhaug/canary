-- Existing delivery methods retain their current rich content. Application
-- creation paths explicitly store the privacy-conscious `standard` default for
-- new methods.
ALTER TABLE contact_notification_methods
ADD COLUMN content_privacy_level TEXT NOT NULL DEFAULT 'detailed'
CHECK (content_privacy_level IN ('minimal', 'standard', 'detailed'));
