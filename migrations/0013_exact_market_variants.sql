ALTER TABLE item_catalog ADD COLUMN max_charges INTEGER;
ALTER TABLE item_catalog ADD COLUMN max_amber_stars INTEGER;
ALTER TABLE item_catalog ADD COLUMN max_cyan_stars INTEGER;

DROP INDEX IF EXISTS market_prices_variant_unique;
CREATE TABLE market_prices_exact (
    snapshot_id INTEGER NOT NULL REFERENCES market_snapshots(id) ON DELETE CASCADE,
    item_slug TEXT NOT NULL REFERENCES item_catalog(slug),
    platform TEXT NOT NULL,
    rank INTEGER,
    subtype TEXT,
    amber_stars INTEGER,
    cyan_stars INTEGER,
    charges INTEGER,
    order_type TEXT NOT NULL,
    median REAL,
    average REAL,
    min_price REAL,
    max_price REAL,
    volume REAL NOT NULL,
    raw_json TEXT NOT NULL
);
INSERT INTO market_prices_exact(
    snapshot_id, item_slug, platform, rank, subtype,
    amber_stars, cyan_stars, charges, order_type,
    median, average, min_price, max_price, volume, raw_json
)
SELECT snapshot_id, item_slug, platform, rank, subtype,
       amber_stars, cyan_stars, NULL, order_type,
       median, average, min_price, max_price, volume, raw_json
FROM market_prices;
DROP TABLE market_prices;
ALTER TABLE market_prices_exact RENAME TO market_prices;
CREATE UNIQUE INDEX market_prices_variant_unique
    ON market_prices(
        snapshot_id,
        item_slug,
        platform,
        COALESCE(rank, -1),
        COALESCE(charges, -1),
        COALESCE(subtype, ''),
        COALESCE(amber_stars, -1),
        COALESCE(cyan_stars, -1),
        order_type
    );

DROP INDEX IF EXISTS market_history_variant_unique;
DROP INDEX IF EXISTS market_history_variant_date;
ALTER TABLE market_history ADD COLUMN charges INTEGER;
CREATE UNIQUE INDEX market_history_variant_unique
    ON market_history(
        source_date,
        item_slug,
        platform,
        COALESCE(rank, -1),
        COALESCE(charges, -1),
        COALESCE(subtype, ''),
        COALESCE(amber_stars, -1),
        COALESCE(cyan_stars, -1)
    );
CREATE INDEX market_history_variant_date
    ON market_history(
        item_slug,
        platform,
        rank,
        charges,
        subtype,
        amber_stars,
        cyan_stars,
        source_date
    );

ALTER TABLE inventory_items ADD COLUMN market_rank INTEGER;
ALTER TABLE inventory_items ADD COLUMN charges INTEGER;
ALTER TABLE inventory_items ADD COLUMN amber_stars INTEGER;
ALTER TABLE inventory_items ADD COLUMN cyan_stars INTEGER;
UPDATE inventory_items SET market_rank = rank WHERE item_slug IS NOT NULL;
DROP INDEX IF EXISTS inventory_items_current_lookup;
CREATE INDEX inventory_items_current_lookup
    ON inventory_items(
        snapshot_id,
        item_slug,
        market_rank,
        charges,
        subtype,
        amber_stars,
        cyan_stars
    );

INSERT OR IGNORE INTO schema_migrations(version, name)
VALUES (13, 'exact_market_variants');
