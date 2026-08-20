CREATE TABLE pending_billing_checkouts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    token TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    provider TEXT NOT NULL,
    subscription_tier TEXT NOT NULL,
    billing_period TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at DATETIME NOT NULL,
    completed_at DATETIME DEFAULT NULL,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX idx_pending_billing_checkouts_user_id
    ON pending_billing_checkouts(user_id);

CREATE TABLE btcpay_subscription_links (
    user_id TEXT PRIMARY KEY,
    checkout_token TEXT NOT NULL UNIQUE,
    customer_id TEXT NOT NULL,
    plan_id TEXT NOT NULL,
    status TEXT NOT NULL,
    last_event_timestamp INTEGER NOT NULL,
    last_event_priority INTEGER NOT NULL,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY (checkout_token) REFERENCES pending_billing_checkouts(token) ON DELETE CASCADE
);

CREATE TABLE processed_btcpay_events (
    delivery_id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    event_timestamp INTEGER NOT NULL,
    processed_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
