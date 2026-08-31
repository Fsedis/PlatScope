#![forbid(unsafe_code)]

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    RelicsRun,
    FrameForgeMirror,
    WarframeMarket,
    LocalCache,
    Import,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    #[default]
    Pc,
    Playstation,
    Xbox,
    Switch,
    Mobile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketOrderType {
    Closed,
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveOrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserStatus {
    InGame,
    Online,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveOrder {
    pub side: LiveOrderSide,
    pub platinum: u32,
    pub quantity: u32,
    pub per_trade: u32,
    pub user_status: UserStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveOrderBook {
    pub key: MarketVariantKey,
    pub fetched_at: DateTime<Utc>,
    pub orders: Vec<LiveOrder>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketItemKind {
    Standard,
    Relic,
    Riven,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceFreshness {
    Fresh,
    Aging,
    Stale,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketVariantKey {
    pub slug: String,
    pub platform: Platform,
    pub rank: Option<u16>,
    pub subtype: Option<String>,
    pub amber_stars: Option<u16>,
    pub cyan_stars: Option<u16>,
}

impl MarketVariantKey {
    /// Создаёт точный рыночный ключ. Пустые измерения не нормализуются в
    /// фиктивные значения: отсутствие subtype/rank остаётся `None`.
    ///
    /// # Errors
    ///
    /// Возвращает [`DomainError`], если slug или переданный subtype пуст.
    pub fn new(
        slug: impl Into<String>,
        platform: Platform,
        rank: Option<u16>,
        subtype: Option<impl Into<String>>,
    ) -> Result<Self, DomainError> {
        let slug = slug.into();
        if slug.trim().is_empty() {
            return Err(DomainError::EmptySlug);
        }

        let subtype = subtype.map(Into::into);
        if subtype
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(DomainError::EmptySubtype);
        }

        Ok(Self {
            slug,
            platform,
            rank,
            subtype,
            amber_stars: None,
            cyan_stars: None,
        })
    }

    #[must_use]
    pub const fn with_stars(mut self, amber_stars: Option<u16>, cyan_stars: Option<u16>) -> Self {
        self.amber_stars = amber_stars;
        self.cyan_stars = cyan_stars;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketRecord {
    pub key: MarketVariantKey,
    pub external_item_id: String,
    pub display_name_en: String,
    pub observed_at: DateTime<Utc>,
    pub order_type: MarketOrderType,
    pub median: Option<f64>,
    pub average: Option<f64>,
    pub min_price: Option<f64>,
    pub max_price: Option<f64>,
    pub volume: f64,
    pub raw_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogMetadata {
    pub provider: ProviderId,
    pub fetched_at: DateTime<Utc>,
    pub schema_version: u32,
    pub item_count: u64,
    pub checksum_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogItem {
    pub item_id: String,
    pub slug: String,
    pub display_name_en: String,
    #[serde(default)]
    pub display_name_ru: Option<String>,
    #[serde(default)]
    pub thumb: Option<String>,
    #[serde(default)]
    pub thumb_ru: Option<String>,
    pub game_ref: Option<String>,
    #[serde(default)]
    pub bulk_tradable: bool,
    pub max_rank: Option<u16>,
    pub subtypes: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemCatalog {
    pub metadata: CatalogMetadata,
    pub items: Vec<CatalogItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameMetadataSource {
    WfcdWarframeItems,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameMetadataSnapshotMetadata {
    pub source: GameMetadataSource,
    pub fetched_at: DateTime<Utc>,
    pub schema_version: u32,
    pub set_count: u64,
    pub relic_count: u64,
    pub prime_part_count: u64,
    #[serde(default)]
    pub riven_disposition_count: u64,
    #[serde(default)]
    pub item_definition_count: u64,
    pub checksum_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VaultStatus {
    Available,
    Vaulted,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelicRefinement {
    Intact,
    Exceptional,
    Flawless,
    Radiant,
}

impl RelicRefinement {
    #[must_use]
    pub const fn market_subtype(self) -> &'static str {
        match self {
            Self::Intact => "intact",
            Self::Exceptional => "exceptional",
            Self::Flawless => "flawless",
            Self::Radiant => "radiant",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrimeSetComponentDefinition {
    pub slug: String,
    pub game_ref: String,
    pub required_quantity: u32,
    pub ducats: Option<u32>,
    #[serde(default)]
    pub image_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrimeSetDefinition {
    pub set_slug: String,
    pub set_game_ref: String,
    pub display_name_en: String,
    pub vault_status: VaultStatus,
    pub components: Vec<PrimeSetComponentDefinition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelicRewardDefinition {
    pub reward_slug: Option<String>,
    pub reward_game_ref: String,
    pub display_name_en: String,
    pub chance_percent: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelicDefinition {
    pub relic_slug: String,
    pub relic_game_ref: String,
    pub display_name_en: String,
    pub refinement: RelicRefinement,
    pub vault_status: VaultStatus,
    pub rewards: Vec<RelicRewardDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrimePartMetadata {
    pub slug: String,
    pub game_ref: String,
    pub ducats: u32,
    pub vault_status: VaultStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RivenWeaponCategory {
    Primary,
    Secondary,
    Melee,
    SentinelWeapon,
    ArchGun,
    ArchMelee,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RivenDispositionDefinition {
    pub weapon_name_en: String,
    pub weapon_game_ref: String,
    pub category: RivenWeaponCategory,
    pub disposition: u8,
    pub multiplier: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameItemDefinition {
    pub slug: String,
    pub game_ref: String,
    pub mastery_requirement: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameItemLocalization {
    pub game_ref: String,
    pub display_name_ru: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyndicateOfferDefinition {
    pub syndicate: String,
    pub required_title: String,
    pub slug: String,
    pub game_ref: String,
    pub display_name_en: String,
    pub display_name_ru: Option<String>,
    pub image_url: Option<String>,
    pub standing_cost: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NightwaveOfferDefinition {
    pub slug: String,
    pub game_ref: String,
    pub display_name_en: String,
    pub display_name_ru: Option<String>,
    pub image_url: Option<String>,
    pub cred_cost: u32,
}

/// Точный товар из текущей недельной ротации магазина Норы.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NightwaveVendorOffer {
    pub game_ref: String,
    pub cred_cost: u32,
}

/// Последний подтверждённый игрой ассортимент магазина Ночной волны.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NightwaveVendorSnapshot {
    pub observed_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub season_tag: String,
    pub vendor_type: String,
    pub offers: Vec<NightwaveVendorOffer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArcaneDissolutionDefinition {
    pub slug: String,
    pub game_ref: String,
    pub display_name_en: String,
    pub display_name_ru: Option<String>,
    pub image_url: Option<String>,
    pub vosfor: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArcanePackComponentDefinition {
    pub game_ref: String,
    pub rarity: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArcanePackDefinition {
    pub key: String,
    pub display_name_ru: String,
    pub rolls: Vec<HashMap<String, f64>>,
    pub components: Vec<ArcanePackComponentDefinition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameMetadataSnapshot {
    pub metadata: GameMetadataSnapshotMetadata,
    pub prime_sets: Vec<PrimeSetDefinition>,
    pub relics: Vec<RelicDefinition>,
    pub prime_parts: Vec<PrimePartMetadata>,
    #[serde(default)]
    pub riven_dispositions: Vec<RivenDispositionDefinition>,
    #[serde(default)]
    pub item_definitions: Vec<GameItemDefinition>,
    #[serde(default)]
    pub item_localizations: Vec<GameItemLocalization>,
    #[serde(default)]
    pub syndicate_offers: Vec<SyndicateOfferDefinition>,
    #[serde(default)]
    pub nightwave_offers: Vec<NightwaveOfferDefinition>,
    #[serde(default)]
    pub arcane_dissolutions: Vec<ArcaneDissolutionDefinition>,
    #[serde(default)]
    pub arcane_packs: Vec<ArcanePackDefinition>,
}

impl MarketRecord {
    /// Проверяет, что все присутствующие цены и объём конечны и неотрицательны.
    ///
    /// # Errors
    ///
    /// Возвращает [`DomainError::InvalidNumber`] для первого некорректного поля.
    pub fn validate(&self) -> Result<(), DomainError> {
        for (field, value) in [
            ("median", self.median),
            ("average", self.average),
            ("min_price", self.min_price),
            ("max_price", self.max_price),
            ("volume", Some(self.volume)),
        ] {
            if value.is_some_and(|number| !number.is_finite() || number < 0.0) {
                return Err(DomainError::InvalidNumber { field });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub provider: ProviderId,
    pub source_date: NaiveDate,
    pub fetched_at: DateTime<Utc>,
    pub schema_version: u32,
    pub item_count: u64,
    pub record_count: u64,
    pub checksum_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedMarketSnapshot {
    pub metadata: SnapshotMetadata,
    pub records: Vec<MarketRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketHistoryPoint {
    pub source_date: NaiveDate,
    pub closed_median: Option<f64>,
    pub closed_volume: f64,
    pub sell_median: Option<f64>,
    pub buy_median: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceConfidence {
    High,
    Medium,
    Low,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tradeability {
    Tradeable,
    Untradeable,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventorySource {
    // Значения ниже сохранены только для чтения старых снимков из локальной БД.
    // Новые снимки создаются исключительно встроенным read-only scanner.
    PlatscopeJson,
    HelperImport,
    OverwolfCompanion,
    TestFixture,
    ReadOnlyScan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventoryResolution {
    Resolved,
    UnknownItem,
    AmbiguousItem,
    ExactVariantUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventorySnapshotMetadata {
    pub source: InventorySource,
    pub observed_at: DateTime<Utc>,
    pub schema_version: u32,
    pub item_count: u64,
    pub checksum_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryItem {
    pub canonical_game_id: String,
    pub quantity: u32,
    pub rank: Option<u16>,
    pub subtype: Option<String>,
    pub tradeability: Tradeability,
    pub leveled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquipmentKind {
    Warframe,
    Primary,
    Secondary,
    Melee,
    Companion,
    CompanionWeapon,
    Archwing,
    Archgun,
    Archmelee,
    Necramech,
    Amp,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryModPlacement {
    pub equipment_instance_key: String,
    pub equipment_game_id: String,
    pub equipment_custom_name: Option<String>,
    pub equipment_kind: EquipmentKind,
    pub config_index: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EquippedModInstance {
    pub canonical_game_id: String,
    pub rank: u16,
    pub tradeability: Tradeability,
    pub placements: Vec<InventoryModPlacement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyndicateStanding {
    pub tag: String,
    pub standing: i64,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerInventory {
    pub metadata: InventorySnapshotMetadata,
    pub items: Vec<InventoryItem>,
    #[serde(default)]
    pub mod_usage_scanned: bool,
    #[serde(default)]
    pub equipped_mods: Vec<EquippedModInstance>,
    #[serde(default)]
    pub credits: Option<u64>,
    #[serde(default)]
    pub syndicates: Vec<SyndicateStanding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedModPlacement {
    pub equipment_instance_key: String,
    pub equipment_game_id: String,
    pub equipment_display_name_en: Option<String>,
    pub equipment_display_name_ru: Option<String>,
    pub equipment_image_url: Option<String>,
    pub equipment_kind: EquipmentKind,
    pub config_index: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedInventoryItem {
    pub canonical_game_id: String,
    pub display_name_en: Option<String>,
    pub display_name_ru: Option<String>,
    pub tags: Vec<String>,
    pub key: Option<MarketVariantKey>,
    pub rank: Option<u16>,
    pub subtype: Option<String>,
    pub owned_quantity: u32,
    pub tradeable_quantity: u32,
    pub untradeable_quantity: u32,
    pub unknown_quantity: u32,
    pub leveled_quantity: u32,
    #[serde(default)]
    pub equipped_quantity: u32,
    #[serde(default)]
    pub equipped_tradeable_quantity: u32,
    #[serde(default)]
    pub equipped_placements: Vec<ResolvedModPlacement>,
    pub sellable_quantity: u32,
    pub resolution: InventoryResolution,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedInventorySnapshot {
    pub metadata: InventorySnapshotMetadata,
    pub keep_copies: u32,
    #[serde(default)]
    pub mod_usage_scanned: bool,
    #[serde(default)]
    pub credits: Option<u64>,
    #[serde(default)]
    pub syndicates: Vec<SyndicateStanding>,
    pub items: Vec<ResolvedInventoryItem>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("market slug must not be empty")]
    EmptySlug,
    #[error("market subtype must not be empty")]
    EmptySubtype,
    #[error("field {field} must be finite and non-negative")]
    InvalidNumber { field: &'static str },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_and_subtype_are_part_of_variant_identity() {
        let intact = MarketVariantKey::new("axi_s18_relic", Platform::Pc, None, Some("intact"))
            .expect("valid key");
        let radiant = MarketVariantKey::new("axi_s18_relic", Platform::Pc, None, Some("radiant"))
            .expect("valid key");
        assert_ne!(intact, radiant);

        let unranked = MarketVariantKey::new("primed_flow", Platform::Pc, Some(0), None::<String>)
            .expect("valid key");
        let max_rank = MarketVariantKey::new("primed_flow", Platform::Pc, Some(10), None::<String>)
            .expect("valid key");
        assert_ne!(unranked, max_rank);

        let empty =
            MarketVariantKey::new("ayatan_anasa_sculpture", Platform::Pc, None, None::<String>)
                .expect("valid key")
                .with_stars(Some(0), Some(0));
        let filled =
            MarketVariantKey::new("ayatan_anasa_sculpture", Platform::Pc, None, None::<String>)
                .expect("valid key")
                .with_stars(Some(2), Some(2));
        assert_ne!(empty, filled);
    }

    #[test]
    fn invalid_market_numbers_are_rejected() {
        let record = MarketRecord {
            key: MarketVariantKey::new("test_item", Platform::Pc, None, None::<String>)
                .expect("valid key"),
            external_item_id: "item-id".into(),
            display_name_en: "Test Item".into(),
            observed_at: Utc::now(),
            order_type: MarketOrderType::Closed,
            median: Some(f64::NAN),
            average: None,
            min_price: None,
            max_price: None,
            volume: 3.0,
            raw_json: "{}".into(),
        };

        assert_eq!(
            record.validate(),
            Err(DomainError::InvalidNumber { field: "median" })
        );
    }

    #[test]
    fn old_set_components_default_to_no_image() {
        let component: PrimeSetComponentDefinition = serde_json::from_str(
            r#"{"slug":"test_prime_barrel","gameRef":"/Lotus/Test/Barrel","requiredQuantity":1,"ducats":45}"#,
        )
        .expect("old metadata remains readable");

        assert!(component.image_url.is_none());
    }
}
