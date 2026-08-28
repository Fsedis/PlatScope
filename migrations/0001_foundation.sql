CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS market_snapshots (
    id INTEGER PRIMARY KEY,
    provider TEXT NOT NULL,
    source_date TEXT NOT NULL,
    fetched_at TEXT NOT NULL,
    promoted_at TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    item_count INTEGER NOT NULL CHECK (item_count >= 0),
    record_count INTEGER NOT NULL CHECK (record_count >= 0),
    checksum_sha256 TEXT NOT NULL,
    is_current INTEGER NOT NULL DEFAULT 0 CHECK (is_current IN (0, 1))
);

CREATE UNIQUE INDEX IF NOT EXISTS market_snapshots_one_current
    ON market_snapshots(is_current)
    WHERE is_current = 1;

CREATE TABLE IF NOT EXISTS item_catalog (
    item_id TEXT PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    display_name_en TEXT NOT NULL,
    game_ref TEXT,
    max_rank INTEGER,
    subtypes_json TEXT NOT NULL DEFAULT '[]',
    tags_json TEXT NOT NULL DEFAULT '[]',
    catalog_source TEXT NOT NULL,
    catalog_fetched_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS market_prices (
    snapshot_id INTEGER NOT NULL REFERENCES market_snapshots(id) ON DELETE CASCADE,
    item_slug TEXT NOT NULL REFERENCES item_catalog(slug),
    platform TEXT NOT NULL,
    rank INTEGER,
    subtype TEXT,
    order_type TEXT NOT NULL,
    median REAL,
    average REAL,
    min_price REAL,
    max_price REAL,
    volume REAL NOT NULL,
    raw_json TEXT NOT NULL,
    PRIMARY KEY (snapshot_id, item_slug, platform, rank, subtype, order_type)
);

CREATE TABLE IF NOT EXISTS source_health (
    provider TEXT PRIMARY KEY,
    last_attempt TEXT,
    last_success TEXT,
    last_error_code TEXT,
    last_error_message_redacted TEXT,
    latency_ms INTEGER,
    consecutive_failures INTEGER NOT NULL DEFAULT 0 CHECK (consecutive_failures >= 0)
);

INSERT OR IGNORE INTO schema_migrations(version, name)
VALUES (1, 'foundation');
