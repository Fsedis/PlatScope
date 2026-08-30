#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use platscope_domain::{
    EquipmentKind, GameMetadataSnapshot, GameMetadataSource, InventoryResolution,
    InventorySnapshotMetadata, InventorySource, ItemCatalog, MarketHistoryPoint, MarketOrderType,
    MarketRecord, MarketVariantKey, NormalizedMarketSnapshot, Platform, ProviderId,
    ResolvedInventoryItem, ResolvedInventorySnapshot, ResolvedModPlacement,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;

const FOUNDATION_MIGRATION: &str = include_str!("../../../migrations/0001_foundation.sql");
const BULK_INGESTION_MIGRATION: &str = include_str!("../../../migrations/0002_bulk_ingestion.sql");
const SCULPTURE_VARIANTS_MIGRATION: &str =
    include_str!("../../../migrations/0003_sculpture_variants.sql");
const MARKET_SEARCH_MIGRATION: &str = include_str!("../../../migrations/0004_market_search.sql");
const MARKET_HISTORY_MIGRATION: &str = include_str!("../../../migrations/0005_market_history.sql");
const INVENTORY_MIGRATION: &str = include_str!("../../../migrations/0006_inventory.sql");
const GAME_METADATA_MIGRATION: &str = include_str!("../../../migrations/0007_game_metadata.sql");
const RIVEN_DISPOSITIONS_MIGRATION: &str =
    include_str!("../../../migrations/0008_riven_dispositions.sql");
const GAME_ITEM_DEFINITIONS_MIGRATION: &str =
    include_str!("../../../migrations/0009_game_item_definitions.sql");
const TRADE_SHIFT_MIGRATION: &str = include_str!("../../../migrations/0010_trade_shift.sql");
const EQUIPPED_MODS_MIGRATION: &str = include_str!("../../../migrations/0011_equipped_mods.sql");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketSnapshotSummary {
    pub provider: ProviderId,
    pub source_date: NaiveDate,
    pub fetched_at: DateTime<Utc>,
    pub promoted_at: DateTime<Utc>,
    pub item_count: u64,
    pub record_count: u64,
    pub checksum_sha256: String,
}

