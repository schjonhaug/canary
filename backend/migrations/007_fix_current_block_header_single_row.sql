-- Drop the existing table with multiple rows
DROP TABLE IF EXISTS current_block_header;

-- Create new table with a single row design
CREATE TABLE current_block_header (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    height INTEGER NOT NULL,
    hash TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Insert a dummy row to initialize the table structure
-- This will be replaced when the first real block header is stored
INSERT INTO current_block_header (id, height, hash, timestamp) 
VALUES (1, 0, '', 0);