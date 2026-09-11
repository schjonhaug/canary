CREATE TABLE admin_support_grants (
    id TEXT PRIMARY KEY,
    actor_user_id TEXT NOT NULL,
    target_user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    reason TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    revoked_at INTEGER
);

CREATE INDEX idx_admin_support_grants_actor_active
    ON admin_support_grants(actor_user_id, revoked_at, expires_at);
