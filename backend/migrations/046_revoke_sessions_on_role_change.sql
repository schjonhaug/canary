-- Revoke both promotions and demotions, including direct operator SQL changes.
-- A later role change back must not resurrect an old privileged token.
CREATE TRIGGER IF NOT EXISTS revoke_sessions_on_role_change
AFTER UPDATE OF is_admin, is_demo ON users
WHEN OLD.is_admin != NEW.is_admin OR OLD.is_demo != NEW.is_demo
BEGIN
    DELETE FROM sessions WHERE user_id = NEW.id;
END;
