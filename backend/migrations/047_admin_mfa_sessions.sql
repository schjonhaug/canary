ALTER TABLE sessions ADD COLUMN admin_mfa_verified_at INTEGER;
ALTER TABLE sessions ADD COLUMN admin_mfa_key_version TEXT;
CREATE TABLE IF NOT EXISTS admin_mfa_replay (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    key_version TEXT NOT NULL,
    last_step INTEGER NOT NULL,
    PRIMARY KEY (user_id, key_version)
);
