CREATE TABLE IF NOT EXISTS current_block_header (
    height INTEGER PRIMARY KEY,
    hash TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Only store one row - the current block header
-- height is PRIMARY KEY so subsequent inserts will replace the current row