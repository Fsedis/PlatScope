ALTER TABLE item_catalog ADD COLUMN display_name_ru TEXT;
ALTER TABLE item_catalog ADD COLUMN search_text TEXT NOT NULL DEFAULT '';

UPDATE item_catalog
SET search_text = lower(slug || ' ' || display_name_en)
WHERE search_text = '';

CREATE INDEX item_catalog_search_text
    ON item_catalog(search_text);

INSERT OR IGNORE INTO schema_migrations(version, name)
VALUES (4, 'market_search');