#[derive(Debug, Clone)]
pub struct MarketVariantBundle {
    pub key: MarketVariantKey,
    pub item_id: String,
    pub display_name_en: String,
    pub display_name_ru: Option<String>,
    pub tags: Vec<String>,
    pub records: Vec<MarketRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryCoverage {
    pub oldest_date: Option<NaiveDate>,
    pub newest_date: Option<NaiveDate>,
    pub day_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderHealth {
    pub provider: ProviderId,
    pub last_attempt: Option<DateTime<Utc>>,
    pub last_success: Option<DateTime<Utc>>,
    pub last_error_code: Option<String>,
    pub last_error_message: Option<String>,
    pub latency_ms: Option<u64>,
    pub consecutive_failures: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeItem {
    pub name: String,
    pub quantity: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TradeEventStatus {
    Pending,
    Reconciled,
    Ignored,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTradeEvent {
    pub fingerprint: String,
    pub occurred_at: DateTime<Utc>,
    pub partner: Option<String>,
    pub platinum_given: u32,
    pub platinum_received: u32,
    pub given_items: Vec<TradeItem>,
    pub received_items: Vec<TradeItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeEvent {
    pub id: i64,
    pub occurred_at: DateTime<Utc>,
    pub partner: Option<String>,
    pub platinum_given: u32,
    pub platinum_received: u32,
    pub given_items: Vec<TradeItem>,
    pub received_items: Vec<TradeItem>,
    pub status: TradeEventStatus,
    pub matched_order_id: Option<String>,
    pub reconciliation_json: Option<String>,
}

pub struct Database {
    connection: Connection,
    path: Option<PathBuf>,
}

impl Database {
    /// Открывает файловую SQLite DB, настраивает соединение и применяет migrations.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`] при ошибке открытия, настройки или migration.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref().to_path_buf();
        let connection = Connection::open(&path)?;
        let database = Self {
            connection,
            path: Some(path),
        };
        database.configure()?;
        database.migrate()?;
        Ok(database)
    }

    /// Открывает временную SQLite DB для unit tests и применяет migrations.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`] при ошибке SQLite или migration.
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let connection = Connection::open_in_memory()?;
        let database = Self {
            connection,
            path: None,
        };
        database.configure()?;
        database.migrate()?;
        Ok(database)
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Возвращает максимальную применённую версию схемы.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`] при ошибке чтения таблицы migrations.
    pub fn schema_version(&self) -> Result<i64, StorageError> {
        self.connection
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .map_err(StorageError::from)
    }

    /// Сериализует и сохраняет настройку по стабильному ключу.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`] при ошибке сериализации или записи SQLite.
    pub fn set_setting<T: Serialize>(&self, key: &str, value: &T) -> Result<(), StorageError> {
        let value_json = serde_json::to_string(value)?;
        self.connection.execute(
            "INSERT INTO settings(key, value_json, updated_at)
             VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
             ON CONFLICT(key) DO UPDATE SET
                value_json = excluded.value_json,
                updated_at = excluded.updated_at",
            params![key, value_json],
        )?;
        Ok(())
    }

    /// Загружает и десериализует настройку; отсутствие ключа не является ошибкой.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`] при ошибке SQLite или несовместимом JSON.
    pub fn get_setting<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, StorageError> {
        let value = self
            .connection
            .query_row(
                "SELECT value_json FROM settings WHERE key = ?1",
                [key],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        value
            .map(|json| serde_json::from_str(&json).map_err(StorageError::from))
            .transpose()
    }

    /// Сохраняет подтверждённую игрой сделку. Повторный маркер из EE.log не создаёт дубль.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`] при нарушении bounds, ошибке JSON или SQLite.
    pub fn record_trade_event(&self, event: &NewTradeEvent) -> Result<bool, StorageError> {
        if event.fingerprint.trim().is_empty() || event.fingerprint.len() > 4_096 {
            return Err(StorageError::Invariant(
                "trade event fingerprint is empty or too long".into(),
            ));
        }
        if event.given_items.len() > 32 || event.received_items.len() > 32 {
            return Err(StorageError::Invariant(
                "trade event contains too many item rows".into(),
            ));
        }
        let changed = self.connection.execute(
            "INSERT OR IGNORE INTO trade_events(
                fingerprint, occurred_at, partner, platinum_given, platinum_received,
                given_items_json, received_items_json, status, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', ?8)",
            params![
                event.fingerprint,
                event.occurred_at.to_rfc3339(),
                event.partner,
                i64::from(event.platinum_given),
                i64::from(event.platinum_received),
                serde_json::to_string(&event.given_items)?,
                serde_json::to_string(&event.received_items)?,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(changed == 1)
    }

    /// Возвращает последние сделки для сверки с WFM-ордерами.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`] при ошибке SQLite, даты или сохранённого JSON.
    pub fn recent_trade_events(&self, limit: usize) -> Result<Vec<TradeEvent>, StorageError> {
        let limit = limit.clamp(1, 100);
        let mut statement = self.connection.prepare(
            "SELECT id, occurred_at, partner, platinum_given, platinum_received,
                    given_items_json, received_items_json, status, matched_order_id,
                    reconciliation_json
             FROM trade_events
             ORDER BY occurred_at DESC, id DESC
             LIMIT ?1",
        )?;
        let rows = statement.query_map([i64::try_from(limit).unwrap_or(100)], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<String>>(9)?,
            ))
        })?;
        let mut events = Vec::new();
        for row in rows {
            let (
                id,
                occurred_at,
                partner,
                platinum_given,
                platinum_received,
                given_items_json,
                received_items_json,
                status,
                matched_order_id,
                reconciliation_json,
            ) = row?;
            events.push(TradeEvent {
                id,
                occurred_at: DateTime::parse_from_rfc3339(&occurred_at)?.with_timezone(&Utc),
                partner,
                platinum_given: u32_from_sql(platinum_given, "trade platinum_given")?,
                platinum_received: u32_from_sql(platinum_received, "trade platinum_received")?,
                given_items: serde_json::from_str(&given_items_json)?,
                received_items: serde_json::from_str(&received_items_json)?,
                status: trade_event_status(&status)?,
                matched_order_id,
                reconciliation_json,
            });
        }
        Ok(events)
    }

    /// Помечает сделку как обработанную или намеренно пропущенную.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`] при ошибке SQLite.
    pub fn set_trade_event_status(
        &self,
        id: i64,
        status: TradeEventStatus,
        matched_order_id: Option<&str>,
        reconciliation_json: Option<&str>,
    ) -> Result<bool, StorageError> {
        let changed = self.connection.execute(
            "UPDATE trade_events
             SET status = ?2, matched_order_id = ?3, reconciliation_json = ?4
             WHERE id = ?1",
            params![
                id,
                trade_event_status_name(status),
                matched_order_id,
                reconciliation_json,
            ],
        )?;
        Ok(changed == 1)
    }

    /// Атомарно публикует проверенный каталог и обновляет индекс item identity.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`], если сериализация или SQLite transaction не завершилась.
    pub fn promote_catalog(&mut self, catalog: &ItemCatalog) -> Result<(), StorageError> {
        if catalog.metadata.item_count != catalog.items.len() as u64 {
            return Err(StorageError::Invariant(
                "catalog item_count differs from normalized items".into(),
            ));
        }
        let catalog_json = serde_json::to_string(catalog)?;
        let promoted_at = Utc::now().to_rfc3339();
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "UPDATE catalog_snapshots SET is_current = 0 WHERE is_current = 1",
            [],
        )?;
        transaction.execute(
            "INSERT INTO catalog_snapshots(
                provider, fetched_at, promoted_at, schema_version, item_count,
                checksum_sha256, catalog_json, is_current
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1)",
            params![
                provider_name(catalog.metadata.provider),
                catalog.metadata.fetched_at.to_rfc3339(),
                promoted_at,
                i64::from(catalog.metadata.schema_version),
                to_i64(catalog.metadata.item_count, "catalog item_count")?,
                catalog.metadata.checksum_sha256,
                catalog_json,
            ],
        )?;
        {
            let mut statement = transaction.prepare_cached(
                "INSERT INTO item_catalog(
                    item_id, slug, display_name_en, display_name_ru, game_ref, max_rank,
                    subtypes_json, tags_json, catalog_source, catalog_fetched_at, search_text
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(item_id) DO UPDATE SET
                    slug = excluded.slug,
                    display_name_en = excluded.display_name_en,
                    display_name_ru = excluded.display_name_ru,
                    game_ref = excluded.game_ref,
                    max_rank = excluded.max_rank,
                    subtypes_json = excluded.subtypes_json,
                    tags_json = excluded.tags_json,
                    catalog_source = excluded.catalog_source,
                    catalog_fetched_at = excluded.catalog_fetched_at,
                    search_text = excluded.search_text",
            )?;
            for item in &catalog.items {
                statement.execute(params![
                    item.item_id,
                    item.slug,
                    item.display_name_en,
                    item.display_name_ru,
                    item.game_ref,
                    item.max_rank.map(i64::from),
                    serde_json::to_string(&item.subtypes)?,
                    serde_json::to_string(&item.tags)?,
                    provider_name(catalog.metadata.provider),
                    catalog.metadata.fetched_at.to_rfc3339(),
                    search_text(item),
                ])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    /// Возвращает последний полностью опубликованный каталог.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`] при ошибке чтения или несовместимом JSON.
    pub fn load_current_catalog(&self) -> Result<Option<ItemCatalog>, StorageError> {
        let json = self
            .connection
            .query_row(
                "SELECT catalog_json FROM catalog_snapshots WHERE is_current = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        json.map(|value| serde_json::from_str(&value).map_err(StorageError::from))
            .transpose()
    }

    /// Атомарно публикует provider-neutral metadata LKG отдельно от price snapshot.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`] при нарушении count invariant, сериализации или SQLite.
    pub fn promote_game_metadata(
        &mut self,
        snapshot: &GameMetadataSnapshot,
    ) -> Result<(), StorageError> {
        if snapshot.metadata.set_count != snapshot.prime_sets.len() as u64
            || snapshot.metadata.relic_count != snapshot.relics.len() as u64
            || snapshot.metadata.prime_part_count != snapshot.prime_parts.len() as u64
            || snapshot.metadata.riven_disposition_count != snapshot.riven_dispositions.len() as u64
            || snapshot.metadata.item_definition_count != snapshot.item_definitions.len() as u64
        {
            return Err(StorageError::Invariant(
                "game metadata counts differ from normalized rows".into(),
            ));
        }
        let metadata_json = serde_json::to_string(snapshot)?;
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "UPDATE game_metadata_snapshots SET is_current = 0 WHERE is_current = 1",
            [],
        )?;
        transaction.execute(
            "INSERT INTO game_metadata_snapshots(
                source, fetched_at, promoted_at, schema_version, set_count,
                relic_count, prime_part_count, riven_disposition_count,
                item_definition_count, checksum_sha256, metadata_json, is_current
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 1)",
            params![
                game_metadata_source_name(snapshot.metadata.source),
                snapshot.metadata.fetched_at.to_rfc3339(),
                Utc::now().to_rfc3339(),
                i64::from(snapshot.metadata.schema_version),
                to_i64(snapshot.metadata.set_count, "metadata set_count")?,
                to_i64(snapshot.metadata.relic_count, "metadata relic_count")?,
                to_i64(
                    snapshot.metadata.prime_part_count,
                    "metadata prime_part_count"
                )?,
                to_i64(
                    snapshot.metadata.riven_disposition_count,
                    "metadata riven_disposition_count"
                )?,
                to_i64(
                    snapshot.metadata.item_definition_count,
                    "metadata item_definition_count"
                )?,
                snapshot.metadata.checksum_sha256,
                metadata_json,
            ],
        )?;
        let snapshot_id = transaction.last_insert_rowid();
        {
            let mut statement = transaction.prepare_cached(
                "INSERT INTO game_item_definitions(
                    snapshot_id, slug, game_ref, mastery_requirement
                 ) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for definition in &snapshot.item_definitions {
                statement.execute(params![
                    snapshot_id,
                    definition.slug,
                    definition.game_ref,
                    i64::from(definition.mastery_requirement),
                ])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    /// Возвращает последний полностью опубликованный metadata snapshot.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`] при SQLite или несовместимом persisted JSON.
    pub fn load_current_game_metadata(&self) -> Result<Option<GameMetadataSnapshot>, StorageError> {
        let json = self
            .connection
            .query_row(
                "SELECT metadata_json FROM game_metadata_snapshots WHERE is_current = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        json.map(|value| serde_json::from_str(&value).map_err(StorageError::from))
            .transpose()
    }

    /// Возвращает компактную проекцию mastery requirement текущего metadata LKG.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`] при ошибке SQLite или некорректном сохранённом значении.
    pub fn current_mastery_requirements(&self) -> Result<HashMap<String, u8>, StorageError> {
        let mut statement = self.connection.prepare_cached(
            "SELECT definition.slug, definition.mastery_requirement
             FROM game_item_definitions definition
             JOIN game_metadata_snapshots snapshot
               ON snapshot.id = definition.snapshot_id AND snapshot.is_current = 1",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut definitions = HashMap::new();
        for row in rows {
            let (slug, mastery_requirement) = row?;
            definitions.insert(
                slug,
                u8_from_sql(mastery_requirement, "mastery_requirement")?,
            );
        }
        Ok(definitions)
    }

    /// Атомарно импортирует все price records и только после этого меняет current pointer.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`]; при любой ошибке transaction откатывается целиком.
    pub fn promote_market_snapshot(
        &mut self,
        snapshot: &NormalizedMarketSnapshot,
    ) -> Result<MarketSnapshotSummary, StorageError> {
        if snapshot.metadata.record_count != snapshot.records.len() as u64 {
            return Err(StorageError::Invariant(
                "snapshot record_count differs from normalized records".into(),
            ));
        }
        let history = aggregate_history(snapshot)?;
        let promoted_at = Utc::now();
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "INSERT INTO market_snapshots(
                provider, source_date, fetched_at, promoted_at, schema_version,
                item_count, record_count, checksum_sha256, is_current
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0)",
            params![
                provider_name(snapshot.metadata.provider),
                snapshot.metadata.source_date.to_string(),
                snapshot.metadata.fetched_at.to_rfc3339(),
                promoted_at.to_rfc3339(),
                i64::from(snapshot.metadata.schema_version),
                to_i64(snapshot.metadata.item_count, "snapshot item_count")?,
                to_i64(snapshot.metadata.record_count, "snapshot record_count")?,
                snapshot.metadata.checksum_sha256,
            ],
        )?;
        let snapshot_id = transaction.last_insert_rowid();
        {
            let mut statement = transaction.prepare_cached(
                "INSERT INTO market_prices(
                    snapshot_id, item_slug, platform, rank, subtype, order_type,
                    median, average, min_price, max_price, volume, raw_json,
                    amber_stars, cyan_stars
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            )?;
            for record in &snapshot.records {
                statement.execute(params![
                    snapshot_id,
                    record.key.slug,
                    platform_name(record.key.platform),
                    record.key.rank.map(i64::from),
                    record.key.subtype,
                    order_type_name(record.order_type),
                    record.median,
                    record.average,
                    record.min_price,
                    record.max_price,
                    record.volume,
                    record.raw_json,
                    record.key.amber_stars.map(i64::from),
                    record.key.cyan_stars.map(i64::from),
                ])?;
            }
        }
        transaction.execute(
            "UPDATE market_snapshots SET is_current = 0 WHERE is_current = 1",
            [],
        )?;
        transaction.execute(
            "UPDATE market_snapshots SET is_current = 1 WHERE id = ?1",
            [snapshot_id],
        )?;
        store_history_snapshot(&transaction, snapshot, &history)?;
        transaction.commit()?;

        Ok(MarketSnapshotSummary {
            provider: snapshot.metadata.provider,
            source_date: snapshot.metadata.source_date,
            fetched_at: snapshot.metadata.fetched_at,
            promoted_at,
            item_count: snapshot.metadata.item_count,
            record_count: snapshot.metadata.record_count,
            checksum_sha256: snapshot.metadata.checksum_sha256.clone(),
        })
    }

    /// Атомарно сохраняет compact daily aggregates без raw historical JSON.
    /// Повторный импорт той же даты полностью заменяет только этот день.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`] при нарушении snapshot invariants или ошибке SQLite.
    pub fn promote_history_snapshot(
        &mut self,
        snapshot: &NormalizedMarketSnapshot,
    ) -> Result<(), StorageError> {
        if snapshot.metadata.record_count != snapshot.records.len() as u64 {
            return Err(StorageError::Invariant(
                "history record_count differs from normalized records".into(),
            ));
        }
        let history = aggregate_history(snapshot)?;
        let transaction = self.connection.transaction()?;
        store_history_snapshot(&transaction, snapshot, &history)?;
        transaction.commit()?;
        Ok(())
    }

    /// Проверяет наличие полностью импортированного immutable дня.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`] при ошибке SQLite.
    pub fn has_history_date(&self, date: NaiveDate) -> Result<bool, StorageError> {
        self.connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM market_history_snapshots WHERE source_date = ?1
                 )",
                [date.to_string()],
                |row| row.get::<_, bool>(0),
            )
            .map_err(StorageError::from)
    }

    /// Возвращает compact coverage без загрузки price rows.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`] при ошибке SQLite или persisted date.
    pub fn history_coverage(&self) -> Result<HistoryCoverage, StorageError> {
        let (oldest, newest, count): (Option<String>, Option<String>, i64) =
            self.connection.query_row(
                "SELECT MIN(source_date), MAX(source_date), COUNT(*)
                 FROM market_history_snapshots",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
        Ok(HistoryCoverage {
            oldest_date: oldest.map(|value| value.parse()).transpose()?,
            newest_date: newest.map(|value| value.parse()).transpose()?,
            day_count: to_u64(count, "history day_count")?,
        })
    }

    /// Загружает не более `days` compact points только для точного варианта.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`] при ошибке SQLite или persisted date/number.
    pub fn market_history(
        &self,
        key: &MarketVariantKey,
        days: u16,
        as_of: NaiveDate,
    ) -> Result<Vec<MarketHistoryPoint>, StorageError> {
        if days == 0 {
            return Ok(Vec::new());
        }
        let first_date = as_of - chrono::Duration::days(i64::from(days.saturating_sub(1)));
        let mut statement = self.connection.prepare_cached(
            "SELECT source_date, closed_median, closed_volume, sell_median, buy_median
             FROM market_history
             WHERE item_slug = ?1
               AND platform = ?2
               AND rank IS ?3
               AND subtype IS ?4
               AND amber_stars IS ?5
               AND cyan_stars IS ?6
               AND source_date BETWEEN ?7 AND ?8
             ORDER BY source_date ASC",
        )?;
        let mut rows = statement.query(params![
            key.slug,
            platform_name(key.platform),
            key.rank.map(i64::from),
            key.subtype,
            key.amber_stars.map(i64::from),
            key.cyan_stars.map(i64::from),
            first_date.to_string(),
            as_of.to_string(),
        ])?;
        let mut points = Vec::new();
        while let Some(row) = rows.next()? {
            points.push(MarketHistoryPoint {
                source_date: row.get::<_, String>(0)?.parse()?,
                closed_median: row.get(1)?,
                closed_volume: row.get(2)?,
                sell_median: row.get(3)?,
                buy_median: row.get(4)?,
            });
        }
        Ok(points)
    }

    /// Читает metadata текущего LKG snapshot без загрузки price rows.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`] при ошибке SQLite или некорректных persisted metadata.
    pub fn current_market_snapshot(&self) -> Result<Option<MarketSnapshotSummary>, StorageError> {
        self.connection
            .query_row(
                "SELECT provider, source_date, fetched_at, promoted_at,
                        item_count, record_count, checksum_sha256
                 FROM market_snapshots WHERE is_current = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                },
            )
            .optional()?
            .map(|row| {
                Ok(MarketSnapshotSummary {
                    provider: parse_provider(&row.0)?,
                    source_date: row.1.parse()?,
                    fetched_at: row.2.parse()?,
                    promoted_at: row.3.parse()?,
                    item_count: to_u64(row.4, "snapshot item_count")?,
                    record_count: to_u64(row.5, "snapshot record_count")?,
                    checksum_sha256: row.6,
                })
            })
            .transpose()
    }

    /// Загружает price signals только для точного rank/subtype/stars варианта текущего LKG.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`] при ошибке SQLite или несовместимых persisted values.
    pub fn current_market_records(
        &self,
        key: &MarketVariantKey,
    ) -> Result<Vec<MarketRecord>, StorageError> {
        let mut statement = self.connection.prepare_cached(
            "SELECT mp.item_slug, mp.platform, mp.rank, mp.subtype,
                    mp.amber_stars, mp.cyan_stars, mp.order_type,
                    mp.median, mp.average, mp.min_price, mp.max_price, mp.volume,
                    mp.raw_json, ms.source_date, ic.item_id, ic.display_name_en
             FROM market_prices mp
             JOIN market_snapshots ms ON ms.id = mp.snapshot_id AND ms.is_current = 1
             JOIN item_catalog ic ON ic.slug = mp.item_slug
             WHERE mp.item_slug = ?1
               AND mp.platform = ?2
               AND mp.rank IS ?3
               AND mp.subtype IS ?4
               AND mp.amber_stars IS ?5
               AND mp.cyan_stars IS ?6",
        )?;
        let mut rows = statement.query(params![
            key.slug,
            platform_name(key.platform),
            key.rank.map(i64::from),
            key.subtype,
            key.amber_stars.map(i64::from),
            key.cyan_stars.map(i64::from),
        ])?;
        let mut records = Vec::new();
        while let Some(row) = rows.next()? {
            let platform = parse_platform(&row.get::<_, String>(1)?)?;
            let rank = optional_u16_from_sql(row.get::<_, Option<i64>>(2)?, "rank")?;
            let amber_stars = optional_u16_from_sql(row.get::<_, Option<i64>>(4)?, "amber_stars")?;
            let cyan_stars = optional_u16_from_sql(row.get::<_, Option<i64>>(5)?, "cyan_stars")?;
            let variant = MarketVariantKey::new(
                row.get::<_, String>(0)?,
                platform,
                rank,
                row.get::<_, Option<String>>(3)?,
            )
            .map_err(|error| StorageError::Invariant(error.to_string()))?
            .with_stars(amber_stars, cyan_stars);
            let source_date: NaiveDate = row.get::<_, String>(13)?.parse()?;
            let observed_at = source_date
                .and_hms_opt(0, 0, 0)
                .ok_or_else(|| StorageError::Invariant("invalid source date time".into()))?
                .and_utc();
            records.push(MarketRecord {
                key: variant,
                external_item_id: row.get(14)?,
                display_name_en: row.get(15)?,
                observed_at,
                order_type: parse_order_type(&row.get::<_, String>(6)?)?,
                median: row.get(7)?,
                average: row.get(8)?,
                min_price: row.get(9)?,
                max_price: row.get(10)?,
                volume: row.get(11)?,
                raw_json: row.get(12)?,
            });
        }
        Ok(records)
    }

    /// Ищет варианты в текущем LKG по локализованному имени или canonical slug.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`] при ошибке SQLite или persisted JSON.
    pub fn search_current_market_variants(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MarketVariantBundle>, StorageError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let normalized_query = query.trim().to_lowercase();
        let mut statement = self.connection.prepare_cached(
            "SELECT mp.item_slug, mp.platform, mp.rank, mp.subtype,
                    mp.amber_stars, mp.cyan_stars, ic.item_id,
                    ic.display_name_en, ic.display_name_ru, ic.tags_json
             FROM market_prices mp
             JOIN market_snapshots ms ON ms.id = mp.snapshot_id AND ms.is_current = 1
             JOIN item_catalog ic ON ic.slug = mp.item_slug
             WHERE ?1 = '' OR instr(ic.search_text, ?1) > 0
             GROUP BY mp.item_slug, mp.platform, mp.rank, mp.subtype,
                      mp.amber_stars, mp.cyan_stars
             ORDER BY
                CASE
                    WHEN lower(mp.item_slug) = ?1 OR lower(ic.display_name_en) = ?1 THEN 0
                    WHEN lower(mp.item_slug) LIKE ?1 || '%' OR lower(ic.display_name_en) LIKE ?1 || '%' THEN 1
                    ELSE 2
                END,
                MAX(CASE WHEN mp.order_type = 'closed' THEN mp.volume ELSE 0 END) DESC,
                ic.display_name_en
             LIMIT ?2",
        )?;
        let mut rows = statement.query(params![
            normalized_query,
            i64::try_from(limit).unwrap_or(i64::MAX)
        ])?;
        let mut bundles = Vec::new();
        while let Some(row) = rows.next()? {
            let rank = optional_u16_from_sql(row.get::<_, Option<i64>>(2)?, "rank")?;
            let amber_stars = optional_u16_from_sql(row.get::<_, Option<i64>>(4)?, "amber_stars")?;
            let cyan_stars = optional_u16_from_sql(row.get::<_, Option<i64>>(5)?, "cyan_stars")?;
            let key = MarketVariantKey::new(
                row.get::<_, String>(0)?,
                parse_platform(&row.get::<_, String>(1)?)?,
                rank,
                row.get::<_, Option<String>>(3)?,
            )
            .map_err(|error| StorageError::Invariant(error.to_string()))?
            .with_stars(amber_stars, cyan_stars);
            let tags: Vec<String> = serde_json::from_str(&row.get::<_, String>(9)?)?;
            let records = self.current_market_records(&key)?;
            bundles.push(MarketVariantBundle {
                key,
                item_id: row.get(6)?,
                display_name_en: row.get(7)?,
                display_name_ru: row.get(8)?,
                tags,
                records,
            });
        }
        Ok(bundles)
    }

    /// Возвращает множество точных вариантов текущего LKG для inventory resolver.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`] при ошибке SQLite или некорректном persisted key.
    pub fn current_market_variant_keys(&self) -> Result<HashSet<MarketVariantKey>, StorageError> {
        let mut statement = self.connection.prepare_cached(
            "SELECT DISTINCT mp.item_slug, mp.platform, mp.rank, mp.subtype,
                    mp.amber_stars, mp.cyan_stars
             FROM market_prices mp
             JOIN market_snapshots ms ON ms.id = mp.snapshot_id AND ms.is_current = 1",
        )?;
        let mut rows = statement.query([])?;
        let mut keys = HashSet::new();
        while let Some(row) = rows.next()? {
            let key = MarketVariantKey::new(
                row.get::<_, String>(0)?,
                parse_platform(&row.get::<_, String>(1)?)?,
                optional_u16_from_sql(row.get::<_, Option<i64>>(2)?, "rank")?,
                row.get::<_, Option<String>>(3)?,
            )
            .map_err(|error| StorageError::Invariant(error.to_string()))?
            .with_stars(
                optional_u16_from_sql(row.get::<_, Option<i64>>(4)?, "amber_stars")?,
                optional_u16_from_sql(row.get::<_, Option<i64>>(5)?, "cyan_stars")?,
            );
            keys.insert(key);
        }
        Ok(keys)
    }

    /// Атомарно публикует полностью resolved inventory snapshot как новый LKG.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`], если нарушены количественные инварианты или transaction
    /// не завершилась. Предыдущий LKG при этом остаётся текущим.
    pub fn promote_inventory_snapshot(
        &mut self,
        snapshot: &ResolvedInventorySnapshot,
    ) -> Result<(), StorageError> {
        validate_inventory_snapshot(snapshot)?;

        let transaction = self.connection.transaction()?;
        transaction.execute(
            "UPDATE inventory_snapshots SET is_current = 0 WHERE is_current = 1",
            [],
        )?;
        transaction.execute(
            "INSERT INTO inventory_snapshots(
                source, observed_at, imported_at, schema_version, item_count,
                resolved_row_count, checksum_sha256, keep_copies, mod_usage_scanned, is_current
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1)",
            params![
                inventory_source_name(snapshot.metadata.source),
                snapshot.metadata.observed_at.to_rfc3339(),
                Utc::now().to_rfc3339(),
                i64::from(snapshot.metadata.schema_version),
                to_i64(snapshot.metadata.item_count, "inventory item_count")?,
                to_i64(snapshot.items.len() as u64, "resolved inventory row_count")?,
                snapshot.metadata.checksum_sha256,
                i64::from(snapshot.keep_copies),
                i64::from(snapshot.mod_usage_scanned),
            ],
        )?;
        let snapshot_id = transaction.last_insert_rowid();
        let mut statement = transaction.prepare_cached(
            "INSERT INTO inventory_items(
                snapshot_id, canonical_game_id, display_name_en, display_name_ru, tags_json,
                item_slug, platform, rank, subtype, owned_quantity, tradeable_quantity,
                untradeable_quantity, unknown_quantity, leveled_quantity,
                equipped_quantity, equipped_tradeable_quantity, sellable_quantity, resolution
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
        )?;
        let mut placement_statement = transaction.prepare_cached(
            "INSERT INTO inventory_mod_placements(
                inventory_item_id, equipment_instance_key, equipment_game_id,
                equipment_display_name_en, equipment_display_name_ru, equipment_image_url,
                equipment_kind, config_index
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
        for item in &snapshot.items {
            statement.execute(params![
                snapshot_id,
                item.canonical_game_id,
                item.display_name_en,
                item.display_name_ru,
                serde_json::to_string(&item.tags)?,
                item.key.as_ref().map(|key| key.slug.as_str()),
                item.key.as_ref().map(|key| platform_name(key.platform)),
                item.rank.map(i64::from),
                item.subtype,
                i64::from(item.owned_quantity),
                i64::from(item.tradeable_quantity),
                i64::from(item.untradeable_quantity),
                i64::from(item.unknown_quantity),
                i64::from(item.leveled_quantity),
                i64::from(item.equipped_quantity),
                i64::from(item.equipped_tradeable_quantity),
                i64::from(item.sellable_quantity),
                inventory_resolution_name(item.resolution),
            ])?;
            let inventory_item_id = transaction.last_insert_rowid();
            for placement in &item.equipped_placements {
                placement_statement.execute(params![
                    inventory_item_id,
                    placement.equipment_instance_key,
                    placement.equipment_game_id,
                    placement.equipment_display_name_en,
                    placement.equipment_display_name_ru,
                    placement.equipment_image_url,
                    equipment_kind_name(placement.equipment_kind),
                    i64::from(placement.config_index),
                ])?;
            }
        }
        drop(placement_statement);
        drop(statement);
        transaction.commit()?;
        Ok(())
    }

    /// Загружает последний полностью опубликованный resolved inventory snapshot.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`] при ошибке SQLite или несовместимом persisted значении.
    pub fn current_inventory_snapshot(
        &self,
    ) -> Result<Option<ResolvedInventorySnapshot>, StorageError> {
        let Some(summary) = load_current_inventory_summary(&self.connection)? else {
            return Ok(None);
        };
        let metadata = inventory_snapshot_metadata(
            &summary.source,
            &summary.observed_at,
            summary.schema_version,
            summary.item_count,
            summary.checksum,
        )?;
        let keep_copies = u32_from_sql(summary.keep_copies, "keep_copies")?;
        let mut statement = self.connection.prepare_cached(
            "SELECT id, canonical_game_id, display_name_en, display_name_ru, tags_json, item_slug,
                    platform, rank, subtype, owned_quantity, tradeable_quantity,
                    untradeable_quantity, unknown_quantity, leveled_quantity,
                    equipped_quantity, equipped_tradeable_quantity, sellable_quantity, resolution
             FROM inventory_items WHERE snapshot_id = ?1
             ORDER BY COALESCE(display_name_en, canonical_game_id), rank, subtype",
        )?;
        let mut rows = statement.query([summary.snapshot_id])?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            let inventory_item_id = row.get::<_, i64>(0)?;
            let tags: Vec<String> = serde_json::from_str(&row.get::<_, String>(4)?)?;
            let slug = row.get::<_, Option<String>>(5)?;
            let platform = row.get::<_, Option<String>>(6)?;
            let rank = optional_u16_from_sql(row.get::<_, Option<i64>>(7)?, "inventory rank")?;
            let subtype = row.get::<_, Option<String>>(8)?;
            let key = match (slug, platform) {
                (Some(slug), Some(platform)) => Some(
                    MarketVariantKey::new(slug, parse_platform(&platform)?, rank, subtype.clone())
                        .map_err(|error| StorageError::Invariant(error.to_string()))?,
                ),
                (None, None) => None,
                _ => {
                    return Err(StorageError::Invariant(
                        "inventory key has incomplete slug/platform".into(),
                    ));
                }
            };
            let placements = load_inventory_mod_placements(&self.connection, inventory_item_id)?;
            items.push(ResolvedInventoryItem {
                canonical_game_id: row.get(1)?,
                display_name_en: row.get(2)?,
                display_name_ru: row.get(3)?,
                tags,
                key,
                rank,
                subtype,
                owned_quantity: u32_from_sql(row.get(9)?, "owned_quantity")?,
                tradeable_quantity: u32_from_sql(row.get(10)?, "tradeable_quantity")?,
                untradeable_quantity: u32_from_sql(row.get(11)?, "untradeable_quantity")?,
                unknown_quantity: u32_from_sql(row.get(12)?, "unknown_quantity")?,
                leveled_quantity: u32_from_sql(row.get(13)?, "leveled_quantity")?,
                equipped_quantity: u32_from_sql(row.get(14)?, "equipped_quantity")?,
                equipped_tradeable_quantity: u32_from_sql(
                    row.get(15)?,
                    "equipped_tradeable_quantity",
                )?,
                equipped_placements: placements,
                sellable_quantity: u32_from_sql(row.get(16)?, "sellable_quantity")?,
                resolution: parse_inventory_resolution(&row.get::<_, String>(17)?)?,
            });
        }
        Ok(Some(ResolvedInventorySnapshot {
            metadata,
            keep_copies,
            mod_usage_scanned: summary.mod_usage_scanned != 0,
            items,
        }))
    }

    /// Фиксирует успешное обращение к provider без raw payload.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`] при ошибке SQLite.
    pub fn record_provider_success(
        &self,
        provider: ProviderId,
        latency_ms: u64,
    ) -> Result<(), StorageError> {
        let now = Utc::now().to_rfc3339();
        self.connection.execute(
            "INSERT INTO source_health(
                provider, last_attempt, last_success, last_error_code,
                last_error_message_redacted, latency_ms, consecutive_failures
             ) VALUES (?1, ?2, ?2, NULL, NULL, ?3, 0)
             ON CONFLICT(provider) DO UPDATE SET
                last_attempt = excluded.last_attempt,
                last_success = excluded.last_success,
                last_error_code = NULL,
                last_error_message_redacted = NULL,
                latency_ms = excluded.latency_ms,
                consecutive_failures = 0",
            params![provider_name(provider), now, to_i64(latency_ms, "latency")?],
        )?;
        Ok(())
    }

    /// Фиксирует безопасную диагностическую причину неуспешного обращения.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`] при ошибке SQLite.
    pub fn record_provider_failure(
        &self,
        provider: ProviderId,
        error_code: &str,
        redacted_message: &str,
        latency_ms: u64,
    ) -> Result<(), StorageError> {
        let message: String = redacted_message.chars().take(500).collect();
        self.connection.execute(
            "INSERT INTO source_health(
                provider, last_attempt, last_success, last_error_code,
                last_error_message_redacted, latency_ms, consecutive_failures
             ) VALUES (?1, ?2, NULL, ?3, ?4, ?5, 1)
             ON CONFLICT(provider) DO UPDATE SET
                last_attempt = excluded.last_attempt,
                last_error_code = excluded.last_error_code,
                last_error_message_redacted = excluded.last_error_message_redacted,
                latency_ms = excluded.latency_ms,
                consecutive_failures = source_health.consecutive_failures + 1",
            params![
                provider_name(provider),
                Utc::now().to_rfc3339(),
                error_code,
                message,
                to_i64(latency_ms, "latency")?,
            ],
        )?;
        Ok(())
    }

    /// Возвращает сохранённое состояние внешних источников без сетевых обращений.
    ///
    /// # Errors
    ///
    /// Возвращает [`StorageError`] при ошибке SQLite или повреждённых диагностических полях.
    pub fn provider_health(&self) -> Result<Vec<ProviderHealth>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT provider, last_attempt, last_success, last_error_code,
                    last_error_message_redacted, latency_ms, consecutive_failures
             FROM source_health
             ORDER BY provider",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<i64>>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })?;

        let mut health = Vec::new();
        for row in rows {
            let (
                provider,
                last_attempt,
                last_success,
                error_code,
                error_message,
                latency,
                failures,
            ) = row?;
            health.push(ProviderHealth {
                provider: parse_provider(&provider)?,
                last_attempt: parse_optional_datetime(last_attempt)?,
                last_success: parse_optional_datetime(last_success)?,
                last_error_code: error_code,
                last_error_message: error_message,
                latency_ms: latency
                    .map(|value| to_u64(value, "provider latency"))
                    .transpose()?,
                consecutive_failures: u32_from_sql(failures, "consecutive_failures")?,
            });
        }
        Ok(health)
    }

    fn configure(&self) -> Result<(), StorageError> {
        self.connection.pragma_update(None, "foreign_keys", true)?;
        self.connection.busy_timeout(Duration::from_secs(5))?;
        if self.path.is_some() {
            self.connection.pragma_update(None, "journal_mode", "WAL")?;
        }
        Ok(())
    }

    fn migrate(&self) -> Result<(), StorageError> {
        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        let migration_result = self
            .connection
            .execute_batch(FOUNDATION_MIGRATION)
            .and_then(|()| {
                let version = self.connection.query_row(
                    "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                    [],
                    |row| row.get::<_, i64>(0),
                )?;
                if version < 2 {
                    self.connection.execute_batch(BULK_INGESTION_MIGRATION)?;
                }
                if version < 3 {
                    self.connection
                        .execute_batch(SCULPTURE_VARIANTS_MIGRATION)?;
                }
                if version < 4 {
                    self.connection.execute_batch(MARKET_SEARCH_MIGRATION)?;
                }
                if version < 5 {
                    self.connection.execute_batch(MARKET_HISTORY_MIGRATION)?;
                }
                if version < 6 {
                    self.connection.execute_batch(INVENTORY_MIGRATION)?;
                }
                if version < 7 {
                    self.connection.execute_batch(GAME_METADATA_MIGRATION)?;
                }
                if version < 8 {
                    self.connection
                        .execute_batch(RIVEN_DISPOSITIONS_MIGRATION)?;
                }
                if version < 9 {
                    self.connection
                        .execute_batch(GAME_ITEM_DEFINITIONS_MIGRATION)?;
                }
                if version < 10 {
                    self.connection.execute_batch(TRADE_SHIFT_MIGRATION)?;
                }
                if version < 11 {
                    self.connection.execute_batch(EQUIPPED_MODS_MIGRATION)?;
                }
                Ok(())
            });
        if let Err(error) = migration_result {
            let _ = self.connection.execute_batch("ROLLBACK");
            return Err(StorageError::Sqlite(error));
        }
        self.connection.execute_batch("COMMIT")?;
        Ok(())
    }
}

struct StoredInventorySummary {
    snapshot_id: i64,
    source: String,
    observed_at: String,
    schema_version: i64,
    item_count: i64,
    checksum: String,
    keep_copies: i64,
    mod_usage_scanned: i64,
}

fn load_current_inventory_summary(
    connection: &Connection,
) -> Result<Option<StoredInventorySummary>, StorageError> {
    connection
        .query_row(
            "SELECT id, source, observed_at, schema_version, item_count,
                    checksum_sha256, keep_copies, mod_usage_scanned
             FROM inventory_snapshots WHERE is_current = 1",
            [],
            |row| {
                Ok(StoredInventorySummary {
                    snapshot_id: row.get(0)?,
                    source: row.get(1)?,
                    observed_at: row.get(2)?,
                    schema_version: row.get(3)?,
                    item_count: row.get(4)?,
                    checksum: row.get(5)?,
                    keep_copies: row.get(6)?,
                    mod_usage_scanned: row.get(7)?,
                })
            },
        )
        .optional()
        .map_err(StorageError::Sqlite)
}

fn validate_inventory_snapshot(snapshot: &ResolvedInventorySnapshot) -> Result<(), StorageError> {
    if snapshot.items.len() as u64 > snapshot.metadata.item_count {
        return Err(StorageError::Invariant(
            "resolved inventory rows exceed source item_count".into(),
        ));
    }
    for item in &snapshot.items {
        let classified = item
            .tradeable_quantity
            .saturating_add(item.untradeable_quantity)
            .saturating_add(item.unknown_quantity);
        if item.owned_quantity == 0
            || classified != item.owned_quantity
            || item.leveled_quantity > item.owned_quantity
            || item.sellable_quantity > item.tradeable_quantity
            || item.equipped_quantity > item.owned_quantity
            || item.equipped_tradeable_quantity > item.equipped_quantity
            || item.equipped_tradeable_quantity > item.tradeable_quantity
        {
            return Err(StorageError::Invariant(format!(
                "invalid inventory quantities for {}",
                item.canonical_game_id
            )));
        }
    }
    Ok(())
}

fn inventory_snapshot_metadata(
    source: &str,
    observed_at: &str,
    schema_version: i64,
    item_count: i64,
    checksum_sha256: String,
) -> Result<InventorySnapshotMetadata, StorageError> {
    Ok(InventorySnapshotMetadata {
        source: parse_inventory_source(source)?,
        observed_at: DateTime::parse_from_rfc3339(observed_at)?.with_timezone(&Utc),
        schema_version: u32::try_from(schema_version)
            .map_err(|_| StorageError::Invariant("invalid inventory schema_version".into()))?,
        item_count: u64::try_from(item_count)
            .map_err(|_| StorageError::Invariant("invalid inventory item_count".into()))?,
        checksum_sha256,
    })
}

fn load_inventory_mod_placements(
    connection: &Connection,
    inventory_item_id: i64,
) -> Result<Vec<ResolvedModPlacement>, StorageError> {
    let mut statement = connection.prepare_cached(
        "SELECT equipment_instance_key, equipment_game_id, equipment_display_name_en,
                equipment_display_name_ru, equipment_image_url, equipment_kind, config_index
         FROM inventory_mod_placements
         WHERE inventory_item_id = ?1
         ORDER BY COALESCE(equipment_display_name_ru, equipment_display_name_en, equipment_game_id),
                  equipment_instance_key, config_index",
    )?;
    statement
        .query_map([inventory_item_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })?
        .map(|placement| {
            let (instance_key, game_id, name_en, name_ru, image_url, kind, config) = placement?;
            Ok(ResolvedModPlacement {
                equipment_instance_key: instance_key,
                equipment_game_id: game_id,
                equipment_display_name_en: name_en,
                equipment_display_name_ru: name_ru,
                equipment_image_url: image_url,
                equipment_kind: parse_equipment_kind(&kind)?,
                config_index: u16::try_from(config).map_err(|_| {
                    StorageError::Invariant("invalid equipment config_index".into())
                })?,
            })
        })
        .collect()
}

#[derive(Default)]
struct HistoryAggregate {
    closed_median: Option<f64>,
    closed_volume: f64,
    sell_median: Option<f64>,
    buy_median: Option<f64>,
}

fn aggregate_history(
    snapshot: &NormalizedMarketSnapshot,
) -> Result<HashMap<MarketVariantKey, HistoryAggregate>, StorageError> {
    let mut aggregates: HashMap<MarketVariantKey, HistoryAggregate> = HashMap::new();
    for record in &snapshot.records {
        record
            .validate()
            .map_err(|error| StorageError::Invariant(error.to_string()))?;
        let aggregate = aggregates.entry(record.key.clone()).or_default();
        match record.order_type {
            MarketOrderType::Closed => {
                aggregate.closed_median = record.median;
                aggregate.closed_volume = record.volume;
            }
            MarketOrderType::Sell => aggregate.sell_median = record.median,
            MarketOrderType::Buy => aggregate.buy_median = record.median,
        }
    }
    Ok(aggregates)
}

fn store_history_snapshot(
    transaction: &Transaction<'_>,
    snapshot: &NormalizedMarketSnapshot,
    aggregates: &HashMap<MarketVariantKey, HistoryAggregate>,
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO market_history_snapshots(
            provider, source_date, fetched_at, imported_at, schema_version,
            item_count, record_count, checksum_sha256
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(source_date) DO UPDATE SET
            provider = excluded.provider,
            fetched_at = excluded.fetched_at,
            imported_at = excluded.imported_at,
            schema_version = excluded.schema_version,
            item_count = excluded.item_count,
            record_count = excluded.record_count,
            checksum_sha256 = excluded.checksum_sha256",
        params![
            provider_name(snapshot.metadata.provider),
            snapshot.metadata.source_date.to_string(),
            snapshot.metadata.fetched_at.to_rfc3339(),
            Utc::now().to_rfc3339(),
            i64::from(snapshot.metadata.schema_version),
            to_i64(snapshot.metadata.item_count, "history item_count")?,
            to_i64(snapshot.metadata.record_count, "history record_count")?,
            snapshot.metadata.checksum_sha256,
        ],
    )?;
    let history_snapshot_id = transaction.query_row(
        "SELECT id FROM market_history_snapshots WHERE source_date = ?1",
        [snapshot.metadata.source_date.to_string()],
        |row| row.get::<_, i64>(0),
    )?;
    transaction.execute(
        "DELETE FROM market_history WHERE source_date = ?1",
        [snapshot.metadata.source_date.to_string()],
    )?;
    let mut statement = transaction.prepare_cached(
        "INSERT INTO market_history(
            snapshot_id, source_date, item_slug, platform, rank, subtype,
            amber_stars, cyan_stars, closed_median, closed_volume,
            sell_median, buy_median
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
    )?;
    for (key, aggregate) in aggregates {
        statement.execute(params![
            history_snapshot_id,
            snapshot.metadata.source_date.to_string(),
            key.slug,
            platform_name(key.platform),
            key.rank.map(i64::from),
            key.subtype,
            key.amber_stars.map(i64::from),
            key.cyan_stars.map(i64::from),
            aggregate.closed_median,
            aggregate.closed_volume,
            aggregate.sell_median,
            aggregate.buy_median,
        ])?;
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("settings serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("invalid persisted date/time: {0}")]
    DateTime(#[from] chrono::ParseError),
    #[error("storage invariant failed: {0}")]
    Invariant(String),
}

const fn trade_event_status_name(status: TradeEventStatus) -> &'static str {
    match status {
        TradeEventStatus::Pending => "pending",
        TradeEventStatus::Reconciled => "reconciled",
        TradeEventStatus::Ignored => "ignored",
    }
}

fn trade_event_status(value: &str) -> Result<TradeEventStatus, StorageError> {
    match value {
        "pending" => Ok(TradeEventStatus::Pending),
        "reconciled" => Ok(TradeEventStatus::Reconciled),
        "ignored" => Ok(TradeEventStatus::Ignored),
        _ => Err(StorageError::Invariant(format!(
            "unknown trade event status: {value}"
        ))),
    }
}

fn provider_name(provider: ProviderId) -> &'static str {
    match provider {
        ProviderId::RelicsRun => "relics_run",
        ProviderId::FrameForgeMirror => "frameforge_mirror",
        ProviderId::WarframeMarket => "warframe_market",
        ProviderId::LocalCache => "local_cache",
        ProviderId::Import => "import",
    }
}

const fn game_metadata_source_name(source: GameMetadataSource) -> &'static str {
    match source {
        GameMetadataSource::WfcdWarframeItems => "wfcd_warframe_items",
    }
}

fn search_text(item: &platscope_domain::CatalogItem) -> String {
    format!(
        "{} {} {}",
        item.slug.to_lowercase(),
        item.display_name_en.to_lowercase(),
        item.display_name_ru
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
    )
}

fn parse_provider(value: &str) -> Result<ProviderId, StorageError> {
    match value {
        "relics_run" => Ok(ProviderId::RelicsRun),
        "frameforge_mirror" => Ok(ProviderId::FrameForgeMirror),
        "warframe_market" => Ok(ProviderId::WarframeMarket),
        "local_cache" => Ok(ProviderId::LocalCache),
        "import" => Ok(ProviderId::Import),
        _ => Err(StorageError::Invariant(format!("unknown provider {value}"))),
    }
}

fn inventory_source_name(source: InventorySource) -> &'static str {
    match source {
        InventorySource::PlatscopeJson => "platscope_json",
        InventorySource::HelperImport => "helper_import",
        InventorySource::OverwolfCompanion => "overwolf_companion",
        InventorySource::TestFixture => "test_fixture",
        InventorySource::ReadOnlyScan => "read_only_scan",
    }
}

