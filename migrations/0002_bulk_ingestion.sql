CREATE TABLE IF NOT EXISTS catalog_snapshots (
    id INTEGER PRIMARY KEY,
    provider TEXT NOT NULL,
    fetched_at TEXT NOT NULL,
    promoted_at TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    item_count INTEGER NOT NULL CHECK (item_count >= 0),
    checksum_sha256 TEXT NOT NULL,
    catalog_json TEXT NOT NULL,
    is_current INTEGER NOT NULL DEFAULT 0 CHECK (is_current IN (0, 1))
);

CREATE UNIQUE INDEX IF NOT EXISTS catalog_snapshots_one_current
    ON catalog_snapshots(is_current)
    WHERE is_current = 1;

CREATE UNIQUE INDEX IF NOT EXISTS market_prices_variant_unique
    ON market_prices(
        snapshot_id,
        item_slug,
        platform,
        COALESCE(rank, -1),
        COALESCE(subtype, ''),
        order_type
    );

INSERT OR IGNORE INTO schema_migrations(version, name)
VALUES (2, 'bulk_ingestion');
