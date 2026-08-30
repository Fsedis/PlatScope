ALTER TABLE inventory_snapshots
    ADD COLUMN credits INTEGER
    CHECK(credits IS NULL OR credits >= 0);

ALTER TABLE inventory_snapshots
    ADD COLUMN syndicates_json TEXT NOT NULL DEFAULT '[]';

INSERT OR IGNORE INTO schema_migrations(version, name)
VALUES (12, 'inventory_account_state');