fn parse_inventory_source(value: &str) -> Result<InventorySource, StorageError> {
    match value {
        "platscope_json" => Ok(InventorySource::PlatscopeJson),
        "helper_import" => Ok(InventorySource::HelperImport),
        "overwolf_companion" => Ok(InventorySource::OverwolfCompanion),
        "test_fixture" => Ok(InventorySource::TestFixture),
        "read_only_scan" => Ok(InventorySource::ReadOnlyScan),
        _ => Err(StorageError::Invariant(format!(
            "unknown inventory source {value}"
        ))),
    }
}

fn inventory_resolution_name(resolution: InventoryResolution) -> &'static str {
    match resolution {
        InventoryResolution::Resolved => "resolved",
        InventoryResolution::UnknownItem => "unknown_item",
        InventoryResolution::AmbiguousItem => "ambiguous_item",
        InventoryResolution::ExactVariantUnavailable => "exact_variant_unavailable",
    }
}

fn parse_inventory_resolution(value: &str) -> Result<InventoryResolution, StorageError> {
    match value {
        "resolved" => Ok(InventoryResolution::Resolved),
        "unknown_item" => Ok(InventoryResolution::UnknownItem),
        "ambiguous_item" => Ok(InventoryResolution::AmbiguousItem),
        "exact_variant_unavailable" => Ok(InventoryResolution::ExactVariantUnavailable),
        _ => Err(StorageError::Invariant(format!(
            "unknown inventory resolution {value}"
        ))),
    }
}

