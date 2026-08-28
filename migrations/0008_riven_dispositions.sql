ALTER TABLE game_metadata_snapshots
ADD COLUMN riven_disposition_count INTEGER NOT NULL DEFAULT 0
CHECK(riven_disposition_count >= 0);

INSERT OR IGNORE INTO schema_migrations(version, name)
VALUES (8, 'riven_dispositions');
