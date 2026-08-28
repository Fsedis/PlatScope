CREATE TABLE IF NOT EXISTS market_history_snapshots (
    id INTEGER PRIMARY KEY,
    provider TEXT NOT NULL,
    source_date TEXT NOT NULL UNIQUE,
    fetched_at TEXT NOT NULL,
    imported_at TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    item_count INTEGER NOT NULL CHECK (item_count >= 0),
    record_count INTEGER NOT NULL CHECK (record_count >= 0),
    checksum_sha256 TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS market_history (
    snapshot_id INTEGER NOT NULL REFERENCES market_history_snapshots(id) ON DELETE CASCADE,
    source_date TEXT NOT NULL,
    item_slug TEXT NOT NULL REFERENCES item_catalog(slug),
    platform TEXT NOT NULL,
    rank INTEGER,
    subtype TEXT,
    amber_stars INTEGER,
    cyan_stars INTEGER,
    closed_median REAL,
    closed_volume REAL NOT NULL DEFAULT 0 CHECK (closed_volume >= 0),
    sell_median REAL,
    buy_median REAL
);

CREATE UNIQUE INDEX IF NOT EXISTS market_history_variant_unique
    ON market_history(
        source_date,
        item_slug,
        platform,
        COALESCE(rank, -1),
        COALESCE(subtype, ''),
        COALESCE(amber_stars, -1),
        COALESCE(cyan_stars, -1)
    );

CREATE INDEX IF NOT EXISTS market_history_variant_date
    ON market_history(
        item_slug,
        platform,
        rank,
        subtype,
        amber_stars,
        cyan_stars,
        source_date
    );

INSERT OR IGNORE INTO market_history_snapshots(
    provider, source_date, fetched_at, imported_at, schema_version,
    item_count, record_count, checksum_sha256
)
SELECT provider, source_date, fetched_at, promoted_at, schema_version,
       item_count, record_count, checksum_sha256
FROM market_snapshots
WHERE is_current = 1;

INSERT OR IGNORE INTO market_history(
    snapshot_id, source_date, item_slug, platform, rank, subtype,
    amber_stars, cyan_stars, closed_median, closed_volume,
    sell_median, buy_median
)
SELECT history_snapshot.id, market_snapshot.source_date,
       price.item_slug, price.platform, price.rank, price.subtype,
       price.amber_stars, price.cyan_stars,
       MAX(CASE WHEN price.order_type = 'closed' THEN price.median END),
       COALESCE(MAX(CASE WHEN price.order_type = 'closed' THEN price.volume END), 0),
       MAX(CASE WHEN price.order_type = 'sell' THEN price.median END),
       MAX(CASE WHEN price.order_type = 'buy' THEN price.median END)
FROM market_prices price
JOIN market_snapshots market_snapshot
  ON market_snapshot.id = price.snapshot_id AND market_snapshot.is_current = 1
JOIN market_history_snapshots history_snapshot
  ON history_snapshot.source_date = market_snapshot.source_date
GROUP BY history_snapshot.id, market_snapshot.source_date,
         price.item_slug, price.platform, price.rank, price.subtype,
         price.amber_stars, price.cyan_stars;

INSERT OR IGNORE INTO schema_migrations(version, name)
VALUES (5, 'market_history');