fn equipment_kind_name(kind: EquipmentKind) -> &'static str {
    match kind {
        EquipmentKind::Warframe => "warframe",
        EquipmentKind::Primary => "primary",
        EquipmentKind::Secondary => "secondary",
        EquipmentKind::Melee => "melee",
        EquipmentKind::Companion => "companion",
        EquipmentKind::CompanionWeapon => "companion_weapon",
        EquipmentKind::Archwing => "archwing",
        EquipmentKind::Archgun => "archgun",
        EquipmentKind::Archmelee => "archmelee",
        EquipmentKind::Necramech => "necramech",
        EquipmentKind::Amp => "amp",
        EquipmentKind::Other => "other",
    }
}

fn parse_equipment_kind(value: &str) -> Result<EquipmentKind, StorageError> {
    match value {
        "warframe" => Ok(EquipmentKind::Warframe),
        "primary" => Ok(EquipmentKind::Primary),
        "secondary" => Ok(EquipmentKind::Secondary),
        "melee" => Ok(EquipmentKind::Melee),
        "companion" => Ok(EquipmentKind::Companion),
        "companion_weapon" => Ok(EquipmentKind::CompanionWeapon),
        "archwing" => Ok(EquipmentKind::Archwing),
        "archgun" => Ok(EquipmentKind::Archgun),
        "archmelee" => Ok(EquipmentKind::Archmelee),
        "necramech" => Ok(EquipmentKind::Necramech),
        "amp" => Ok(EquipmentKind::Amp),
        "other" => Ok(EquipmentKind::Other),
        _ => Err(StorageError::Invariant(format!(
            "unknown equipment kind {value}"
        ))),
    }
}

