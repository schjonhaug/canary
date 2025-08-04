-- Development mode users (only inserted in debug builds)
-- These users are pre-created for testing purposes

-- Alice - Regular user
INSERT OR IGNORE INTO users (phone_number, name, is_admin) 
VALUES ('+4799999901', 'Alice', FALSE);

-- Bob - Regular user  
INSERT OR IGNORE INTO users (phone_number, name, is_admin)
VALUES ('+4699999902', 'Bob', FALSE);

-- Note: Charlie (+3399999903) is intentionally NOT pre-created
-- to test the new user registration flow