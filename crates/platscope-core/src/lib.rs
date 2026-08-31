#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{Duration as ChronoDuration, NaiveDate, Utc};
pub use platscope_account::{AccountOrder, AccountProfile, CreateListingInput, UpdateListingInput};
use platscope_account::{CredentialStore, OsCredentialStore, WfmAccountClient};
use platscope_domain::{
    ArcanePackDefinition, CatalogItem, EquipmentKind, GameMetadataSnapshot,
    GameMetadataSnapshotMetadata, InventoryResolution, InventorySnapshotMetadata, ItemCatalog,
    LiveOrder, LiveOrderBook, LiveOrderSide, MarketHistoryPoint, MarketItemKind, MarketRecord,
    MarketVariantKey, NightwaveVendorSnapshot, Platform, PlayerInventory, PriceConfidence,
    PriceFreshness, PrimePartMetadata, PrimeSetComponentDefinition, PrimeSetDefinition, ProviderId,
    RelicDefinition, RelicRewardDefinition, ResolvedInventoryItem, ResolvedInventorySnapshot,
    SyndicateStanding, UserStatus, VaultStatus,
};
use platscope_insights::{
    DucatEfficiency, RelicExpectedValue, RelicRewardInput, SetComparison, SetComparisonInput,
    SetPartInput, calculate_ducat_efficiency, calculate_relic_ev, compare_set,
};
use platscope_inventory::{
    InventoryError, apply_keep_copies, parse_read_only_scan_json, resolve_inventory,
};
pub use platscope_pricing::PriceRecommendation;
use platscope_pricing::{PricingContext, recommend};
use platscope_providers::{
    BulkMarketProvider, DailyMarketState, FrameForgeMirrorProvider, GameMetadataProvider,
    HistoricalMarketProvider, LiveMarketProvider, MetadataProvider, ProviderError,
    ProviderErrorCode, RelicsRunProvider, WarframeMarketProvider, WarframeWorldstateProvider,
    WfcdMetadataProvider,
};
use platscope_selling::{SellPriorityInput, SellPriorityScore, calculate_priority, nominal_value};
use platscope_storage::{Database, HistoryCoverage, MarketSnapshotSummary};
use platscope_trends::{TrendContext, TrendSummary};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

#[cfg(test)]
use platscope_domain::InventorySource;

pub const SETTINGS_KEY: &str = "app.settings";
pub const NIGHTWAVE_VENDOR_CACHE_KEY: &str = "nightwave.vendor_snapshot";
pub const DEFAULT_LIVE_QUOTE_TTL_SECONDS: u64 = 90;
const MINIMUM_RELATIVE_SNAPSHOT_PERCENT: u128 = 20;
pub const HISTORY_TARGET_DAYS: u16 = 90;
pub const HISTORY_IMPORTS_PER_RUN: usize = 7;
const HISTORY_FETCH_ATTEMPTS: u8 = 3;
pub const DEFAULT_KEEP_COPIES: u32 = 1;
pub const DEFAULT_REWARD_OVERLAY_SCALE_PERCENT: u16 = 100;
pub const DEFAULT_REWARD_OVERLAY_OFFSET_PERCENT: i16 = 0;
const CURRENT_GAME_METADATA_SCHEMA_VERSION: u32 = 6;
const CURRENT_CATALOG_SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    English,
    #[default]
    Russian,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppSettings {
    pub language: Language,
    pub platform: Platform,
    pub crossplay: bool,
    pub bulk_refresh_hours: u8,
    pub live_quote_ttl_seconds: u64,
    #[serde(default = "default_keep_copies")]
    pub keep_inventory_copies: u32,
    #[serde(default = "default_reward_overlay_scale_percent")]
    pub reward_overlay_scale_percent: u16,
    #[serde(default = "default_reward_overlay_offset_percent")]
    pub reward_overlay_offset_x_percent: i16,
    #[serde(default = "default_reward_overlay_offset_percent")]
    pub reward_overlay_offset_y_percent: i16,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: Language::Russian,
            platform: Platform::Pc,
            crossplay: true,
            bulk_refresh_hours: 4,
            live_quote_ttl_seconds: DEFAULT_LIVE_QUOTE_TTL_SECONDS,
            keep_inventory_copies: DEFAULT_KEEP_COPIES,
            reward_overlay_scale_percent: DEFAULT_REWARD_OVERLAY_SCALE_PERCENT,
            reward_overlay_offset_x_percent: DEFAULT_REWARD_OVERLAY_OFFSET_PERCENT,
            reward_overlay_offset_y_percent: DEFAULT_REWARD_OVERLAY_OFFSET_PERCENT,
        }
    }
}

const fn default_keep_copies() -> u32 {
    DEFAULT_KEEP_COPIES
}

const fn default_reward_overlay_scale_percent() -> u16 {
    DEFAULT_REWARD_OVERLAY_SCALE_PERCENT
}

const fn default_reward_overlay_offset_percent() -> i16 {
    DEFAULT_REWARD_OVERLAY_OFFSET_PERCENT
}