fn platform_name(platform: Platform) -> &'static str {
    match platform {
        Platform::Pc => "pc",
        Platform::Playstation => "playstation",
        Platform::Xbox => "xbox",
        Platform::Switch => "switch",
        Platform::Mobile => "mobile",
    }
}

fn parse_platform(value: &str) -> Result<Platform, StorageError> {
    match value {
        "pc" => Ok(Platform::Pc),
        "playstation" => Ok(Platform::Playstation),
        "xbox" => Ok(Platform::Xbox),
        "switch" => Ok(Platform::Switch),
        "mobile" => Ok(Platform::Mobile),
        _ => Err(StorageError::Invariant(format!("unknown platform {value}"))),
    }
}

fn order_type_name(order_type: MarketOrderType) -> &'static str {
    match order_type {
        MarketOrderType::Closed => "closed",
        MarketOrderType::Buy => "buy",
        MarketOrderType::Sell => "sell",
    }
}

fn parse_order_type(value: &str) -> Result<MarketOrderType, StorageError> {
    match value {
        "closed" => Ok(MarketOrderType::Closed),
        "buy" => Ok(MarketOrderType::Buy),
        "sell" => Ok(MarketOrderType::Sell),
        _ => Err(StorageError::Invariant(format!(
            "unknown market order type {value}"
        ))),
    }
}

