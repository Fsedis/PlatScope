CREATE TABLE IF NOT EXISTS trade_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    fingerprint TEXT NOT NULL UNIQUE,
    occurred_at TEXT NOT NULL,
    partner TEXT,
    platinum_given INTEGER NOT NULL DEFAULT 0 CHECK(platinum_given >= 0),
    platinum_received INTEGER NOT NULL DEFAULT 0 CHECK(platinum_received >= 0),
    given_items_json TEXT NOT NULL DEFAULT '[]',
    received_items_json TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending', 'reconciled', 'ignored')),
    matched_order_id TEXT,
    reconciliation_json TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS trade_events_recent
ON trade_events(occurred_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS trade_events_pending
ON trade_events(status, occurred_at DESC);

INSERT OR IGNORE INTO schema_migrations(version, name)
VALUES (10, 'trade_shift');