pub struct LoggingGuard {
    _guard: WorkerGuard,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshFailure {
    pub provider: ProviderId,
    pub code: ProviderErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketRefreshOutcome {
    pub snapshot: MarketSnapshotSummary,
    pub catalog_item_count: u64,
    pub stale: bool,
    pub used_fallback: bool,
    pub catalog_from_cache: bool,
    pub failures: Vec<RefreshFailure>,
}

pub struct MarketDataService {
    catalog_provider: Arc<dyn MetadataProvider>,
    market_providers: Vec<Arc<dyn BulkMarketProvider>>,
    refresh_lock: tokio::sync::Mutex<()>,
}

pub struct HistoryService {
    provider: Arc<dyn HistoricalMarketProvider>,
    bootstrap_lock: tokio::sync::Mutex<()>,
}

pub struct InventoryService;

pub struct GameMetadataService {
    provider: Arc<dyn GameMetadataProvider>,
    refresh_lock: tokio::sync::Mutex<()>,
}

pub struct InsightsService;

pub struct ResourceConverterService {
    provider: WarframeWorldstateProvider,
    cache: tokio::sync::Mutex<Option<(Instant, DailyMarketState)>>,
}

pub struct AccountService {
    client: WfmAccountClient,
    credentials: Arc<dyn CredentialStore>,
    operation_lock: tokio::sync::Mutex<()>,
    device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountView {
    pub connected: bool,
    pub profile: Option<AccountProfile>,
    pub orders: Vec<AccountOrder>,
    #[serde(default)]
    pub order_items: HashMap<String, AccountOrderItemView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountOrderItemView {
    pub slug: String,
    pub display_name: String,
    pub display_name_en: String,
    pub image_url: Option<String>,
    pub item_kind: MarketItemKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameMetadataRefreshOutcome {
    pub metadata: GameMetadataSnapshotMetadata,
    pub stale: bool,
    pub used_lkg: bool,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetComponentInsight {
    pub definition: PrimeSetComponentDefinition,
    pub item_id: Option<String>,
    pub display_name: String,
    pub image_url: Option<String>,
    pub owned_quantity: u32,
    pub recommendation: Option<PriceRecommendation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetInsightRow {
    pub definition: PrimeSetDefinition,
    pub item_id: Option<String>,
    pub display_name: String,
    pub image_url: Option<String>,
    pub set_recommendation: Option<PriceRecommendation>,
    pub comparison: SetComparison,
    pub components: Vec<SetComponentInsight>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelicRewardInsight {
    pub definition: RelicRewardDefinition,
    pub display_name: String,
    pub image_url: Option<String>,
    pub recommendation: Option<PriceRecommendation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelicInsightRow {
    pub definition: RelicDefinition,
    pub display_name: String,
    pub image_url: Option<String>,
    pub owned_quantity: u32,
    pub sellable_quantity: u32,
    pub relic_recommendation: Option<PriceRecommendation>,
    pub expected_value: RelicExpectedValue,
    pub rewards: Vec<RelicRewardInsight>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DucatInsightRow {
    pub metadata: PrimePartMetadata,
    pub display_name: String,
    pub image_url: Option<String>,
    pub owned_quantity: u32,
    pub sellable_quantity: u32,
    pub recommendation: Option<PriceRecommendation>,
    pub efficiency: DucatEfficiency,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightsView {
    pub metadata: GameMetadataSnapshotMetadata,
    pub inventory_available: bool,
    pub sets: Vec<SetInsightRow>,
    pub relics: Vec<RelicInsightRow>,
    pub ducats: Vec<DucatInsightRow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceSource {
    Syndicate,
    Nightwave,
    VoidTrader,
    SteelPath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceCurrency {
    Standing,
    NightwaveCred,
    Ducat,
    SteelEssence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceRouteStatus {
    Ready,
    Conditional,
    Waiting,
    Unavailable,
    NeedsData,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceConversionAction {
    pub vendor_name: String,
    pub currency: ResourceCurrency,
    pub balance: u64,
    pub cost: u64,
    pub item_slug: String,
    pub item_name: String,
    pub image_url: Option<String>,
    pub quantity: u32,
    pub unit_price: f64,
    pub estimated_platinum: f64,
    pub included_in_total: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceConversionRoute {
    pub source: ResourceSource,
    pub status: ResourceRouteStatus,
    pub reason: String,
    pub actions: Vec<ResourceConversionAction>,
    pub available_at: Option<chrono::DateTime<Utc>>,
    pub available_until: Option<chrono::DateTime<Utc>>,
    pub location: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArcaneDecisionKind {
    Sell,
    Dissolve,
    Hold,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArcaneConversionDecision {
    pub decision: ArcaneDecisionKind,
    pub slug: String,
    pub display_name: String,
    pub image_url: Option<String>,
    pub rank: u16,
    pub quantity: u32,
    pub market_price_each: Option<f64>,
    pub vosfor_each: u32,
    pub vosfor_total: u64,
    pub equivalent_platinum_each: f64,
    pub estimated_platinum: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArcaneConversionSummary {
    pub available: bool,
    pub reason: String,
    pub best_pack_name: Option<String>,
    pub pack_expected_platinum: Option<f64>,
    pub price_coverage_percent: f64,
    pub sell: Vec<ArcaneConversionDecision>,
    pub dissolve: Vec<ArcaneConversionDecision>,
    pub hold: Vec<ArcaneConversionDecision>,
    pub direct_sale_platinum: f64,
    pub dissolution_expected_platinum: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceConverterView {
    pub fetched_at: chrono::DateTime<Utc>,
    pub inventory_observed_at: chrono::DateTime<Utc>,
    pub market_source_date: Option<NaiveDate>,
    pub confirmed_platinum: f64,
    pub expected_vosfor_platinum: f64,
    pub routes: Vec<ResourceConversionRoute>,
    pub arcanes: ArcaneConversionSummary,
    pub unavailable_sources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventorySummary {
    pub owned_quantity: u64,
    pub sellable_quantity: u64,
    pub resolved_rows: usize,
    pub attention_rows: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryViewItem {
    pub canonical_game_id: String,
    pub item_id: Option<String>,
    pub bulk_tradable: bool,
    pub display_name: String,
    pub image_url: Option<String>,
    pub tags: Vec<String>,
    pub key: Option<MarketVariantKey>,
    pub rank: Option<u16>,
    pub subtype: Option<String>,
    pub owned_quantity: u32,
    pub tradeable_quantity: u32,
    pub untradeable_quantity: u32,
    pub unknown_quantity: u32,
    pub leveled_quantity: u32,
    pub equipped_quantity: u32,
    pub equipped_placements: Vec<EquippedModPlacementView>,
    pub sellable_quantity: u32,
    pub resolution: InventoryResolution,
    pub vault_status: VaultStatus,
    pub closed_median_48h: Option<f64>,
    pub has_reliable_price: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EquippedModPlacementView {
    pub equipment_instance_key: String,
    pub equipment_game_id: String,
    pub equipment_display_name: String,
    pub equipment_image_url: Option<String>,
    pub equipment_kind: EquipmentKind,
    pub config_index: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryView {
    pub metadata: InventorySnapshotMetadata,
    pub keep_copies: u32,
    pub mod_usage_scanned: bool,
    pub summary: InventorySummary,
    pub items: Vec<InventoryViewItem>,
}

pub struct SellNowService;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SellNowRow {
    pub inventory: InventoryViewItem,
    pub item_kind: MarketItemKind,
    pub recommendation: Option<PriceRecommendation>,
    pub trend: Option<TrendSummary>,
    pub priority: SellPriorityScore,
    pub nominal_value: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SellNowSummary {
    pub candidate_rows: usize,
    pub priced_rows: usize,
    pub high_priority_rows: usize,
    /// Номинальная стоимость всех разрешённых owned-копий с надёжной bulk-ценой.
    pub inventory_nominal_value: f64,
    /// Номинальная стоимость только sellable-копий после резерва и tradeability.
    pub nominal_value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SellNowView {
    pub inventory_metadata: InventorySnapshotMetadata,
    pub inventory_summary: InventorySummary,
    pub keep_copies: u32,
    pub mod_usage_scanned: bool,
    pub market_snapshot: Option<MarketSnapshotSummary>,
    pub summary: SellNowSummary,
    pub rows: Vec<SellNowRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveSellNowResult {
    pub row: SellNowRow,
    pub fetched_at: chrono::DateTime<Utc>,
    pub quote_state: LiveQuoteState,
    pub sell_order_count: usize,
    pub buy_order_count: usize,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryBootstrapFailure {
    pub source_date: NaiveDate,
    pub code: ProviderErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryBootstrapOutcome {
    pub target_days: u16,
    pub imported_days: usize,
    pub skipped_days: usize,
    pub coverage: HistoryCoverage,
    pub failures: Vec<HistoryBootstrapFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketHistoryView {
    pub key: MarketVariantKey,
    pub requested_days: u16,
    pub points: Vec<MarketHistoryPoint>,
    pub trend: TrendSummary,
    pub coverage: HistoryCoverage,
}

pub struct PricingService;

impl PricingService {
    /// Рассчитывает bulk-рекомендацию из текущего LKG snapshot для точного варианта.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`] при недоступном database state или ошибке чтения SQLite.
    pub fn price_current_variant(
        database: &Mutex<Database>,
        key: &MarketVariantKey,
        item_kind: MarketItemKind,
    ) -> Result<Option<PriceRecommendation>, CoreError> {
        Self::price_current_variant_with_live(database, key, item_kind, None)
    }

    fn price_current_variant_with_live(
        database: &Mutex<Database>,
        key: &MarketVariantKey,
        item_kind: MarketItemKind,
        live_order_book: Option<&LiveOrderBook>,
    ) -> Result<Option<PriceRecommendation>, CoreError> {
        let database = lock_database(database)?;
        let Some(snapshot) = database.current_market_snapshot()? else {
            return Ok(None);
        };
        let records = current_market_records_with_regular_fallback(&database, key)?;
        Ok(Some(recommend(PricingContext {
            key,
            item_kind,
            source_date: snapshot.source_date,
            as_of: Utc::now().date_naive(),
            provider: snapshot.provider,
            source_is_fallback: snapshot.provider == ProviderId::FrameForgeMirror,
            bulk_records: &records,
            live_order_book,
        })))
    }
}

fn current_market_records_with_regular_fallback(
    database: &Database,
    key: &MarketVariantKey,
) -> Result<Vec<MarketRecord>, CoreError> {
    let records = database.current_market_records(key)?;
    if !records.is_empty() || key.subtype.as_deref() != Some("regular") {
        return Ok(records);
    }
    let mut legacy_key = key.clone();
    legacy_key.subtype = None;
    let mut legacy_records = database.current_market_records(&legacy_key)?;
    for record in &mut legacy_records {
        record.key = key.clone();
    }
    Ok(legacy_records)
}

fn market_history_with_regular_fallback(
    database: &Database,
    key: &MarketVariantKey,
    days: u16,
    as_of: NaiveDate,
) -> Result<Vec<MarketHistoryPoint>, CoreError> {
    let points = database.market_history(key, days, as_of)?;
    if !points.is_empty() || key.subtype.as_deref() != Some("regular") {
        return Ok(points);
    }
    let mut legacy_key = key.clone();
    legacy_key.subtype = None;
    Ok(database.market_history(&legacy_key, days, as_of)?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveQuoteState {
    Network,
    Cache,
    StaleCache,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LivePricingResult {
    pub recommendation: PriceRecommendation,
    pub fetched_at: chrono::DateTime<Utc>,
    pub quote_state: LiveQuoteState,
    pub sell_order_count: usize,
    pub buy_order_count: usize,
    pub orders: Vec<LiveOrderView>,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveOrderView {
    pub side: LiveOrderSide,
    pub platinum: u32,
    pub quantity: u32,
    pub per_trade: u32,
    pub user_status: UserStatus,
}

struct CachedLiveQuote {
    stored_at: Instant,
    book: LiveOrderBook,
}

pub struct LivePricingService {
    provider: Arc<dyn LiveMarketProvider>,
    cache: tokio::sync::Mutex<HashMap<(MarketVariantKey, bool), CachedLiveQuote>>,
}

impl LivePricingService {
    /// Создаёт production WFM v2 service.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`], если HTTP client нельзя инициализировать.
    pub fn production() -> Result<Self, CoreError> {
        Ok(Self {
            provider: Arc::new(WarframeMarketProvider::new()?),
            cache: tokio::sync::Mutex::new(HashMap::new()),
        })
    }

    /// Возвращает live-рекомендацию с TTL cache и stale fallback.
    /// Один mutex вокруг check/fetch/insert объединяет одновременные запросы и не допускает burst.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`], если WFM недоступен без cached quote либо SQLite не читается.
    pub async fn price_current_variant(
        &self,
        database: &Mutex<Database>,
        key: &MarketVariantKey,
        item_kind: MarketItemKind,
        settings: &AppSettings,
    ) -> Result<Option<LivePricingResult>, CoreError> {
        let cache_key = (key.clone(), settings.crossplay);
        let ttl = Duration::from_secs(settings.live_quote_ttl_seconds.clamp(15, 600));
        let mut cache = self.cache.lock().await;
        if let Some(cached) = cache.get(&cache_key)
            && cached.stored_at.elapsed() <= ttl
        {
            return build_live_result(
                database,
                key,
                item_kind,
                &cached.book,
                LiveQuoteState::Cache,
                None,
            );
        }

        let language = match settings.language {
            Language::English => "en",
            Language::Russian => "ru",
        };
        let started = Instant::now();
        match self
            .provider
            .fetch_orders(key, language, settings.crossplay)
            .await
        {
            Ok(book) => {
                lock_database(database)?
                    .record_provider_success(self.provider.id(), elapsed_millis(started))?;
                let result = build_live_result(
                    database,
                    key,
                    item_kind,
                    &book,
                    LiveQuoteState::Network,
                    None,
                )?;
                cache.insert(
                    cache_key,
                    CachedLiveQuote {
                        stored_at: Instant::now(),
                        book,
                    },
                );
                Ok(result)
            }
            Err(error) => {
                record_failure(database, self.provider.id(), &error, started)?;
                if let Some(cached) = cache.get(&cache_key) {
                    build_live_result(
                        database,
                        key,
                        item_kind,
                        &cached.book,
                        LiveQuoteState::StaleCache,
                        Some(format!("Live WFM не обновлён: {error}")),
                    )
                } else {
                    Err(CoreError::MarketData(format!(
                        "live WFM request failed: {error}"
                    )))
                }
            }
        }
    }
}

fn build_live_result(
    database: &Mutex<Database>,
    key: &MarketVariantKey,
    item_kind: MarketItemKind,
    book: &LiveOrderBook,
    quote_state: LiveQuoteState,
    warning: Option<String>,
) -> Result<Option<LivePricingResult>, CoreError> {
    let Some(recommendation) =
        PricingService::price_current_variant_with_live(database, key, item_kind, Some(book))?
    else {
        return Ok(None);
    };
    let active_orders: Vec<_> = book
        .orders
        .iter()
        .filter(|order| active_live_order(order))
        .collect();
    let sell_order_count = active_orders
        .iter()
        .filter(|order| order.side == LiveOrderSide::Sell)
        .count();
    let buy_order_count = active_orders.len().saturating_sub(sell_order_count);
    let orders = bounded_live_orders(&active_orders);
    Ok(Some(LivePricingResult {
        recommendation,
        fetched_at: book.fetched_at,
        quote_state,
        sell_order_count,
        buy_order_count,
        orders,
        warning,
    }))
}

fn active_live_order(order: &LiveOrder) -> bool {
    order.user_status != UserStatus::Offline
        && order.platinum > 0
        && order.quantity > 0
        && order.per_trade > 0
}

fn bounded_live_orders(active_orders: &[&LiveOrder]) -> Vec<LiveOrderView> {
    const ORDERS_PER_SIDE: usize = 5;
    let mut sells: Vec<_> = active_orders
        .iter()
        .copied()
        .filter(|order| order.side == LiveOrderSide::Sell)
        .collect();
    let mut buys: Vec<_> = active_orders
        .iter()
        .copied()
        .filter(|order| order.side == LiveOrderSide::Buy)
        .collect();
    sells.sort_by_key(|order| (order.platinum, user_status_order(order.user_status)));
    buys.sort_by_key(|order| {
        (
            std::cmp::Reverse(order.platinum),
            user_status_order(order.user_status),
        )
    });
    sells
        .into_iter()
        .take(ORDERS_PER_SIDE)
        .chain(buys.into_iter().take(ORDERS_PER_SIDE))
        .map(|order| LiveOrderView {
            side: order.side,
            platinum: order.platinum,
            quantity: order.quantity,
            per_trade: order.per_trade,
            user_status: order.user_status,
        })
        .collect()
}

const fn user_status_order(status: UserStatus) -> u8 {
    match status {
        UserStatus::InGame => 0,
        UserStatus::Online => 1,
        UserStatus::Offline => 2,
    }
}

pub const DEFAULT_MARKET_SEARCH_LIMIT: usize = 60;
pub const MAX_MARKET_SEARCH_LIMIT: usize = 100;
pub const MAX_MARKET_QUERY_CHARACTERS: usize = 80;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketSearchRow {
    pub item_id: String,
    pub display_name: String,
    pub image_url: Option<String>,
    pub item_kind: MarketItemKind,
    pub mastery_requirement: Option<u8>,
    pub recommendation: PriceRecommendation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketSearchResult {
    pub query: String,
    pub rows: Vec<MarketSearchRow>,
    pub truncated: bool,
    pub snapshot: Option<MarketSnapshotSummary>,
}

pub struct MarketBrowserService;

impl MarketBrowserService {
    /// Ищет market variants в текущем LKG и рассчитывает bulk price для каждой строки.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`] при слишком длинном запросе, недоступной DB или ошибке SQLite.
    pub fn search(
        database: &Mutex<Database>,
        query: &str,
        requested_limit: usize,
        language: Language,
        platform: Platform,
    ) -> Result<MarketSearchResult, CoreError> {
        let query = query.trim();
        if query.chars().count() > MAX_MARKET_QUERY_CHARACTERS {
            return Err(CoreError::MarketData(format!(
                "поисковый запрос должен быть не длиннее {MAX_MARKET_QUERY_CHARACTERS} символов"
            )));
        }
        let limit = requested_limit.clamp(1, MAX_MARKET_SEARCH_LIMIT);
        let database = lock_database(database)?;
        let snapshot = database.current_market_snapshot()?;
        let Some(snapshot_metadata) = snapshot.as_ref() else {
            return Ok(MarketSearchResult {
                query: query.to_owned(),
                rows: Vec::new(),
                truncated: false,
                snapshot,
            });
        };
        let mut bundles =
            database.search_current_market_variants(query, limit.saturating_add(1))?;
        let catalog = database.load_current_catalog()?;
        let (image_by_item_id, implicit_regular_slugs) =
            market_search_catalog_context(catalog.as_ref(), language);
        let component_images = database
            .load_current_game_metadata()?
            .as_ref()
            .map_or_else(HashMap::new, component_image_urls_by_slug);
        let mastery_requirements = database.current_mastery_requirements()?;
        let truncated = bundles.len() > limit;
        bundles.truncate(limit);
        let rows = bundles
            .into_iter()
            .map(|mut bundle| {
                if bundle.key.subtype.is_none() && implicit_regular_slugs.contains(&bundle.key.slug)
                {
                    bundle.key.subtype = Some("regular".to_owned());
                    for record in &mut bundle.records {
                        if record.key.subtype.is_none() {
                            record.key.subtype = Some("regular".to_owned());
                        }
                    }
                }
                // Catalog and daily bulk data are currently PC-backed. Retarget only the
                // requested identity: exact-key pricing refuses PC records on another
                // platform while an explicit live request uses that platform.
                bundle.key.platform = platform;
                let item_kind = market_item_kind(&bundle.tags, &bundle.key);
                let display_name = match language {
                    Language::Russian => bundle
                        .display_name_ru
                        .filter(|name| !name.trim().is_empty())
                        .unwrap_or(bundle.display_name_en),
                    Language::English => bundle.display_name_en,
                };
                let recommendation = recommend(PricingContext {
                    key: &bundle.key,
                    item_kind,
                    source_date: snapshot_metadata.source_date,
                    as_of: Utc::now().date_naive(),
                    provider: snapshot_metadata.provider,
                    source_is_fallback: snapshot_metadata.provider == ProviderId::FrameForgeMirror,
                    bulk_records: &bundle.records,
                    live_order_book: None,
                });
                MarketSearchRow {
                    image_url: component_images.get(&bundle.key.slug).cloned().or_else(|| {
                        image_by_item_id
                            .get(&bundle.item_id)
                            .map(|thumb| market_image_url(thumb))
                    }),
                    item_id: bundle.item_id,
                    display_name,
                    item_kind,
                    mastery_requirement: mastery_requirements.get(&bundle.key.slug).copied(),
                    recommendation,
                }
            })
            .collect();
        Ok(MarketSearchResult {
            query: query.to_owned(),
            rows,
            truncated,
            snapshot,
        })
    }
}

fn market_search_catalog_context(
    catalog: Option<&ItemCatalog>,
    language: Language,
) -> (HashMap<String, String>, HashSet<String>) {
    let Some(catalog) = catalog else {
        return (HashMap::new(), HashSet::new());
    };
    let images = catalog
        .items
        .iter()
        .filter_map(|item| {
            let thumb = match language {
                Language::Russian => item.thumb_ru.as_ref().or(item.thumb.as_ref()),
                Language::English => item.thumb.as_ref(),
            };
            thumb.map(|thumb| (item.item_id.clone(), thumb.clone()))
        })
        .collect();
    let implicit_regular_slugs = catalog
        .items
        .iter()
        .filter(|item| item.subtypes.iter().any(|subtype| subtype == "regular"))
        .map(|item| item.slug.clone())
        .collect();
    (images, implicit_regular_slugs)
}

impl InventoryService {
    /// Импортирует только ответ встроенного read-only scanner и помечает снимок
    /// отдельным доверенным источником до атомарной публикации LKG.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`] при schema/validation, отсутствии каталога или
    /// ошибке SQLite; предыдущий LKG при этом сохраняется.
    pub fn import_read_only_scan_json(
        database: &Mutex<Database>,
        raw_json: &str,
        settings: &AppSettings,
    ) -> Result<InventoryView, CoreError> {
        let inventory = parse_read_only_scan_json(raw_json)?;
        Self::publish_inventory(database, &inventory, settings)
    }

    fn publish_inventory(
        database: &Mutex<Database>,
        inventory: &PlayerInventory,
        settings: &AppSettings,
    ) -> Result<InventoryView, CoreError> {
        let mut database_guard = lock_database(database)?;
        let catalog = database_guard.load_current_catalog()?.ok_or_else(|| {
            CoreError::InventoryData(
                "inventory resolver requires a local catalog; refresh market data first".into(),
            )
        })?;
        let variants = database_guard.current_market_variant_keys()?;
        let game_metadata = database_guard.load_current_game_metadata()?;
        let resolved = resolve_inventory(
            inventory,
            &catalog,
            &variants,
            settings.platform,
            settings.keep_inventory_copies,
        );
        let resolved = relink_implicit_regular_inventory(
            &resolved,
            Some(&catalog),
            &variants,
            settings.platform,
        );
        let resolved = relink_exact_relic_inventory(
            &resolved,
            Some(&catalog),
            game_metadata.as_ref(),
            &variants,
            settings.platform,
        );
        database_guard.promote_inventory_snapshot(&resolved)?;
        drop(database_guard);
        enrich_inventory_view(
            database,
            inventory_view_from_snapshot(
                &resolved,
                settings.language,
                settings.platform,
                settings.keep_inventory_copies,
            ),
            settings.language,
        )
    }

    /// Читает текущий LKG inventory и пересчитывает пользовательский резерв без повторного import.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`] при недоступном mutex или ошибке SQLite.
    pub fn view(
        database: &Mutex<Database>,
        settings: &AppSettings,
    ) -> Result<Option<InventoryView>, CoreError> {
        let (snapshot, catalog, game_metadata, variants) = {
            let database = lock_database(database)?;
            (
                database.current_inventory_snapshot()?,
                database.load_current_catalog()?,
                database.load_current_game_metadata()?,
                database.current_market_variant_keys()?,
            )
        };
        snapshot
            .map(|snapshot| {
                let snapshot = relink_implicit_regular_inventory(
                    &snapshot,
                    catalog.as_ref(),
                    &variants,
                    settings.platform,
                );
                let snapshot = relink_exact_relic_inventory(
                    &snapshot,
                    catalog.as_ref(),
                    game_metadata.as_ref(),
                    &variants,
                    settings.platform,
                );
                enrich_inventory_view(
                    database,
                    inventory_view_from_snapshot(
                        &snapshot,
                        settings.language,
                        settings.platform,
                        settings.keep_inventory_copies,
                    ),
                    settings.language,
                )
            })
            .transpose()
    }
}

impl GameMetadataService {
    /// Создаёт production pipeline метаданных WFCD, независимый от price refresh.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`], если bounded HTTP provider нельзя инициализировать.
    pub fn production() -> Result<Self, CoreError> {
        Ok(Self {
            provider: Arc::new(WfcdMetadataProvider::production()?),
            refresh_lock: tokio::sync::Mutex::new(()),
        })
    }

    /// Загружает и атомарно публикует normalized game metadata; при сбое сохраняет LKG.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`], только если недоступны и новый snapshot, и локальный LKG.
    pub async fn refresh(
        &self,
        database: &Mutex<Database>,
    ) -> Result<GameMetadataRefreshOutcome, CoreError> {
        let _refresh_guard = self.refresh_lock.lock().await;
        self.refresh_locked(database).await
    }

    /// Обновляет игровые определения только когда LKG отсутствует или устарел.
    /// Ручной и фоновый paths используют один lock и не дублируют download.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`] по тем же правилам, что и [`Self::refresh`].
    pub async fn refresh_if_due(
        &self,
        database: &Mutex<Database>,
        refresh_hours: u16,
    ) -> Result<Option<GameMetadataRefreshOutcome>, CoreError> {
        let _refresh_guard = self.refresh_lock.lock().await;
        let current = lock_database(database)?.load_current_game_metadata()?;
        if !game_metadata_refresh_due(
            current.as_ref().map(|snapshot| &snapshot.metadata),
            Utc::now(),
            refresh_hours,
        ) {
            return Ok(None);
        }
        self.refresh_locked(database).await.map(Some)
    }

    async fn refresh_locked(
        &self,
        database: &Mutex<Database>,
    ) -> Result<GameMetadataRefreshOutcome, CoreError> {
        let (catalog, previous) = {
            let database = lock_database(database)?;
            (
                database.load_current_catalog()?,
                database.load_current_game_metadata()?,
            )
        };
        let result = match catalog {
            Some(catalog) => self
                .provider
                .fetch_latest()
                .await
                .and_then(|dump| self.provider.normalize(&dump, &catalog))
                .and_then(|snapshot| {
                    validate_game_metadata_relative(previous.as_ref(), &snapshot.metadata)?;
                    Ok(snapshot)
                }),
            None => Err(ProviderError::validation(
                "game metadata requires a local item catalog; refresh market data first",
            )),
        };

        match result {
            Ok(snapshot) => {
                let metadata = snapshot.metadata.clone();
                lock_database(database)?.promote_game_metadata(&snapshot)?;
                tracing::info!(
                    event = "game_metadata_promoted",
                    set_count = metadata.set_count,
                    relic_count = metadata.relic_count,
                    prime_part_count = metadata.prime_part_count,
                    "normalized game metadata promoted"
                );
                Ok(GameMetadataRefreshOutcome {
                    metadata,
                    stale: false,
                    used_lkg: false,
                    warning: None,
                })
            }
            Err(error) => {
                let Some(previous) = previous else {
                    return Err(CoreError::MetadataData(format!(
                        "не удалось загрузить игровые метаданные, локального LKG ещё нет: {}",
                        public_error_message(&error)
                    )));
                };
                let warning = public_error_message(&error);
                tracing::warn!(
                    event = "game_metadata_lkg_used",
                    error = %warning,
                    "metadata refresh failed without affecting pricing"
                );
                Ok(GameMetadataRefreshOutcome {
                    metadata: previous.metadata,
                    stale: true,
                    used_lkg: true,
                    warning: Some(warning),
                })
            }
        }
    }
}

#[must_use]
pub fn game_metadata_refresh_due(
    current: Option<&GameMetadataSnapshotMetadata>,
    now: chrono::DateTime<Utc>,
    refresh_hours: u16,
) -> bool {
    let interval = ChronoDuration::hours(i64::from(refresh_hours.clamp(1, 168)));
    current.is_none_or(|metadata| {
        metadata.schema_version < CURRENT_GAME_METADATA_SCHEMA_VERSION
            || now.signed_duration_since(metadata.fetched_at) >= interval
    })
}

impl InsightsService {
    /// Соединяет metadata LKG, локальный inventory и точные bulk-рекомендации.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`] при недоступном database state или повреждённом LKG.
    pub fn view(
        database: &Mutex<Database>,
        settings: &AppSettings,
    ) -> Result<Option<InsightsView>, CoreError> {
        let Some(snapshot) = lock_database(database)?.load_current_game_metadata()? else {
            return Ok(None);
        };
        let inventory = InventoryService::view(database, settings)?;
        let inventory_available = inventory.is_some();
        let inventory_items = inventory
            .as_ref()
            .map_or(&[][..], |view| view.items.as_slice());
        let mut sets =
            build_set_insights(database, settings, &snapshot.prime_sets, inventory_items)?;
        let mut relics = build_relic_insights(
            database,
            settings,
            &snapshot.relics,
            &snapshot.prime_sets,
            inventory_items,
        )?;
        let mut ducats =
            build_ducat_insights(database, settings, &snapshot.prime_parts, inventory_items)?;
        sets.sort_by(|left, right| {
            right
                .comparison
                .complete_sets
                .cmp(&left.comparison.complete_sets)
                .then_with(|| {
                    left.definition
                        .display_name_en
                        .cmp(&right.definition.display_name_en)
                })
        });
        relics.sort_by(|left, right| {
            right
                .sellable_quantity
                .cmp(&left.sellable_quantity)
                .then_with(|| {
                    left.definition
                        .display_name_en
                        .cmp(&right.definition.display_name_en)
                })
        });
        ducats.sort_by(|left, right| {
            left.efficiency
                .platinum_per_ducat
                .partial_cmp(&right.efficiency.platinum_per_ducat)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.display_name.cmp(&right.display_name))
        });
        Ok(Some(InsightsView {
            metadata: snapshot.metadata,
            inventory_available,
            sets,
            relics,
            ducats,
        }))
    }
}

const RESOURCE_WORLDSTATE_CACHE_TTL: Duration = Duration::from_secs(5 * 60);
const SYNDICATE_MOD_COST: u64 = 25_000;
const VOSFOR_PACK_COST: f64 = 200.0;
const CONVERTER_PRICE_MARGIN: f64 = 0.10;

impl ResourceConverterService {
    /// Создаёт сервис конвертации валют с коротким кэшем публичного worldstate.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`], если сетевой провайдер не удалось инициализировать.
    pub fn production() -> Result<Self, CoreError> {
        Ok(Self {
            provider: WarframeWorldstateProvider::production()?,
            cache: tokio::sync::Mutex::new(None),
        })
    }

    /// Сопоставляет текущий read-only инвентарь, продавцов и надёжные рыночные цены.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`] при недоступной БД, повреждённом LKG или ошибке worldstate.
    pub async fn view(
        &self,
        database: &Mutex<Database>,
        settings: &AppSettings,
    ) -> Result<Option<ResourceConverterView>, CoreError> {
        let (metadata, inventory, catalog, market_source_date, nightwave_vendor) = {
            let database = lock_database(database)?;
            let Some(metadata) = database.load_current_game_metadata()? else {
                return Ok(None);
            };
            let Some(inventory) = database.current_inventory_snapshot()? else {
                return Ok(None);
            };
            let Some(catalog) = database.load_current_catalog()? else {
                return Ok(None);
            };
            let market_source_date = database
                .current_market_snapshot()?
                .map(|row| row.source_date);
            let nightwave_vendor =
                database.get_setting::<NightwaveVendorSnapshot>(NIGHTWAVE_VENDOR_CACHE_KEY)?;
            (
                metadata,
                inventory,
                catalog,
                market_source_date,
                nightwave_vendor,
            )
        };
        let daily = self.daily_state().await?;
        let context = ResourceConverterBuildContext {
            metadata: &metadata,
            inventory: &inventory,
            catalog: &catalog,
            daily: &daily,
            nightwave_vendor: nightwave_vendor.as_ref(),
            market_source_date,
            now: Utc::now(),
        };
        build_resource_converter_view(database, settings, &context).map(Some)
    }

    /// Сохраняет уже проверенный read-only сканером ассортимент Норы.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`] при недоступной БД.
    pub fn cache_nightwave_vendor(
        database: &Mutex<Database>,
        snapshot: &NightwaveVendorSnapshot,
    ) -> Result<(), CoreError> {
        lock_database(database)?.set_setting(NIGHTWAVE_VENDOR_CACHE_KEY, snapshot)?;
        Ok(())
    }

    async fn daily_state(&self) -> Result<DailyMarketState, CoreError> {
        let mut cache = self.cache.lock().await;
        if let Some((stored_at, state)) = cache.as_ref()
            && stored_at.elapsed() <= RESOURCE_WORLDSTATE_CACHE_TTL
        {
            return Ok(state.clone());
        }
        let state = self.provider.fetch().await?;
        *cache = Some((Instant::now(), state.clone()));
        Ok(state)
    }
}

fn build_resource_converter_view(
    database: &Mutex<Database>,
    settings: &AppSettings,
    context: &ResourceConverterBuildContext<'_>,
) -> Result<ResourceConverterView, CoreError> {
    let catalog_by_ref: HashMap<&str, &CatalogItem> = context
        .catalog
        .items
        .iter()
        .filter_map(|item| item.game_ref.as_deref().map(|game_ref| (game_ref, item)))
        .collect();
    let routes = vec![
        build_syndicate_route(database, settings, context.metadata, context.inventory)?,
        build_nightwave_route(database, settings, context)?,
        build_void_trader_route(
            database,
            settings,
            context.inventory,
            context.catalog,
            context.daily,
            context.now,
        )?,
        build_steel_path_route(
            database,
            settings,
            context.inventory,
            context.catalog,
            context.daily,
            context.now,
        )?,
    ];
    let arcanes = build_arcane_conversion(
        database,
        settings,
        context.metadata,
        context.inventory,
        &catalog_by_ref,
    )?;
    let route_platinum = routes
        .iter()
        .flat_map(|route| &route.actions)
        .filter(|action| action.included_in_total)
        .map(|action| action.estimated_platinum)
        .sum::<f64>();
    Ok(ResourceConverterView {
        fetched_at: context.daily.fetched_at,
        inventory_observed_at: context.inventory.metadata.observed_at,
        market_source_date: context.market_source_date,
        confirmed_platinum: route_platinum + arcanes.direct_sale_platinum,
        expected_vosfor_platinum: arcanes.dissolution_expected_platinum,
        routes,
        arcanes,
        unavailable_sources: context.daily.unavailable_sources.clone(),
    })
}

struct ResourceConverterBuildContext<'a> {
    metadata: &'a GameMetadataSnapshot,
    inventory: &'a ResolvedInventorySnapshot,
    catalog: &'a ItemCatalog,
    daily: &'a DailyMarketState,
    nightwave_vendor: Option<&'a NightwaveVendorSnapshot>,
    market_source_date: Option<NaiveDate>,
    now: chrono::DateTime<Utc>,
}

fn build_syndicate_route(
    database: &Mutex<Database>,
    settings: &AppSettings,
    metadata: &GameMetadataSnapshot,
    inventory: &ResolvedInventorySnapshot,
) -> Result<ResourceConversionRoute, CoreError> {
    if inventory.syndicates.is_empty() {
        return Ok(empty_resource_route(
            ResourceSource::Syndicate,
            ResourceRouteStatus::NeedsData,
            "refresh_inventory",
        ));
    }
    if metadata.syndicate_offers.is_empty() {
        return Ok(empty_resource_route(
            ResourceSource::Syndicate,
            ResourceRouteStatus::NeedsData,
            "refresh_item_data",
        ));
    }
    let mut actions = Vec::new();
    for standing in &inventory.syndicates {
        let Some(syndicate) = syndicate_from_affiliation(standing) else {
            continue;
        };
        let Ok(balance) = u64::try_from(standing.standing.max(0)) else {
            continue;
        };
        if balance < SYNDICATE_MOD_COST {
            continue;
        }
        let quantity = u32::try_from(balance / SYNDICATE_MOD_COST).unwrap_or(u32::MAX);
        let mut best: Option<ResourceConversionAction> = None;
        for offer in metadata.syndicate_offers.iter().filter(|offer| {
            offer.syndicate == syndicate
                && syndicate_offer_accessible(standing, offer.required_title.as_str())
        }) {
            let Some(unit_price) = converter_price(
                database,
                &offer.slug,
                settings.platform,
                Some(0),
                MarketItemKind::Standard,
            )?
            else {
                continue;
            };
            let action = ResourceConversionAction {
                vendor_name: syndicate_name_ru(syndicate).to_owned(),
                currency: ResourceCurrency::Standing,
                balance,
                cost: u64::from(offer.standing_cost),
                item_slug: offer.slug.clone(),
                item_name: localized_name(
                    offer.display_name_ru.as_deref(),
                    &offer.display_name_en,
                    settings.language,
                ),
                image_url: offer.image_url.clone(),
                quantity,
                unit_price,
                estimated_platinum: unit_price * f64::from(quantity),
                included_in_total: true,
            };
            if best
                .as_ref()
                .is_none_or(|current| action.estimated_platinum > current.estimated_platinum)
            {
                best = Some(action);
            }
        }
        if let Some(best) = best {
            actions.push(best);
        }
    }
    actions.sort_by(|left, right| right.estimated_platinum.total_cmp(&left.estimated_platinum));
    Ok(ResourceConversionRoute {
        source: ResourceSource::Syndicate,
        status: if actions.is_empty() {
            ResourceRouteStatus::Unavailable
        } else {
            ResourceRouteStatus::Ready
        },
        reason: if actions.is_empty() {
            "no_accessible_priced_mod".to_owned()
        } else {
            "confirmed".to_owned()
        },
        actions,
        available_at: None,
        available_until: None,
        location: None,
    })
}

fn build_nightwave_route(
    database: &Mutex<Database>,
    settings: &AppSettings,
    context: &ResourceConverterBuildContext<'_>,
) -> Result<ResourceConversionRoute, CoreError> {
    let Some(nightwave) = context.daily.nightwave.as_ref() else {
        return Ok(empty_resource_route(
            ResourceSource::Nightwave,
            ResourceRouteStatus::NeedsData,
            "worldstate_unavailable",
        ));
    };
    if context.now < nightwave.activation || context.now > nightwave.expiry {
        return Ok(ResourceConversionRoute {
            source: ResourceSource::Nightwave,
            status: ResourceRouteStatus::Waiting,
            reason: "season_inactive".to_owned(),
            actions: Vec::new(),
            available_at: Some(nightwave.activation),
            available_until: Some(nightwave.expiry),
            location: None,
        });
    }
    let Some(currency_ref) = nightwave_currency_ref(&nightwave.tag) else {
        return Ok(empty_resource_route(
            ResourceSource::Nightwave,
            ResourceRouteStatus::NeedsData,
            "currency_not_resolved",
        ));
    };
    let balance = inventory_quantity(context.inventory, &currency_ref);
    if balance == 0 {
        return Ok(empty_resource_route(
            ResourceSource::Nightwave,
            ResourceRouteStatus::Unavailable,
            "no_currency",
        ));
    }

    let exact_vendor = context.nightwave_vendor.filter(|snapshot| {
        snapshot.expires_at > context.now
            && compact_nightwave_tag(&snapshot.season_tag) == compact_nightwave_tag(&nightwave.tag)
    });
    if let Some(vendor) = exact_vendor {
        return build_confirmed_nightwave_route(
            database,
            settings,
            context.catalog,
            vendor,
            balance,
        );
    }
    build_unconfirmed_nightwave_route(
        database,
        settings,
        context.metadata,
        balance,
        nightwave.activation,
        nightwave.expiry,
    )
}

fn build_confirmed_nightwave_route(
    database: &Mutex<Database>,
    settings: &AppSettings,
    catalog: &ItemCatalog,
    vendor: &NightwaveVendorSnapshot,
    balance: u64,
) -> Result<ResourceConversionRoute, CoreError> {
    let catalog_by_ref: HashMap<&str, &CatalogItem> = catalog
        .items
        .iter()
        .filter_map(|item| item.game_ref.as_deref().map(|game_ref| (game_ref, item)))
        .collect();
    let mut best: Option<ResourceConversionAction> = None;
    for offer in &vendor.offers {
        let cost = u64::from(offer.cred_cost);
        let Some(item) = catalog_by_ref.get(offer.game_ref.as_str()).copied() else {
            continue;
        };
        if cost == 0 || balance < cost {
            continue;
        }
        let Some(unit_price) = converter_price(
            database,
            &item.slug,
            settings.platform,
            Some(0),
            MarketItemKind::Standard,
        )?
        else {
            continue;
        };
        let quantity = u32::try_from(balance / cost).unwrap_or(u32::MAX);
        let action = ResourceConversionAction {
            vendor_name: "Нора Найт".to_owned(),
            currency: ResourceCurrency::NightwaveCred,
            balance,
            cost,
            item_slug: item.slug.clone(),
            item_name: localized_name(
                item.display_name_ru.as_deref(),
                &item.display_name_en,
                settings.language,
            ),
            image_url: catalog_item_image(item, settings.language),
            quantity,
            unit_price,
            estimated_platinum: unit_price * f64::from(quantity),
            included_in_total: true,
        };
        if best
            .as_ref()
            .is_none_or(|current| action.estimated_platinum > current.estimated_platinum)
        {
            best = Some(action);
        }
    }
    Ok(ResourceConversionRoute {
        source: ResourceSource::Nightwave,
        status: if best.is_some() {
            ResourceRouteStatus::Ready
        } else {
            ResourceRouteStatus::Unavailable
        },
        reason: if best.is_some() {
            "nightwave_stock_confirmed".to_owned()
        } else {
            "no_priced_offer".to_owned()
        },
        actions: best.into_iter().collect(),
        available_at: None,
        available_until: Some(vendor.expires_at),
        location: None,
    })
}

fn build_unconfirmed_nightwave_route(
    database: &Mutex<Database>,
    settings: &AppSettings,
    metadata: &GameMetadataSnapshot,
    balance: u64,
    activation: chrono::DateTime<Utc>,
    expiry: chrono::DateTime<Utc>,
) -> Result<ResourceConversionRoute, CoreError> {
    let mut best: Option<ResourceConversionAction> = None;
    for offer in &metadata.nightwave_offers {
        let cost = u64::from(offer.cred_cost);
        if cost == 0 || balance < cost {
            continue;
        }
        let Some(unit_price) = converter_price(
            database,
            &offer.slug,
            settings.platform,
            Some(0),
            MarketItemKind::Standard,
        )?
        else {
            continue;
        };
        let quantity = u32::try_from(balance / cost).unwrap_or(u32::MAX);
        let action = ResourceConversionAction {
            vendor_name: "Нора Найт".to_owned(),
            currency: ResourceCurrency::NightwaveCred,
            balance,
            cost,
            item_slug: offer.slug.clone(),
            item_name: localized_name(
                offer.display_name_ru.as_deref(),
                &offer.display_name_en,
                settings.language,
            ),
            image_url: offer.image_url.clone(),
            quantity,
            unit_price,
            estimated_platinum: unit_price * f64::from(quantity),
            included_in_total: false,
        };
        if best
            .as_ref()
            .is_none_or(|current| action.estimated_platinum > current.estimated_platinum)
        {
            best = Some(action);
        }
    }
    Ok(ResourceConversionRoute {
        source: ResourceSource::Nightwave,
        status: if best.is_some() {
            ResourceRouteStatus::Conditional
        } else {
            ResourceRouteStatus::Unavailable
        },
        reason: if best.is_some() {
            "refresh_nightwave_stock".to_owned()
        } else {
            "no_priced_offer".to_owned()
        },
        actions: best.into_iter().collect(),
        available_at: Some(activation),
        available_until: Some(expiry),
        location: None,
    })
}

fn compact_nightwave_tag(tag: &str) -> String {
    tag.chars()
        .filter(|character| !character.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn build_void_trader_route(
    database: &Mutex<Database>,
    settings: &AppSettings,
    inventory: &ResolvedInventorySnapshot,
    catalog: &ItemCatalog,
    daily: &DailyMarketState,
    now: chrono::DateTime<Utc>,
) -> Result<ResourceConversionRoute, CoreError> {
    let Some(trader) = daily.void_trader.as_ref() else {
        return Ok(empty_resource_route(
            ResourceSource::VoidTrader,
            ResourceRouteStatus::NeedsData,
            "worldstate_unavailable",
        ));
    };
    if now < trader.activation {
        return Ok(ResourceConversionRoute {
            source: ResourceSource::VoidTrader,
            status: ResourceRouteStatus::Waiting,
            reason: "trader_not_arrived".to_owned(),
            actions: Vec::new(),
            available_at: Some(trader.activation),
            available_until: Some(trader.expiry),
            location: Some(trader.location.clone()),
        });
    }
    if now > trader.expiry {
        return Ok(empty_resource_route(
            ResourceSource::VoidTrader,
            ResourceRouteStatus::Waiting,
            "trader_left",
        ));
    }
    let ducats = inventory_quantity(inventory, "/Lotus/Types/Items/MiscItems/PrimeBucks");
    let Some(credits) = inventory.credits else {
        return Ok(empty_resource_route(
            ResourceSource::VoidTrader,
            ResourceRouteStatus::NeedsData,
            "refresh_inventory_for_credits",
        ));
    };
    let mut best: Option<ResourceConversionAction> = None;
    for offered in &trader.inventory {
        if offered.ducats == 0 || offered.credits == 0 {
            continue;
        }
        let Some(item) = catalog_item_by_name(catalog, &offered.name) else {
            continue;
        };
        let quantity_by_ducats = ducats / u64::from(offered.ducats);
        let quantity_by_credits = credits / offered.credits;
        let quantity =
            u32::try_from(quantity_by_ducats.min(quantity_by_credits)).unwrap_or(u32::MAX);
        if quantity == 0 {
            continue;
        }
        let rank = item.max_rank.map(|_| 0);
        let Some(unit_price) = converter_price(
            database,
            &item.slug,
            settings.platform,
            rank,
            MarketItemKind::Standard,
        )?
        else {
            continue;
        };
        let action = ResourceConversionAction {
            vendor_name: "Баро Ки’Тиир".to_owned(),
            currency: ResourceCurrency::Ducat,
            balance: ducats,
            cost: u64::from(offered.ducats),
            item_slug: item.slug.clone(),
            item_name: catalog_name(item, settings.language),
            image_url: catalog_item_image(item, settings.language),
            quantity,
            unit_price,
            estimated_platinum: unit_price * f64::from(quantity),
            included_in_total: true,
        };
        if best
            .as_ref()
            .is_none_or(|current| action.estimated_platinum > current.estimated_platinum)
        {
            best = Some(action);
        }
    }
    Ok(ResourceConversionRoute {
        source: ResourceSource::VoidTrader,
        status: if best.is_some() {
            ResourceRouteStatus::Ready
        } else {
            ResourceRouteStatus::Unavailable
        },
        reason: if best.is_some() {
            "confirmed".to_owned()
        } else if trader.inventory.is_empty() {
            "inventory_not_published".to_owned()
        } else {
            "no_affordable_priced_offer".to_owned()
        },
        actions: best.into_iter().collect(),
        available_at: Some(trader.activation),
        available_until: Some(trader.expiry),
        location: Some(trader.location.clone()),
    })
}

fn build_steel_path_route(
    database: &Mutex<Database>,
    settings: &AppSettings,
    inventory: &ResolvedInventorySnapshot,
    catalog: &ItemCatalog,
    daily: &DailyMarketState,
    now: chrono::DateTime<Utc>,
) -> Result<ResourceConversionRoute, CoreError> {
    let Some(steel_path) = daily.steel_path.as_ref() else {
        return Ok(empty_resource_route(
            ResourceSource::SteelPath,
            ResourceRouteStatus::NeedsData,
            "worldstate_unavailable",
        ));
    };
    if now < steel_path.activation || now > steel_path.expiry {
        return Ok(ResourceConversionRoute {
            source: ResourceSource::SteelPath,
            status: ResourceRouteStatus::Waiting,
            reason: "rotation_inactive".to_owned(),
            actions: Vec::new(),
            available_at: Some(steel_path.activation),
            available_until: Some(steel_path.expiry),
            location: None,
        });
    }
    let balance = inventory_quantity(inventory, "/Lotus/Types/Items/MiscItems/SteelEssence");
    let reward = &steel_path.current_reward;
    let Some(item) = catalog_item_by_name(catalog, &reward.name) else {
        return Ok(ResourceConversionRoute {
            source: ResourceSource::SteelPath,
            status: ResourceRouteStatus::Unavailable,
            reason: "reward_not_tradeable".to_owned(),
            actions: Vec::new(),
            available_at: Some(steel_path.activation),
            available_until: Some(steel_path.expiry),
            location: None,
        });
    };
    if reward.cost == 0 || balance < u64::from(reward.cost) {
        return Ok(ResourceConversionRoute {
            source: ResourceSource::SteelPath,
            status: ResourceRouteStatus::Unavailable,
            reason: "insufficient_balance".to_owned(),
            actions: Vec::new(),
            available_at: Some(steel_path.activation),
            available_until: Some(steel_path.expiry),
            location: None,
        });
    }
    let item_kind = if item.tags.iter().any(|tag| tag == "riven") {
        MarketItemKind::Riven
    } else {
        MarketItemKind::Standard
    };
    if item_kind == MarketItemKind::Riven {
        return Ok(ResourceConversionRoute {
            source: ResourceSource::SteelPath,
            status: ResourceRouteStatus::Unavailable,
            reason: "reward_uses_auction_price".to_owned(),
            actions: Vec::new(),
            available_at: Some(steel_path.activation),
            available_until: Some(steel_path.expiry),
            location: None,
        });
    }
    let rank = item.max_rank.map(|_| 0);
    let Some(unit_price) =
        converter_price(database, &item.slug, settings.platform, rank, item_kind)?
    else {
        return Ok(ResourceConversionRoute {
            source: ResourceSource::SteelPath,
            status: ResourceRouteStatus::Unavailable,
            reason: "no_reliable_price".to_owned(),
            actions: Vec::new(),
            available_at: Some(steel_path.activation),
            available_until: Some(steel_path.expiry),
            location: None,
        });
    };
    let quantity = u32::try_from(balance / u64::from(reward.cost)).unwrap_or(u32::MAX);
    Ok(ResourceConversionRoute {
        source: ResourceSource::SteelPath,
        status: ResourceRouteStatus::Ready,
        reason: "confirmed".to_owned(),
        actions: vec![ResourceConversionAction {
            vendor_name: "Тешин".to_owned(),
            currency: ResourceCurrency::SteelEssence,
            balance,
            cost: u64::from(reward.cost),
            item_slug: item.slug.clone(),
            item_name: catalog_name(item, settings.language),
            image_url: catalog_item_image(item, settings.language),
            quantity,
            unit_price,
            estimated_platinum: unit_price * f64::from(quantity),
            included_in_total: true,
        }],
        available_at: Some(steel_path.activation),
        available_until: Some(steel_path.expiry),
        location: None,
    })
}

fn build_arcane_conversion(
    database: &Mutex<Database>,
    settings: &AppSettings,
    metadata: &GameMetadataSnapshot,
    inventory: &ResolvedInventorySnapshot,
    catalog_by_ref: &HashMap<&str, &CatalogItem>,
) -> Result<ArcaneConversionSummary, CoreError> {
    if metadata.arcane_dissolutions.is_empty() || metadata.arcane_packs.is_empty() {
        return Ok(empty_arcane_summary("refresh_item_data"));
    }
    let Some((best_pack_name, pack_expected_platinum, coverage)) =
        best_arcane_pack(database, settings, &metadata.arcane_packs, catalog_by_ref)?
    else {
        return Ok(empty_arcane_summary("pack_prices_missing"));
    };
    let definitions: HashMap<&str, _> = metadata
        .arcane_dissolutions
        .iter()
        .map(|definition| (definition.game_ref.as_str(), definition))
        .collect();
    let mut decisions = ArcaneDecisionBuckets::default();
    for item in &inventory.items {
        let Some(definition) = definitions.get(item.canonical_game_id.as_str()).copied() else {
            continue;
        };
        let rank = item.rank.unwrap_or(0);
        let vosfor_each = definition
            .vosfor
            .saturating_mul(arcane_rank_copy_count(rank));
        let equivalent_each = pack_expected_platinum * f64::from(vosfor_each) / VOSFOR_PACK_COST;
        let market_price = converter_price(
            database,
            &definition.slug,
            settings.platform,
            Some(rank),
            MarketItemKind::Standard,
        )?;
        let input = ArcaneDecisionInput {
            definition,
            display_name: localized_name(
                definition.display_name_ru.as_deref(),
                &definition.display_name_en,
                settings.language,
            ),
            rank,
            market_price_each: market_price,
            vosfor_each,
            equivalent_platinum_each: equivalent_each,
        };
        append_arcane_decisions(item, inventory.keep_copies, &input, &mut decisions);
    }
    for rows in [
        &mut decisions.sell,
        &mut decisions.dissolve,
        &mut decisions.hold,
    ] {
        rows.sort_by(|left, right| right.estimated_platinum.total_cmp(&left.estimated_platinum));
    }
    let direct_sale_platinum = decisions
        .sell
        .iter()
        .map(|row| row.estimated_platinum)
        .sum();
    let dissolution_expected_platinum = decisions
        .dissolve
        .iter()
        .map(|row| row.estimated_platinum)
        .sum();
    Ok(ArcaneConversionSummary {
        available: true,
        reason: "calculated".to_owned(),
        best_pack_name: Some(best_pack_name),
        pack_expected_platinum: Some(pack_expected_platinum),
        price_coverage_percent: coverage,
        sell: decisions.sell,
        dissolve: decisions.dissolve,
        hold: decisions.hold,
        direct_sale_platinum,
        dissolution_expected_platinum,
    })
}

fn best_arcane_pack(
    database: &Mutex<Database>,
    settings: &AppSettings,
    packs: &[ArcanePackDefinition],
    catalog_by_ref: &HashMap<&str, &CatalogItem>,
) -> Result<Option<(String, f64, f64)>, CoreError> {
    let mut prices = HashMap::<String, Option<f64>>::new();
    let mut best: Option<(String, f64, f64)> = None;
    for pack in packs {
        let mut pack_expected = 0.0;
        let mut pack_coverage = 0.0;
        for roll in &pack.rolls {
            let mut roll_expected = 0.0;
            let mut roll_coverage = 0.0;
            for (rarity, weight) in roll {
                if *weight <= 0.0 {
                    continue;
                }
                let components = pack
                    .components
                    .iter()
                    .filter(|component| component.rarity == *rarity)
                    .collect::<Vec<_>>();
                if components.is_empty() {
                    continue;
                }
                let mut priced_sum = 0.0;
                let mut priced_count = 0usize;
                for component in &components {
                    let price = if let Some(price) = prices.get(&component.game_ref) {
                        *price
                    } else {
                        let price =
                            if let Some(item) = catalog_by_ref.get(component.game_ref.as_str()) {
                                converter_price(
                                    database,
                                    &item.slug,
                                    settings.platform,
                                    Some(0),
                                    MarketItemKind::Standard,
                                )?
                            } else {
                                None
                            };
                        prices.insert(component.game_ref.clone(), price);
                        price
                    };
                    if let Some(price) = price {
                        priced_sum += price;
                        priced_count += 1;
                    }
                }
                let count = bounded_count(components.len());
                roll_expected += weight * (priced_sum / count);
                roll_coverage += weight * (bounded_count(priced_count) / count);
            }
            pack_expected += roll_expected;
            pack_coverage += roll_coverage;
        }
        let coverage = if pack.rolls.is_empty() {
            0.0
        } else {
            100.0 * pack_coverage / bounded_count(pack.rolls.len())
        };
        if coverage < 80.0 {
            continue;
        }
        if best
            .as_ref()
            .is_none_or(|(_, expected, _)| pack_expected > *expected)
        {
            best = Some((pack.display_name_ru.clone(), pack_expected, coverage));
        }
    }
    Ok(best)
}

#[derive(Default)]
struct ArcaneDecisionBuckets {
    sell: Vec<ArcaneConversionDecision>,
    dissolve: Vec<ArcaneConversionDecision>,
    hold: Vec<ArcaneConversionDecision>,
}

struct ArcaneDecisionInput<'a> {
    definition: &'a platscope_domain::ArcaneDissolutionDefinition,
    display_name: String,
    rank: u16,
    market_price_each: Option<f64>,
    vosfor_each: u32,
    equivalent_platinum_each: f64,
}

fn append_arcane_decisions(
    item: &ResolvedInventoryItem,
    keep_copies: u32,
    input: &ArcaneDecisionInput<'_>,
    decisions: &mut ArcaneDecisionBuckets,
) {
    let spare = item
        .owned_quantity
        .saturating_sub(keep_copies.min(item.owned_quantity));
    if spare == 0 {
        return;
    }
    if input.market_price_each.is_some_and(|price| {
        price >= input.equivalent_platinum_each * (1.0 + CONVERTER_PRICE_MARGIN)
    }) {
        let sell_quantity = item.sellable_quantity.min(spare);
        if sell_quantity > 0 {
            decisions.sell.push(arcane_decision(
                ArcaneDecisionKind::Sell,
                input,
                sell_quantity,
            ));
        }
        let dissolve_quantity = spare.saturating_sub(sell_quantity);
        if dissolve_quantity > 0 {
            decisions.dissolve.push(arcane_decision(
                ArcaneDecisionKind::Dissolve,
                input,
                dissolve_quantity,
            ));
        }
    } else if input.market_price_each.is_none_or(|price| {
        input.equivalent_platinum_each >= price * (1.0 + CONVERTER_PRICE_MARGIN)
    }) {
        decisions
            .dissolve
            .push(arcane_decision(ArcaneDecisionKind::Dissolve, input, spare));
    } else {
        decisions
            .hold
            .push(arcane_decision(ArcaneDecisionKind::Hold, input, spare));
    }
}

fn arcane_decision(
    decision: ArcaneDecisionKind,
    input: &ArcaneDecisionInput<'_>,
    quantity: u32,
) -> ArcaneConversionDecision {
    let estimated_each = if decision == ArcaneDecisionKind::Sell {
        input.market_price_each.unwrap_or_default()
    } else {
        input.equivalent_platinum_each
    };
    ArcaneConversionDecision {
        decision,
        slug: input.definition.slug.clone(),
        display_name: input.display_name.clone(),
        image_url: input.definition.image_url.clone(),
        rank: input.rank,
        quantity,
        market_price_each: input.market_price_each,
        vosfor_each: input.vosfor_each,
        vosfor_total: u64::from(input.vosfor_each) * u64::from(quantity),
        equivalent_platinum_each: input.equivalent_platinum_each,
        estimated_platinum: estimated_each * f64::from(quantity),
    }
}

fn bounded_count(value: usize) -> f64 {
    f64::from(u32::try_from(value).unwrap_or(u32::MAX))
}

const fn arcane_rank_copy_count(rank: u16) -> u32 {
    match rank {
        0 => 1,
        1 => 3,
        2 => 6,
        3 => 10,
        4 => 15,
        _ => 21,
    }
}

fn empty_arcane_summary(reason: &str) -> ArcaneConversionSummary {
    ArcaneConversionSummary {
        available: false,
        reason: reason.to_owned(),
        best_pack_name: None,
        pack_expected_platinum: None,
        price_coverage_percent: 0.0,
        sell: Vec::new(),
        dissolve: Vec::new(),
        hold: Vec::new(),
        direct_sale_platinum: 0.0,
        dissolution_expected_platinum: 0.0,
    }
}

fn empty_resource_route(
    source: ResourceSource,
    status: ResourceRouteStatus,
    reason: &str,
) -> ResourceConversionRoute {
    ResourceConversionRoute {
        source,
        status,
        reason: reason.to_owned(),
        actions: Vec::new(),
        available_at: None,
        available_until: None,
        location: None,
    }
}

fn converter_price(
    database: &Mutex<Database>,
    slug: &str,
    platform: Platform,
    rank: Option<u16>,
    item_kind: MarketItemKind,
) -> Result<Option<f64>, CoreError> {
    let key = MarketVariantKey::new(slug, platform, rank, None::<String>).map_err(|error| {
        CoreError::MetadataData(format!("invalid converter market identity: {error}"))
    })?;
    let recommendation = PricingService::price_current_variant(database, &key, item_kind)?;
    Ok(recommendation.and_then(|recommendation| {
        let credible = matches!(
            recommendation.confidence,
            PriceConfidence::High | PriceConfidence::Medium
        ) && matches!(
            recommendation.freshness,
            PriceFreshness::Fresh | PriceFreshness::Aging
        );
        credible.then_some(recommendation.fair_price).flatten()
    }))
}

fn inventory_quantity(inventory: &ResolvedInventorySnapshot, game_ref: &str) -> u64 {
    inventory
        .items
        .iter()
        .filter(|item| item.canonical_game_id == game_ref)
        .map(|item| u64::from(item.owned_quantity))
        .sum()
}

fn syndicate_from_affiliation(standing: &SyndicateStanding) -> Option<&'static str> {
    let normalized = standing
        .tag
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect::<String>();
    if normalized.contains("steelmeridian") {
        Some("Steel Meridian")
    } else if normalized.contains("arbiters") {
        Some("Arbiters of Hexis")
    } else if normalized.contains("cephalonsuda") {
        Some("Cephalon Suda")
    } else if normalized.contains("perrin") {
        Some("The Perrin Sequence")
    } else if normalized.contains("redveil") {
        Some("Red Veil")
    } else if normalized.contains("newloka") {
        Some("New Loka")
    } else {
        None
    }
}

fn syndicate_offer_accessible(standing: &SyndicateStanding, required_title: &str) -> bool {
    let Some(title) = standing.title.as_deref() else {
        return false;
    };
    if title.eq_ignore_ascii_case(required_title) {
        return true;
    }
    let Some(syndicate) = syndicate_from_affiliation(standing) else {
        return false;
    };
    title.eq_ignore_ascii_case(syndicate_max_title(syndicate))
}

fn syndicate_max_title(syndicate: &str) -> &'static str {
    match syndicate {
        "Steel Meridian" => "General",
        "Arbiters of Hexis" => "Maxim",
        "Cephalon Suda" => "Genius",
        "The Perrin Sequence" => "Partner",
        "Red Veil" => "Exalted",
        "New Loka" => "Flawless",
        _ => "",
    }
}

fn syndicate_name_ru(syndicate: &str) -> &'static str {
    match syndicate {
        "Steel Meridian" => "Стальной Меридиан",
        "Arbiters of Hexis" => "Арбитры Гексиса",
        "Cephalon Suda" => "Цефалон Суда",
        "The Perrin Sequence" => "Последовательность Перрина",
        "Red Veil" => "Красная Вуаль",
        "New Loka" => "Новая Лока",
        _ => "Синдикат",
    }
}

fn nightwave_currency_ref(tag: &str) -> Option<String> {
    let marker = "Intermission";
    let suffix = tag.split_once(marker)?.1;
    let digits = suffix
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    let number: u8 = digits.parse().ok()?;
    let word = match number {
        1 => "One",
        2 => "Two",
        3 => "Three",
        4 => "Four",
        5 => "Five",
        6 => "Six",
        7 => "Seven",
        8 => "Eight",
        9 => "Nine",
        10 => "Ten",
        11 => "Eleven",
        12 => "Twelve",
        13 => "Thirteen",
        14 => "Fourteen",
        15 => "Fifteen",
        16 => "Sixteen",
        17 => "Seventeen",
        18 => "Eighteen",
        19 => "Nineteen",
        20 => "Twenty",
        _ => return None,
    };
    Some(format!(
        "/Lotus/Types/Items/MiscItems/NoraIntermission{word}Creds"
    ))
}

fn catalog_item_by_name<'a>(catalog: &'a ItemCatalog, name: &str) -> Option<&'a CatalogItem> {
    let normalized = name.trim();
    catalog
        .items
        .iter()
        .find(|item| item.display_name_en.eq_ignore_ascii_case(normalized))
}

fn localized_name(ru: Option<&str>, en: &str, language: Language) -> String {
    match language {
        Language::Russian => ru
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(en)
            .to_owned(),
        Language::English => en.to_owned(),
    }
}

fn catalog_name(item: &CatalogItem, language: Language) -> String {
    localized_name(
        item.display_name_ru.as_deref(),
        &item.display_name_en,
        language,
    )
}

fn catalog_item_image(item: &CatalogItem, language: Language) -> Option<String> {
    let thumb = match language {
        Language::Russian => item.thumb_ru.as_ref().or(item.thumb.as_ref()),
        Language::English => item.thumb.as_ref(),
    }?;
    Some(market_image_url(thumb))
}

impl AccountService {
    /// Создаёт opt-in account service с OS keychain и отдельным WFM client.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`] при некорректном device ID или невозможности создать HTTP client.
    pub fn production(device_id: String) -> Result<Self, CoreError> {
        if device_id.trim().len() < 6 || device_id.len() > 256 {
            return Err(CoreError::AccountData("invalid account device ID".into()));
        }
        Ok(Self {
            client: WfmAccountClient::production()?,
            credentials: Arc::new(OsCredentialStore),
            operation_lock: tokio::sync::Mutex::new(()),
            device_id,
        })
    }

    /// Возвращает disconnected state или актуальные private profile/orders.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`] при keychain, network, auth или schema error.
    pub async fn view(&self) -> Result<AccountView, CoreError> {
        let _operation_guard = self.operation_lock.lock().await;
        let Some(token) = self.credentials.load()? else {
            return Ok(disconnected_account_view());
        };
        self.view_with_token(&token).await
    }

    /// Выполняет одноразовый sign-in, сохраняет только token и возвращает account view.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`] при rejected credentials, keychain или account API error.
    pub async fn connect(&self, email: &str, password: &str) -> Result<AccountView, CoreError> {
        let _operation_guard = self.operation_lock.lock().await;
        let token = self
            .client
            .sign_in(email, password, &self.device_id)
            .await?;
        self.credentials.save(&token)?;
        match self.view_with_token(&token).await {
            Ok(view) => Ok(view),
            Err(error) => {
                self.credentials.delete()?;
                Err(error)
            }
        }
    }

    /// Завершает remote session по возможности и всегда удаляет local credential.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`] только если OS keychain не смог удалить credential.
    pub async fn disconnect(&self) -> Result<bool, CoreError> {
        let _operation_guard = self.operation_lock.lock().await;
        let token = self.credentials.load()?;
        let remotely_revoked = match token.as_ref() {
            Some(token) => self.client.sign_out(token).await.is_ok(),
            None => true,
        };
        self.credentials.delete()?;
        Ok(remotely_revoked)
    }

    /// Создаёт listing только после явного подтверждения caller-а.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`] без confirmation, credential или при rejected API write.
    pub async fn create_listing(
        &self,
        input: &CreateListingInput,
        confirmed: bool,
    ) -> Result<AccountOrder, CoreError> {
        require_write_confirmation(confirmed)?;
        let _operation_guard = self.operation_lock.lock().await;
        let token = self.require_token()?;
        Ok(self.client.create_order(&token, input).await?)
    }

    /// Изменяет listing только после явного подтверждения caller-а.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`] без confirmation, credential или при rejected API write.
    pub async fn update_listing(
        &self,
        id: &str,
        input: &UpdateListingInput,
        confirmed: bool,
    ) -> Result<AccountOrder, CoreError> {
        require_write_confirmation(confirmed)?;
        let _operation_guard = self.operation_lock.lock().await;
        let token = self.require_token()?;
        Ok(self.client.update_order(&token, id, input).await?)
    }

    /// Удаляет listing только после явного подтверждения caller-а.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`] без confirmation, credential или при rejected API write.
    pub async fn delete_listing(
        &self,
        id: &str,
        confirmed: bool,
    ) -> Result<AccountOrder, CoreError> {
        require_write_confirmation(confirmed)?;
        let _operation_guard = self.operation_lock.lock().await;
        let token = self.require_token()?;
        Ok(self.client.delete_order(&token, id).await?)
    }

    async fn view_with_token(
        &self,
        token: &platscope_account::AccountToken,
    ) -> Result<AccountView, CoreError> {
        let profile = self.client.me(token).await?;
        let orders = self.client.my_orders(token).await?;
        Ok(AccountView {
            connected: true,
            profile: Some(profile),
            orders,
            order_items: HashMap::new(),
        })
    }

    fn require_token(&self) -> Result<platscope_account::AccountToken, CoreError> {
        self.credentials
            .load()?
            .ok_or_else(|| CoreError::AccountData("WFM account is not connected".into()))
    }
}

fn disconnected_account_view() -> AccountView {
    AccountView {
        connected: false,
        profile: None,
        orders: Vec::new(),
        order_items: HashMap::new(),
    }
}

/// Добавляет к WFM-ордерам локализованные имена и изображения из текущего каталога.
///
/// # Errors
///
/// Возвращает [`CoreError`] при недоступном локальном каталоге.
pub fn enrich_account_view(
    database: &Mutex<Database>,
    language: Language,
    mut view: AccountView,
) -> Result<AccountView, CoreError> {
    let database = lock_database(database)?;
    let Some(catalog) = database.load_current_catalog()? else {
        return Ok(view);
    };
    let component_images = database
        .load_current_game_metadata()?
        .as_ref()
        .map_or_else(HashMap::new, component_image_urls_by_slug);
    drop(database);
    let wanted: std::collections::HashSet<&str> = view
        .orders
        .iter()
        .filter_map(|order| order.item_id.as_deref())
        .collect();
    view.order_items = catalog
        .items
        .into_iter()
        .filter(|item| wanted.contains(item.item_id.as_str()))
        .map(|item| {
            let item_kind = market_item_kind_from_slug(&item.tags, &item.slug);
            let display_name_en = item.display_name_en;
            let display_name = match language {
                Language::Russian => item
                    .display_name_ru
                    .unwrap_or_else(|| display_name_en.clone()),
                Language::English => display_name_en.clone(),
            };
            let thumb = match language {
                Language::Russian => item.thumb_ru.or(item.thumb),
                Language::English => item.thumb,
            };
            (
                item.item_id,
                AccountOrderItemView {
                    image_url: component_images
                        .get(&item.slug)
                        .cloned()
                        .or_else(|| thumb.map(|thumb| market_image_url(&thumb))),
                    slug: item.slug,
                    display_name,
                    display_name_en,
                    item_kind,
                },
            )
        })
        .collect();
    Ok(view)
}

fn market_image_url(thumb: &str) -> String {
    format!("https://warframe.market/static/assets/{thumb}")
}

fn require_write_confirmation(confirmed: bool) -> Result<(), CoreError> {
    if confirmed {
        Ok(())
    } else {
        Err(CoreError::AccountData(
            "explicit confirmation is required for every WFM write".into(),
        ))
    }
}

type InsightCatalogIdentity = (String, String, Option<String>);

fn load_insight_catalog(
    database: &Mutex<Database>,
    language: Language,
) -> Result<HashMap<String, InsightCatalogIdentity>, CoreError> {
    Ok(lock_database(database)?
        .load_current_catalog()?
        .map(|catalog| {
            catalog
                .items
                .into_iter()
                .map(|item| {
                    let display_name = match language {
                        Language::Russian => item
                            .display_name_ru
                            .filter(|name| !name.trim().is_empty())
                            .unwrap_or(item.display_name_en),
                        Language::English => item.display_name_en,
                    };
                    let thumb = match language {
                        Language::Russian => item.thumb_ru.or(item.thumb),
                        Language::English => item.thumb,
                    };
                    (item.slug, (item.item_id, display_name, thumb))
                })
                .collect()
        })
        .unwrap_or_default())
}

fn insight_image_url(
    slug: &str,
    inventory: &[InventoryViewItem],
    catalog: &HashMap<String, InsightCatalogIdentity>,
) -> Option<String> {
    inventory
        .iter()
        .find(|item| item.key.as_ref().is_some_and(|key| key.slug == slug))
        .and_then(|item| item.image_url.clone())
        .or_else(|| {
            catalog
                .get(slug)
                .and_then(|(_, _, thumb)| thumb.as_deref())
                .map(market_image_url)
        })
}

fn build_set_components(
    database: &Mutex<Database>,
    settings: &AppSettings,
    definition: &PrimeSetDefinition,
    inventory: &[InventoryViewItem],
    catalog: &HashMap<String, InsightCatalogIdentity>,
) -> Result<Vec<SetComponentInsight>, CoreError> {
    definition
        .components
        .iter()
        .map(|component| {
            let catalog_item = catalog.get(&component.slug);
            Ok(SetComponentInsight {
                definition: component.clone(),
                item_id: catalog_item.map(|(item_id, _, _)| item_id.clone()),
                display_name: catalog_item.map_or_else(
                    || component.slug.replace('_', " "),
                    |(_, display_name, _)| display_name.clone(),
                ),
                image_url: component
                    .image_url
                    .clone()
                    .or_else(|| insight_image_url(&component.slug, inventory, catalog)),
                owned_quantity: sellable_quantity(inventory, &component.slug),
                recommendation: price_slug(
                    database,
                    &component.slug,
                    settings.platform,
                    None,
                    MarketItemKind::Standard,
                )?,
            })
        })
        .collect()
}

fn build_set_insights(
    database: &Mutex<Database>,
    settings: &AppSettings,
    definitions: &[PrimeSetDefinition],
    inventory: &[InventoryViewItem],
) -> Result<Vec<SetInsightRow>, CoreError> {
    let catalog_by_slug = load_insight_catalog(database, settings.language)?;
    let mut rows = Vec::new();
    for definition in definitions {
        let components =
            build_set_components(database, settings, definition, inventory, &catalog_by_slug)?;
        if components
            .iter()
            .all(|component| component.owned_quantity == 0)
        {
            continue;
        }
        let set_recommendation = price_slug(
            database,
            &definition.set_slug,
            settings.platform,
            None,
            MarketItemKind::Standard,
        )?;
        let part_inputs = components
            .iter()
            .map(|component| SetPartInput {
                slug: &component.definition.slug,
                required_quantity: component.definition.required_quantity,
                owned_quantity: component.owned_quantity,
                fair_price: component
                    .recommendation
                    .as_ref()
                    .and_then(|recommendation| recommendation.fair_price),
                closed_volume: component
                    .recommendation
                    .as_ref()
                    .and_then(|recommendation| recommendation.closed_volume),
                confidence: component.recommendation.as_ref().map_or(
                    platscope_domain::PriceConfidence::Unknown,
                    |recommendation| recommendation.confidence,
                ),
            })
            .collect::<Vec<_>>();
        let comparison = compare_set(SetComparisonInput {
            set_slug: &definition.set_slug,
            set_fair_price: set_recommendation
                .as_ref()
                .and_then(|recommendation| recommendation.fair_price),
            set_closed_volume: set_recommendation
                .as_ref()
                .and_then(|recommendation| recommendation.closed_volume),
            set_confidence: set_recommendation.as_ref().map_or(
                platscope_domain::PriceConfidence::Unknown,
                |recommendation| recommendation.confidence,
            ),
            parts: &part_inputs,
        });
        rows.push(SetInsightRow {
            definition: definition.clone(),
            item_id: catalog_by_slug
                .get(&definition.set_slug)
                .map(|(item_id, _, _)| item_id.clone()),
            display_name: catalog_by_slug.get(&definition.set_slug).map_or_else(
                || definition.display_name_en.clone(),
                |(_, display_name, _)| display_name.clone(),
            ),
            image_url: insight_image_url(&definition.set_slug, inventory, &catalog_by_slug),
            set_recommendation,
            comparison,
            components,
        });
    }
    Ok(rows)
}

fn build_relic_insights(
    database: &Mutex<Database>,
    settings: &AppSettings,
    definitions: &[RelicDefinition],
    prime_sets: &[PrimeSetDefinition],
    inventory: &[InventoryViewItem],
) -> Result<Vec<RelicInsightRow>, CoreError> {
    let catalog_by_slug = load_insight_catalog(database, settings.language)?;
    let component_images = prime_sets
        .iter()
        .flat_map(|set| &set.components)
        .filter_map(|component| {
            component
                .image_url
                .as_ref()
                .map(|image_url| (component.slug.as_str(), image_url.clone()))
        })
        .collect::<HashMap<_, _>>();
    let mut rows = Vec::new();
    for definition in definitions {
        let subtype = definition.refinement.market_subtype();
        let Some(owned) = inventory.iter().find(|item| {
            matches!(
                item.resolution,
                InventoryResolution::Resolved | InventoryResolution::ExactVariantUnavailable
            ) && item.key.as_ref().is_some_and(|key| {
                key.slug == definition.relic_slug && key.subtype.as_deref() == Some(subtype)
            })
        }) else {
            continue;
        };
        let relic_recommendation = price_slug(
            database,
            &definition.relic_slug,
            settings.platform,
            Some(subtype),
            MarketItemKind::Relic,
        )?;
        let mut rewards = Vec::with_capacity(definition.rewards.len());
        for reward in &definition.rewards {
            let recommendation = reward
                .reward_slug
                .as_deref()
                .map(|slug| {
                    price_slug(
                        database,
                        slug,
                        settings.platform,
                        None,
                        MarketItemKind::Standard,
                    )
                })
                .transpose()?
                .flatten();
            rewards.push(RelicRewardInsight {
                definition: reward.clone(),
                display_name: reward.reward_slug.as_deref().map_or_else(
                    || reward.display_name_en.clone(),
                    |slug| {
                        catalog_by_slug.get(slug).map_or_else(
                            || reward.display_name_en.clone(),
                            |(_, display_name, _)| display_name.clone(),
                        )
                    },
                ),
                image_url: reward.reward_slug.as_deref().and_then(|slug| {
                    component_images
                        .get(slug)
                        .cloned()
                        .or_else(|| insight_image_url(slug, inventory, &catalog_by_slug))
                }),
                recommendation,
            });
        }
        let reward_inputs = rewards
            .iter()
            .map(|reward| RelicRewardInput {
                reward_slug: reward.definition.reward_slug.as_deref(),
                chance_percent: reward.definition.chance_percent,
                fair_price: reward
                    .recommendation
                    .as_ref()
                    .and_then(|recommendation| recommendation.fair_price),
                confidence: reward.recommendation.as_ref().map_or(
                    platscope_domain::PriceConfidence::Unknown,
                    |recommendation| recommendation.confidence,
                ),
            })
            .collect::<Vec<_>>();
        rows.push(RelicInsightRow {
            definition: definition.clone(),
            display_name: catalog_by_slug.get(&definition.relic_slug).map_or_else(
                || definition.display_name_en.clone(),
                |(_, display_name, _)| display_name.clone(),
            ),
            image_url: owned
                .image_url
                .clone()
                .or_else(|| insight_image_url(&definition.relic_slug, inventory, &catalog_by_slug)),
            owned_quantity: owned.owned_quantity,
            sellable_quantity: owned.sellable_quantity,
            relic_recommendation,
            expected_value: calculate_relic_ev(&reward_inputs),
            rewards,
        });
    }
    Ok(rows)
}

fn build_ducat_insights(
    database: &Mutex<Database>,
    settings: &AppSettings,
    metadata: &[PrimePartMetadata],
    inventory: &[InventoryViewItem],
) -> Result<Vec<DucatInsightRow>, CoreError> {
    let mut rows = Vec::new();
    for part in metadata {
        let matching = inventory.iter().filter(|item| {
            item.resolution == InventoryResolution::Resolved
                && item.key.as_ref().is_some_and(|key| key.slug == part.slug)
        });
        let (owned_quantity, sellable_quantity, display_name, image_url) = matching.fold(
            (0_u32, 0_u32, None, None),
            |(owned, sellable, display, image), item| {
                (
                    owned.saturating_add(item.owned_quantity),
                    sellable.saturating_add(item.sellable_quantity),
                    display.or_else(|| Some(item.display_name.clone())),
                    image.or_else(|| item.image_url.clone()),
                )
            },
        );
        if owned_quantity == 0 {
            continue;
        }
        let recommendation = price_slug(
            database,
            &part.slug,
            settings.platform,
            None,
            MarketItemKind::Standard,
        )?;
        let efficiency = calculate_ducat_efficiency(
            recommendation
                .as_ref()
                .and_then(|recommendation| recommendation.fair_price),
            part.ducats,
            recommendation.as_ref().map_or(
                platscope_domain::PriceConfidence::Unknown,
                |recommendation| recommendation.confidence,
            ),
        );
        rows.push(DucatInsightRow {
            metadata: part.clone(),
            display_name: display_name.unwrap_or_else(|| part.slug.clone()),
            image_url,
            owned_quantity,
            sellable_quantity,
            recommendation,
            efficiency,
        });
    }
    Ok(rows)
}

fn sellable_quantity(inventory: &[InventoryViewItem], slug: &str) -> u32 {
    inventory
        .iter()
        .filter(|item| {
            item.resolution == InventoryResolution::Resolved
                && item.key.as_ref().is_some_and(|key| key.slug == slug)
        })
        .fold(0_u32, |total, item| {
            total.saturating_add(item.sellable_quantity)
        })
}

fn price_slug(
    database: &Mutex<Database>,
    slug: &str,
    platform: Platform,
    subtype: Option<&str>,
    item_kind: MarketItemKind,
) -> Result<Option<PriceRecommendation>, CoreError> {
    let key = MarketVariantKey::new(slug, platform, None, subtype).map_err(|error| {
        CoreError::MetadataData(format!("invalid normalized market identity: {error}"))
    })?;
    PricingService::price_current_variant(database, &key, item_kind)
}

impl SellNowService {
    /// Соединяет локальный inventory LKG с bulk pricing и историческим timing.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`] при недоступной DB или нарушенном persisted state.
    pub fn view(
        database: &Mutex<Database>,
        settings: &AppSettings,
    ) -> Result<Option<SellNowView>, CoreError> {
        let Some(inventory) = InventoryService::view(database, settings)? else {
            return Ok(None);
        };
        let market_snapshot = lock_database(database)?.current_market_snapshot()?;
        let inventory_metadata = inventory.metadata;
        let inventory_summary = inventory.summary;
        let keep_copies = inventory.keep_copies;
        let mod_usage_scanned = inventory.mod_usage_scanned;
        let inventory_nominal_value = inventory_nominal_value(database, &inventory.items)?;
        let mut rows = Vec::new();
        for item in inventory.items {
            let (item_kind, recommendation) = if item.resolution == InventoryResolution::Resolved {
                if let Some(key) = item.key.as_ref() {
                    let item_kind = market_item_kind(&item.tags, key);
                    let recommendation =
                        PricingService::price_current_variant(database, key, item_kind)?;
                    (item_kind, recommendation)
                } else {
                    (MarketItemKind::Standard, None)
                }
            } else {
                (MarketItemKind::Standard, None)
            };
            rows.push(build_sell_now_row(
                database,
                item,
                item_kind,
                recommendation,
                None,
            )?);
        }
        rows.sort_by(|left, right| {
            right
                .priority
                .score
                .cmp(&left.priority.score)
                .then_with(|| {
                    left.inventory
                        .display_name
                        .cmp(&right.inventory.display_name)
                })
        });
        let summary = sell_now_summary(&rows, inventory_nominal_value);
        Ok(Some(SellNowView {
            inventory_metadata,
            inventory_summary,
            keep_copies,
            mod_usage_scanned,
            market_snapshot,
            summary,
            rows,
        }))
    }

    /// Обогащает одну sellable строку live WFM, не создавая сетевой burst по всей таблице.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`] при live/provider, DB или inventory ошибке.
    pub async fn live_row(
        live_pricing_service: &LivePricingService,
        database: &Mutex<Database>,
        key: &MarketVariantKey,
        settings: &AppSettings,
    ) -> Result<Option<LiveSellNowResult>, CoreError> {
        let Some(inventory) = InventoryService::view(database, settings)? else {
            return Ok(None);
        };
        let Some(item) = inventory.items.into_iter().find(|item| {
            item.sellable_quantity > 0
                && item.resolution == InventoryResolution::Resolved
                && item.key.as_ref() == Some(key)
        }) else {
            return Ok(None);
        };
        let item_kind = market_item_kind(&item.tags, key);
        let Some(live) = live_pricing_service
            .price_current_variant(database, key, item_kind, settings)
            .await?
        else {
            return Ok(None);
        };
        let LivePricingResult {
            recommendation,
            fetched_at,
            quote_state,
            sell_order_count,
            buy_order_count,
            orders: _,
            warning,
        } = live;
        let live_lowest_ask = recommendation.lowest_ask;
        let row = build_sell_now_row(
            database,
            item,
            item_kind,
            Some(recommendation),
            live_lowest_ask,
        )?;
        Ok(Some(LiveSellNowResult {
            row,
            fetched_at,
            quote_state,
            sell_order_count,
            buy_order_count,
            warning,
        }))
    }
}

fn build_sell_now_row(
    database: &Mutex<Database>,
    item: InventoryViewItem,
    item_kind: MarketItemKind,
    recommendation: Option<PriceRecommendation>,
    live_lowest_ask: Option<f64>,
) -> Result<SellNowRow, CoreError> {
    let trend = recommendation
        .as_ref()
        .map(|recommendation| {
            HistoryService::view(
                database,
                &recommendation.key,
                90,
                recommendation.fair_price,
                live_lowest_ask,
            )
            .map(|history| history.trend)
        })
        .transpose()?;
    let priority = calculate_priority(SellPriorityInput {
        sellable_quantity: item.sellable_quantity,
        fair_price: recommendation
            .as_ref()
            .and_then(|recommendation| recommendation.fair_price),
        closed_volume: recommendation
            .as_ref()
            .and_then(|recommendation| recommendation.closed_volume),
        confidence: recommendation.as_ref().map_or(
            platscope_domain::PriceConfidence::Unknown,
            |recommendation| recommendation.confidence,
        ),
        timing: trend.as_ref().and_then(|trend| trend.timing),
    });
    let nominal_value = nominal_value(
        item.sellable_quantity,
        recommendation
            .as_ref()
            .and_then(|recommendation| recommendation.fair_price),
    );
    Ok(SellNowRow {
        inventory: item,
        item_kind,
        recommendation,
        trend,
        priority,
        nominal_value,
    })
}

fn inventory_nominal_value(
    database: &Mutex<Database>,
    items: &[InventoryViewItem],
) -> Result<f64, CoreError> {
    let mut total = 0.0;
    for item in items
        .iter()
        .filter(|item| item.resolution == InventoryResolution::Resolved)
    {
        let Some(key) = item.key.as_ref() else {
            continue;
        };
        let recommendation = PricingService::price_current_variant(
            database,
            key,
            market_item_kind(&item.tags, key),
        )?;
        total += nominal_value(
            item.owned_quantity,
            recommendation.and_then(|value| value.fair_price),
        )
        .unwrap_or(0.0);
    }
    Ok(total)
}

fn sell_now_summary(rows: &[SellNowRow], inventory_nominal_value: f64) -> SellNowSummary {
    SellNowSummary {
        candidate_rows: rows
            .iter()
            .filter(|row| {
                row.inventory.sellable_quantity > 0
                    && row.inventory.resolution == InventoryResolution::Resolved
            })
            .count(),
        priced_rows: rows
            .iter()
            .filter(|row| {
                row.inventory.sellable_quantity > 0
                    && row
                        .recommendation
                        .as_ref()
                        .and_then(|recommendation| recommendation.fair_price)
                        .is_some()
            })
            .count(),
        high_priority_rows: rows
            .iter()
            .filter(|row| row.inventory.sellable_quantity > 0 && row.priority.score >= 50)
            .count(),
        inventory_nominal_value,
        nominal_value: rows.iter().filter_map(|row| row.nominal_value).sum(),
    }
}

fn market_item_kind(tags: &[String], key: &MarketVariantKey) -> MarketItemKind {
    market_item_kind_from_slug(tags, &key.slug)
}

fn market_item_kind_from_slug(tags: &[String], slug: &str) -> MarketItemKind {
    if tags.iter().any(|tag| tag == "riven") || slug.contains("_riven_") {
        MarketItemKind::Riven
    } else if tags.iter().any(|tag| tag == "relic") || slug.ends_with("_relic") {
        MarketItemKind::Relic
    } else {
        MarketItemKind::Standard
    }
}

/// В старых рыночных снимках обычный вариант мода не имел subtype. Текущий WFM
/// требует `regular` для предметов, у которых появился альтернативный вариант
/// `atragraph`, поэтому восстанавливаем точную торговую идентичность из каталога.
fn relink_implicit_regular_inventory(
    snapshot: &ResolvedInventorySnapshot,
    catalog: Option<&ItemCatalog>,
    available_variants: &HashSet<MarketVariantKey>,
    platform: Platform,
) -> ResolvedInventorySnapshot {
    let Some(catalog) = catalog else {
        return snapshot.clone();
    };
    let implicit_regular_slugs: HashSet<&str> = catalog
        .items
        .iter()
        .filter(|item| item.subtypes.iter().any(|subtype| subtype == "regular"))
        .map(|item| item.slug.as_str())
        .collect();
    let mut repaired = snapshot.clone();
    for item in &mut repaired.items {
        let Some(key) = item.key.as_mut() else {
            continue;
        };
        if key.subtype.is_some() || !implicit_regular_slugs.contains(key.slug.as_str()) {
            continue;
        }
        key.platform = platform;
        key.subtype = Some("regular".to_owned());
        item.subtype = Some("regular".to_owned());
        item.resolution = if market_variant_available(available_variants, key) {
            InventoryResolution::Resolved
        } else {
            InventoryResolution::ExactVariantUnavailable
        };
    }
    apply_keep_copies(&repaired, repaired.keep_copies)
}

fn market_variant_available(
    available_variants: &HashSet<MarketVariantKey>,
    key: &MarketVariantKey,
) -> bool {
    available_variants.iter().any(|candidate| {
        candidate.slug == key.slug
            && candidate.rank == key.rank
            && (candidate.subtype == key.subtype
                || (key.subtype.as_deref() == Some("regular") && candidate.subtype.is_none()))
            && candidate.amber_stars == key.amber_stars
            && candidate.cyan_stars == key.cyan_stars
    })
}

/// Восстанавливает точный рыночный вариант реликвии по неизменяемому игровому пути WFCD.
///
/// `warframe.market` хранит одну карточку реликвии с subtype, а read-only inventory отдаёт
/// отдельный `ItemType` для каждого уровня улучшения. Каталог предметов не обязан содержать
/// эти четыре игровых пути, поэтому обычный resolver не может связать их самостоятельно.
/// Сопоставление здесь только точное: неизвестные или неоднозначные пути не изменяются.
fn relink_exact_relic_inventory(
    snapshot: &ResolvedInventorySnapshot,
    catalog: Option<&ItemCatalog>,
    metadata: Option<&GameMetadataSnapshot>,
    available_variants: &HashSet<MarketVariantKey>,
    platform: Platform,
) -> ResolvedInventorySnapshot {
    let Some(metadata) = metadata else {
        return snapshot.clone();
    };

    let mut aliases: HashMap<String, Option<&RelicDefinition>> = HashMap::new();
    for definition in &metadata.relics {
        let identity = definition.relic_game_ref.trim().to_lowercase();
        match aliases.entry(identity) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(Some(definition));
            }
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                let same_variant = entry.get().is_some_and(|existing| {
                    existing.relic_slug == definition.relic_slug
                        && existing.refinement == definition.refinement
                });
                if !same_variant {
                    entry.insert(None);
                }
            }
        }
    }
    let catalog_by_slug: HashMap<&str, _> = catalog.map_or_else(HashMap::new, |catalog| {
        catalog
            .items
            .iter()
            .map(|item| (item.slug.as_str(), item))
            .collect()
    });

    let mut repaired = snapshot.clone();
    for item in &mut repaired.items {
        let identity = item.canonical_game_id.trim().to_lowercase();
        let Some(Some(definition)) = aliases.get(&identity) else {
            continue;
        };
        let subtype = definition.refinement.market_subtype().to_owned();
        let key = MarketVariantKey::new(
            definition.relic_slug.clone(),
            platform,
            None,
            Some(subtype.clone()),
        )
        .expect("validated relic metadata creates a market key");
        let resolution = if available_variants.iter().any(|candidate| {
            candidate.slug == key.slug
                && candidate.rank == key.rank
                && candidate.subtype == key.subtype
                && candidate.amber_stars == key.amber_stars
                && candidate.cyan_stars == key.cyan_stars
        }) {
            InventoryResolution::Resolved
        } else {
            InventoryResolution::ExactVariantUnavailable
        };

        if let Some(catalog_item) = catalog_by_slug.get(definition.relic_slug.as_str()) {
            item.display_name_en = Some(catalog_item.display_name_en.clone());
            item.display_name_ru
                .clone_from(&catalog_item.display_name_ru);
            item.tags.clone_from(&catalog_item.tags);
        } else {
            item.display_name_en = Some(definition.display_name_en.clone());
            item.tags = vec!["relic".into()];
        }
        item.key = Some(key);
        item.rank = None;
        item.subtype = Some(subtype);
        item.resolution = resolution;
    }
    apply_keep_copies(&repaired, repaired.keep_copies)
}

fn inventory_view_from_snapshot(
    snapshot: &ResolvedInventorySnapshot,
    language: Language,
    platform: Platform,
    keep_copies: u32,
) -> InventoryView {
    let mut snapshot = apply_keep_copies(snapshot, keep_copies);
    for item in &mut snapshot.items {
        if let Some(key) = &mut item.key {
            key.platform = platform;
        }
    }
    let mut items: Vec<InventoryViewItem> = snapshot
        .items
        .into_iter()
        .filter(inventory_item_visible)
        .map(|item| {
            let display_name = match language {
                Language::Russian => item
                    .display_name_ru
                    .clone()
                    .or(item.display_name_en.clone()),
                Language::English => item.display_name_en.clone(),
            }
            .unwrap_or_else(|| match language {
                Language::Russian => "Неизвестный предмет".to_owned(),
                Language::English => "Unknown item".to_owned(),
            });
            let equipped_placements = item
                .equipped_placements
                .into_iter()
                .map(|placement| {
                    let equipment_display_name = match language {
                        Language::Russian => placement
                            .equipment_display_name_ru
                            .or(placement.equipment_display_name_en),
                        Language::English => placement.equipment_display_name_en,
                    }
                    .unwrap_or_else(|| placement.equipment_game_id.clone());
                    EquippedModPlacementView {
                        equipment_instance_key: placement.equipment_instance_key,
                        equipment_game_id: placement.equipment_game_id,
                        equipment_display_name,
                        equipment_image_url: placement.equipment_image_url,
                        equipment_kind: placement.equipment_kind,
                        config_index: placement.config_index,
                    }
                })
                .collect();
            InventoryViewItem {
                canonical_game_id: item.canonical_game_id,
                item_id: None,
                bulk_tradable: false,
                display_name,
                image_url: None,
                tags: item.tags,
                key: item.key,
                rank: item.rank,
                subtype: item.subtype,
                owned_quantity: item.tradeable_quantity,
                tradeable_quantity: item.tradeable_quantity,
                untradeable_quantity: 0,
                unknown_quantity: 0,
                leveled_quantity: 0,
                equipped_quantity: item.equipped_quantity,
                equipped_placements,
                sellable_quantity: item.sellable_quantity,
                resolution: item.resolution,
                vault_status: VaultStatus::Unknown,
                closed_median_48h: None,
                has_reliable_price: false,
            }
        })
        .collect();
    items.sort_by_cached_key(|item| item.display_name.to_lowercase());
    let summary = InventorySummary {
        owned_quantity: items
            .iter()
            .map(|item| u64::from(item.owned_quantity))
            .sum(),
        sellable_quantity: items
            .iter()
            .map(|item| u64::from(item.sellable_quantity))
            .sum(),
        resolved_rows: items
            .iter()
            .filter(|item| item.resolution == InventoryResolution::Resolved)
            .count(),
        attention_rows: items
            .iter()
            .filter(|item| {
                item.resolution != InventoryResolution::Resolved || item.unknown_quantity > 0
            })
            .count(),
    };
    InventoryView {
        metadata: snapshot.metadata,
        keep_copies,
        mod_usage_scanned: snapshot.mod_usage_scanned,
        summary,
        items,
    }
}

fn inventory_item_visible(item: &ResolvedInventoryItem) -> bool {
    matches!(
        item.resolution,
        InventoryResolution::Resolved | InventoryResolution::ExactVariantUnavailable
    ) && (item.tradeable_quantity > 0 || item.equipped_quantity > 0)
}

fn enrich_inventory_view(
    database: &Mutex<Database>,
    mut view: InventoryView,
    language: Language,
) -> Result<InventoryView, CoreError> {
    let catalog_by_slug: HashMap<String, (String, String, Option<String>, bool)> =
        lock_database(database)?
            .load_current_catalog()?
            .map(|catalog| {
                catalog
                    .items
                    .into_iter()
                    .map(|item| {
                        // Каталоги schema v1 ещё не содержали bulkTradable. Эти теги
                        // полностью состоят из bulk-предметов в WFM и позволяют
                        // корректно обновиться без ожидания следующего refresh.
                        let display_name = match language {
                            Language::Russian => item
                                .display_name_ru
                                .filter(|name| !name.trim().is_empty())
                                .unwrap_or_else(|| item.display_name_en.clone()),
                            Language::English => item.display_name_en.clone(),
                        };
                        let thumb = match language {
                            Language::Russian => item.thumb_ru.or(item.thumb),
                            Language::English => item.thumb,
                        };
                        (
                            item.slug,
                            (
                                item.item_id,
                                display_name,
                                thumb,
                                catalog_bulk_tradable(item.bulk_tradable, &item.tags),
                            ),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
    let metadata = lock_database(database)?.load_current_game_metadata()?;
    let vault_statuses = metadata
        .as_ref()
        .map_or_else(HashMap::new, vault_status_by_slug);
    let component_images = metadata
        .as_ref()
        .map_or_else(HashMap::new, component_image_urls_by_slug);
    let equipment_names_ru: HashMap<&str, &str> = metadata
        .as_ref()
        .filter(|_| language == Language::Russian)
        .map_or_else(HashMap::new, |snapshot| {
            snapshot
                .item_localizations
                .iter()
                .map(|item| (item.game_ref.as_str(), item.display_name_ru.as_str()))
                .collect()
        });
    let market_source_date = lock_database(database)?
        .current_market_snapshot()?
        .map(|snapshot| snapshot.source_date);

    for item in &mut view.items {
        for placement in &mut item.equipped_placements {
            let localized = equipment_names_ru
                .get(placement.equipment_game_id.as_str())
                .copied()
                .or_else(|| russian_equipment_name_fallback(&placement.equipment_game_id));
            placement.equipment_display_name =
                clean_equipment_display_name(&placement.equipment_display_name, localized);
        }
        if let Some(key) = item.key.as_ref()
            && let Some((item_id, display_name, thumb, bulk_tradable)) =
                catalog_by_slug.get(&key.slug)
        {
            item.item_id = Some(item_id.clone());
            item.display_name.clone_from(display_name);
            item.bulk_tradable = *bulk_tradable;
            item.image_url = component_images.get(&key.slug).cloned().or_else(|| {
                thumb
                    .as_ref()
                    .map(|thumb| format!("https://warframe.market/static/assets/{thumb}"))
            });
        }
        let vault_slug = item
            .key
            .as_ref()
            .map_or(item.canonical_game_id.as_str(), |key| key.slug.as_str());
        item.vault_status = vault_statuses
            .get(vault_slug)
            .copied()
            .unwrap_or(VaultStatus::Unknown);
        let Some(key) = item.key.as_ref() else {
            continue;
        };
        if item.resolution != InventoryResolution::Resolved {
            continue;
        }
        item.closed_median_48h = if let Some(as_of) = market_source_date {
            let database_guard = lock_database(database)?;
            let points = market_history_with_regular_fallback(&database_guard, key, 2, as_of)?;
            weighted_closed_median(&points)
        } else {
            None
        };
        item.has_reliable_price = item.closed_median_48h.is_some();
    }
    Ok(view)
}

fn clean_equipment_display_name(current: &str, localized: Option<&str>) -> String {
    let current = current.trim();
    let (custom, fallback) = current
        .split_once(" — ")
        .map_or((None, current), |(custom, base)| {
            (visible_equipment_custom_name(custom), base.trim())
        });
    let base = localized
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(fallback);

    if let Some(custom) = custom.filter(|custom| !custom.eq_ignore_ascii_case(base)) {
        return format!("{custom} — {base}");
    }
    if current.starts_with("/Lotus/Language/") {
        if let Some(custom) = visible_equipment_custom_name(current) {
            return localized
                .map_or_else(|| custom.to_owned(), |name| format!("{custom} — {name}"));
        }
        return localized.unwrap_or("Неизвестный предмет").to_owned();
    }
    base.to_owned()
}

fn visible_equipment_custom_name(value: &str) -> Option<&str> {
    let value = value.trim();
    let visible = if value.starts_with("/Lotus/Language/") {
        value.split_once('|')?.1.trim()
    } else {
        value
    };
    (!visible.is_empty()).then_some(visible)
}

fn russian_equipment_name_fallback(game_ref: &str) -> Option<&'static str> {
    match game_ref {
        "/Lotus/Powersuits/Operator/AdultOperatorSuitRemaster" => Some("Скиталец"),
        "/Lotus/Powersuits/Operator/ChildOperatorSuitRemaster" => Some("Оператор"),
        "/Lotus/Types/Friendly/Pets/BeastWeapons/ChesaPetWeapon" => Some("Чеса кубрау"),
        "/Lotus/Types/Friendly/Pets/BeastWeapons/HelminthPetWeapon" => Some("Гельминтов инфестоид"),
        "/Lotus/Types/Friendly/Pets/BeastWeapons/HurasPetWeapon" => Some("Хурас кубрау"),
        "/Lotus/Types/Friendly/Pets/BeastWeapons/PanzerVulpaphylaPetWeapon" => {
            Some("Панцирная вульпафила")
        }
        "/Lotus/Types/Friendly/Pets/BeastWeapons/SmeetaPetWeapon" => Some("Кават смита"),
        "/Lotus/Types/Friendly/Pets/BeastWeapons/VenariPetWeapon" => Some("Венари"),
        "/Lotus/Types/Game/CrewShip/RailJack/DefaultHarness" => Some("Плексус"),
        "/Lotus/Weapons/Tenno/HackingDevices/TnHackingDevice/TnHackingDeviceWeapon" => {
            Some("Паразон")
        }
        _ => None,
    }
}

fn catalog_bulk_tradable(explicit: bool, tags: &[String]) -> bool {
    explicit
        || tags.iter().any(|tag| {
            matches!(
                tag.as_str(),
                "relic"
                    | "arcane_enhancement"
                    | "fish"
                    | "gem"
                    | "ayatan_sculpture"
                    | "ayatan_star"
            )
        })
}

/// Медиана дневных closed-агрегатов за последние 48 часов с весом по числу
/// фактически закрытых сделок. Активные sell/buy-ордера сюда не попадают.
fn weighted_closed_median(points: &[MarketHistoryPoint]) -> Option<f64> {
    let mut priced: Vec<(f64, f64)> = points
        .iter()
        .filter_map(|point| {
            let price = point.closed_median?;
            (price.is_finite()
                && price > 0.0
                && point.closed_volume.is_finite()
                && point.closed_volume > 0.0)
                .then_some((price, point.closed_volume))
        })
        .collect();
    priced.sort_by(|left, right| left.0.total_cmp(&right.0));
    let total_volume: f64 = priced.iter().map(|(_, volume)| volume).sum();
    let midpoint = total_volume / 2.0;
    let mut cumulative = 0.0;
    for (price, volume) in priced {
        cumulative += volume;
        if cumulative >= midpoint {
            return Some(price);
        }
    }
    None
}

fn vault_status_by_slug(
    snapshot: &platscope_domain::GameMetadataSnapshot,
) -> HashMap<String, VaultStatus> {
    let mut statuses = HashMap::new();
    for set in &snapshot.prime_sets {
        merge_vault_status(&mut statuses, &set.set_slug, set.vault_status);
        for component in &set.components {
            merge_vault_status(&mut statuses, &component.slug, set.vault_status);
        }
    }
    for relic in &snapshot.relics {
        merge_vault_status(&mut statuses, &relic.relic_slug, relic.vault_status);
    }
    for part in &snapshot.prime_parts {
        merge_vault_status(&mut statuses, &part.slug, part.vault_status);
    }
    statuses
}

fn component_image_urls_by_slug(
    snapshot: &platscope_domain::GameMetadataSnapshot,
) -> HashMap<String, String> {
    snapshot
        .prime_sets
        .iter()
        .flat_map(|set| &set.components)
        .filter_map(|component| {
            component
                .image_url
                .as_ref()
                .map(|image_url| (component.slug.clone(), image_url.clone()))
        })
        .collect()
}

fn merge_vault_status(
    statuses: &mut HashMap<String, VaultStatus>,
    slug: &str,
    status: VaultStatus,
) {
    statuses
        .entry(slug.to_owned())
        .and_modify(|current| {
            if *current != status {
                *current = VaultStatus::Unknown;
            }
        })
        .or_insert(status);
}

impl HistoryService {
    /// Создаёт incremental history bootstrap на основе immutable relics.run dumps.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`], если HTTP provider нельзя инициализировать.
    pub fn production() -> Result<Self, CoreError> {
        Ok(Self {
            provider: Arc::new(RelicsRunProvider::new()?),
            bootstrap_lock: tokio::sync::Mutex::new(()),
        })
    }

    /// Импортирует в фоне не более семи отсутствующих дней за запуск, постепенно достигая 90.
    /// Raw JSON живёт только до нормализации и не сохраняется в SQLite.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`] только для локального state; provider failures входят в outcome.
    pub async fn bootstrap(
        &self,
        database: &Mutex<Database>,
    ) -> Result<HistoryBootstrapOutcome, CoreError> {
        self.bootstrap_with_limit(database, HISTORY_IMPORTS_PER_RUN)
            .await
    }

    /// Импортирует все отсутствующие дни целевого 90-дневного окна по ручному запросу.
    /// Raw JSON живёт только до нормализации и не сохраняется в SQLite.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`] только для локального state; provider failures входят в outcome.
    pub async fn bootstrap_full(
        &self,
        database: &Mutex<Database>,
    ) -> Result<HistoryBootstrapOutcome, CoreError> {
        self.bootstrap_with_limit(database, usize::from(HISTORY_TARGET_DAYS))
            .await
    }

    async fn bootstrap_with_limit(
        &self,
        database: &Mutex<Database>,
        import_limit: usize,
    ) -> Result<HistoryBootstrapOutcome, CoreError> {
        let _bootstrap_guard = self.bootstrap_lock.lock().await;
        let (current, catalog) = {
            let database = lock_database(database)?;
            (
                database.current_market_snapshot()?,
                database.load_current_catalog()?,
            )
        };
        let (Some(current), Some(catalog)) = (current, catalog) else {
            return Ok(HistoryBootstrapOutcome {
                target_days: HISTORY_TARGET_DAYS,
                imported_days: 0,
                skipped_days: 0,
                coverage: lock_database(database)?.history_coverage()?,
                failures: Vec::new(),
            });
        };

        let mut imported_days = 0;
        let mut skipped_days = 0;
        let mut failures = Vec::new();
        for offset in 0..HISTORY_TARGET_DAYS {
            if imported_days >= import_limit {
                break;
            }
            let source_date = current.source_date - ChronoDuration::days(i64::from(offset));
            if lock_database(database)?.has_history_date(source_date)? {
                skipped_days += 1;
                continue;
            }
            match self.fetch_history_day(source_date, &catalog).await {
                Ok(snapshot) => {
                    lock_database(database)?.promote_history_snapshot(&snapshot)?;
                    imported_days += 1;
                    tracing::info!(
                        event = "history_day_imported",
                        source_date = %source_date,
                        provider = ?self.provider.id(),
                        "compact history day imported"
                    );
                }
                Err(error) => {
                    let should_continue =
                        error.code == ProviderErrorCode::NotPublished || error.retryable;
                    let message = public_error_message(&error);
                    tracing::warn!(
                        event = "history_day_failed",
                        source_date = %source_date,
                        code = ?error.code,
                        message = %message,
                        "history day could not be imported"
                    );
                    failures.push(HistoryBootstrapFailure {
                        source_date,
                        code: error.code,
                        message,
                    });
                    if !should_continue {
                        break;
                    }
                }
            }
        }
        Ok(HistoryBootstrapOutcome {
            target_days: HISTORY_TARGET_DAYS,
            imported_days,
            skipped_days,
            coverage: lock_database(database)?.history_coverage()?,
            failures,
        })
    }

    async fn fetch_history_day(
        &self,
        source_date: NaiveDate,
        catalog: &ItemCatalog,
    ) -> Result<platscope_domain::NormalizedMarketSnapshot, ProviderError> {
        for attempt in 1..=HISTORY_FETCH_ATTEMPTS {
            let result = self
                .provider
                .fetch_day(source_date)
                .await
                .and_then(|dump| self.provider.normalize_history(&dump, catalog));
            match result {
                Ok(snapshot) => return Ok(snapshot),
                Err(error) if error.retryable && attempt < HISTORY_FETCH_ATTEMPTS => {
                    tracing::warn!(
                        event = "history_day_retry",
                        source_date = %source_date,
                        attempt,
                        code = ?error.code,
                        "retrying transient history download failure"
                    );
                    tokio::time::sleep(Duration::from_millis(500 * u64::from(attempt))).await;
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("history retry loop always returns")
    }

    /// Загружает только запрошенный ряд точного варианта и рассчитывает 7/30/90 trend.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`] для неподдерживаемого диапазона или ошибки SQLite.
    pub fn view(
        database: &Mutex<Database>,
        key: &MarketVariantKey,
        requested_days: u16,
        current_price: Option<f64>,
        live_lowest_ask: Option<f64>,
    ) -> Result<MarketHistoryView, CoreError> {
        if !matches!(requested_days, 7 | 30 | 90) {
            return Err(CoreError::MarketData(
                "history range must be 7, 30 or 90 days".into(),
            ));
        }
        let database = lock_database(database)?;
        let as_of = database
            .current_market_snapshot()?
            .map_or_else(|| Utc::now().date_naive(), |snapshot| snapshot.source_date);
        let all_points =
            market_history_with_regular_fallback(&database, key, HISTORY_TARGET_DAYS, as_of)?;
        let first_date = as_of - ChronoDuration::days(i64::from(requested_days - 1));
        let points = all_points
            .iter()
            .filter(|point| point.source_date >= first_date)
            .cloned()
            .collect();
        let trend = platscope_trends::calculate(
            &all_points,
            TrendContext {
                as_of,
                current_price,
                live_lowest_ask,
            },
        );
        Ok(MarketHistoryView {
            key: key.clone(),
            requested_days,
            points,
            trend,
            coverage: database.history_coverage()?,
        })
    }
}

impl MarketDataService {
    /// Собирает production provider chain: relics.run, затем `FrameForge` mirror.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`], если HTTP clients нельзя создать.
    pub fn production() -> Result<Self, CoreError> {
        Ok(Self {
            catalog_provider: Arc::new(WarframeMarketProvider::new()?),
            market_providers: vec![
                Arc::new(RelicsRunProvider::new()?),
                Arc::new(FrameForgeMirrorProvider::new()?),
            ],
            refresh_lock: tokio::sync::Mutex::new(()),
        })
    }

    /// Обновляет catalog и price snapshot; при сбое возвращает целый LKG snapshot.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`], только если нет ни валидного network snapshot, ни LKG.
    pub async fn refresh(
        &self,
        database: &Mutex<Database>,
    ) -> Result<MarketRefreshOutcome, CoreError> {
        let _refresh_guard = self.refresh_lock.lock().await;
        self.refresh_locked(database).await
    }

    /// Обновляет рынок только когда текущий LKG старше заданного интервала.
    /// Проверка выполняется после захвата общего refresh lock, поэтому ручной и фоновый
    /// запросы не создают последовательный двойной download.
    ///
    /// # Errors
    ///
    /// Возвращает [`CoreError`] по тем же правилам, что и [`Self::refresh`].
    pub async fn refresh_if_due(
        &self,
        database: &Mutex<Database>,
        refresh_hours: u8,
    ) -> Result<Option<MarketRefreshOutcome>, CoreError> {
        let _refresh_guard = self.refresh_lock.lock().await;
        let current = lock_database(database)?.current_market_snapshot()?;
        let catalog_needs_refresh =
            lock_database(database)?
                .load_current_catalog()?
                .is_none_or(|catalog| {
                    catalog.metadata.schema_version < CURRENT_CATALOG_SCHEMA_VERSION
                        || catalog.items.iter().all(|item| item.thumb.is_none())
                        || catalog
                            .items
                            .iter()
                            .all(|item| item.display_name_ru.is_none())
                });
        if !market_refresh_due(current.as_ref(), Utc::now(), refresh_hours)
            && !catalog_needs_refresh
        {
            return Ok(None);
        }
        self.refresh_locked(database).await.map(Some)
    }

    async fn refresh_locked(
        &self,
        database: &Mutex<Database>,
    ) -> Result<MarketRefreshOutcome, CoreError> {
        let mut failures = Vec::new();
        let (catalog, catalog_from_cache) = self.refresh_catalog(database, &mut failures).await?;
        let previous = lock_database(database)?.current_market_snapshot()?;

        for (index, provider) in self.market_providers.iter().enumerate() {
            let started = Instant::now();
            let result = provider
                .fetch_latest()
                .await
                .and_then(|dump| provider.normalize(&dump, &catalog))
                .and_then(|snapshot| {
                    validate_relative_size(previous.as_ref(), &snapshot.metadata)?;
                    Ok(snapshot)
                });
            match result {
                Ok(snapshot) => {
                    let summary = {
                        let mut database = lock_database(database)?;
                        let summary = database.promote_market_snapshot(&snapshot)?;
                        database.record_provider_success(provider.id(), elapsed_millis(started))?;
                        summary
                    };
                    tracing::info!(
                        event = "market_snapshot_promoted",
                        provider = ?summary.provider,
                        source_date = %summary.source_date,
                        item_count = summary.item_count,
                        record_count = summary.record_count,
                        "bulk market snapshot promoted"
                    );
                    return Ok(MarketRefreshOutcome {
                        snapshot: summary,
                        catalog_item_count: catalog.metadata.item_count,
                        stale: false,
                        used_fallback: index > 0,
                        catalog_from_cache,
                        failures,
                    });
                }
                Err(error) => {
                    record_failure(database, provider.id(), &error, started)?;
                    failures.push(RefreshFailure {
                        provider: provider.id(),
                        code: error.code,
                        message: public_error_message(&error),
                    });
                }
            }
        }

        let Some(snapshot) = lock_database(database)?.current_market_snapshot()? else {
            let detail = failures
                .iter()
                .map(|failure| format!("{:?}: {}", failure.provider, failure.message))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(CoreError::MarketData(format!(
                "не удалось загрузить рыночные данные, локального LKG snapshot ещё нет: {detail}"
            )));
        };
        tracing::warn!(
            event = "market_snapshot_lkg_used",
            source_date = %snapshot.source_date,
            "all bulk providers failed; continuing with LKG"
        );
        Ok(MarketRefreshOutcome {
            snapshot,
            catalog_item_count: catalog.metadata.item_count,
            stale: true,
            used_fallback: true,
            catalog_from_cache,
            failures,
        })
    }

    async fn refresh_catalog(
        &self,
        database: &Mutex<Database>,
        failures: &mut Vec<RefreshFailure>,
    ) -> Result<(ItemCatalog, bool), CoreError> {
        let started = Instant::now();
        let previous_count = lock_database(database)?
            .load_current_catalog()?
            .map(|value| value.metadata.item_count);
        let result = self
            .catalog_provider
            .load_metadata()
            .await
            .and_then(|raw| self.catalog_provider.normalize_metadata(&raw))
            .and_then(|catalog| {
                validate_relative_count(
                    previous_count,
                    catalog.metadata.item_count,
                    "catalog item_count",
                )?;
                Ok(catalog)
            });
        match result {
            Ok(catalog) => {
                {
                    let mut database = lock_database(database)?;
                    database.promote_catalog(&catalog)?;
                    database.record_provider_success(
                        self.catalog_provider.id(),
                        elapsed_millis(started),
                    )?;
                }
                Ok((catalog, false))
            }
            Err(error) => {
                record_failure(database, self.catalog_provider.id(), &error, started)?;
                failures.push(RefreshFailure {
                    provider: self.catalog_provider.id(),
                    code: error.code,
                    message: public_error_message(&error),
                });
                let Some(catalog) = lock_database(database)?.load_current_catalog()? else {
                    return Err(CoreError::MarketData(format!(
                        "не удалось загрузить каталог и локального LKG нет: {}",
                        public_error_message(&error)
                    )));
                };
                Ok((catalog, true))
            }
        }
    }
}

#[must_use]
pub fn market_refresh_due(
    current: Option<&MarketSnapshotSummary>,
    now: chrono::DateTime<Utc>,
    refresh_hours: u8,
) -> bool {
    let interval = ChronoDuration::hours(i64::from(refresh_hours.clamp(1, 24)));
    current.is_none_or(|snapshot| now.signed_duration_since(snapshot.promoted_at) >= interval)
}

fn validate_relative_size(
    previous: Option<&MarketSnapshotSummary>,
    candidate: &platscope_domain::SnapshotMetadata,
) -> Result<(), ProviderError> {
    if let Some(previous) = previous {
        validate_relative_count(
            Some(previous.item_count),
            candidate.item_count,
            "snapshot item_count",
        )?;
        validate_relative_count(
            Some(previous.record_count),
            candidate.record_count,
            "snapshot record_count",
        )?;
    }
    Ok(())
}

fn validate_game_metadata_relative(
    previous: Option<&platscope_domain::GameMetadataSnapshot>,
    candidate: &GameMetadataSnapshotMetadata,
) -> Result<(), ProviderError> {
    if let Some(previous) = previous {
        validate_relative_count(
            Some(previous.metadata.set_count),
            candidate.set_count,
            "metadata set_count",
        )?;
        validate_relative_count(
            Some(previous.metadata.relic_count),
            candidate.relic_count,
            "metadata relic_count",
        )?;
        validate_relative_count(
            Some(previous.metadata.prime_part_count),
            candidate.prime_part_count,
            "metadata prime_part_count",
        )?;
        validate_relative_count(
            Some(previous.metadata.riven_disposition_count),
            candidate.riven_disposition_count,
            "metadata riven_disposition_count",
        )?;
        validate_relative_count(
            Some(previous.metadata.item_definition_count),
            candidate.item_definition_count,
            "metadata item_definition_count",
        )?;
    }
    Ok(())
}

fn validate_relative_count(
    previous: Option<u64>,
    candidate: u64,
    field: &str,
) -> Result<(), ProviderError> {
    if previous.is_some_and(|previous| {
        previous > 0
            && u128::from(candidate).saturating_mul(100)
                < u128::from(previous).saturating_mul(MINIMUM_RELATIVE_SNAPSHOT_PERCENT)
    }) {
        return Err(ProviderError::validation(format!(
            "{field} резко уменьшился относительно LKG"
        )));
    }
    Ok(())
}

fn record_failure(
    database: &Mutex<Database>,
    provider: ProviderId,
    error: &ProviderError,
    started: Instant,
) -> Result<(), CoreError> {
    lock_database(database)?.record_provider_failure(
        provider,
        &format!("{:?}", error.code),
        &public_error_message(error),
        elapsed_millis(started),
    )?;
    Ok(())
}

fn lock_database(
    database: &Mutex<Database>,
) -> Result<std::sync::MutexGuard<'_, Database>, CoreError> {
    database
        .lock()
        .map_err(|_| CoreError::DatabaseState("database mutex is poisoned".into()))
}

fn elapsed_millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn public_error_message(error: &ProviderError) -> String {
    error.message.chars().take(300).collect()
}

/// Инициализирует JSONL logging с дневной ротацией и возвращает guard writer.
///
/// # Errors
///
/// Возвращает [`CoreError`] при ошибке создания каталога или global subscriber.
pub fn init_logging(log_directory: &Path) -> Result<LoggingGuard, CoreError> {
    fs::create_dir_all(log_directory)?;
    let file_appender = tracing_appender::rolling::daily(log_directory, "platscope.jsonl");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
    let filter = std::env::var("PLATSCOPE_LOG")
        .ok()
        .and_then(|value| EnvFilter::try_new(value).ok())
        .unwrap_or_else(|| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .json()
        .with_env_filter(filter)
        .with_writer(file_writer)
        .try_init()
        .map_err(|error| CoreError::Logging(error.to_string()))?;

    tracing::info!(
        event = "logging_initialized",
        "structured logging initialized"
    );
    Ok(LoggingGuard { _guard: guard })
}

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("unable to initialize log directory: {0}")]
    Io(#[from] std::io::Error),
    #[error("unable to initialize logging: {0}")]
    Logging(String),
    #[error("provider error: {0}")]
    Provider(#[from] ProviderError),
    #[error("WFM account error: {0}")]
    Account(#[from] platscope_account::AccountError),
    #[error("storage error: {0}")]
    Storage(#[from] platscope_storage::StorageError),
    #[error("inventory import error: {0}")]
    Inventory(#[from] InventoryError),
    #[error("inventory data unavailable: {0}")]
    InventoryData(String),
    #[error("game metadata unavailable: {0}")]
    MetadataData(String),
    #[error("WFM account data unavailable: {0}")]
    AccountData(String),
    #[error("database state unavailable: {0}")]
    DatabaseState(String),
    #[error("market data refresh failed: {0}")]
    MarketData(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use platscope_domain::{
        CatalogItem, CatalogMetadata, MarketOrderType, MarketRecord, NormalizedMarketSnapshot,
        SnapshotMetadata,
    };

    #[test]
    fn current_nightwave_tag_resolves_exact_currency() {
        assert_eq!(
            nightwave_currency_ref("Radio Legion Intermission16 Syndicate").as_deref(),
            Some("/Lotus/Types/Items/MiscItems/NoraIntermissionSixteenCreds")
        );
        assert!(nightwave_currency_ref("unexpected season").is_none());
    }

    fn nightwave_market_fixture(now: chrono::DateTime<Utc>) -> (Mutex<Database>, ItemCatalog) {
        let key = MarketVariantKey::new(
            "corrosive_projection",
            Platform::Pc,
            Some(0),
            None::<String>,
        )
        .expect("market key");
        let catalog = ItemCatalog {
            metadata: CatalogMetadata {
                provider: ProviderId::WarframeMarket,
                fetched_at: now,
                schema_version: 1,
                item_count: 1,
                checksum_sha256: "nightwave-catalog".into(),
            },
            items: vec![CatalogItem {
                item_id: "corrosive-projection-id".into(),
                slug: key.slug.clone(),
                display_name_en: "Corrosive Projection".into(),
                display_name_ru: Some("Коррозийный Выброс".into()),
                thumb: None,
                thumb_ru: None,
                game_ref: Some("/Lotus/Upgrades/Mods/Aura/EnemyArmorReductionAuraMod".into()),
                bulk_tradable: false,
                max_rank: Some(5),
                subtypes: Vec::new(),
                tags: vec!["mod".into()],
            }],
        };
        let market = NormalizedMarketSnapshot {
            metadata: SnapshotMetadata {
                provider: ProviderId::RelicsRun,
                source_date: now.date_naive(),
                fetched_at: now,
                schema_version: 1,
                item_count: 1,
                record_count: 1,
                checksum_sha256: "nightwave-market".into(),
            },
            records: vec![MarketRecord {
                key,
                external_item_id: "corrosive-projection-id".into(),
                display_name_en: "Corrosive Projection".into(),
                observed_at: now,
                order_type: MarketOrderType::Closed,
                median: Some(12.0),
                average: Some(12.0),
                min_price: Some(10.0),
                max_price: Some(14.0),
                volume: 20.0,
                raw_json: "{}".into(),
            }],
        };
        let database = Mutex::new(Database::open_in_memory().expect("database opens"));
        {
            let mut database = database.lock().expect("database lock");
            database
                .promote_catalog(&catalog)
                .expect("catalog promoted");
            database
                .promote_market_snapshot(&market)
                .expect("market promoted");
        }
        (database, catalog)
    }

    fn nightwave_inventory_fixture(now: chrono::DateTime<Utc>) -> ResolvedInventorySnapshot {
        ResolvedInventorySnapshot {
            metadata: InventorySnapshotMetadata {
                source: InventorySource::ReadOnlyScan,
                observed_at: now,
                schema_version: 1,
                item_count: 1,
                checksum_sha256: "nightwave-inventory".into(),
            },
            keep_copies: 0,
            mod_usage_scanned: false,
            credits: None,
            syndicates: Vec::new(),
            items: vec![ResolvedInventoryItem {
                canonical_game_id: "/Lotus/Types/Items/MiscItems/NoraIntermissionSixteenCreds"
                    .into(),
                display_name_en: None,
                display_name_ru: None,
                tags: Vec::new(),
                key: None,
                rank: None,
                subtype: None,
                owned_quantity: 100,
                tradeable_quantity: 0,
                untradeable_quantity: 100,
                unknown_quantity: 0,
                leveled_quantity: 0,
                equipped_quantity: 0,
                equipped_tradeable_quantity: 0,
                equipped_placements: Vec::new(),
                sellable_quantity: 0,
                resolution: InventoryResolution::UnknownItem,
            }],
        }
    }

    fn empty_game_metadata_fixture(now: chrono::DateTime<Utc>) -> GameMetadataSnapshot {
        GameMetadataSnapshot {
            metadata: GameMetadataSnapshotMetadata {
                source: platscope_domain::GameMetadataSource::WfcdWarframeItems,
                fetched_at: now,
                schema_version: 1,
                set_count: 0,
                relic_count: 0,
                prime_part_count: 0,
                riven_disposition_count: 0,
                item_definition_count: 0,
                checksum_sha256: "nightwave-metadata".into(),
            },
            prime_sets: Vec::new(),
            relics: Vec::new(),
            prime_parts: Vec::new(),
            riven_dispositions: Vec::new(),
            item_definitions: Vec::new(),
            item_localizations: Vec::new(),
            syndicate_offers: Vec::new(),
            nightwave_offers: Vec::new(),
            arcane_dissolutions: Vec::new(),
            arcane_packs: Vec::new(),
        }
    }

    fn nightwave_daily_fixture(now: chrono::DateTime<Utc>) -> DailyMarketState {
        DailyMarketState {
            fetched_at: now,
            void_trader: None,
            nightwave: Some(platscope_providers::NightwaveState {
                activation: now - ChronoDuration::days(1),
                expiry: now + ChronoDuration::days(30),
                season: 18,
                tag: "Radio Legion Intermission16 Syndicate".into(),
            }),
            steel_path: None,
            unavailable_sources: Vec::new(),
        }
    }

    fn nightwave_vendor_fixture(now: chrono::DateTime<Utc>) -> NightwaveVendorSnapshot {
        NightwaveVendorSnapshot {
            observed_at: now,
            expires_at: now + ChronoDuration::days(1),
            season_tag: "RadioLegionIntermission16Syndicate".into(),
            vendor_type:
                "/Lotus/Types/Game/VendorManifests/Events/RadioLegionIntermission16VendorManifest"
                    .into(),
            offers: vec![platscope_domain::NightwaveVendorOffer {
                game_ref: "/Lotus/Upgrades/Mods/Aura/EnemyArmorReductionAuraMod".into(),
                cred_cost: 20,
            }],
        }
    }

    #[test]
    fn confirmed_nightwave_stock_is_included_in_direct_value() {
        let now = Utc::now();
        let (database, catalog) = nightwave_market_fixture(now);
        let inventory = nightwave_inventory_fixture(now);
        let metadata = empty_game_metadata_fixture(now);
        let daily = nightwave_daily_fixture(now);
        let vendor = nightwave_vendor_fixture(now);

        let context = ResourceConverterBuildContext {
            metadata: &metadata,
            inventory: &inventory,
            catalog: &catalog,
            daily: &daily,
            nightwave_vendor: Some(&vendor),
            market_source_date: Some(now.date_naive()),
            now,
        };
        let route = build_nightwave_route(&database, &AppSettings::default(), &context)
            .expect("route builds");

        assert_eq!(route.status, ResourceRouteStatus::Ready);
        assert_eq!(route.reason, "nightwave_stock_confirmed");
        assert_eq!(route.actions[0].quantity, 5);
        assert!((route.actions[0].estimated_platinum - 60.0).abs() < f64::EPSILON);
        assert!(route.actions[0].included_in_total);
    }

    #[test]
    fn arcane_rank_costs_match_consumed_copy_counts() {
        assert_eq!(
            (0..=5).map(arcane_rank_copy_count).collect::<Vec<_>>(),
            vec![1, 3, 6, 10, 15, 21]
        );
    }

    #[test]
    fn arcane_sale_keeps_reserve_and_dissolves_only_the_remainder() {
        let definition = platscope_domain::ArcaneDissolutionDefinition {
            slug: "arcane_test".into(),
            game_ref: "/Lotus/Test/Arcane".into(),
            display_name_en: "Test Arcane".into(),
            display_name_ru: Some("РўРµСЃС‚РѕРІС‹Р№ РјРёСЃС‚РёС„РёРєР°С‚РѕСЂ".into()),
            image_url: None,
            vosfor: 20,
        };
        let input = ArcaneDecisionInput {
            definition: &definition,
            display_name: "РўРµСЃС‚РѕРІС‹Р№ РјРёСЃС‚РёС„РёРєР°С‚РѕСЂ".into(),
            rank: 0,
            market_price_each: Some(10.0),
            vosfor_each: 20,
            equivalent_platinum_each: 4.0,
        };
        let item = ResolvedInventoryItem {
            canonical_game_id: definition.game_ref.clone(),
            display_name_en: Some(definition.display_name_en.clone()),
            display_name_ru: definition.display_name_ru.clone(),
            tags: vec!["arcane_enhancement".into()],
            key: None,
            rank: Some(0),
            subtype: None,
            owned_quantity: 5,
            tradeable_quantity: 2,
            untradeable_quantity: 3,
            unknown_quantity: 0,
            leveled_quantity: 0,
            equipped_quantity: 0,
            equipped_tradeable_quantity: 0,
            equipped_placements: Vec::new(),
            sellable_quantity: 2,
            resolution: InventoryResolution::Resolved,
        };
        let mut decisions = ArcaneDecisionBuckets::default();

        append_arcane_decisions(&item, 2, &input, &mut decisions);

        assert_eq!(decisions.sell[0].quantity, 2);
        assert_eq!(decisions.dissolve[0].quantity, 1);
        assert!(decisions.hold.is_empty());
    }

    #[test]
    fn max_rank_syndicate_can_access_lower_rank_offers() {
        let standing = SyndicateStanding {
            tag: "SteelMeridianSyndicate".into(),
            standing: 50_000,
            title: Some("General".into()),
        };
        assert_eq!(
            syndicate_from_affiliation(&standing),
            Some("Steel Meridian")
        );
        assert!(syndicate_offer_accessible(&standing, "Protector"));
        assert_eq!(arcane_rank_copy_count(5), 21);
    }

    #[test]
    fn equipment_names_use_russian_localization_without_internal_paths() {
        assert_eq!(
            clean_equipment_display_name(
                "/Lotus/Language/Weapons/CrpBEArcaPlasmorName|HILDI ONIA — Crp B E Arca Plasmor",
                Some("Арка Плазмор Догмат"),
            ),
            "HILDI ONIA — Арка Плазмор Догмат"
        );
        assert_eq!(
            clean_equipment_display_name("Скорость — Volt Prime", Some("Вольт Прайм")),
            "Скорость — Вольт Прайм"
        );
        assert_eq!(
            clean_equipment_display_name("Kuva Nukor", Some("Нюкор Кува")),
            "Нюкор Кува"
        );
    }

    #[test]
    fn equipment_name_fallback_still_hides_localization_paths() {
        assert_eq!(
            clean_equipment_display_name(
                "/Lotus/Language/Weapons/KuvaNukorName|POKK CRAA — Kuva Nukor",
                None,
            ),
            "POKK CRAA — Kuva Nukor"
        );
        assert_eq!(
            russian_equipment_name_fallback("/Lotus/Types/Game/CrewShip/RailJack/DefaultHarness",),
            Some("Плексус")
        );
        assert_eq!(
            russian_equipment_name_fallback(
                "/Lotus/Weapons/Tenno/HackingDevices/TnHackingDevice/TnHackingDeviceWeapon",
            ),
            Some("Паразон")
        );
    }

    #[test]
    fn closed_median_48h_uses_closed_trade_volume_as_weight() {
        let points = vec![
            MarketHistoryPoint {
                source_date: NaiveDate::from_ymd_opt(2026, 8, 26).expect("valid date"),
                closed_median: Some(20.0),
                closed_volume: 3.0,
                sell_median: Some(18.0),
                buy_median: Some(16.0),
            },
            MarketHistoryPoint {
                source_date: NaiveDate::from_ymd_opt(2026, 8, 27).expect("valid date"),
                closed_median: Some(30.0),
                closed_volume: 9.0,
                sell_median: Some(28.0),
                buy_median: Some(25.0),
            },
        ];

        assert_eq!(weighted_closed_median(&points), Some(30.0));
    }

    #[test]
    fn closed_median_48h_ignores_days_without_real_sales() {
        let points = vec![MarketHistoryPoint {
            source_date: NaiveDate::from_ymd_opt(2026, 8, 27).expect("valid date"),
            closed_median: Some(30.0),
            closed_volume: 0.0,
            sell_median: Some(28.0),
            buy_median: Some(25.0),
        }];

        assert_eq!(weighted_closed_median(&points), None);
    }

    #[test]
    fn inventory_view_exposes_tradeable_items_without_guessing_variants() {
        let visible = ResolvedInventoryItem {
            canonical_game_id: "/Lotus/Test/Tradeable".into(),
            display_name_en: Some("Tradeable".into()),
            display_name_ru: None,
            tags: vec![],
            key: None,
            rank: None,
            subtype: None,
            owned_quantity: 2,
            tradeable_quantity: 1,
            untradeable_quantity: 1,
            unknown_quantity: 0,
            leveled_quantity: 1,
            equipped_quantity: 0,
            equipped_tradeable_quantity: 0,
            equipped_placements: Vec::new(),
            sellable_quantity: 1,
            resolution: InventoryResolution::Resolved,
        };
        assert!(inventory_item_visible(&visible));
        assert!(!inventory_item_visible(&ResolvedInventoryItem {
            tradeable_quantity: 0,
            ..visible.clone()
        }));
        assert!(!inventory_item_visible(&ResolvedInventoryItem {
            resolution: InventoryResolution::UnknownItem,
            ..visible
        }));

        assert!(inventory_item_visible(&ResolvedInventoryItem {
            canonical_game_id: "/Lotus/Test/RankUnknown".into(),
            display_name_en: Some("Rank unknown".into()),
            display_name_ru: None,
            tags: vec!["mod".into()],
            key: None,
            rank: None,
            subtype: None,
            owned_quantity: 2,
            tradeable_quantity: 2,
            untradeable_quantity: 0,
            unknown_quantity: 0,
            leveled_quantity: 0,
            equipped_quantity: 0,
            equipped_tradeable_quantity: 0,
            equipped_placements: Vec::new(),
            sellable_quantity: 0,
            resolution: InventoryResolution::ExactVariantUnavailable,
        }));
    }

    #[test]
    fn persisted_ranked_mod_is_relinked_to_required_regular_subtype() {
        let legacy_key =
            MarketVariantKey::new("animal_instinct", Platform::Pc, Some(5), None::<String>)
                .expect("legacy key");
        let snapshot = ResolvedInventorySnapshot {
            metadata: InventorySnapshotMetadata {
                source: InventorySource::ReadOnlyScan,
                observed_at: Utc::now(),
                schema_version: 1,
                item_count: 1,
                checksum_sha256: "inventory".into(),
            },
            keep_copies: 1,
            mod_usage_scanned: false,
            credits: None,
            syndicates: Vec::new(),
            items: vec![ResolvedInventoryItem {
                canonical_game_id: "/Lotus/Upgrades/Mods/Sentinel/AnimalInstinct".into(),
                display_name_en: Some("Animal Instinct".into()),
                display_name_ru: Some("Животный Инстинкт".into()),
                tags: vec!["mod".into()],
                key: Some(legacy_key.clone()),
                rank: Some(5),
                subtype: None,
                owned_quantity: 3,
                tradeable_quantity: 3,
                untradeable_quantity: 0,
                unknown_quantity: 0,
                leveled_quantity: 3,
                equipped_quantity: 0,
                equipped_tradeable_quantity: 0,
                equipped_placements: Vec::new(),
                sellable_quantity: 2,
                resolution: InventoryResolution::Resolved,
            }],
        };
        let catalog = ItemCatalog {
            metadata: CatalogMetadata {
                provider: ProviderId::WarframeMarket,
                fetched_at: Utc::now(),
                schema_version: CURRENT_CATALOG_SCHEMA_VERSION,
                item_count: 1,
                checksum_sha256: "catalog".into(),
            },
            items: vec![CatalogItem {
                item_id: "559dacd3e779897ba8819969".into(),
                slug: "animal_instinct".into(),
                display_name_en: "Animal Instinct".into(),
                display_name_ru: Some("Животный Инстинкт".into()),
                thumb: None,
                thumb_ru: None,
                game_ref: None,
                bulk_tradable: false,
                max_rank: Some(5),
                subtypes: vec!["regular".into(), "atragraph".into()],
                tags: vec!["mod".into()],
            }],
        };

        let repaired = relink_implicit_regular_inventory(
            &snapshot,
            Some(&catalog),
            &HashSet::from([legacy_key]),
            Platform::Pc,
        );

        assert_eq!(repaired.items[0].subtype.as_deref(), Some("regular"));
        assert_eq!(
            repaired.items[0]
                .key
                .as_ref()
                .and_then(|key| key.subtype.as_deref()),
            Some("regular")
        );
        assert_eq!(repaired.items[0].resolution, InventoryResolution::Resolved);
        assert_eq!(repaired.items[0].sellable_quantity, 2);
    }

    fn relic_refinement_fixtures() -> [(
        platscope_domain::RelicRefinement,
        &'static str,
        &'static str,
    ); 4] {
        use platscope_domain::RelicRefinement;
        [
            (RelicRefinement::Intact, "Bronze", "intact"),
            (RelicRefinement::Exceptional, "Silver", "exceptional"),
            (RelicRefinement::Flawless, "Gold", "flawless"),
            (RelicRefinement::Radiant, "Platinum", "radiant"),
        ]
    }

    fn relic_repair_metadata(
        refinements: &[(platscope_domain::RelicRefinement, &str, &str)],
    ) -> GameMetadataSnapshot {
        use platscope_domain::GameMetadataSource;
        let definitions = refinements
            .iter()
            .map(|(refinement, suffix, _)| RelicDefinition {
                relic_slug: "axi_n10_relic".into(),
                relic_game_ref: format!("/Lotus/Types/Game/Projections/AxiN10{suffix}"),
                display_name_en: "Axi N10 Relic".into(),
                refinement: *refinement,
                vault_status: VaultStatus::Vaulted,
                rewards: vec![],
            })
            .collect::<Vec<_>>();
        GameMetadataSnapshot {
            metadata: GameMetadataSnapshotMetadata {
                source: GameMetadataSource::WfcdWarframeItems,
                fetched_at: Utc::now(),
                schema_version: CURRENT_GAME_METADATA_SCHEMA_VERSION,
                set_count: 0,
                relic_count: definitions.len() as u64,
                prime_part_count: 0,
                riven_disposition_count: 0,
                item_definition_count: 0,
                checksum_sha256: "metadata".into(),
            },
            prime_sets: vec![],
            relics: definitions,
            prime_parts: vec![],
            riven_dispositions: vec![],
            item_definitions: vec![],
            item_localizations: vec![],
            syndicate_offers: vec![],
            nightwave_offers: vec![],
            arcane_dissolutions: vec![],
            arcane_packs: vec![],
        }
    }

    fn relic_repair_catalog(
        refinements: &[(platscope_domain::RelicRefinement, &str, &str)],
    ) -> ItemCatalog {
        ItemCatalog {
            metadata: CatalogMetadata {
                provider: ProviderId::WarframeMarket,
                fetched_at: Utc::now(),
                schema_version: CURRENT_CATALOG_SCHEMA_VERSION,
                item_count: 1,
                checksum_sha256: "catalog".into(),
            },
            items: vec![CatalogItem {
                item_id: "axi-n10".into(),
                slug: "axi_n10_relic".into(),
                display_name_en: "Axi N10 Relic".into(),
                display_name_ru: Some("Реликвия Акси N10".into()),
                thumb: None,
                thumb_ru: None,
                game_ref: None,
                bulk_tradable: true,
                max_rank: None,
                subtypes: refinements
                    .iter()
                    .map(|(_, _, subtype)| (*subtype).into())
                    .collect(),
                tags: vec!["relic".into()],
            }],
        }
    }

    #[test]
    fn persisted_relic_paths_are_relinked_for_all_refinements_without_a_new_scan() {
        let refinements = relic_refinement_fixtures();
        let metadata = relic_repair_metadata(&refinements);
        let catalog = relic_repair_catalog(&refinements);
        let snapshot = ResolvedInventorySnapshot {
            metadata: InventorySnapshotMetadata {
                source: InventorySource::ReadOnlyScan,
                observed_at: Utc::now(),
                schema_version: 1,
                item_count: refinements.len() as u64,
                checksum_sha256: "inventory".into(),
            },
            keep_copies: 1,
            mod_usage_scanned: false,
            credits: None,
            syndicates: Vec::new(),
            items: refinements
                .iter()
                .map(|(_, suffix, _)| ResolvedInventoryItem {
                    canonical_game_id: format!("/Lotus/Types/Game/Projections/AxiN10{suffix}"),
                    display_name_en: None,
                    display_name_ru: None,
                    tags: vec![],
                    key: None,
                    rank: None,
                    subtype: None,
                    owned_quantity: 3,
                    tradeable_quantity: 3,
                    untradeable_quantity: 0,
                    unknown_quantity: 0,
                    leveled_quantity: 0,
                    equipped_quantity: 0,
                    equipped_tradeable_quantity: 0,
                    equipped_placements: Vec::new(),
                    sellable_quantity: 0,
                    resolution: InventoryResolution::UnknownItem,
                })
                .collect(),
        };
        let variants = refinements
            .iter()
            .map(|(_, _, subtype)| {
                MarketVariantKey::new("axi_n10_relic", Platform::Pc, None, Some(*subtype))
                    .expect("valid variant")
            })
            .collect();

        let repaired = relink_exact_relic_inventory(
            &snapshot,
            Some(&catalog),
            Some(&metadata),
            &variants,
            Platform::Pc,
        );

        assert_eq!(repaired.items.len(), 4);
        for item in repaired.items {
            assert_eq!(item.resolution, InventoryResolution::Resolved);
            assert_eq!(
                item.key.as_ref().map(|key| key.slug.as_str()),
                Some("axi_n10_relic")
            );
            assert!(
                refinements
                    .iter()
                    .any(|(_, _, subtype)| { item.subtype.as_deref() == Some(*subtype) })
            );
            assert_eq!(item.display_name_ru.as_deref(), Some("Реликвия Акси N10"));
            assert_eq!(item.sellable_quantity, 2);
        }
    }

    #[test]
    fn legacy_catalog_tags_preserve_bulk_order_requirements() {
        assert!(catalog_bulk_tradable(
            false,
            &["arcane_enhancement".into(), "rare".into()]
        ));
        assert!(!catalog_bulk_tradable(false, &["mod".into()]));
        assert!(catalog_bulk_tradable(true, &["mod".into()]));
    }

    #[test]
    fn background_market_refresh_respects_clamped_interval() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-08-27T12:00:00Z")
            .expect("valid timestamp")
            .with_timezone(&Utc);
        let snapshot = MarketSnapshotSummary {
            provider: ProviderId::RelicsRun,
            source_date: now.date_naive(),
            fetched_at: now - ChronoDuration::hours(3),
            promoted_at: now - ChronoDuration::hours(3),
            item_count: 1,
            record_count: 1,
            checksum_sha256: "fixture".into(),
        };

        assert!(market_refresh_due(None, now, 4));
        assert!(!market_refresh_due(Some(&snapshot), now, 4));
        assert!(market_refresh_due(Some(&snapshot), now, 2));
        assert!(market_refresh_due(Some(&snapshot), now, 0));

        let future = MarketSnapshotSummary {
            promoted_at: now + ChronoDuration::minutes(5),
            ..snapshot
        };
        assert!(!market_refresh_due(Some(&future), now, 1));
    }

    #[test]
    fn background_game_metadata_refresh_is_daily_and_due_when_missing() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-08-27T12:00:00Z")
            .expect("valid timestamp")
            .with_timezone(&Utc);
        let metadata = GameMetadataSnapshotMetadata {
            source: platscope_domain::GameMetadataSource::WfcdWarframeItems,
            fetched_at: now - ChronoDuration::hours(23),
            schema_version: CURRENT_GAME_METADATA_SCHEMA_VERSION,
            set_count: 1,
            relic_count: 1,
            prime_part_count: 1,
            riven_disposition_count: 1,
            item_definition_count: 1,
            checksum_sha256: "fixture".into(),
        };

        assert!(game_metadata_refresh_due(None, now, 24));
        assert!(!game_metadata_refresh_due(Some(&metadata), now, 24));
        assert!(game_metadata_refresh_due(Some(&metadata), now, 12));
        assert!(game_metadata_refresh_due(Some(&metadata), now, 0));
        let old_metadata = GameMetadataSnapshotMetadata {
            schema_version: CURRENT_GAME_METADATA_SCHEMA_VERSION - 1,
            ..metadata
        };
        assert!(game_metadata_refresh_due(Some(&old_metadata), now, 24));
    }

    #[test]
    fn current_orders_are_active_sorted_and_bounded_per_side() {
        let mut orders = Vec::new();
        for price in [16, 10, 13, 11, 15, 12, 14] {
            orders.push(LiveOrder {
                side: LiveOrderSide::Sell,
                platinum: price,
                quantity: 2,
                per_trade: 1,
                user_status: UserStatus::Online,
            });
        }
        for price in [20, 26, 21, 25, 22, 24, 23] {
            orders.push(LiveOrder {
                side: LiveOrderSide::Buy,
                platinum: price,
                quantity: 1,
                per_trade: 1,
                user_status: UserStatus::InGame,
            });
        }
        orders.push(LiveOrder {
            side: LiveOrderSide::Sell,
            platinum: 1,
            quantity: 1,
            per_trade: 1,
            user_status: UserStatus::Offline,
        });
        let active: Vec<_> = orders
            .iter()
            .filter(|order| active_live_order(order))
            .collect();

        let visible = bounded_live_orders(&active);
        assert_eq!(visible.len(), 10);
        assert_eq!(
            visible
                .iter()
                .filter(|order| order.side == LiveOrderSide::Sell)
                .map(|order| order.platinum)
                .collect::<Vec<_>>(),
            vec![10, 11, 12, 13, 14]
        );
        assert_eq!(
            visible
                .iter()
                .filter(|order| order.side == LiveOrderSide::Buy)
                .map(|order| order.platinum)
                .collect::<Vec<_>>(),
            vec![26, 25, 24, 23, 22]
        );
    }

    #[test]
    fn riven_catalog_items_use_the_separate_pricing_boundary() {
        let key = MarketVariantKey::new("soma_riven_mod", Platform::Pc, None, None::<String>)
            .expect("Riven key");
        assert_eq!(
            market_item_kind(&["riven".into(), "mod".into()], &key),
            MarketItemKind::Riven
        );
    }

    #[test]
    fn market_search_never_reuses_pc_bulk_price_on_another_platform() {
        let database = Mutex::new(Database::open_in_memory().expect("database opens"));
        let observed_at = chrono::DateTime::parse_from_rfc3339("2026-08-27T08:00:00Z")
            .expect("valid timestamp")
            .with_timezone(&Utc);
        let source_date = observed_at.date_naive();
        let pc_key = MarketVariantKey::new("nyx_prime_set", Platform::Pc, None, None::<String>)
            .expect("valid key");
        let catalog = ItemCatalog {
            metadata: CatalogMetadata {
                provider: ProviderId::RelicsRun,
                fetched_at: observed_at,
                schema_version: 1,
                item_count: 1,
                checksum_sha256: "catalog".into(),
            },
            items: vec![CatalogItem {
                item_id: "nyx-set".into(),
                slug: pc_key.slug.clone(),
                display_name_en: "Nyx Prime Set".into(),
                display_name_ru: Some("Никс Прайм: комплект".into()),
                thumb: None,
                thumb_ru: None,
                game_ref: None,
                bulk_tradable: false,
                max_rank: None,
                subtypes: Vec::new(),
                tags: vec!["prime".into(), "set".into()],
            }],
        };
        let snapshot = NormalizedMarketSnapshot {
            metadata: SnapshotMetadata {
                provider: ProviderId::RelicsRun,
                source_date,
                fetched_at: observed_at,
                schema_version: 1,
                item_count: 1,
                record_count: 1,
                checksum_sha256: "snapshot".into(),
            },
            records: vec![MarketRecord {
                key: pc_key,
                external_item_id: "nyx-set".into(),
                display_name_en: "Nyx Prime Set".into(),
                observed_at,
                order_type: MarketOrderType::Closed,
                median: Some(80.0),
                average: Some(80.0),
                min_price: Some(75.0),
                max_price: Some(85.0),
                volume: 12.0,
                raw_json: "{}".into(),
            }],
        };
        {
            let mut database = database.lock().expect("database lock");
            database
                .promote_catalog(&catalog)
                .expect("catalog promotion");
            database
                .promote_market_snapshot(&snapshot)
                .expect("snapshot promotion");
        }

        let pc =
            MarketBrowserService::search(&database, "nyx", 10, Language::English, Platform::Pc)
                .expect("PC search");
        assert_eq!(pc.rows[0].recommendation.fair_price, Some(80.0));

        let xbox =
            MarketBrowserService::search(&database, "nyx", 10, Language::English, Platform::Xbox)
                .expect("Xbox search");
        assert_eq!(xbox.rows[0].recommendation.key.platform, Platform::Xbox);
        assert_eq!(xbox.rows[0].recommendation.fair_price, None);
        assert_eq!(xbox.rows[0].recommendation.closed_volume, None);
    }

    #[test]
    fn regular_mod_variant_reuses_legacy_bulk_price_without_losing_order_subtype() {
        let database = Mutex::new(Database::open_in_memory().expect("database opens"));
        let observed_at = chrono::DateTime::parse_from_rfc3339("2026-08-29T08:00:00Z")
            .expect("valid timestamp")
            .with_timezone(&Utc);
        let legacy_key =
            MarketVariantKey::new("animal_instinct", Platform::Pc, Some(5), None::<String>)
                .expect("legacy key");
        let regular_key =
            MarketVariantKey::new("animal_instinct", Platform::Pc, Some(5), Some("regular"))
                .expect("regular key");
        let catalog = ItemCatalog {
            metadata: CatalogMetadata {
                provider: ProviderId::WarframeMarket,
                fetched_at: observed_at,
                schema_version: CURRENT_CATALOG_SCHEMA_VERSION,
                item_count: 1,
                checksum_sha256: "catalog".into(),
            },
            items: vec![CatalogItem {
                item_id: "559dacd3e779897ba8819969".into(),
                slug: legacy_key.slug.clone(),
                display_name_en: "Animal Instinct".into(),
                display_name_ru: Some("Животный Инстинкт".into()),
                thumb: None,
                thumb_ru: None,
                game_ref: None,
                bulk_tradable: false,
                max_rank: Some(5),
                subtypes: vec!["regular".into(), "atragraph".into()],
                tags: vec!["mod".into()],
            }],
        };
        let snapshot = NormalizedMarketSnapshot {
            metadata: SnapshotMetadata {
                provider: ProviderId::RelicsRun,
                source_date: observed_at.date_naive(),
                fetched_at: observed_at,
                schema_version: 1,
                item_count: 1,
                record_count: 1,
                checksum_sha256: "snapshot".into(),
            },
            records: vec![MarketRecord {
                key: legacy_key,
                external_item_id: "559dacd3e779897ba8819969".into(),
                display_name_en: "Animal Instinct".into(),
                observed_at,
                order_type: MarketOrderType::Closed,
                median: Some(10.0),
                average: Some(10.0),
                min_price: Some(10.0),
                max_price: Some(10.0),
                volume: 5.0,
                raw_json: "{}".into(),
            }],
        };
        {
            let mut database = database.lock().expect("database lock");
            database
                .promote_catalog(&catalog)
                .expect("catalog promotion");
            database
                .promote_market_snapshot(&snapshot)
                .expect("snapshot promotion");
        }

        let recommendation = PricingService::price_current_variant(
            &database,
            &regular_key,
            MarketItemKind::Standard,
        )
        .expect("pricing succeeds")
        .expect("snapshot exists");
        assert_eq!(recommendation.key.subtype.as_deref(), Some("regular"));
        assert_eq!(recommendation.fair_price, Some(10.0));

        let search = MarketBrowserService::search(
            &database,
            "animal instinct",
            10,
            Language::Russian,
            Platform::Pc,
        )
        .expect("market search succeeds");
        assert_eq!(
            search.rows[0].recommendation.key.subtype.as_deref(),
            Some("regular")
        );
        assert_eq!(search.rows[0].recommendation.fair_price, Some(10.0));
    }

    #[test]
    fn conflicting_vault_metadata_fails_closed_to_unknown() {
        let mut statuses = HashMap::new();
        merge_vault_status(&mut statuses, "nyx_prime_set", VaultStatus::Vaulted);
        merge_vault_status(&mut statuses, "nyx_prime_set", VaultStatus::Vaulted);
        assert_eq!(statuses.get("nyx_prime_set"), Some(&VaultStatus::Vaulted));

        merge_vault_status(&mut statuses, "nyx_prime_set", VaultStatus::Available);
        assert_eq!(statuses.get("nyx_prime_set"), Some(&VaultStatus::Unknown));
    }

    fn verify_market_search(database: &Mutex<Database>) {
        let search_started = Instant::now();
        let search = MarketBrowserService::search(
            database,
            "nyx prime",
            60,
            Language::Russian,
            Platform::Pc,
        )
        .expect("market search succeeds");
        let search_elapsed = search_started.elapsed();
        let mastery_rows = search
            .rows
            .iter()
            .filter(|row| row.mastery_requirement.is_some())
            .count();
        eprintln!(
            "search_rows={} mastery_rows={} truncated={} elapsed_ms={} first={}",
            search.rows.len(),
            mastery_rows,
            search.truncated,
            search_elapsed.as_millis(),
            search
                .rows
                .first()
                .map_or("—", |row| row.display_name.as_str())
        );
        assert!(!search.rows.is_empty());
        assert!(mastery_rows > 0);
        assert!(search_elapsed.as_millis() < 500);
    }

    async fn verify_live_and_history(database: &Mutex<Database>, market_key: &MarketVariantKey) {
        let live_service = LivePricingService::production().expect("live service initializes");
        let live = live_service
            .price_current_variant(
                database,
                market_key,
                MarketItemKind::Standard,
                &AppSettings::default(),
            )
            .await
            .expect("live pricing succeeds")
            .expect("bulk snapshot exists");
        eprintln!(
            "lowest={:?} low3={:?} low5={:?} quick={:?} sell_orders={} buy_orders={} visible_orders={} state={:?}",
            live.recommendation.lowest_ask,
            live.recommendation.depth_three,
            live.recommendation.depth_price,
            live.recommendation.quick_sell,
            live.sell_order_count,
            live.buy_order_count,
            live.orders.len(),
            live.quote_state
        );
        assert!(live.recommendation.lowest_ask.is_some());
        assert!(live.recommendation.depth_three.is_some());
        assert!(live.recommendation.depth_price.is_some());
        assert!(live.sell_order_count + live.buy_order_count > 0);
        assert!(!live.orders.is_empty());
        assert!(live.orders.len() <= 10);
        assert!(live.orders.iter().all(|order| order.quantity > 0));
        assert_eq!(live.quote_state, LiveQuoteState::Network);

        let cached = live_service
            .price_current_variant(
                database,
                market_key,
                MarketItemKind::Standard,
                &AppSettings::default(),
            )
            .await
            .expect("cached live pricing succeeds")
            .expect("bulk snapshot exists");
        assert_eq!(cached.quote_state, LiveQuoteState::Cache);

        let history_service = HistoryService::production().expect("history service initializes");
        let history_bootstrap = history_service
            .bootstrap(database)
            .await
            .expect("history bootstrap succeeds");
        eprintln!(
            "history_imported={} skipped={} coverage={} failures={}",
            history_bootstrap.imported_days,
            history_bootstrap.skipped_days,
            history_bootstrap.coverage.day_count,
            history_bootstrap.failures.len()
        );
        assert!(history_bootstrap.imported_days > 0);
        assert!(history_bootstrap.coverage.day_count >= 4);

        let history = HistoryService::view(
            database,
            market_key,
            7,
            live.recommendation.fair_price,
            live.recommendation.lowest_ask,
        )
        .expect("history view succeeds");
        eprintln!(
            "history_points={} median_7d={:?} change_7d={:?} timing={:?}",
            history.points.len(),
            history.trend.median_7d,
            history.trend.change_7d,
            history.trend.timing
        );
        assert!(history.points.len() >= 3);
        assert!(history.trend.median_7d.is_some());
    }

    async fn refresh_and_verify_metadata(database: &Mutex<Database>) {
        let metadata_service =
            GameMetadataService::production().expect("metadata provider initializes");
        let metadata = metadata_service
            .refresh(database)
            .await
            .expect("WFCD metadata refresh succeeds");
        eprintln!(
            "metadata_sets={} relics={} prime_parts={} riven_dispositions={} item_definitions={} stale={}",
            metadata.metadata.set_count,
            metadata.metadata.relic_count,
            metadata.metadata.prime_part_count,
            metadata.metadata.riven_disposition_count,
            metadata.metadata.item_definition_count,
            metadata.stale
        );
        assert!(metadata.metadata.set_count > 100);
        assert!(metadata.metadata.relic_count > 1_000);
        assert!(metadata.metadata.prime_part_count > 500);
        assert!(metadata.metadata.riven_disposition_count > 500);
        assert!(metadata.metadata.item_definition_count > 100);
        assert!(!metadata.stale);
    }

    #[test]
    fn settings_defaults_are_offline_safe() {
        let settings = AppSettings::default();
        assert_eq!(settings.language, Language::Russian);
        assert_eq!(settings.platform, Platform::Pc);
        assert_eq!(settings.live_quote_ttl_seconds, 90);
        assert_eq!(settings.keep_inventory_copies, 1);
        assert!(settings.crossplay);
    }

    #[test]
    fn account_writes_require_explicit_confirmation() {
        assert!(require_write_confirmation(true).is_ok());
        let error = require_write_confirmation(false).expect_err("missing confirmation rejected");
        assert!(error.to_string().contains("explicit confirmation"));
    }

    #[tokio::test]
    #[ignore = "требует доступ к production bulk providers"]
    async fn production_bulk_refresh_smoke_test() {
        let database = Mutex::new(Database::open_in_memory().expect("database opens"));
        let service = MarketDataService::production().expect("providers initialize");

        let outcome = service
            .refresh(&database)
            .await
            .expect("live bulk refresh succeeds");

        eprintln!(
            "provider={:?} source_date={} catalog_items={} market_items={} records={}",
            outcome.snapshot.provider,
            outcome.snapshot.source_date,
            outcome.catalog_item_count,
            outcome.snapshot.item_count,
            outcome.snapshot.record_count
        );

        assert!(outcome.catalog_item_count >= 3_000);
        assert!(outcome.snapshot.item_count >= 3_000);
        assert!(outcome.snapshot.record_count >= 9_000);
        assert!(!outcome.stale);

        let market_key =
            MarketVariantKey::new("secura_dual_cestra", Platform::Pc, None, None::<String>)
                .expect("known live key");
        let price =
            PricingService::price_current_variant(&database, &market_key, MarketItemKind::Standard)
                .expect("pricing query succeeds")
                .expect("snapshot exists");
        eprintln!(
            "fair={:?} list={:?} confidence={:?} reasons={}",
            price.fair_price,
            price.list_price,
            price.confidence,
            price.reasons.len()
        );
        assert!(price.fair_price.is_some());
        assert!(!price.reasons.is_empty());

        refresh_and_verify_metadata(&database).await;

        verify_market_search(&database);

        verify_live_and_history(&database, &market_key).await;
    }
}