fn optional_u16_from_sql(value: Option<i64>, field: &str) -> Result<Option<u16>, StorageError> {
    value
        .map(|value| {
            u16::try_from(value)
                .map_err(|_| StorageError::Invariant(format!("invalid {field} value {value}")))
        })
        .transpose()
}

fn parse_optional_datetime(value: Option<String>) -> Result<Option<DateTime<Utc>>, StorageError> {
    value
        .map(|value| DateTime::parse_from_rfc3339(&value).map(|date| date.with_timezone(&Utc)))
        .transpose()
        .map_err(StorageError::from)
}

fn u32_from_sql(value: i64, field: &str) -> Result<u32, StorageError> {
    u32::try_from(value)
        .map_err(|_| StorageError::Invariant(format!("invalid {field} value {value}")))
}

fn u8_from_sql(value: i64, field: &str) -> Result<u8, StorageError> {
    u8::try_from(value).map_err(|_| StorageError::Invariant(format!("{field} is outside u8 range")))
}

fn to_i64(value: u64, field: &str) -> Result<i64, StorageError> {
    i64::try_from(value)
        .map_err(|_| StorageError::Invariant(format!("{field} does not fit SQLite integer")))
}

fn to_u64(value: i64, field: &str) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| StorageError::Invariant(format!("{field} is negative")))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use platscope_domain::{
        CatalogItem, CatalogMetadata, GameItemDefinition, GameMetadataSnapshot,
        GameMetadataSnapshotMetadata, GameMetadataSource, MarketRecord, MarketVariantKey,
        PrimePartMetadata, PrimeSetDefinition, RelicDefinition, RelicRefinement, SnapshotMetadata,
        VaultStatus,
    };
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct FixtureSetting {
        language: String,
        refresh_hours: u8,
    }

    #[test]
    fn foundation_migration_is_idempotent() {
        let database = Database::open_in_memory().expect("database opens");
        database.migrate().expect("migration can run twice");
        assert_eq!(database.schema_version().expect("version"), 11);
    }

    #[test]
    fn market_search_migration_backfills_existing_catalog() {
        let connection = Connection::open_in_memory().expect("connection opens");
        connection
            .execute_batch(FOUNDATION_MIGRATION)
            .expect("foundation migration applies");
        connection
            .execute_batch(BULK_INGESTION_MIGRATION)
            .expect("bulk migration applies");
        connection
            .execute_batch(SCULPTURE_VARIANTS_MIGRATION)
            .expect("variant migration applies");
        connection
            .execute(
                "INSERT INTO item_catalog(
                    item_id, slug, display_name_en, subtypes_json, tags_json,
                    catalog_source, catalog_fetched_at
                 ) VALUES ('old-item', 'nyx_prime_set', 'Nyx Prime Set', '[]', '[]',
                           'relics_run', '2026-08-26T00:00:00Z')",
                [],
            )
            .expect("legacy catalog row inserts");

        let database = Database {
            connection,
            path: None,
        };
        database.migrate().expect("search migration applies");

        let (display_name_ru, search_text): (Option<String>, String) = database
            .connection
            .query_row(
                "SELECT display_name_ru, search_text
                 FROM item_catalog
                 WHERE item_id = 'old-item'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("migrated row loads");
        assert_eq!(display_name_ru, None);
        assert_eq!(search_text, "nyx_prime_set nyx prime set");
        assert_eq!(database.schema_version().expect("version"), 11);
    }

    #[test]
    fn settings_round_trip_as_json() {
        let database = Database::open_in_memory().expect("database opens");
        let expected = FixtureSetting {
            language: "ru".into(),
            refresh_hours: 4,
        };

        database
            .set_setting("app", &expected)
            .expect("setting saved");
        let actual: FixtureSetting = database
            .get_setting("app")
            .expect("setting loads")
            .expect("setting exists");

        assert_eq!(actual, expected);
    }

    #[test]
    fn confirmed_trade_events_are_deduplicated_and_reconciled() {
        let database = Database::open_in_memory().expect("database opens");
        let event = NewTradeEvent {
            fingerprint: "session:123.4:primed-flow".into(),
            occurred_at: Utc.with_ymd_and_hms(2026, 8, 29, 12, 30, 0).unwrap(),
            partner: Some("MarketTenno".into()),
            platinum_given: 0,
            platinum_received: 130,
            given_items: vec![TradeItem {
                name: "Primed Flow".into(),
                quantity: 1,
            }],
            received_items: Vec::new(),
        };
        assert!(database.record_trade_event(&event).expect("event inserts"));
        assert!(
            !database
                .record_trade_event(&event)
                .expect("duplicate ignored")
        );

        let events = database.recent_trade_events(10).expect("events load");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].status, TradeEventStatus::Pending);
        assert_eq!(events[0].platinum_received, 130);
        assert!(
            database
                .set_trade_event_status(
                    events[0].id,
                    TradeEventStatus::Reconciled,
                    Some("order-1"),
                    Some("[]"),
                )
                .expect("status updates")
        );
        let reconciled = database.recent_trade_events(1).expect("event reloads");
        assert_eq!(reconciled[0].status, TradeEventStatus::Reconciled);
        assert_eq!(reconciled[0].matched_order_id.as_deref(), Some("order-1"));
    }

    #[test]
    fn provider_health_preserves_success_and_redacted_failure() {
        let database = Database::open_in_memory().expect("database opens");
        database
            .record_provider_success(ProviderId::RelicsRun, 42)
            .expect("success is recorded");
        database
            .record_provider_failure(
                ProviderId::WarframeMarket,
                "HttpStatus",
                "безопасное диагностическое сообщение",
                175,
            )
            .expect("failure is recorded");

        let health = database.provider_health().expect("health loads");
        assert_eq!(health.len(), 2);
        let success = health
            .iter()
            .find(|row| row.provider == ProviderId::RelicsRun)
            .expect("relics.run row");
        assert!(success.last_success.is_some());
        assert_eq!(success.latency_ms, Some(42));
        assert_eq!(success.consecutive_failures, 0);
        assert!(success.last_error_code.is_none());

        let failure = health
            .iter()
            .find(|row| row.provider == ProviderId::WarframeMarket)
            .expect("WFM row");
        assert!(failure.last_success.is_none());
        assert_eq!(failure.last_error_code.as_deref(), Some("HttpStatus"));
        assert_eq!(
            failure.last_error_message.as_deref(),
            Some("безопасное диагностическое сообщение")
        );
        assert_eq!(failure.consecutive_failures, 1);
    }

    #[test]
    fn missing_setting_is_not_an_error() {
        let database = Database::open_in_memory().expect("database opens");
        let missing = database
            .get_setting::<FixtureSetting>("missing")
            .expect("query succeeds");
        assert_eq!(missing, None);
    }

    fn fixture_catalog() -> ItemCatalog {
        ItemCatalog {
            metadata: CatalogMetadata {
                provider: ProviderId::RelicsRun,
                fetched_at: Utc.with_ymd_and_hms(2026, 8, 27, 0, 0, 0).unwrap(),
                schema_version: 1,
                item_count: 1,
                checksum_sha256: "catalog-checksum".into(),
            },
            items: vec![CatalogItem {
                item_id: "item-id".into(),
                slug: "test_item".into(),
                display_name_en: "Test Item".into(),
                display_name_ru: Some("Тестовый предмет".into()),
                thumb: None,
                thumb_ru: None,
                game_ref: None,
                bulk_tradable: false,
                max_rank: None,
                subtypes: Vec::new(),
                tags: vec!["test".into()],
            }],
        }
    }

    fn fixture_snapshot(slug: &str, checksum: &str) -> NormalizedMarketSnapshot {
        let observed_at = Utc.with_ymd_and_hms(2026, 8, 26, 0, 0, 0).unwrap();
        NormalizedMarketSnapshot {
            metadata: SnapshotMetadata {
                provider: ProviderId::RelicsRun,
                source_date: observed_at.date_naive(),
                fetched_at: Utc.with_ymd_and_hms(2026, 8, 27, 0, 0, 0).unwrap(),
                schema_version: 1,
                item_count: 1,
                record_count: 1,
                checksum_sha256: checksum.into(),
            },
            records: vec![MarketRecord {
                key: MarketVariantKey::new(slug, Platform::Pc, None, None::<String>)
                    .expect("fixture key"),
                external_item_id: "item-id".into(),
                display_name_en: "Test Item".into(),
                observed_at,
                order_type: MarketOrderType::Closed,
                median: Some(10.0),
                average: Some(10.0),
                min_price: Some(9.0),
                max_price: Some(11.0),
                volume: 3.0,
                raw_json: "{}".into(),
            }],
        }
    }

    #[test]
    fn failed_price_import_keeps_previous_current_snapshot() {
        let mut database = Database::open_in_memory().expect("database opens");
        database
            .promote_catalog(&fixture_catalog())
            .expect("catalog promoted");
        database
            .promote_market_snapshot(&fixture_snapshot("test_item", "first"))
            .expect("first snapshot promoted");

        let error = database
            .promote_market_snapshot(&fixture_snapshot("unknown_item", "second"))
            .expect_err("foreign key must reject incomplete import");
        assert!(matches!(error, StorageError::Sqlite(_)));

        let current = database
            .current_market_snapshot()
            .expect("current query")
            .expect("current exists");
        assert_eq!(current.checksum_sha256, "first");
    }

    #[test]
    fn current_records_require_exact_variant() {
        let mut database = Database::open_in_memory().expect("database opens");
        database
            .promote_catalog(&fixture_catalog())
            .expect("catalog promoted");
        database
            .promote_market_snapshot(&fixture_snapshot("test_item", "first"))
            .expect("snapshot promoted");

        let exact_key = MarketVariantKey::new("test_item", Platform::Pc, None, None::<String>)
            .expect("exact key");
        assert_eq!(
            database
                .current_market_records(&exact_key)
                .expect("exact query")
                .len(),
            1
        );

        let wrong_rank = MarketVariantKey::new("test_item", Platform::Pc, Some(10), None::<String>)
            .expect("ranked key");
        assert!(
            database
                .current_market_records(&wrong_rank)
                .expect("rank query")
                .is_empty()
        );

        let russian = database
            .search_current_market_variants("тестовый", 10)
            .expect("localized search");
        assert_eq!(russian.len(), 1);
        assert_eq!(russian[0].key, exact_key);

        let missing = database
            .search_current_market_variants("несуществующий", 10)
            .expect("empty search");
        assert!(missing.is_empty());
    }

    #[test]
    fn local_search_stays_bounded_with_thousands_of_items() {
        const ITEM_COUNT: u64 = 4_000;
        const RESULT_LIMIT: usize = 61;

        let fetched_at = Utc.with_ymd_and_hms(2026, 8, 27, 0, 0, 0).unwrap();
        let observed_at = Utc.with_ymd_and_hms(2026, 8, 26, 0, 0, 0).unwrap();
        let items = (0..ITEM_COUNT)
            .map(|index| {
                let slug = format!("perf_item_{index:04}");
                CatalogItem {
                    item_id: format!("perf-id-{index:04}"),
                    slug,
                    display_name_en: if index % 10 == 0 {
                        format!("Needle Item {index:04}")
                    } else {
                        format!("Catalog Item {index:04}")
                    },
                    display_name_ru: None,
                    thumb: None,
                    thumb_ru: None,
                    game_ref: None,
                    bulk_tradable: false,
                    max_rank: None,
                    subtypes: Vec::new(),
                    tags: vec!["performance".into()],
                }
            })
            .collect::<Vec<_>>();
        let records = items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let price_step =
                    f64::from(u32::try_from(index % 100).expect("price step fits fixture bounds"));
                let volume =
                    f64::from(u32::try_from(index % 50).expect("volume fits fixture bounds")) + 1.0;
                MarketRecord {
                    key: MarketVariantKey::new(&item.slug, Platform::Pc, None, None::<String>)
                        .expect("performance fixture key"),
                    external_item_id: item.item_id.clone(),
                    display_name_en: item.display_name_en.clone(),
                    observed_at,
                    order_type: MarketOrderType::Closed,
                    median: Some(10.0 + price_step),
                    average: Some(10.0 + price_step),
                    min_price: Some(9.0),
                    max_price: Some(110.0),
                    volume,
                    raw_json: "{}".into(),
                }
            })
            .collect::<Vec<_>>();
        let catalog = ItemCatalog {
            metadata: CatalogMetadata {
                provider: ProviderId::RelicsRun,
                fetched_at,
                schema_version: 1,
                item_count: ITEM_COUNT,
                checksum_sha256: "performance-catalog".into(),
            },
            items,
        };
        let snapshot = NormalizedMarketSnapshot {
            metadata: SnapshotMetadata {
                provider: ProviderId::RelicsRun,
                source_date: observed_at.date_naive(),
                fetched_at,
                schema_version: 1,
                item_count: ITEM_COUNT,
                record_count: ITEM_COUNT,
                checksum_sha256: "performance-snapshot".into(),
            },
            records,
        };

        let mut database = Database::open_in_memory().expect("database opens");
        database
            .promote_catalog(&catalog)
            .expect("large catalog promoted");
        database
            .promote_market_snapshot(&snapshot)
            .expect("large snapshot promoted");

        let started = std::time::Instant::now();
        let results = database
            .search_current_market_variants("needle", RESULT_LIMIT)
            .expect("bounded local search succeeds");
        let elapsed = started.elapsed();

        eprintln!(
            "performance_search items={ITEM_COUNT} rows={} elapsed_ms={}",
            results.len(),
            elapsed.as_millis()
        );
        assert_eq!(results.len(), RESULT_LIMIT);
        assert!(
            elapsed < std::time::Duration::from_millis(250),
            "4k-item local search took {elapsed:?}"
        );
    }

    #[test]
    fn current_and_historical_snapshots_build_compact_history() {
        let mut database = Database::open_in_memory().expect("database opens");
        database
            .promote_catalog(&fixture_catalog())
            .expect("catalog promoted");
        database
            .promote_market_snapshot(&fixture_snapshot("test_item", "current"))
            .expect("current snapshot promoted");

        let mut older = fixture_snapshot("test_item", "older");
        older.metadata.source_date = NaiveDate::from_ymd_opt(2026, 8, 25).expect("date");
        older.records[0].observed_at = Utc.with_ymd_and_hms(2026, 8, 25, 0, 0, 0).unwrap();
        older.records[0].median = Some(8.0);
        older.records[0].volume = 6.0;
        database
            .promote_history_snapshot(&older)
            .expect("historical day promoted");

        let key =
            MarketVariantKey::new("test_item", Platform::Pc, None, None::<String>).expect("key");
        let history = database
            .market_history(&key, 7, NaiveDate::from_ymd_opt(2026, 8, 26).expect("date"))
            .expect("history loads");
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].closed_median, Some(8.0));
        assert_eq!(history[1].closed_median, Some(10.0));

        let coverage = database.history_coverage().expect("coverage loads");
        assert_eq!(coverage.day_count, 2);
        assert_eq!(coverage.oldest_date, NaiveDate::from_ymd_opt(2026, 8, 25));
        assert!(
            database
                .has_history_date(older.metadata.source_date)
                .expect("date query")
        );
    }

    #[test]
    fn invalid_inventory_snapshot_keeps_previous_lkg() {
        let mut database = Database::open_in_memory().expect("database opens");
        let key =
            MarketVariantKey::new("test_item", Platform::Pc, None, None::<String>).expect("key");
        let snapshot = ResolvedInventorySnapshot {
            metadata: InventorySnapshotMetadata {
                source: InventorySource::TestFixture,
                observed_at: Utc.with_ymd_and_hms(2026, 8, 27, 0, 0, 0).unwrap(),
                schema_version: 1,
                item_count: 1,
                checksum_sha256: "first".into(),
            },
            keep_copies: 1,
            mod_usage_scanned: true,
            items: vec![ResolvedInventoryItem {
                canonical_game_id: "test_item".into(),
                display_name_en: Some("Test Item".into()),
                display_name_ru: Some("Тестовый предмет".into()),
                tags: vec!["component".into()],
                key: Some(key),
                rank: None,
                subtype: None,
                owned_quantity: 2,
                tradeable_quantity: 2,
                untradeable_quantity: 0,
                unknown_quantity: 0,
                leveled_quantity: 0,
                equipped_quantity: 1,
                equipped_tradeable_quantity: 1,
                equipped_placements: vec![ResolvedModPlacement {
                    equipment_instance_key: "equipment-hash".into(),
                    equipment_game_id: "/Lotus/Test/VoltPrime".into(),
                    equipment_display_name_en: Some("Volt Prime".into()),
                    equipment_display_name_ru: Some("Вольт Прайм".into()),
                    equipment_image_url: Some("https://example.invalid/volt.png".into()),
                    equipment_kind: EquipmentKind::Warframe,
                    config_index: 1,
                }],
                sellable_quantity: 1,
                resolution: InventoryResolution::Resolved,
            }],
        };
        database
            .promote_inventory_snapshot(&snapshot)
            .expect("inventory promoted");

        let mut invalid = snapshot.clone();
        invalid.metadata.checksum_sha256 = "invalid".into();
        invalid.items[0].owned_quantity = 3;
        database
            .promote_inventory_snapshot(&invalid)
            .expect_err("inconsistent quantities fail");

        let current = database
            .current_inventory_snapshot()
            .expect("inventory loads")
            .expect("inventory exists");
        assert_eq!(current.metadata.checksum_sha256, "first");
        assert_eq!(current.items[0].sellable_quantity, 1);
        assert!(current.mod_usage_scanned);
        assert_eq!(current.items[0].equipped_quantity, 1);
        assert_eq!(current.items[0].equipped_placements.len(), 1);
        assert_eq!(current.items[0].equipped_placements[0].config_index, 1);
    }

    fn fixture_game_metadata(checksum: &str) -> GameMetadataSnapshot {
        GameMetadataSnapshot {
            metadata: GameMetadataSnapshotMetadata {
                source: GameMetadataSource::WfcdWarframeItems,
                fetched_at: Utc.with_ymd_and_hms(2026, 8, 27, 0, 0, 0).unwrap(),
                schema_version: 1,
                set_count: 1,
                relic_count: 1,
                prime_part_count: 1,
                riven_disposition_count: 0,
                item_definition_count: 1,
                checksum_sha256: checksum.into(),
            },
            prime_sets: vec![PrimeSetDefinition {
                set_slug: "test_prime_set".into(),
                set_game_ref: "test-set-ref".into(),
                display_name_en: "Test Prime Set".into(),
                vault_status: VaultStatus::Unknown,
                components: Vec::new(),
            }],
            relics: vec![RelicDefinition {
                relic_slug: "axi_t1_relic".into(),
                relic_game_ref: "test-relic-ref".into(),
                display_name_en: "Axi T1 Relic".into(),
                refinement: RelicRefinement::Intact,
                vault_status: VaultStatus::Available,
                rewards: Vec::new(),
            }],
            prime_parts: vec![PrimePartMetadata {
                slug: "test_prime_part".into(),
                game_ref: "test-part-ref".into(),
                ducats: 15,
                vault_status: VaultStatus::Unknown,
            }],
            riven_dispositions: Vec::new(),
            item_definitions: vec![GameItemDefinition {
                slug: "test_prime_set".into(),
                game_ref: "test-set-ref".into(),
                mastery_requirement: 8,
            }],
        }
    }

    #[test]
    fn invalid_game_metadata_keeps_previous_lkg() {
        let mut database = Database::open_in_memory().expect("database opens");
        database
            .promote_game_metadata(&fixture_game_metadata("first"))
            .expect("metadata promoted");

        let mut invalid = fixture_game_metadata("invalid");
        invalid.metadata.set_count = 2;
        database
            .promote_game_metadata(&invalid)
            .expect_err("inconsistent counts fail");

        let current = database
            .load_current_game_metadata()
            .expect("metadata loads")
            .expect("metadata exists");
        assert_eq!(current.metadata.checksum_sha256, "first");
        assert_eq!(current.prime_sets.len(), 1);
        assert_eq!(
            database
                .current_mastery_requirements()
                .expect("mastery projection loads")
                .get("test_prime_set"),
            Some(&8)
        );
    }

    #[test]
    fn legacy_game_metadata_defaults_missing_riven_fields() {
        let mut database = Database::open_in_memory().expect("database opens");
        let snapshot = fixture_game_metadata("legacy");
        database
            .promote_game_metadata(&snapshot)
            .expect("metadata promoted");

        let mut legacy = serde_json::to_value(&snapshot).expect("snapshot serializes");
        legacy
            .as_object_mut()
            .expect("snapshot object")
            .remove("rivenDispositions");
        legacy["metadata"]
            .as_object_mut()
            .expect("metadata object")
            .remove("rivenDispositionCount");
        legacy
            .as_object_mut()
            .expect("snapshot object")
            .remove("itemDefinitions");
        legacy["metadata"]
            .as_object_mut()
            .expect("metadata object")
            .remove("itemDefinitionCount");
        database
            .connection
            .execute(
                "UPDATE game_metadata_snapshots SET metadata_json = ?1 WHERE is_current = 1",
                [serde_json::to_string(&legacy).expect("legacy serializes")],
            )
            .expect("legacy JSON stored");

        let loaded = database
            .load_current_game_metadata()
            .expect("legacy metadata loads")
            .expect("metadata exists");
        assert_eq!(loaded.metadata.riven_disposition_count, 0);
        assert!(loaded.riven_dispositions.is_empty());
        assert_eq!(loaded.metadata.item_definition_count, 0);
        assert!(loaded.item_definitions.is_empty());
    }
}
