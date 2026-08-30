ALTER TABLE inventory_snapshots
    ADD COLUMN mod_usage_scanned INTEGER NOT NULL DEFAULT 0
    CHECK(mod_usage_scanned IN (0, 1));

ALTER TABLE inventory_items
    ADD COLUMN equipped_quantity INTEGER NOT NULL DEFAULT 0
    CHECK(equipped_quantity >= 0);

ALTER TABLE inventory_items
    ADD COLUMN equipped_tradeable_quantity INTEGER NOT NULL DEFAULT 0
    CHECK(equipped_tradeable_quantity >= 0);

CREATE TABLE IF NOT EXISTS inventory_mod_placements (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    inventory_item_id INTEGER NOT NULL REFERENCES inventory_items(id) ON DELETE CASCADE,
    equipment_instance_key TEXT NOT NULL,
    equipment_game_id TEXT NOT NULL,
    equipment_display_name_en TEXT,
    equipment_display_name_ru TEXT,
    equipment_image_url TEXT,
    equipment_kind TEXT NOT NULL,
    config_index INTEGER NOT NULL CHECK(config_index >= 0),
    UNIQUE(inventory_item_id, equipment_instance_key, config_index)
);

CREATE INDEX IF NOT EXISTS inventory_mod_placements_item
    ON inventory_mod_placements(inventory_item_id, equipment_kind, equipment_instance_key);

INSERT OR IGNORE INTO schema_migrations(version, name)
VALUES (11, 'equipped_mods');
