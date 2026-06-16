-- Migration 036: Generic self-hosted instance settings

CREATE TABLE IF NOT EXISTS instance_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

INSERT OR IGNORE INTO instance_settings (key, value)
VALUES ('nostr_dm_mode', 'auto');
