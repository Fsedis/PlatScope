ALTER TABLE game_metadata_snapshots
ADD COLUMN item_definition_count INTEGER NOT NULL DEFAULT 0
CHECK(item_definition_count >= 0);

CREATE TABLE IF NOT EXISTS game_item_definitions (
    snapshot_id INTEGER NOT NULL REFERENCES game_metadata_snapshots(id) ON DELETE CASCADE,
    slug TEXT NOT NULL,
    game_ref TEXT NOT NULL,
    mastery_requirement INTEGER NOT NULL CHECK(mastery_requirement BETWEEN 0 AND 50),
    PRIMARY KEY(snapshot_id, slug)
);

CREATE INDEX IF NOT EXISTS game_item_definitions_slug
ON game_item_definitions(slug, snapshot_id);

INSERT OR IGNORE INTO schema_migrations(version, name)
VALUES (9, 'game_item_definitions');
