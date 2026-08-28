CREATE TABLE IF NOT EXISTS inventory_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source TEXT NOT NULL,
    observed_at TEXT NOT NULL,
    imported_at TEXT NOT NULL,
    schema_version INTEGER NOT NULL CHECK(schema_version > 0),
    item_count INTEGER NOT NULL CHECK(item_count >= 0),
    resolved_row_count INTEGER NOT NULL CHECK(resolved_row_count >= 0),
    checksum_sha256 TEXT NOT NULL,
    keep_copies INTEGER NOT NULL CHECK(keep_copies >= 0),
    is_current INTEGER NOT NULL DEFAULT 0 CHECK(is_current IN (0, 1))
);

CREATE UNIQUE INDEX IF NOT EXISTS inventory_single_current
    ON inventory_snapshots(is_current)
    WHERE is_current = 1;

CREATE TABLE IF NOT EXISTS inventory_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    snapshot_id INTEGER NOT NULL REFERENCES inventory_snapshots(id) ON DELETE CASCADE,
    canonical_game_id TEXT NOT NULL,
    display_name_en TEXT,
    display_name_ru TEXT,
    tags_json TEXT NOT NULL DEFAULT '[]',
    item_slug TEXT,
    platform TEXT,
    rank INTEGER,
    subtype TEXT,
    owned_quantity INTEGER NOT NULL CHECK(owned_quantity > 0),
    tradeable_quantity INTEGER NOT NULL CHECK(tradeable_quantity >= 0),
    untradeable_quantity INTEGER NOT NULL CHECK(untradeable_quantity >= 0),
    unknown_quantity INTEGER NOT NULL CHECK(unknown_quantity >= 0),
    leveled_quantity INTEGER NOT NULL CHECK(leveled_quantity >= 0),
    sellable_quantity INTEGER NOT NULL CHECK(sellable_quantity >= 0),
    resolution TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS inventory_items_current_lookup
    ON inventory_items(snapshot_id, item_slug, rank, subtype);

INSERT OR IGNORE INTO schema_migrations(version, name)
VALUES (6, 'inventory');
