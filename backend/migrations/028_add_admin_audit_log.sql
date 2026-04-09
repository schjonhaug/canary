-- Add persistent audit trail for destructive admin operations.

CREATE TABLE admin_audit_log (
    id TEXT PRIMARY KEY,
    -- Intentionally not a foreign key: audit records should survive user deletion.
    actor_user_id TEXT NOT NULL,
    operation TEXT NOT NULL,
    target TEXT NOT NULL,
    details_json TEXT NOT NULL CHECK (json_valid(details_json)),
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_admin_audit_log_actor_user_id ON admin_audit_log(actor_user_id);
CREATE INDEX idx_admin_audit_log_operation ON admin_audit_log(operation);
CREATE INDEX idx_admin_audit_log_created_at ON admin_audit_log(created_at);
