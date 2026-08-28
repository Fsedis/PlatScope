DROP INDEX IF EXISTS market_prices_variant_unique;

ALTER TABLE market_prices ADD COLUMN amber_stars INTEGER;
ALTER TABLE market_prices ADD COLUMN cyan_stars INTEGER;

CREATE UNIQUE INDEX market_prices_variant_unique
    ON market_prices(
        snapshot_id,
        item_slug,
        platform,
        COALESCE(rank, -1),
        COALESCE(subtype, ''),
        COALESCE(amber_stars, -1),
        COALESCE(cyan_stars, -1),
        order_type
    );

INSERT OR IGNORE INTO schema_migrations(version, name)
VALUES (3, 'sculpture_variants');
