CREATE TABLE IF NOT EXISTS game_metadata_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source TEXT NOT NULL,
    fetched_at TEXT NOT NULL,
    promoted_at TEXT NOT NULL,
    schema_version INTEGER NOT NULL CHECK(schema_version > 0),
    set_count INTEGER NOT NULL CHECK(set_count >= 0),
    relic_count INTEGER NOT NULL CHECK(relic_count >= 0),
    prime_part_count INTEGER NOT NULL CHECK(prime_part_count >= 0),
    checksum_sha256 TEXT NOT NULL,
    metadata_json TEXT NOT NULL,
    is_current INTEGER NOT NULL DEFAULT 0 CHECK(is_current IN (0, 1))
);

CREATE UNIQUE INDEX IF NOT EXISTS game_metadata_single_current
    ON game_metadata_snapshots(is_current)
    WHERE is_current = 1;

INSERT OR IGNORE INTO schema_migrations(version, name)
VALUES (7, 'game_metadata');
