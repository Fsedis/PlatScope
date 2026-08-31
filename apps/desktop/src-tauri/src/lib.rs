#![forbid(unsafe_code)]

mod trade_log;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use platscope_core::{
    AccountOrder, AccountOrderItemView, AccountOrderType, AccountService, AccountSetComponentView,
    AccountView, AppSettings, CreateListingInput, DEFAULT_MARKET_SEARCH_LIMIT,
    GameMetadataRefreshOutcome, GameMetadataService, HistoryBootstrapOutcome, HistoryService,
    InsightsService, InsightsView, InventoryService, InventoryView, LivePricingResult,
    LivePricingService, LiveSellNowResult, LoggingGuard, MarketBrowserService, MarketDataService,
    MarketHistoryView, MarketRefreshOutcome, MarketSearchResult, MarketSearchRow,
    PriceRecommendation, PricingService, ResourceConverterService, ResourceConverterView,
    SETTINGS_KEY, SellNowService, SellNowView, SetComponentInsight, UpdateListingInput,
    enrich_account_view, init_logging,
};
use platscope_domain::{
    GameMetadataSnapshot, InventoryResolution, MarketItemKind, MarketVariantKey, PriceConfidence,
    PrimeSetDefinition,
};
use platscope_readonly_scan::inventory::{
    InventoryScanner as ReadOnlyInventoryScanner, ReadOnlyScanResult,
};
use platscope_storage::{
    Database, HistoryCoverage, MarketSnapshotSummary, NewTradeEvent, ProviderHealth, TradeEvent,
    TradeEventStatus, TradeItem, TradeSalesSummary,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State};
use tauri_plugin_opener::OpenerExt;

struct AppState {
    database: Mutex<Database>,
    // OCR наград чувствителен к задержкам: обновление рынка может удерживать основной
    // mutex БД дольше, чем открыт экран выбора. Отдельное WAL-чтение не блокирует награды.
    reward_database: Mutex<Database>,
    market_data_service: MarketDataService,
    live_pricing_service: LivePricingService,
    history_service: HistoryService,
    game_metadata_service: GameMetadataService,
    resource_converter_service: ResourceConverterService,
    account_service: AccountService,
    trade_reconciliation_lock: tokio::sync::Mutex<()>,
    read_only_inventory_scanner: Arc<ReadOnlyInventoryScanner>,
    reward_scan_in_flight: AtomicBool,
    reward_realtime_active: AtomicBool,
    reward_relic_paths: Mutex<HashSet<String>>,
    latest_reward_scan: Mutex<Option<RelicRewardScanView>>,
    reward_overlay_generation: AtomicU64,
    data_directory: PathBuf,
    _logging_guard: LoggingGuard,
}

const ACCOUNT_DEVICE_ID_KEY: &str = "account.device_id";
const BULK_REFRESH_CHECK_INTERVAL: Duration = Duration::from_secs(5 * 60);
const GAME_METADATA_REFRESH_HOURS: u16 = 24;
const MAX_MARKET_ITEMS_PER_OPEN: usize = 6;
const MAX_MARKET_SLUG_BYTES: usize = 96;
const REWARD_OCR_EXECUTABLE: &str = "platscope-reward-ocr.exe";
const REWARD_LOG_POLL_INTERVAL: Duration = Duration::from_millis(350);
const REWARD_LOG_DEBOUNCE: Duration = Duration::from_secs(8);
const REWARD_LOG_READ_LIMIT: u64 = 256 * 1024;
const REWARD_OVERLAY_VISIBLE_FOR: Duration = Duration::from_secs(18);
const WFCD_IMAGE_BASE_URL: &str = "https://cdn.warframestat.us/img/";
const COMPONENT_IMAGE_PROTOCOL: &str = "component-image";
const COMPONENT_IMAGE_CACHE_DIRECTORY: &str = "component-images";
const MAX_COMPONENT_IMAGE_BYTES: usize = 1024 * 1024;
const MIN_REWARD_RECOMMENDATION_OCR_CONFIDENCE: f64 = 0.75;
static COMPONENT_IMAGE_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn hide_process_window(command: &mut Command) {
    command.creation_flags(CREATE_NO_WINDOW);
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RewardOcrCatalogItem {
    item_id: String,
    slug: String,
    name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RewardOcrRequest {
    catalog: Vec<RewardOcrCatalogItem>,
    image_path: Option<String>,
    tessdata_path: Option<String>,
    ui_scale: Option<f64>,
    max_attempts: u8,
    retry_interval_ms: u16,
    initial_delay_ms: u16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RewardWatcherRelic {
    relic_game_ref: String,
    reward_slugs: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RewardWatcherRequest {
    catalog: Vec<RewardOcrCatalogItem>,
    relics: Vec<RewardWatcherRelic>,
    ui_scale: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RewardTriggerEvent {
    #[serde(rename = "type")]
    event_type: String,
    already_exists: Option<bool>,
    path: Option<String>,
    reset: Option<bool>,
    source: Option<String>,
}

struct RewardScanGuard<'a>(&'a AtomicBool);

impl Drop for RewardScanGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RewardOcrResponse {
    status: String,
    message: Option<String>,
    capture_width: Option<u32>,
    capture_height: Option<u32>,
    theme: Option<String>,
    rewards: Vec<RewardOcrMatch>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RewardOcrMatch {
    slot: u8,
    raw_text: String,
    item_id: Option<String>,
    slug: Option<String>,
    name: Option<String>,
    confidence: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RewardSetCompletion {
    set_name: String,
    set_price: Option<f64>,
    incremental_value: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RewardSetPart {
    name: String,
    image_url: Option<String>,
    owned_quantity: u32,
    required_quantity: u32,
    is_reward: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RewardSetOverview {
    set_name: String,
    set_price: Option<f64>,
    completed_sets: Option<u32>,
    target_set_number: Option<u32>,
    ready_components: Option<u32>,
    total_components: u32,
    parts: Vec<RewardSetPart>,
}

type RewardCatalogDetails = HashMap<String, (String, Option<String>)>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RelicRewardChoice {
    slot: u8,
    raw_text: String,
    confidence: f64,
    item_id: Option<String>,
    slug: Option<String>,
    display_name: Option<String>,
    market: Option<MarketSearchRow>,
    ducats: Option<u32>,
    owned_quantity: Option<u32>,
    set: Option<RewardSetOverview>,
    completes_set: Option<RewardSetCompletion>,
    choice_value: Option<f64>,
    recommended: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RelicRewardScanView {
    status: String,
    message: Option<String>,
    recognized_count: usize,
    scan_duration_ms: u64,
    capture_width: Option<u32>,
    capture_height: Option<u32>,
    overlay_scale: f64,
    theme: Option<String>,
    rewards: Vec<RelicRewardChoice>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WarframeWindowRect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RewardOverlayGeometry {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FoundationStatus {
    app_name: &'static str,
    app_version: &'static str,
    database_path: String,
    schema_version: i64,
    offline_ready: bool,
    market_snapshot: Option<MarketSnapshotSummary>,
    catalog_item_count: Option<u64>,
    history_coverage: HistoryCoverage,
    inventory_item_count: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticsStatus {
    generated_at: DateTime<Utc>,
    foundation: FoundationStatus,
    providers: Vec<ProviderHealth>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SafeDiagnosticsReport {
    report_version: u8,
    generated_at: DateTime<Utc>,
    app_name: &'static str,
    app_version: &'static str,
    schema_version: i64,
    offline_ready: bool,
    market_snapshot: Option<MarketSnapshotSummary>,
    catalog_item_count: Option<u64>,
    history_coverage: HistoryCoverage,
    inventory_item_count: Option<u64>,
    providers: Vec<ProviderHealth>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticsExportResult {
    path: String,
    bytes: u64,
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
fn foundation_status(state: State<'_, AppState>) -> Result<FoundationStatus, String> {
    load_foundation_status(&state)
}

fn load_foundation_status(state: &AppState) -> Result<FoundationStatus, String> {
    let database = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?;
    let schema_version = database
        .schema_version()
        .map_err(|error| error.to_string())?;

    let market_snapshot = database
        .current_market_snapshot()
        .map_err(|error| error.to_string())?;
    let catalog_item_count = database
        .load_current_catalog()
        .map_err(|error| error.to_string())?
        .map(|catalog| catalog.metadata.item_count);
    let history_coverage = database
        .history_coverage()
        .map_err(|error| error.to_string())?;
    let inventory_item_count = database
        .current_inventory_snapshot()
        .map_err(|error| error.to_string())?
        .map(|snapshot| snapshot.metadata.item_count);

    Ok(FoundationStatus {
        app_name: "PlatScope",
        app_version: env!("CARGO_PKG_VERSION"),
        database_path: state
            .data_directory
            .join("platscope.db")
            .display()
            .to_string(),
        schema_version,
        offline_ready: schema_version >= 1,
        market_snapshot,
        catalog_item_count,
        history_coverage,
        inventory_item_count,
    })
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
fn diagnostics_status(state: State<'_, AppState>) -> Result<DiagnosticsStatus, String> {
    load_diagnostics_status(&state)
}

fn load_diagnostics_status(state: &AppState) -> Result<DiagnosticsStatus, String> {
    let foundation = load_foundation_status(state)?;
    let providers = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .provider_health()
        .map_err(|error| error.to_string())?;
    Ok(DiagnosticsStatus {
        generated_at: Utc::now(),
        foundation,
        providers,
    })
}

fn safe_diagnostics_report(status: DiagnosticsStatus) -> SafeDiagnosticsReport {
    SafeDiagnosticsReport {
        report_version: 1,
        generated_at: status.generated_at,
        app_name: status.foundation.app_name,
        app_version: status.foundation.app_version,
        schema_version: status.foundation.schema_version,
        offline_ready: status.foundation.offline_ready,
        market_snapshot: status.foundation.market_snapshot,
        catalog_item_count: status.foundation.catalog_item_count,
        history_coverage: status.foundation.history_coverage,
        inventory_item_count: status.foundation.inventory_item_count,
        providers: status.providers,
    }
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
fn export_diagnostics_report(
    state: State<'_, AppState>,
) -> Result<DiagnosticsExportResult, String> {
    let report = safe_diagnostics_report(load_diagnostics_status(&state)?);
    let (destination, bytes) =
        write_safe_diagnostics_report(&state.data_directory.join("diagnostics"), &report)?;
    tracing::info!(
        event = "diagnostics_report_exported",
        bytes,
        "safe diagnostic report exported"
    );
    Ok(DiagnosticsExportResult {
        path: destination.display().to_string(),
        bytes,
    })
}

fn write_safe_diagnostics_report(
    directory: &Path,
    report: &SafeDiagnosticsReport,
) -> Result<(PathBuf, u64), String> {
    let raw = serde_json::to_vec_pretty(report).map_err(|error| error.to_string())?;
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let timestamp = report.generated_at.format("%Y%m%dT%H%M%S%3fZ");
    let destination = directory.join(format!("platscope-diagnostics-{timestamp}.json"));
    let temporary = directory.join(format!(".platscope-diagnostics-{timestamp}.tmp"));
    fs::write(&temporary, &raw).map_err(|error| error.to_string())?;
    if let Err(error) = fs::rename(&temporary, &destination) {
        let _ = fs::remove_file(&temporary);
        return Err(error.to_string());
    }
    let bytes = u64::try_from(raw.len()).map_err(|_| "report is too large".to_owned())?;
    Ok((destination, bytes))
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri owns command state and app handle.
async fn scan_read_only_inventory(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<InventoryView, String> {
    let scanner = Arc::clone(&state.read_only_inventory_scanner);
    let scan_result = tauri::async_runtime::spawn_blocking(move || scanner.scan(None, None))
        .await
        .map_err(|error| format!("scan task failed to run: {error}"))?
        .map_err(|error| match error {
            platscope_readonly_scan::error::ScanError::Busy => {
                "inventory scan is already running; wait for it to finish".to_owned()
            }
            platscope_readonly_scan::error::ScanError::Failed(_) => {
                "read-only Warframe scan failed; session credentials were discarded".to_owned()
            }
        })?;
    let ReadOnlyScanResult {
        inventory_bytes: bytes,
        session: scan_info,
        nightwave_vendor,
        nightwave_status,
    } = scan_result;
    let response_bytes = bytes.len();
    let raw_json = String::from_utf8(bytes)
        .map_err(|_| "Digital Extremes returned non-UTF-8 inventory JSON".to_owned())?;
    let settings = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .get_setting::<AppSettings>(SETTINGS_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    let view = localize_inventory_images(
        InventoryService::import_read_only_scan_json(&state.database, &raw_json, &settings)
            .map_err(|error| error.to_string())?,
    );
    let nightwave_offer_count = nightwave_vendor.as_ref().map_or(0, |snapshot| {
        let count = snapshot.offers.len();
        if let Err(error) =
            ResourceConverterService::cache_nightwave_vendor(&state.database, snapshot)
        {
            tracing::warn!(
                event = "nightwave_vendor_cache_failed",
                error = %error,
                "exact Nightwave vendor snapshot was not cached"
            );
            return 0;
        }
        count
    });
    tracing::info!(
        event = "read_only_inventory_scan_finished",
        build = scan_info.build.as_deref().unwrap_or("unknown"),
        platform_tag = scan_info.ct,
        credential_hits = scan_info.cred_hits,
        distinct_credentials = scan_info.distinct_creds,
        response_bytes,
        source_rows = view.metadata.item_count,
        resolved_rows = view.summary.resolved_rows,
        attention_rows = view.summary.attention_rows,
        nightwave_status = nightwave_status.code(),
        nightwave_offer_count,
        "read-only Warframe inventory scan imported"
    );
    app.emit("inventory-updated", ())
        .map_err(|error| error.to_string())?;
    Ok(view)
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
fn load_inventory(state: State<'_, AppState>) -> Result<Option<InventoryView>, String> {
    let settings = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .get_setting::<AppSettings>(SETTINGS_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    InventoryService::view(&state.database, &settings)
        .map(|view| view.map(localize_inventory_images))
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
fn set_inventory_keep_copies(
    keep_copies: u32,
    state: State<'_, AppState>,
) -> Result<Option<InventoryView>, String> {
    if keep_copies > 10 {
        return Err("keep copies must be within 0..=10".into());
    }
    let settings = {
        let database = state
            .database
            .lock()
            .map_err(|_| "database state is unavailable".to_owned())?;
        let mut settings = database
            .get_setting::<AppSettings>(SETTINGS_KEY)
            .map_err(|error| error.to_string())?
            .unwrap_or_default();
        settings.keep_inventory_copies = keep_copies;
        database
            .set_setting(SETTINGS_KEY, &settings)
            .map_err(|error| error.to_string())?;
        settings
    };
    InventoryService::view(&state.database, &settings)
        .map(|view| view.map(localize_inventory_images))
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
fn sell_now(state: State<'_, AppState>) -> Result<Option<SellNowView>, String> {
    let settings = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .get_setting::<AppSettings>(SETTINGS_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    SellNowService::view(&state.database, &settings)
        .map(|view| view.map(localize_sell_now_images))
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri deserializes command values by ownership.
async fn sell_now_live(
    key: MarketVariantKey,
    state: State<'_, AppState>,
) -> Result<Option<LiveSellNowResult>, String> {
    let settings = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .get_setting::<AppSettings>(SETTINGS_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    SellNowService::live_row(
        &state.live_pricing_service,
        &state.database,
        &key,
        &settings,
    )
    .await
    .map(|view| view.map(localize_live_sell_now_images))
    .map_err(|error| error.to_string())
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri deserializes command values by ownership.
fn market_history(
    key: MarketVariantKey,
    days: u16,
    current_price: Option<f64>,
    live_lowest_ask: Option<f64>,
    state: State<'_, AppState>,
) -> Result<MarketHistoryView, String> {
    HistoryService::view(&state.database, &key, days, current_price, live_lowest_ask)
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
async fn bootstrap_history(state: State<'_, AppState>) -> Result<HistoryBootstrapOutcome, String> {
    state
        .history_service
        .bootstrap_full(&state.database)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
async fn refresh_market_data(state: State<'_, AppState>) -> Result<MarketRefreshOutcome, String> {
    state
        .market_data_service
        .refresh(&state.database)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
async fn refresh_game_metadata(
    state: State<'_, AppState>,
) -> Result<GameMetadataRefreshOutcome, String> {
    state
        .game_metadata_service
        .refresh(&state.database)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
fn insights(state: State<'_, AppState>) -> Result<Option<InsightsView>, String> {
    let settings = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .get_setting::<AppSettings>(SETTINGS_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    InsightsService::view(&state.database, &settings)
        .map(|view| view.map(localize_insight_images))
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
async fn resource_converter(
    state: State<'_, AppState>,
) -> Result<Option<ResourceConverterView>, String> {
    let settings = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .get_setting::<AppSettings>(SETTINGS_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    state
        .resource_converter_service
        .view(&state.database, &settings)
        .await
        .map(|view| view.map(localize_resource_converter_images))
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri injects AppHandle into commands by value.
fn open_market_items(slugs: Vec<String>, app: AppHandle) -> Result<usize, String> {
    let slugs = validate_market_slugs(slugs)?;
    for slug in &slugs {
        app.opener()
            .open_url(market_item_url(slug), None::<&str>)
            .map_err(|error| format!("failed to open Warframe Market: {error}"))?;
    }
    Ok(slugs.len())
}

fn market_item_url(slug: &str) -> String {
    format!("https://warframe.market/ru/items/{slug}")
}

fn validate_market_slugs(slugs: Vec<String>) -> Result<Vec<String>, String> {
    if slugs.is_empty() || slugs.len() > MAX_MARKET_ITEMS_PER_OPEN {
        return Err(format!(
            "expected 1 to {MAX_MARKET_ITEMS_PER_OPEN} market items"
        ));
    }
    let mut unique = HashSet::new();
    let mut validated = Vec::with_capacity(slugs.len());
    for slug in slugs {
        let slug = slug.trim();
        let valid = !slug.is_empty()
            && slug.len() <= MAX_MARKET_SLUG_BYTES
            && slug
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
        if !valid {
            return Err("invalid Warframe Market item identity".into());
        }
        if unique.insert(slug.to_owned()) {
            validated.push(slug.to_owned());
        }
    }
    Ok(validated)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
async fn account_status(state: State<'_, AppState>) -> Result<AccountView, String> {
    let view = state
        .account_service
        .view()
        .await
        .map_err(|error| error.to_string())?;
    let settings = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .get_setting::<AppSettings>(SETTINGS_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    enrich_account_view(&state.database, settings.language, view)
        .map(localize_account_images)
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri deserializes credentials by ownership.
async fn account_connect(
    email: String,
    password: String,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<AccountView, String> {
    let view = state
        .account_service
        .connect(&email, &password)
        .await
        .map_err(|error| error.to_string())?;
    let settings = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .get_setting::<AppSettings>(SETTINGS_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    let view = enrich_account_view(&state.database, settings.language, view)
        .map(localize_account_images)
        .map_err(|error| error.to_string())?;
    tracing::info!(event = "wfm_account_connected", "WFM account connected");
    spawn_pending_trade_reconciliation(app_handle);
    Ok(view)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
async fn account_disconnect(state: State<'_, AppState>) -> Result<bool, String> {
    let revoked = state
        .account_service
        .disconnect()
        .await
        .map_err(|error| error.to_string())?;
    tracing::info!(
        event = "wfm_account_disconnected",
        remotely_revoked = revoked,
        "local WFM credential removed"
    );
    Ok(revoked)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SellListingIntent {
    item_id: String,
    quantity: u32,
    per_trade: u32,
    rank: Option<u16>,
    charges: Option<u16>,
    subtype: Option<String>,
    amber_stars: Option<u16>,
    cyan_stars: Option<u16>,
}

impl SellListingIntent {
    fn from_create(input: &CreateListingInput) -> Self {
        Self {
            item_id: input.item_id.clone(),
            quantity: input.quantity,
            per_trade: input.per_trade.unwrap_or(1),
            rank: input.rank,
            charges: input.charges,
            subtype: input.subtype.clone(),
            amber_stars: input.amber_stars,
            cyan_stars: input.cyan_stars,
        }
    }

    fn from_update(order: &AccountOrder, input: &UpdateListingInput) -> Result<Self, String> {
        Ok(Self {
            item_id: order.item_id.clone().ok_or_else(|| {
                "У ордера не определён предмет; обновите список ордеров.".to_owned()
            })?,
            quantity: input.quantity.unwrap_or(order.quantity),
            per_trade: input.per_trade.or(order.per_trade).unwrap_or(1),
            rank: input.rank.or(order.rank),
            charges: input.charges.or(order.charges),
            subtype: input.subtype.clone().or_else(|| order.subtype.clone()),
            amber_stars: input.amber_stars.or(order.amber_stars),
            cyan_stars: input.cyan_stars.or(order.cyan_stars),
        })
    }

    fn matches_order(&self, order: &AccountOrder) -> bool {
        order.order_type == AccountOrderType::Sell
            && order.item_id.as_deref() == Some(self.item_id.as_str())
            && order.rank == self.rank
            && order.charges == self.charges
            && order.subtype == self.subtype
            && order.amber_stars == self.amber_stars
            && order.cyan_stars == self.cyan_stars
    }

    fn matches_inventory_key(&self, item_id: Option<&str>, key: &MarketVariantKey) -> bool {
        item_id == Some(self.item_id.as_str())
            && key.rank == self.rank
            && key.charges == self.charges
            && key.subtype == self.subtype
            && key.amber_stars == self.amber_stars
            && key.cyan_stars == self.cyan_stars
    }
}

fn validate_sell_listing_inventory(
    intent: &SellListingIntent,
    inventory: &InventoryView,
    existing_orders: &[AccountOrder],
    excluded_order_id: Option<&str>,
    prime_set: Option<&PrimeSetDefinition>,
    set_component_reservations: &HashMap<String, u32>,
) -> Result<(), String> {
    if !(1..=6).contains(&intent.per_trade) || !intent.quantity.is_multiple_of(intent.per_trade) {
        return Err("Количество должно делиться на размер одного торгового лота (1–6).".into());
    }
    if let Some(definition) = prime_set {
        return validate_prime_set_listing(
            intent,
            inventory,
            existing_orders,
            excluded_order_id,
            definition,
            set_component_reservations,
        );
    }
    let available = inventory
        .items
        .iter()
        .filter(|item| {
            item.resolution == InventoryResolution::Resolved
                && item
                    .key
                    .as_ref()
                    .is_some_and(|key| intent.matches_inventory_key(item.item_id.as_deref(), key))
        })
        .fold(0_u32, |total, item| {
            total.saturating_add(item.sellable_quantity)
        });
    if available == 0 {
        return Err(
            "Этот точный вариант сейчас нельзя продать: проверьте ранг, заряды и резерв копий."
                .into(),
        );
    }
    let reserved = existing_orders
        .iter()
        .filter(|order| {
            order.visible
                && excluded_order_id != Some(order.id.as_str())
                && intent.matches_order(order)
        })
        .fold(0_u32, |total, order| total.saturating_add(order.quantity));
    let reserved_by_sets = set_component_reservations
        .get(&intent.item_id)
        .copied()
        .unwrap_or(0);
    let total_reserved = reserved.saturating_add(reserved_by_sets);
    let free = available.saturating_sub(total_reserved);
    if intent.quantity > free {
        return Err(format!(
            "Для продажи доступно {free} шт.: ещё {total_reserved} шт. уже зарезервировано активными ордерами."
        ));
    }
    Ok(())
}

fn validate_prime_set_listing(
    intent: &SellListingIntent,
    inventory: &InventoryView,
    existing_orders: &[AccountOrder],
    excluded_order_id: Option<&str>,
    definition: &PrimeSetDefinition,
    set_component_reservations: &HashMap<String, u32>,
) -> Result<(), String> {
    if intent.rank.is_some()
        || intent.charges.is_some()
        || intent.subtype.is_some()
        || intent.amber_stars.is_some()
        || intent.cyan_stars.is_some()
    {
        return Err("Полный комплект не должен иметь ранг, заряды или вариант детали.".into());
    }
    let available_sets = definition
        .components
        .iter()
        .filter(|component| component.required_quantity > 0)
        .map(|component| {
            let component_items = inventory
                .items
                .iter()
                .filter(|item| {
                    item.resolution == InventoryResolution::Resolved
                        && item
                            .key
                            .as_ref()
                            .is_some_and(|key| key.slug == component.slug)
                })
                .collect::<Vec<_>>();
            let available = component_items.iter().fold(0_u32, |total, item| {
                total.saturating_add(item.sellable_quantity)
            });
            let reserved = existing_orders
                .iter()
                .filter(|order| {
                    order.visible
                        && excluded_order_id != Some(order.id.as_str())
                        && component_items.iter().any(|item| {
                            item.key.as_ref().is_some_and(|key| {
                                order.order_type == AccountOrderType::Sell
                                    && order.item_id.as_deref() == item.item_id.as_deref()
                                    && order.rank == key.rank
                                    && order.charges == key.charges
                                    && order.subtype == key.subtype
                                    && order.amber_stars == key.amber_stars
                                    && order.cyan_stars == key.cyan_stars
                            })
                        })
                })
                .fold(0_u32, |total, order| total.saturating_add(order.quantity));
            let reserved_by_sets = component_items
                .iter()
                .filter_map(|item| item.item_id.as_deref())
                .collect::<HashSet<_>>()
                .into_iter()
                .fold(0_u32, |total, item_id| {
                    total.saturating_add(
                        set_component_reservations
                            .get(item_id)
                            .copied()
                            .unwrap_or(0),
                    )
                });
            available.saturating_sub(reserved.saturating_add(reserved_by_sets))
                / component.required_quantity
        })
        .min()
        .unwrap_or(0);
    if intent.quantity > available_sets {
        return Err(format!(
            "Для продажи доступно полных комплектов: {available_sets}. Проверьте резерв копий и активные ордера на детали."
        ));
    }
    Ok(())
}

fn prime_set_for_listing(
    state: &AppState,
    item_id: &str,
) -> Result<Option<PrimeSetDefinition>, String> {
    let database = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?;
    let Some(catalog) = database
        .load_current_catalog()
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    let Some(set_slug) = catalog
        .items
        .iter()
        .find(|item| item.item_id == item_id)
        .map(|item| item.slug.as_str())
    else {
        return Ok(None);
    };
    Ok(database
        .load_current_game_metadata()
        .map_err(|error| error.to_string())?
        .and_then(|metadata| {
            metadata
                .prime_sets
                .into_iter()
                .find(|definition| definition.set_slug == set_slug)
        }))
}

fn active_set_component_reservations(
    state: &AppState,
    orders: &[AccountOrder],
    excluded_order_id: Option<&str>,
) -> Result<HashMap<String, u32>, String> {
    let database = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?;
    let Some(catalog) = database
        .load_current_catalog()
        .map_err(|error| error.to_string())?
    else {
        return Ok(HashMap::new());
    };
    let Some(metadata) = database
        .load_current_game_metadata()
        .map_err(|error| error.to_string())?
    else {
        return Ok(HashMap::new());
    };
    let slug_by_item_id = catalog
        .items
        .iter()
        .map(|item| (item.item_id.as_str(), item.slug.as_str()))
        .collect::<HashMap<_, _>>();
    let item_id_by_slug = catalog
        .items
        .iter()
        .map(|item| (item.slug.as_str(), item.item_id.as_str()))
        .collect::<HashMap<_, _>>();
    let set_by_slug = metadata
        .prime_sets
        .iter()
        .map(|definition| (definition.set_slug.as_str(), definition))
        .collect::<HashMap<_, _>>();
    let mut reservations = HashMap::<String, u32>::new();
    for order in orders.iter().filter(|order| {
        order.visible
            && order.order_type == AccountOrderType::Sell
            && excluded_order_id != Some(order.id.as_str())
            && order.rank.is_none()
            && order.charges.is_none()
            && order.subtype.is_none()
            && order.amber_stars.is_none()
            && order.cyan_stars.is_none()
    }) {
        let Some(set_slug) = order
            .item_id
            .as_deref()
            .and_then(|item_id| slug_by_item_id.get(item_id).copied())
        else {
            continue;
        };
        let Some(definition) = set_by_slug.get(set_slug).copied() else {
            continue;
        };
        for component in &definition.components {
            let Some(item_id) = item_id_by_slug.get(component.slug.as_str()).copied() else {
                continue;
            };
            let quantity = order.quantity.saturating_mul(component.required_quantity);
            reservations
                .entry(item_id.to_owned())
                .and_modify(|reserved| *reserved = reserved.saturating_add(quantity))
                .or_insert(quantity);
        }
    }
    Ok(reservations)
}

fn inventory_for_listing(state: &AppState) -> Result<InventoryView, String> {
    let settings = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .get_setting::<AppSettings>(SETTINGS_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    InventoryService::view(&state.database, &settings)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Сначала обновите инвентарь из Warframe.".to_owned())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri deserializes command values by ownership.
async fn account_create_listing(
    input: CreateListingInput,
    confirmed: bool,
    state: State<'_, AppState>,
) -> Result<AccountOrder, String> {
    input.validate().map_err(|error| error.to_string())?;
    if input.order_type == AccountOrderType::Sell {
        let inventory = inventory_for_listing(&state)?;
        let prime_set = prime_set_for_listing(&state, &input.item_id)?;
        let account = state
            .account_service
            .view()
            .await
            .map_err(|error| error.to_string())?;
        let set_component_reservations =
            active_set_component_reservations(&state, &account.orders, None)?;
        validate_sell_listing_inventory(
            &SellListingIntent::from_create(&input),
            &inventory,
            &account.orders,
            None,
            prime_set.as_ref(),
            &set_component_reservations,
        )?;
    }
    let order = state
        .account_service
        .create_listing(&input, confirmed)
        .await
        .map_err(|error| error.to_string())?;
    tracing::info!(
        event = "wfm_listing_created",
        "explicit WFM listing create completed"
    );
    Ok(order)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri deserializes command values by ownership.
async fn account_update_listing(
    id: String,
    input: UpdateListingInput,
    confirmed: bool,
    state: State<'_, AppState>,
) -> Result<AccountOrder, String> {
    input.validate().map_err(|error| error.to_string())?;
    let account = state
        .account_service
        .view()
        .await
        .map_err(|error| error.to_string())?;
    let current = account
        .orders
        .iter()
        .find(|order| order.id == id)
        .ok_or_else(|| "Ордер не найден; обновите список ордеров.".to_owned())?;
    let effective_quantity = input.quantity.unwrap_or(current.quantity);
    let effective_per_trade = input.per_trade.or(current.per_trade).unwrap_or(1);
    if !(1..=6).contains(&effective_per_trade)
        || !effective_quantity.is_multiple_of(effective_per_trade)
    {
        return Err(
            "После изменения количество должно делиться на размер одного торгового лота (1–6)."
                .into(),
        );
    }
    let final_visible = input.visible.unwrap_or(current.visible);
    if current.order_type == AccountOrderType::Sell && final_visible {
        let inventory = inventory_for_listing(&state)?;
        let intent = SellListingIntent::from_update(current, &input)?;
        let prime_set = prime_set_for_listing(&state, &intent.item_id)?;
        let set_component_reservations =
            active_set_component_reservations(&state, &account.orders, Some(&id))?;
        validate_sell_listing_inventory(
            &intent,
            &inventory,
            &account.orders,
            Some(&id),
            prime_set.as_ref(),
            &set_component_reservations,
        )?;
    }
    let order = state
        .account_service
        .update_listing(&id, &input, confirmed)
        .await
        .map_err(|error| error.to_string())?;
    tracing::info!(
        event = "wfm_listing_updated",
        "explicit WFM listing update completed"
    );
    Ok(order)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri deserializes command values by ownership.
async fn account_delete_listing(
    id: String,
    confirmed: bool,
    state: State<'_, AppState>,
) -> Result<AccountOrder, String> {
    let order = state
        .account_service
        .delete_listing(&id, confirmed)
        .await
        .map_err(|error| error.to_string())?;
    tracing::info!(
        event = "wfm_listing_deleted",
        "explicit WFM listing delete completed"
    );
    Ok(order)
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
fn trade_events(state: State<'_, AppState>) -> Result<Vec<TradeEvent>, String> {
    state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .recent_trade_events(30)
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
fn trade_sales_summary(state: State<'_, AppState>) -> Result<TradeSalesSummary, String> {
    state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .trade_sales_summary()
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri deserializes command values by ownership.
fn trade_event_reconciled(
    id: i64,
    order_id: Option<String>,
    reconciliation_json: Option<String>,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    if reconciliation_json
        .as_ref()
        .is_some_and(|value| value.len() > 16_384)
    {
        return Err("trade reconciliation payload is too long".to_owned());
    }
    state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .set_trade_event_status(
            id,
            TradeEventStatus::Reconciled,
            order_id.as_deref(),
            reconciliation_json.as_deref(),
        )
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
fn trade_event_ignore(id: i64, state: State<'_, AppState>) -> Result<bool, String> {
    state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .set_trade_event_status(id, TradeEventStatus::Ignored, None, None)
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
fn trade_event_restore(id: i64, state: State<'_, AppState>) -> Result<bool, String> {
    state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .set_trade_event_status(id, TradeEventStatus::Pending, None, None)
        .map_err(|error| error.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AutomaticTradeActionKind {
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AutomaticTradeAction {
    kind: AutomaticTradeActionKind,
    before: AccountOrder,
    item_name: String,
    sold_quantity: u32,
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri deserializes command values by ownership.
async fn trade_event_retry(id: i64, app_handle: AppHandle) -> Result<bool, String> {
    let event = app_handle
        .state::<AppState>()
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .recent_trade_events(100)
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|event| event.id == id)
        .ok_or_else(|| "Сделка не найдена в локальном журнале.".to_owned())?;
    reconcile_trade_event(&app_handle, event).await
}

fn is_confirmed_sale(event: &TradeEvent) -> bool {
    event.platinum_received > 0
        && event.platinum_given == 0
        && !event.given_items.is_empty()
        && event.received_items.is_empty()
}

fn plan_automatic_trade_reconciliation(
    event: &TradeEvent,
    account: &AccountView,
) -> Option<Vec<AutomaticTradeAction>> {
    if !is_confirmed_sale(event) {
        return None;
    }
    let sold_items = aggregate_trade_items(&event.given_items);
    if sold_items.is_empty() {
        return None;
    }
    let complete_set_candidates = account
        .orders
        .iter()
        .filter_map(|order| {
            if order.order_type != AccountOrderType::Sell {
                return None;
            }
            let item = order
                .item_id
                .as_ref()
                .and_then(|item_id| account.order_items.get(item_id))?;
            if item.set_components.is_empty() {
                return None;
            }
            let sold_quantity = complete_set_quantity(&sold_items, &item.set_components)?;
            Some((order, item, sold_quantity))
        })
        .collect::<Vec<_>>();
    if !complete_set_candidates.is_empty() {
        if complete_set_candidates.len() != 1 {
            return None;
        }
        let (order, item, sold_quantity) = complete_set_candidates[0];
        if sold_quantity > order.quantity || !is_safe_trade_order(order, sold_quantity, event) {
            return None;
        }
        return Some(vec![AutomaticTradeAction {
            kind: AutomaticTradeActionKind::Close,
            before: order.clone(),
            item_name: item.display_name.clone(),
            sold_quantity,
        }]);
    }

    let mut used_order_ids = HashSet::new();
    let mut actions = Vec::with_capacity(sold_items.len());
    for sold in sold_items {
        let candidates = account
            .orders
            .iter()
            .filter_map(|order| {
                if order.order_type != AccountOrderType::Sell {
                    return None;
                }
                let item = order
                    .item_id
                    .as_ref()
                    .and_then(|item_id| account.order_items.get(item_id))?;
                item_matches_trade_name(item, &sold.name).then_some((order, item))
            })
            .collect::<Vec<_>>();
        if candidates.len() != 1 {
            return None;
        }
        let (order, item) = candidates[0];
        if !used_order_ids.insert(order.id.clone())
            || sold.quantity > order.quantity
            || !is_safe_trade_order(order, sold.quantity, event)
        {
            return None;
        }
        actions.push(AutomaticTradeAction {
            kind: AutomaticTradeActionKind::Close,
            before: order.clone(),
            item_name: item.display_name.clone(),
            sold_quantity: sold.quantity,
        });
    }
    (!actions.is_empty()).then_some(actions)
}

fn aggregate_trade_items(items: &[TradeItem]) -> Vec<TradeItem> {
    let mut positions = HashMap::<String, usize>::new();
    let mut aggregated = Vec::<TradeItem>::new();
    for item in items.iter().filter(|item| item.quantity > 0) {
        let identity = normalize_trade_name(&item.name);
        if identity.is_empty() {
            continue;
        }
        if let Some(position) = positions.get(&identity).copied() {
            aggregated[position].quantity =
                aggregated[position].quantity.saturating_add(item.quantity);
        } else {
            positions.insert(identity, aggregated.len());
            aggregated.push(item.clone());
        }
    }
    aggregated
}

fn complete_set_quantity(
    sold_items: &[TradeItem],
    components: &[AccountSetComponentView],
) -> Option<u32> {
    if components.is_empty() || sold_items.len() != components.len() {
        return None;
    }
    let mut used_items = HashSet::new();
    let mut complete_sets = None;
    for component in components {
        if component.required_quantity == 0 {
            return None;
        }
        let aliases = [
            normalize_trade_name(&component.display_name),
            normalize_trade_name(&component.display_name_en),
        ];
        let matching = sold_items
            .iter()
            .enumerate()
            .filter(|(index, sold)| {
                !used_items.contains(index) && aliases.contains(&normalize_trade_name(&sold.name))
            })
            .collect::<Vec<_>>();
        if matching.len() != 1 {
            return None;
        }
        let (index, sold) = matching[0];
        if !sold.quantity.is_multiple_of(component.required_quantity) {
            return None;
        }
        let quantity = sold.quantity / component.required_quantity;
        if quantity == 0 || complete_sets.is_some_and(|current| current != quantity) {
            return None;
        }
        complete_sets = Some(quantity);
        used_items.insert(index);
    }
    (used_items.len() == sold_items.len())
        .then_some(complete_sets)
        .flatten()
}

fn item_matches_trade_name(item: &AccountOrderItemView, trade_name: &str) -> bool {
    let normalized = normalize_trade_name(trade_name);
    [&item.display_name, &item.display_name_en]
        .into_iter()
        .any(|candidate| normalize_trade_name(candidate) == normalized)
}

fn is_safe_trade_order(order: &AccountOrder, sold_quantity: u32, event: &TradeEvent) -> bool {
    order.rank.is_none()
        && order.charges.is_none()
        && order.subtype.is_none()
        && order.amber_stars.is_none()
        && order.cyan_stars.is_none()
        && order.updated_at <= event.occurred_at
        && order.per_trade.is_none_or(|per_trade| {
            per_trade > 0
                && sold_quantity.is_multiple_of(per_trade)
                && (order.quantity <= sold_quantity
                    || (order.quantity - sold_quantity).is_multiple_of(per_trade))
        })
}

fn normalize_trade_name(value: &str) -> String {
    let mut value = value
        .trim()
        .to_lowercase()
        .replace('ё', "е")
        .replace(['’', '\'', 'ʼ'], "");
    for prefix in ["чертеж:", "blueprint:"] {
        if let Some(stripped) = value.strip_prefix(prefix) {
            value = stripped.trim().to_owned();
            break;
        }
    }
    for suffix in ["(чертеж)", "(blueprint)"] {
        if let Some(stripped) = value.strip_suffix(suffix) {
            value = stripped.trim().to_owned();
            break;
        }
    }
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(" :", ":")
        .replace(": ", ":")
}

async fn reconcile_trade_event(app_handle: &AppHandle, event: TradeEvent) -> Result<bool, String> {
    let state = app_handle.state::<AppState>();
    let _reconciliation_guard = state.trade_reconciliation_lock.lock().await;
    let still_pending = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .recent_trade_events(100)
        .map_err(|error| error.to_string())?
        .into_iter()
        .any(|stored| stored.id == event.id && stored.status == TradeEventStatus::Pending);
    if !still_pending {
        return Ok(false);
    }
    let account = state
        .account_service
        .view()
        .await
        .map_err(|error| error.to_string())?;
    if !account.connected {
        return Ok(false);
    }
    let language = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .get_setting::<AppSettings>(SETTINGS_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default()
        .language;
    let account = enrich_account_view(&state.database, language, account)
        .map_err(|error| error.to_string())?;
    let Some(actions) = plan_automatic_trade_reconciliation(&event, &account) else {
        return Ok(false);
    };
    let planned_count = actions.len();
    let mut completed = Vec::with_capacity(planned_count);
    let mut failure = None;
    for action in actions {
        match state
            .account_service
            .close_listing(&action.before.id, action.sold_quantity)
            .await
        {
            Ok(()) => completed.push(action),
            Err(error) => {
                failure = Some(error.to_string());
                break;
            }
        }
    }
    if completed.is_empty() {
        return failure.map_or(Ok(false), Err);
    }
    let reconciliation_json =
        serde_json::to_string(&completed).map_err(|error| error.to_string())?;
    let matched_order_id = (completed.len() == 1).then(|| completed[0].before.id.as_str());
    state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .set_trade_event_status(
            event.id,
            TradeEventStatus::Reconciled,
            matched_order_id,
            Some(&reconciliation_json),
        )
        .map_err(|error| error.to_string())?;
    tracing::info!(
        event = "wfm_trade_auto_closed",
        trade_event_id = event.id,
        completed = completed.len(),
        planned = planned_count,
        partial = failure.is_some(),
        "confirmed game sale was automatically recorded through WFM order close"
    );
    if let Some(error) = failure {
        tracing::warn!(
            event = "wfm_trade_auto_close_partial",
            trade_event_id = event.id,
            error = %error,
            "only a safe completed subset was recorded; automatic retry is disabled"
        );
    }
    let _ = app_handle.emit("trade-reconciled", event.id);
    Ok(completed.len() == planned_count)
}

fn spawn_trade_reconciliation(app_handle: AppHandle, event: TradeEvent) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = reconcile_trade_event(&app_handle, event).await {
            tracing::warn!(
                event = "wfm_trade_auto_close_failed",
                error = %error,
                "confirmed game sale remains pending for automatic retry"
            );
            let _ = app_handle.emit("trade-reconciliation-failed", ());
        }
    });
}

fn spawn_pending_trade_reconciliation(app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let events = app_handle
            .state::<AppState>()
            .database
            .lock()
            .ok()
            .and_then(|database| database.recent_trade_events(100).ok())
            .unwrap_or_default();
        for event in events
            .into_iter()
            .filter(|event| event.status == TradeEventStatus::Pending && is_confirmed_sale(event))
        {
            if let Err(error) = reconcile_trade_event(&app_handle, event).await {
                tracing::warn!(
                    event = "wfm_pending_trade_retry_failed",
                    error = %error,
                    "pending confirmed sale could not be synchronized"
                );
            }
        }
    });
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri deserializes command values by ownership.
async fn live_price_current_variant(
    key: MarketVariantKey,
    item_kind: MarketItemKind,
    state: State<'_, AppState>,
) -> Result<Option<LivePricingResult>, String> {
    let settings = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .get_setting::<AppSettings>(SETTINGS_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    state
        .live_pricing_service
        .price_current_variant(&state.database, &key, item_kind, &settings, None)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri deserializes command values by ownership.
fn price_current_variant(
    key: MarketVariantKey,
    item_kind: MarketItemKind,
    state: State<'_, AppState>,
) -> Result<Option<PriceRecommendation>, String> {
    PricingService::price_current_variant(&state.database, &key, item_kind)
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri deserializes command values by ownership.
fn search_market(
    query: String,
    limit: Option<u32>,
    state: State<'_, AppState>,
) -> Result<MarketSearchResult, String> {
    let settings = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .get_setting::<AppSettings>(SETTINGS_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    MarketBrowserService::search(
        &state.database,
        &query,
        limit
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(DEFAULT_MARKET_SEARCH_LIMIT),
        settings.language,
        settings.platform,
    )
    .map(localize_market_search_images)
    .map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State and AppHandle.
async fn scan_relic_rewards(
    image_path: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RelicRewardScanView, String> {
    let total_started = Instant::now();
    if state
        .reward_scan_in_flight
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        tracing::info!(
            event = "reward_scan_skipped",
            reason = "already_in_flight",
            "duplicate reward OCR request skipped"
        );
        return Err("Распознавание наград уже выполняется.".to_owned());
    }
    let _scan_guard = RewardScanGuard(&state.reward_scan_in_flight);
    let executable = find_reward_ocr_executable(&app)?;
    let tessdata_path = executable
        .parent()
        .map(|parent| parent.join("tessdata"))
        .filter(|path| path.join("rus.traineddata").is_file())
        .map(|path| path.display().to_string());
    let inputs_started = Instant::now();
    let (settings, request) = reward_scan_inputs(&state, image_path, tessdata_path)?;
    let inputs_ms = elapsed_millis(inputs_started);
    let ocr_started = Instant::now();
    let response =
        tauri::async_runtime::spawn_blocking(move || run_reward_ocr_process(&executable, &request))
            .await
            .map_err(|error| format!("OCR process join failed: {error}"))??;
    let ocr_ms = elapsed_millis(ocr_started);
    let slots = response.rewards.len();
    let recognized = response
        .rewards
        .iter()
        .filter(|reward| reward.item_id.is_some())
        .count();
    let enrichment_started = Instant::now();
    let mut view = build_reward_scan_view(&state, &settings, response)?;
    let enrichment_ms = elapsed_millis(enrichment_started);
    view.scan_duration_ms = elapsed_millis(total_started);
    tracing::info!(
        event = "reward_scan_completed",
        status = %view.status,
        recognized,
        slots,
        inputs_ms,
        ocr_ms,
        enrichment_ms,
        total_ms = view.scan_duration_ms,
        "reward OCR and local enrichment completed"
    );
    publish_reward_scan(&app, &state, &settings, &view)?;
    Ok(view)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri deserializes settings and injects handles by ownership.
fn preview_reward_overlay(
    settings: AppSettings,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RelicRewardScanView, String> {
    validate_app_settings(&settings).map_err(str::to_owned)?;
    let rect = warframe_window_rect(&app)?;
    let insights = InsightsService::view(&state.reward_database, &settings)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Сначала обновите данные предметов в настройках.".to_owned())?;
    let response = build_reward_preview_response(&insights, &rect)?;
    let view = build_reward_scan_view(&state, &settings, response)?;
    present_reward_scan(&app, &state, &settings, &view)?;
    Ok(view)
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
fn latest_relic_rewards(state: State<'_, AppState>) -> Result<Option<RelicRewardScanView>, String> {
    state
        .latest_reward_scan
        .lock()
        .map(|view| view.clone())
        .map_err(|_| "reward overlay state is unavailable".to_owned())
}

fn publish_reward_scan(
    app: &AppHandle,
    state: &AppState,
    settings: &AppSettings,
    view: &RelicRewardScanView,
) -> Result<(), String> {
    *state
        .latest_reward_scan
        .lock()
        .map_err(|_| "reward overlay state is unavailable".to_owned())? = Some(view.clone());
    present_reward_scan(app, state, settings, view)
}

fn present_reward_scan(
    app: &AppHandle,
    state: &AppState,
    settings: &AppSettings,
    view: &RelicRewardScanView,
) -> Result<(), String> {
    app.emit("relic-rewards-updated", view)
        .map_err(|error| error.to_string())?;
    if view.status != "ok" || view.recognized_count < 2 {
        hide_reward_overlay(app);
        return Ok(());
    }

    let max_set_parts = view
        .rewards
        .iter()
        .filter_map(|reward| reward.set.as_ref())
        .map(|set| set.parts.len())
        .max()
        .unwrap_or(0);
    if let Err(error) = show_reward_overlay(app, settings, view.rewards.len(), max_set_parts) {
        tracing::warn!(
            event = "reward_overlay_show_failed",
            error = %error,
            "reward scan succeeded but overlay could not be shown"
        );
        return Ok(());
    }
    let generation = state
        .reward_overlay_generation
        .fetch_add(1, Ordering::AcqRel)
        .wrapping_add(1);
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(REWARD_OVERLAY_VISIBLE_FOR).await;
        let state = app_handle.state::<AppState>();
        if state.reward_overlay_generation.load(Ordering::Acquire) == generation {
            hide_reward_overlay(&app_handle);
        }
    });
    Ok(())
}

fn reward_scan_inputs(
    state: &AppState,
    image_path: Option<String>,
    tessdata_path: Option<String>,
) -> Result<(AppSettings, RewardOcrRequest), String> {
    let is_live_capture = image_path.as_deref().is_none_or(str::is_empty);
    let active_relic_paths = state
        .reward_relic_paths
        .lock()
        .map_err(|_| "reward relic pool is unavailable".to_owned())?
        .clone();
    let (settings, catalog) = {
        let database = state
            .reward_database
            .lock()
            .map_err(|_| "database state is unavailable".to_owned())?;
        let settings = database
            .get_setting::<AppSettings>(SETTINGS_KEY)
            .map_err(|error| error.to_string())?
            .unwrap_or_default();
        let catalog = build_reward_ocr_catalog(&database, &active_relic_paths)?;
        (settings, catalog)
    };
    Ok((
        settings,
        RewardOcrRequest {
            catalog,
            image_path,
            tessdata_path,
            ui_scale: None,
            max_attempts: if is_live_capture { 6 } else { 1 },
            retry_interval_ms: if is_live_capture { 250 } else { 0 },
            initial_delay_ms: if is_live_capture { 300 } else { 0 },
        },
    ))
}

fn build_reward_ocr_catalog(
    database: &Database,
    active_relic_paths: &HashSet<String>,
) -> Result<Vec<RewardOcrCatalogItem>, String> {
    let market_catalog = database
        .load_current_catalog()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Сначала обновите данные рынка в настройках.".to_owned())?;
    let metadata = database
        .load_current_game_metadata()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Сначала обновите данные предметов в настройках.".to_owned())?;
    let active_reward_slugs: HashSet<_> = metadata
        .relics
        .iter()
        .filter(|relic| active_relic_paths.contains(&relic.relic_game_ref))
        .flat_map(|relic| {
            relic
                .rewards
                .iter()
                .filter_map(|reward| reward.reward_slug.clone())
        })
        .collect();
    let prime_slugs: HashSet<_> = metadata
        .prime_parts
        .iter()
        .map(|part| part.slug.as_str())
        .collect();
    let mut result = Vec::new();
    for item in market_catalog
        .items
        .into_iter()
        .filter(|item| prime_slugs.contains(item.slug.as_str()))
    {
        if let Some(name) = russian_reward_ocr_name(item.display_name_ru) {
            result.push(RewardOcrCatalogItem {
                item_id: item.item_id,
                slug: item.slug,
                name,
            });
        }
    }
    // Relics seen in the log are a useful hint, not a complete party roster: players can join
    // after the first projections were loaded. Keep every prime reward available to OCR and only
    // move likely rewards to the front so late joiners' choices cannot become unrecognizable.
    result.sort_by_key(|item| !active_reward_slugs.contains(&item.slug));
    if result.is_empty() {
        return Err(
            "В данных рынка нет русских названий наград. Обновите данные рынка в настройках."
                .to_owned(),
        );
    }
    result.push(RewardOcrCatalogItem {
        item_id: "non_market_forma_blueprint".into(),
        slug: "forma_blueprint".into(),
        name: "Чертёж: Форма".into(),
    });
    result.push(RewardOcrCatalogItem {
        item_id: "non_market_forma_blueprint".into(),
        slug: "forma_blueprint".into(),
        name: "X2 Чертёж: Форма".into(),
    });
    Ok(result)
}

fn russian_reward_ocr_name(name: Option<String>) -> Option<String> {
    name.map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
}

fn build_reward_watcher_request(database: &Database) -> Result<RewardWatcherRequest, String> {
    let catalog = build_reward_ocr_catalog(database, &HashSet::new())?;
    let metadata = database
        .load_current_game_metadata()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Сначала обновите данные предметов в настройках.".to_owned())?;
    let relics = metadata
        .relics
        .into_iter()
        .map(|relic| RewardWatcherRelic {
            relic_game_ref: relic.relic_game_ref,
            reward_slugs: relic
                .rewards
                .into_iter()
                .filter_map(|reward| reward.reward_slug)
                .collect(),
        })
        .collect();
    Ok(RewardWatcherRequest {
        catalog,
        relics,
        ui_scale: None,
    })
}

fn build_reward_preview_response(
    insights: &InsightsView,
    rect: &WarframeWindowRect,
) -> Result<RewardOcrResponse, String> {
    let mut sets = insights
        .sets
        .iter()
        .filter(|set| {
            set.components
                .iter()
                .any(|component| component.owned_quantity > 0)
        })
        .collect::<Vec<_>>();
    sets.sort_by_key(|set| {
        let ready = set
            .components
            .iter()
            .filter(|component| component.owned_quantity >= component.definition.required_quantity)
            .count();
        let owned = set.components.iter().fold(0_u32, |total, component| {
            total.saturating_add(component.owned_quantity)
        });
        (ready < set.components.len(), ready, owned)
    });
    sets.reverse();

    let mut selected_slugs = HashSet::new();
    let mut selected = Vec::with_capacity(4);
    for set in &sets {
        let component = set
            .components
            .iter()
            .filter(|component| component.item_id.is_some())
            .find(|component| component.owned_quantity < component.definition.required_quantity)
            .or_else(|| {
                set.components
                    .iter()
                    .filter(|component| component.item_id.is_some())
                    .max_by_key(|component| component.owned_quantity)
            });
        if let Some(component) = component
            && selected_slugs.insert(component.definition.slug.clone())
        {
            selected.push(component);
        }
        if selected.len() == 4 {
            break;
        }
    }
    if selected.len() < 4 {
        for component in sets.iter().flat_map(|set| &set.components) {
            if component.item_id.is_some()
                && selected_slugs.insert(component.definition.slug.clone())
            {
                selected.push(component);
            }
            if selected.len() == 4 {
                break;
            }
        }
    }
    if selected.len() < 2 {
        return Err(
            "В инвентаре не найдено хотя бы двух распознанных частей прайм-сетов.".to_owned(),
        );
    }

    let rewards = selected
        .into_iter()
        .enumerate()
        .map(|(slot, component)| reward_preview_match(slot, component))
        .collect();
    Ok(RewardOcrResponse {
        status: "ok".to_owned(),
        message: Some("Тестовые награды из прайм-сетов вашего инвентаря.".to_owned()),
        capture_width: Some(rect.width),
        capture_height: Some(rect.height),
        theme: Some("inventory_preview".to_owned()),
        rewards,
    })
}

fn reward_preview_match(slot: usize, component: &SetComponentInsight) -> RewardOcrMatch {
    RewardOcrMatch {
        slot: u8::try_from(slot).unwrap_or(u8::MAX),
        raw_text: component.display_name.clone(),
        item_id: component.item_id.clone(),
        slug: Some(component.definition.slug.clone()),
        name: Some(component.display_name.clone()),
        confidence: 1.0,
    }
}

fn build_reward_scan_view(
    state: &AppState,
    settings: &AppSettings,
    response: RewardOcrResponse,
) -> Result<RelicRewardScanView, String> {
    let (metadata, inventory, catalog) = reward_scan_local_data(state, settings)?;
    let overlay_scale = reward_scan_overlay_scale(&response, settings);
    let slot_count = response.rewards.len();
    let recognized_count = response
        .rewards
        .iter()
        .filter(|reward| reward.item_id.is_some())
        .count();
    let mut rewards = Vec::with_capacity(response.rewards.len());
    for reward in response.rewards {
        rewards.push(build_reward_choice(
            state,
            settings,
            metadata.as_ref(),
            inventory.as_ref(),
            &catalog,
            reward,
        )?);
    }

    mark_recommended_reward(&mut rewards);

    Ok(RelicRewardScanView {
        status: response.status,
        message: if recognized_count < slot_count {
            Some(format!(
                "Распознано {recognized_count} из {slot_count} наград."
            ))
        } else {
            response.message
        },
        recognized_count,
        scan_duration_ms: 0,
        capture_width: response.capture_width,
        capture_height: response.capture_height,
        overlay_scale,
        theme: response.theme,
        rewards,
    })
}

fn build_reward_choice(
    state: &AppState,
    settings: &AppSettings,
    metadata: Option<&GameMetadataSnapshot>,
    inventory: Option<&InventoryView>,
    catalog: &RewardCatalogDetails,
    reward: RewardOcrMatch,
) -> Result<RelicRewardChoice, String> {
    // OCR уже возвращает канонические id и slug из каталога. Цена по точному ключу не теряет
    // награду из-за того, что нечёткий текстовый поиск обрезал выдачу на двенадцатой строке.
    let mut market = reward_market_row(
        &state.reward_database,
        settings,
        catalog,
        reward.item_id.as_deref(),
        reward.slug.as_deref(),
        reward.name.as_deref(),
    )?;
    override_reward_market_image(&mut market, metadata, reward.slug.as_deref());
    let completes_set = reward
        .slug
        .as_deref()
        .map(|slug| {
            reward_set_completion(
                &state.reward_database,
                settings,
                metadata,
                inventory,
                catalog,
                slug,
            )
        })
        .transpose()?
        .flatten();
    let ducats = reward
        .slug
        .as_deref()
        .and_then(|slug| reward_ducats(metadata, slug));
    let owned_quantity = reward
        .slug
        .as_deref()
        .and_then(|slug| reward_owned_quantity(inventory, slug));
    let set = reward
        .slug
        .as_deref()
        .map(|slug| {
            reward_set_overview(
                &state.reward_database,
                settings,
                metadata,
                inventory,
                catalog,
                slug,
            )
        })
        .transpose()?
        .flatten();
    let part_value = market.as_ref().and_then(credible_reward_market_value);
    let choice_value = [
        part_value,
        completes_set
            .as_ref()
            .and_then(|completion| completion.incremental_value),
    ]
    .into_iter()
    .flatten()
    .reduce(f64::max);
    let display_name = reward_display_name(market.as_ref(), reward.name);
    Ok(RelicRewardChoice {
        slot: reward.slot,
        raw_text: reward.raw_text,
        confidence: reward.confidence,
        item_id: reward.item_id,
        slug: reward.slug,
        display_name,
        market,
        ducats,
        owned_quantity,
        set,
        completes_set,
        choice_value,
        recommended: false,
    })
}

fn reward_market_row(
    database: &Mutex<Database>,
    settings: &AppSettings,
    catalog: &RewardCatalogDetails,
    item_id: Option<&str>,
    slug: Option<&str>,
    fallback_name: Option<&str>,
) -> Result<Option<MarketSearchRow>, String> {
    let (Some(item_id), Some(slug)) = (item_id, slug) else {
        return Ok(None);
    };
    let key = MarketVariantKey::new(slug.to_owned(), settings.platform, None, None::<String>)
        .map_err(|error| error.to_string())?;
    let recommendation =
        PricingService::price_current_variant(database, &key, MarketItemKind::Standard)
            .map_err(|error| error.to_string())?;
    Ok(recommendation.map(|recommendation| {
        let details = catalog.get(slug);
        MarketSearchRow {
            item_id: item_id.to_owned(),
            display_name: fallback_name
                .map(str::to_owned)
                .or_else(|| details.map(|(name, _)| name.clone()))
                .unwrap_or_else(|| slug.to_owned()),
            image_url: details.and_then(|(_, image_url)| image_url.clone()),
            item_kind: MarketItemKind::Standard,
            mastery_requirement: None,
            recommendation,
        }
    }))
}

fn credible_reward_market_value(row: &MarketSearchRow) -> Option<f64> {
    credible_reward_value(
        row.recommendation.confidence,
        row.recommendation.fair_price,
        row.recommendation.list_price,
        row.recommendation.quick_sell,
    )
}

fn credible_reward_value(
    confidence: PriceConfidence,
    fair_price: Option<f64>,
    list_price: Option<f64>,
    quick_sell: Option<f64>,
) -> Option<f64> {
    if !matches!(confidence, PriceConfidence::High | PriceConfidence::Medium) {
        return None;
    }
    fair_price
        .or(list_price)
        .or(quick_sell)
        .filter(|value| value.is_finite() && *value > 0.0)
}

fn mark_recommended_reward(rewards: &mut [RelicRewardChoice]) {
    for reward in rewards.iter_mut() {
        reward.recommended = false;
    }
    let best = rewards
        .iter()
        .enumerate()
        .filter(|(_, reward)| {
            reward.item_id.is_some()
                && reward.confidence.is_finite()
                && reward.confidence >= MIN_REWARD_RECOMMENDATION_OCR_CONFIDENCE
        })
        .max_by(|(_, left), (_, right)| {
            left.choice_value
                .unwrap_or(f64::NEG_INFINITY)
                .total_cmp(&right.choice_value.unwrap_or(f64::NEG_INFINITY))
                .then_with(|| left.ducats.unwrap_or(0).cmp(&right.ducats.unwrap_or(0)))
                .then_with(|| left.confidence.total_cmp(&right.confidence))
                .then_with(|| right.slot.cmp(&left.slot))
        })
        .map(|(index, _)| index);
    if let Some(index) = best {
        rewards[index].recommended = true;
    }
}

fn reward_display_name(
    market: Option<&MarketSearchRow>,
    fallback: Option<String>,
) -> Option<String> {
    market.map(|row| row.display_name.clone()).or(fallback)
}

fn reward_scan_overlay_scale(response: &RewardOcrResponse, settings: &AppSettings) -> f64 {
    reward_overlay_scale_factor(
        response.capture_width.unwrap_or(1920),
        response.capture_height.unwrap_or(1080),
        response.rewards.len(),
        settings,
    )
}

fn reward_scan_local_data(
    state: &AppState,
    settings: &AppSettings,
) -> Result<
    (
        Option<GameMetadataSnapshot>,
        Option<InventoryView>,
        RewardCatalogDetails,
    ),
    String,
> {
    let (metadata, catalog) = state
        .reward_database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())
        .and_then(|database| {
            let metadata = database
                .load_current_game_metadata()
                .map_err(|error| error.to_string())?;
            let catalog = database
                .load_current_catalog()
                .map_err(|error| error.to_string())?;
            Ok((metadata, catalog))
        })?;
    let catalog = catalog
        .map(|catalog| {
            catalog
                .items
                .into_iter()
                .map(|item| {
                    let display_name = russian_reward_ocr_name(item.display_name_ru)
                        .unwrap_or_else(|| "Неизвестная награда".to_owned());
                    let thumb = item.thumb_ru.or(item.thumb);
                    let image_url = thumb.as_deref().map(reward_market_image_url);
                    (item.slug, (display_name, image_url))
                })
                .collect()
        })
        .unwrap_or_default();
    let inventory = InventoryService::view(&state.reward_database, settings)
        .map_err(|error| error.to_string())?;
    Ok((metadata, inventory, catalog))
}

fn reward_ducats(metadata: Option<&GameMetadataSnapshot>, reward_slug: &str) -> Option<u32> {
    metadata?
        .prime_parts
        .iter()
        .find(|part| part.slug == reward_slug)
        .map(|part| part.ducats)
}

fn reward_component_image(
    metadata: Option<&GameMetadataSnapshot>,
    reward_slug: &str,
) -> Option<String> {
    let remote_url = metadata?
        .prime_sets
        .iter()
        .flat_map(|set| &set.components)
        .find(|component| component.slug == reward_slug)
        .and_then(|component| component.image_url.as_deref())?;
    component_image_protocol_url(remote_url)
}

fn override_reward_market_image(
    market: &mut Option<MarketSearchRow>,
    metadata: Option<&GameMetadataSnapshot>,
    reward_slug: Option<&str>,
) {
    if let Some(image_url) = reward_slug.and_then(|slug| reward_component_image(metadata, slug))
        && let Some(market) = market
    {
        market.image_url = Some(image_url);
    }
}

fn reward_owned_quantity(inventory: Option<&InventoryView>, reward_slug: &str) -> Option<u32> {
    inventory.map(|inventory| {
        inventory
            .items
            .iter()
            .filter(|item| item.key.as_ref().is_some_and(|key| key.slug == reward_slug))
            .fold(0_u32, |total, item| {
                total.saturating_add(item.owned_quantity)
            })
    })
}

fn reward_set_overview(
    database: &Mutex<Database>,
    settings: &AppSettings,
    metadata: Option<&GameMetadataSnapshot>,
    inventory: Option<&InventoryView>,
    catalog: &RewardCatalogDetails,
    reward_slug: &str,
) -> Result<Option<RewardSetOverview>, String> {
    let Some(metadata) = metadata else {
        return Ok(None);
    };
    let mut best: Option<RewardSetOverview> = None;
    for definition in metadata.prime_sets.iter().filter(|set| {
        set.components
            .iter()
            .any(|component| component.slug == reward_slug)
    }) {
        let set_price = reward_set_price(database, settings, definition)?;
        let mut parts = Vec::with_capacity(definition.components.len());
        let completed_sets = inventory.map(|inventory| {
            definition
                .components
                .iter()
                .filter(|component| component.required_quantity > 0)
                .map(|component| {
                    reward_owned_quantity(Some(inventory), &component.slug).unwrap_or(0)
                        / component.required_quantity
                })
                .min()
                .unwrap_or(0)
        });
        let mut ready_components = inventory.map(|_| 0_u32);
        for component in &definition.components {
            let total_owned = reward_owned_quantity(inventory, &component.slug).unwrap_or(0);
            let owned_quantity = completed_sets.map_or(total_owned, |completed| {
                next_set_owned_quantity(total_owned, component.required_quantity, completed)
            });
            if let Some(ready) = &mut ready_components
                && owned_quantity >= component.required_quantity
            {
                *ready = ready.saturating_add(1);
            }
            parts.push(RewardSetPart {
                name: reward_set_component_name(&component.slug, definition, catalog),
                image_url: component
                    .image_url
                    .as_deref()
                    .and_then(component_image_protocol_url)
                    .or_else(|| {
                        catalog
                            .get(&component.slug)
                            .and_then(|(_, image_url)| image_url.clone())
                    }),
                owned_quantity,
                required_quantity: component.required_quantity,
                is_reward: component.slug == reward_slug,
            });
        }
        let total_components = u32::try_from(definition.components.len()).unwrap_or(u32::MAX);
        let candidate = RewardSetOverview {
            set_name: catalog
                .get(&definition.set_slug)
                .map_or_else(|| "Прайм-комплект".to_owned(), |(name, _)| name.clone()),
            set_price,
            completed_sets,
            target_set_number: completed_sets.map(|count| count.saturating_add(1)),
            ready_components,
            total_components,
            parts,
        };
        let candidate_score = (
            candidate.ready_components.unwrap_or(0),
            candidate.set_price.unwrap_or(0.0),
        );
        let best_score = best.as_ref().map_or((0, 0.0), |current| {
            (
                current.ready_components.unwrap_or(0),
                current.set_price.unwrap_or(0.0),
            )
        });
        if best.is_none() || candidate_score > best_score {
            best = Some(candidate);
        }
    }
    Ok(best)
}

fn next_set_owned_quantity(total_owned: u32, required: u32, completed_sets: u32) -> u32 {
    total_owned.saturating_sub(completed_sets.saturating_mul(required))
}

fn reward_set_price(
    database: &Mutex<Database>,
    settings: &AppSettings,
    definition: &PrimeSetDefinition,
) -> Result<Option<f64>, String> {
    let key = MarketVariantKey::new(
        definition.set_slug.clone(),
        settings.platform,
        None,
        None::<String>,
    )
    .map_err(|error| error.to_string())?;
    PricingService::price_current_variant(database, &key, MarketItemKind::Standard)
        .map(|recommendation| {
            recommendation.and_then(|price| {
                matches!(
                    price.confidence,
                    PriceConfidence::High | PriceConfidence::Medium
                )
                .then(|| price.fair_price.or(price.list_price).or(price.quick_sell))
                .flatten()
                .filter(|value| value.is_finite() && *value > 0.0)
            })
        })
        .map_err(|error| error.to_string())
}

fn reward_set_component_name(
    component_slug: &str,
    definition: &PrimeSetDefinition,
    catalog: &RewardCatalogDetails,
) -> String {
    if let Some((catalog_name, _)) = catalog.get(component_slug) {
        if let Some((_, short_name)) = catalog_name.split_once(": ")
            && !short_name.trim().is_empty()
        {
            return compact_russian_component_name(short_name);
        }
        let set_name = catalog
            .get(&definition.set_slug)
            .map(|(name, _)| name.as_str())
            .and_then(|name| name.split_once(": ").map(|(base_name, _)| base_name))
            .unwrap_or_default();
        let short_name = catalog_name
            .strip_prefix(set_name)
            .map(str::trim)
            .filter(|name| !name.is_empty());
        return compact_russian_component_name(short_name.unwrap_or(catalog_name));
    }
    "Часть комплекта".to_owned()
}

fn compact_russian_component_name(name: &str) -> String {
    let name = name.trim();
    if matches!(name, "(Чертеж)" | "(Чертёж)" | "Чертеж" | "Чертёж") {
        return "Чертёж".to_owned();
    }
    name.strip_suffix(" (Чертеж)")
        .or_else(|| name.strip_suffix(" (Чертёж)"))
        .unwrap_or(name)
        .trim()
        .to_owned()
}

fn reward_market_image_url(thumb: &str) -> String {
    if thumb.starts_with("https://") || thumb.starts_with("http://") {
        thumb.to_owned()
    } else {
        format!("https://warframe.market/static/assets/{thumb}")
    }
}

fn component_image_file_name(remote_url: &str) -> Option<&str> {
    let file_name = remote_url.strip_prefix(WFCD_IMAGE_BASE_URL)?;
    let valid = !file_name.is_empty()
        && file_name.len() <= 128
        && Path::new(file_name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
        && file_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    valid.then_some(file_name)
}

fn component_image_protocol_url(remote_url: &str) -> Option<String> {
    let file_name = component_image_file_name(remote_url)?;
    Some(format!(
        "http://{COMPONENT_IMAGE_PROTOCOL}.localhost/{file_name}"
    ))
}

fn localize_component_image_url(image_url: &mut Option<String>) {
    if let Some(local_url) = image_url.as_deref().and_then(component_image_protocol_url) {
        *image_url = Some(local_url);
    }
}

fn localize_inventory_images(mut view: InventoryView) -> InventoryView {
    for item in &mut view.items {
        localize_component_image_url(&mut item.image_url);
    }
    view
}

fn localize_sell_now_images(mut view: SellNowView) -> SellNowView {
    for row in &mut view.rows {
        localize_component_image_url(&mut row.inventory.image_url);
    }
    view
}

fn localize_live_sell_now_images(mut view: LiveSellNowResult) -> LiveSellNowResult {
    localize_component_image_url(&mut view.row.inventory.image_url);
    view
}

fn localize_market_search_images(mut view: MarketSearchResult) -> MarketSearchResult {
    for row in &mut view.rows {
        localize_component_image_url(&mut row.image_url);
    }
    view
}

fn localize_account_images(mut view: AccountView) -> AccountView {
    for item in view.order_items.values_mut() {
        localize_component_image_url(&mut item.image_url);
    }
    view
}

fn localize_insight_images(mut view: InsightsView) -> InsightsView {
    for set in &mut view.sets {
        localize_component_image_url(&mut set.image_url);
        for component in &mut set.components {
            localize_component_image_url(&mut component.image_url);
            localize_component_image_url(&mut component.definition.image_url);
        }
        for component in &mut set.definition.components {
            localize_component_image_url(&mut component.image_url);
        }
    }
    for relic in &mut view.relics {
        localize_component_image_url(&mut relic.image_url);
        for reward in &mut relic.rewards {
            localize_component_image_url(&mut reward.image_url);
        }
    }
    for part in &mut view.ducats {
        localize_component_image_url(&mut part.image_url);
    }
    view
}

fn localize_resource_converter_images(mut view: ResourceConverterView) -> ResourceConverterView {
    for route in &mut view.routes {
        for action in &mut route.actions {
            localize_component_image_url(&mut action.image_url);
        }
    }
    for decision in view
        .arcanes
        .sell
        .iter_mut()
        .chain(view.arcanes.dissolve.iter_mut())
        .chain(view.arcanes.hold.iter_mut())
    {
        localize_component_image_url(&mut decision.image_url);
    }
    view
}

fn component_image_response(
    cache_directory: &Path,
    request_path: &str,
) -> tauri::http::Response<Vec<u8>> {
    let file_name = request_path.trim_start_matches('/');
    if component_image_file_name(&format!("{WFCD_IMAGE_BASE_URL}{file_name}")) != Some(file_name) {
        return component_image_http_response(
            tauri::http::StatusCode::BAD_REQUEST,
            b"invalid component image".to_vec(),
            "text/plain; charset=utf-8",
        );
    }
    match load_component_image(cache_directory, file_name) {
        Ok(image) => component_image_http_response(tauri::http::StatusCode::OK, image, "image/png"),
        Err(error) => {
            tracing::warn!(
                event = "component_image_load_failed",
                file_name,
                error = %error,
                "component image could not be loaded"
            );
            component_image_http_response(
                tauri::http::StatusCode::BAD_GATEWAY,
                b"component image unavailable".to_vec(),
                "text/plain; charset=utf-8",
            )
        }
    }
}

fn component_image_http_response(
    status: tauri::http::StatusCode,
    body: Vec<u8>,
    content_type: &'static str,
) -> tauri::http::Response<Vec<u8>> {
    let mut response = tauri::http::Response::new(body);
    *response.status_mut() = status;
    response.headers_mut().insert(
        tauri::http::header::CONTENT_TYPE,
        tauri::http::HeaderValue::from_static(content_type),
    );
    response.headers_mut().insert(
        tauri::http::header::CACHE_CONTROL,
        tauri::http::HeaderValue::from_static("public, max-age=604800, immutable"),
    );
    response
}

#[allow(clippy::needless_pass_by_value)] // Tauri protocol handlers own context and request values.
fn serve_component_image_protocol(
    context: tauri::UriSchemeContext<'_, tauri::Wry>,
    request: tauri::http::Request<Vec<u8>>,
    responder: tauri::UriSchemeResponder,
) {
    let cache_directory = context
        .app_handle()
        .path()
        .local_data_dir()
        .map(|directory| {
            directory
                .join("PlatScope")
                .join(COMPONENT_IMAGE_CACHE_DIRECTORY)
        });
    let request_path = request.uri().path().to_owned();
    std::thread::spawn(move || {
        let response = cache_directory.map_or_else(
            |error| {
                component_image_http_response(
                    tauri::http::StatusCode::INTERNAL_SERVER_ERROR,
                    error.to_string().into_bytes(),
                    "text/plain; charset=utf-8",
                )
            },
            |directory| component_image_response(&directory, &request_path),
        );
        responder.respond(response);
    });
}

fn load_component_image(cache_directory: &Path, file_name: &str) -> Result<Vec<u8>, String> {
    fs::create_dir_all(cache_directory).map_err(|error| error.to_string())?;
    let cache_file = cache_directory.join(file_name);
    if let Ok(cached) = fs::read(&cache_file)
        && valid_component_png(&cached)
    {
        return Ok(cached);
    }

    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(4))
        .timeout(Duration::from_secs(10))
        .user_agent("PlatScope/0.1")
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get(format!("{WFCD_IMAGE_BASE_URL}{file_name}"))
        .send()
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_COMPONENT_IMAGE_BYTES as u64)
    {
        return Err("component image exceeds the size limit".to_owned());
    }
    let image = response
        .bytes()
        .map_err(|error| error.to_string())?
        .to_vec();
    if !valid_component_png(&image) {
        return Err("component image is not a valid bounded PNG".to_owned());
    }

    let nonce = COMPONENT_IMAGE_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temporary = cache_directory.join(format!(".{file_name}.{nonce}.tmp"));
    if fs::write(&temporary, &image).is_ok() && fs::rename(&temporary, &cache_file).is_err() {
        let _ = fs::remove_file(&temporary);
    }
    Ok(image)
}

fn valid_component_png(image: &[u8]) -> bool {
    image.len() <= MAX_COMPONENT_IMAGE_BYTES
        && image.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A])
}

fn find_reward_ocr_executable(app: &AppHandle) -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    if let Some(configured) = std::env::var_os("PLATSCOPE_REWARD_OCR_PATH") {
        candidates.push(PathBuf::from(configured));
    }
    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(
            resource_dir
                .join("resources")
                .join("reward-ocr")
                .join(REWARD_OCR_EXECUTABLE),
        );
        candidates.push(resource_dir.join("reward-ocr").join(REWARD_OCR_EXECUTABLE));
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../reward-ocr/bin/Release/net8.0-windows/win-x64")
            .join(REWARD_OCR_EXECUTABLE),
    );
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| "OCR-помощник не собран. Выполните полную сборку PlatScope.".to_owned())
}

fn reward_overlay_geometry(
    rect: &WarframeWindowRect,
    settings: &AppSettings,
    cards: usize,
    max_set_parts: usize,
) -> RewardOverlayGeometry {
    let cards = u32::try_from(cards.clamp(2, 4)).unwrap_or(4);
    let reference_width = 320 + 324 * cards.saturating_sub(1);
    let scale_ratio = reward_overlay_scale_ratio(rect.width, rect.height, cards as usize, settings);
    let width =
        scale_reward_overlay_ratio(reference_width, scale_ratio).clamp(1, rect.width.max(1));
    let reference_height = reward_overlay_reference_height(max_set_parts);
    let height =
        scale_reward_overlay_ratio(reference_height, scale_ratio).clamp(1, rect.height.max(1));
    let centered_offset = i64::from(rect.width.saturating_sub(width)) / 2;
    let x_raw = i64::from(rect.x)
        .saturating_add(centered_offset)
        .saturating_add(reward_overlay_offset(
            rect.width,
            settings.reward_overlay_offset_x_percent,
        ));
    let min_x = i64::from(rect.x);
    let max_x = min_x.saturating_add(i64::from(rect.width.saturating_sub(width)));
    let x_raw = x_raw.clamp(min_x, max_x);
    let x = i32::try_from(x_raw).unwrap_or(if x_raw.is_negative() {
        i32::MIN
    } else {
        i32::MAX
    });
    let y_raw = i64::from(rect.y)
        .saturating_add(i64::from(scale_reward_overlay_dimension(430, rect.height)))
        .saturating_add(reward_overlay_offset(
            rect.height,
            settings.reward_overlay_offset_y_percent,
        ));
    let min_y = i64::from(rect.y);
    let max_y = min_y.saturating_add(i64::from(rect.height.saturating_sub(height)));
    let y_raw = y_raw.clamp(min_y, max_y);
    let y = i32::try_from(y_raw).unwrap_or(if y_raw.is_negative() {
        i32::MIN
    } else {
        i32::MAX
    });
    RewardOverlayGeometry {
        x,
        y,
        width,
        height,
    }
}

fn reward_overlay_reference_height(max_set_parts: usize) -> u32 {
    let part_rows = max_set_parts.div_ceil(2);
    let extra_rows = u32::try_from(part_rows.saturating_sub(2)).unwrap_or(u32::MAX);
    400_u32.saturating_add(extra_rows.saturating_mul(48))
}

fn scale_reward_overlay_dimension(reference: u32, game_height: u32) -> u32 {
    let scaled = u64::from(reference)
        .saturating_mul(u64::from(game_height))
        .saturating_add(540)
        / 1080;
    u32::try_from(scaled).unwrap_or(u32::MAX)
}

fn scale_reward_overlay_ratio(value: u32, (numerator, denominator): (u64, u64)) -> u32 {
    let scaled = u64::from(value)
        .saturating_mul(numerator)
        .saturating_add(denominator / 2)
        / denominator.max(1);
    u32::try_from(scaled).unwrap_or(u32::MAX)
}

fn reward_overlay_scale_ratio(
    game_width: u32,
    game_height: u32,
    cards: usize,
    settings: &AppSettings,
) -> (u64, u64) {
    let cards = u32::try_from(cards.clamp(2, 4)).unwrap_or(4);
    let reference_width = 320 + 324 * cards.saturating_sub(1);
    let requested = (
        u64::from(game_height).saturating_mul(u64::from(settings.reward_overlay_scale_percent)),
        108_000_u64,
    );
    let width_limit = (u64::from(game_width.max(1)), u64::from(reference_width));
    if requested.0.saturating_mul(width_limit.1) <= width_limit.0.saturating_mul(requested.1) {
        requested
    } else {
        width_limit
    }
}

#[allow(clippy::cast_precision_loss)] // Window dimensions are far below f64 integer precision.
fn reward_overlay_scale_factor(
    game_width: u32,
    game_height: u32,
    cards: usize,
    settings: &AppSettings,
) -> f64 {
    let (numerator, denominator) =
        reward_overlay_scale_ratio(game_width, game_height, cards, settings);
    numerator as f64 / denominator as f64
}

fn reward_overlay_offset(dimension: u32, percent: i16) -> i64 {
    i64::from(dimension).saturating_mul(i64::from(percent)) / 100
}

fn warframe_window_rect(app: &AppHandle) -> Result<WarframeWindowRect, String> {
    let executable = find_reward_ocr_executable(app)?;
    let mut command = Command::new(&executable);
    command
        .current_dir(executable.parent().unwrap_or_else(|| Path::new(".")))
        .arg("--warframe-window-rect")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_process_window(&mut command);
    let output = command
        .output()
        .map_err(|error| format!("Не удалось определить окно Warframe: {error}"))?;
    serde_json::from_slice(&output.stdout).map_err(|error| {
        let stderr = String::from_utf8_lossy(&output.stderr);
        format!("Окно Warframe недоступно: {error}. {stderr}")
    })
}

fn show_reward_overlay(
    app: &AppHandle,
    settings: &AppSettings,
    cards: usize,
    max_set_parts: usize,
) -> Result<(), String> {
    let rect = warframe_window_rect(app)?;
    let geometry = reward_overlay_geometry(&rect, settings, cards, max_set_parts);
    let window = app
        .get_webview_window("reward-overlay")
        .ok_or_else(|| "reward overlay window is unavailable".to_owned())?;
    window
        .set_size(PhysicalSize::new(geometry.width, geometry.height))
        .map_err(|error| error.to_string())?;
    window
        .set_position(PhysicalPosition::new(geometry.x, geometry.y))
        .map_err(|error| error.to_string())?;
    window
        .set_focusable(false)
        .map_err(|error| error.to_string())?;
    window
        .set_ignore_cursor_events(true)
        .map_err(|error| error.to_string())?;
    window
        .set_always_on_top(true)
        .map_err(|error| error.to_string())?;
    window.show().map_err(|error| error.to_string())?;
    window
        .set_always_on_top(true)
        .map_err(|error| error.to_string())?;
    tracing::info!(
        event = "reward_overlay_shown",
        cards,
        max_set_parts,
        x = geometry.x,
        y = geometry.y,
        width = geometry.width,
        height = geometry.height,
        "reward overlay shown over Warframe"
    );
    Ok(())
}

fn hide_reward_overlay(app: &AppHandle) {
    app.state::<AppState>()
        .reward_overlay_generation
        .fetch_add(1, Ordering::AcqRel);
    if let Some(window) = app.get_webview_window("reward-overlay")
        && let Err(error) = window.hide()
    {
        tracing::warn!(
            event = "reward_overlay_hide_failed",
            error = %error,
            "reward overlay could not be hidden"
        );
    }
}

fn run_reward_ocr_process(
    executable: &Path,
    request: &RewardOcrRequest,
) -> Result<RewardOcrResponse, String> {
    let mut command = Command::new(executable);
    command
        .current_dir(executable.parent().unwrap_or_else(|| Path::new(".")))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_process_window(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("Не удалось запустить OCR-помощник: {error}"))?;
    let payload = serde_json::to_vec(request).map_err(|error| error.to_string())?;
    child
        .stdin
        .take()
        .ok_or_else(|| "OCR stdin is unavailable".to_owned())?
        .write_all(&payload)
        .map_err(|error| format!("Не удалось передать данные OCR-помощнику: {error}"))?;
    let output = child
        .wait_with_output()
        .map_err(|error| format!("Не удалось дождаться OCR-помощника: {error}"))?;
    serde_json::from_slice(&output.stdout).map_err(|error| {
        let stderr = String::from_utf8_lossy(&output.stderr);
        format!("OCR-помощник вернул неверный ответ: {error}. {stderr}")
    })
}

fn reward_set_completion(
    database: &Mutex<Database>,
    settings: &AppSettings,
    metadata: Option<&GameMetadataSnapshot>,
    inventory: Option<&InventoryView>,
    catalog: &RewardCatalogDetails,
    reward_slug: &str,
) -> Result<Option<RewardSetCompletion>, String> {
    let (Some(metadata), Some(inventory)) = (metadata, inventory) else {
        return Ok(None);
    };
    let mut best: Option<RewardSetCompletion> = None;
    for definition in metadata.prime_sets.iter().filter(|set| {
        set.components
            .iter()
            .any(|component| component.slug == reward_slug)
    }) {
        let before = definition
            .components
            .iter()
            .map(|component| {
                reward_owned_quantity(Some(inventory), &component.slug).unwrap_or(0)
                    / component.required_quantity
            })
            .min()
            .unwrap_or(0);
        let after = definition
            .components
            .iter()
            .map(|component| {
                let owned = reward_owned_quantity(Some(inventory), &component.slug).unwrap_or(0);
                let gained = u32::from(component.slug == reward_slug);
                owned.saturating_add(gained) / component.required_quantity
            })
            .min()
            .unwrap_or(0);
        if after <= before {
            continue;
        }

        let set_price = reward_set_price(database, settings, definition)?;
        let mut owned_parts_value = Some(0.0);
        for component in &definition.components {
            let key = MarketVariantKey::new(
                component.slug.clone(),
                settings.platform,
                None,
                None::<String>,
            )
            .map_err(|error| error.to_string())?;
            let price =
                PricingService::price_current_variant(database, &key, MarketItemKind::Standard)
                    .map_err(|error| error.to_string())?
                    .and_then(|price| {
                        matches!(
                            price.confidence,
                            PriceConfidence::High | PriceConfidence::Medium
                        )
                        .then_some(price.fair_price)
                        .flatten()
                    });
            // Копии прошлых полных комплектов уже израсходованы. Альтернативная стоимость
            // учитывает только детали, выделенные на новый завершаемый комплект.
            let owned = next_set_owned_quantity(
                reward_owned_quantity(Some(inventory), &component.slug).unwrap_or(0),
                component.required_quantity,
                before,
            )
            .min(component.required_quantity);
            owned_parts_value = owned_parts_value
                .zip(price)
                .map(|(total, price)| total + price * f64::from(owned));
        }
        let incremental_value = set_price
            .zip(owned_parts_value)
            .map(|(set_value, parts_value)| (set_value - parts_value).max(0.0));
        let candidate = RewardSetCompletion {
            set_name: catalog.get(&definition.set_slug).map_or_else(
                || definition.display_name_en.clone(),
                |(name, _)| name.clone(),
            ),
            set_price,
            incremental_value,
        };
        let candidate_value = candidate.incremental_value.unwrap_or(0.0);
        let best_value = best
            .as_ref()
            .and_then(|current| current.incremental_value)
            .unwrap_or(0.0);
        if best.is_none() || candidate_value > best_value {
            best = Some(candidate);
        }
    }
    Ok(best)
}

fn elapsed_millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
fn load_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    let database = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?;
    database
        .get_setting(SETTINGS_KEY)
        .map(Option::unwrap_or_default)
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri deserializes the command body by value.
fn save_settings(settings: AppSettings, state: State<'_, AppState>) -> Result<(), String> {
    validate_app_settings(&settings).map_err(str::to_owned)?;
    let database = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?;
    database
        .set_setting(SETTINGS_KEY, &settings)
        .map_err(|error| error.to_string())
}

fn validate_app_settings(settings: &AppSettings) -> Result<(), &'static str> {
    if !(1..=24).contains(&settings.bulk_refresh_hours) {
        return Err("bulk refresh interval must be between 1 and 24 hours");
    }
    if !(15..=600).contains(&settings.live_quote_ttl_seconds) {
        return Err("live quote TTL must be between 15 and 600 seconds");
    }
    if settings.keep_inventory_copies > 10 {
        return Err("inventory copy reserve must be between 0 and 10");
    }
    if !(70..=140).contains(&settings.reward_overlay_scale_percent) {
        return Err("reward overlay scale must be between 70 and 140 percent");
    }
    if !(-40..=40).contains(&settings.reward_overlay_offset_x_percent)
        || !(-40..=40).contains(&settings.reward_overlay_offset_y_percent)
    {
        return Err("reward overlay offsets must be between -40 and 40 percent");
    }
    Ok(())
}

fn spawn_history_bootstrap(app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state = app_handle.state::<AppState>();
        match state.history_service.bootstrap(&state.database).await {
            Ok(outcome) => tracing::info!(
                event = "history_bootstrap_finished",
                imported_days = outcome.imported_days,
                coverage_days = outcome.coverage.day_count,
                failures = outcome.failures.len(),
                "background history bootstrap finished"
            ),
            Err(error) => tracing::warn!(
                event = "history_bootstrap_failed",
                error = %error,
                "background history bootstrap failed without blocking startup"
            ),
        }
    });
}

fn spawn_market_refresh_scheduler(app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(BULK_REFRESH_CHECK_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let state = app_handle.state::<AppState>();
            let settings = if let Ok(database) = state.database.lock() {
                database
                    .get_setting::<AppSettings>(SETTINGS_KEY)
                    .map(Option::unwrap_or_default)
            } else {
                tracing::warn!(
                    event = "market_refresh_scheduler_state_unavailable",
                    "background bulk refresh skipped because database state is unavailable"
                );
                continue;
            };
            let settings = match settings {
                Ok(settings) => settings,
                Err(error) => {
                    tracing::warn!(
                        event = "market_refresh_scheduler_settings_failed",
                        error = %error,
                        "background bulk refresh skipped because settings could not be read"
                    );
                    continue;
                }
            };
            refresh_market_in_background(&app_handle, &state, settings.bulk_refresh_hours).await;
            refresh_game_metadata_in_background(&app_handle, &state).await;
        }
    });
}

fn handle_trade_log_chunk(
    app_handle: &AppHandle,
    machine: &mut trade_log::TradeMachine,
    line_tail: &mut String,
    chunk: &str,
    now_ms: u64,
) {
    line_tail.push_str(chunk);
    let has_partial_line = !line_tail.ends_with('\n');
    let mut lines: Vec<String> = line_tail
        .split('\n')
        .map(|line| line.trim_end_matches('\r').to_owned())
        .collect();
    *line_tail = if has_partial_line {
        lines.pop().unwrap_or_default()
    } else {
        String::new()
    };
    for line in lines {
        let Some(trade) = machine.feed(&line, now_ms) else {
            continue;
        };
        let fingerprint = format!(
            "ee:{}:{}:{}:{}:{}:{}",
            trade.log_stamp.as_deref().unwrap_or("no-stamp"),
            trade.partner.as_deref().unwrap_or("unknown"),
            trade.platinum_given,
            trade.platinum_received,
            serde_json::to_string(&trade.given_items).unwrap_or_default(),
            serde_json::to_string(&trade.received_items).unwrap_or_default(),
        );
        let event = NewTradeEvent {
            fingerprint,
            occurred_at: Utc::now(),
            partner: trade.partner,
            platinum_given: trade.platinum_given,
            platinum_received: trade.platinum_received,
            given_items: trade.given_items,
            received_items: trade.received_items,
        };
        let inserted_id = app_handle
            .state::<AppState>()
            .database
            .lock()
            .ok()
            .and_then(|database| database.record_trade_event(&event).ok())
            .flatten();
        let Some(event_id) = inserted_id else {
            continue;
        };
        let trade_event = TradeEvent {
            id: event_id,
            occurred_at: event.occurred_at,
            partner: event.partner,
            platinum_given: event.platinum_given,
            platinum_received: event.platinum_received,
            given_items: event.given_items,
            received_items: event.received_items,
            status: TradeEventStatus::Pending,
            matched_order_id: None,
            reconciliation_json: None,
        };
        tracing::info!(
            event = "confirmed_trade_detected",
            "confirmed trade was recorded from EE.log"
        );
        if let Err(error) = app_handle.emit("trade-detected", ()) {
            tracing::warn!(
                event = "trade_detected_event_failed",
                error = %error,
                "trade was recorded but UI event failed"
            );
        }
        spawn_trade_reconciliation(app_handle.clone(), trade_event);
    }
}

fn handle_reward_markers(
    app_handle: &AppHandle,
    chunk: &str,
    tail: &mut String,
    last_emitted: &mut Option<Instant>,
    last_projection: &mut Option<Instant>,
) {
    let projection_paths = reward_log_projection_paths(chunk);
    if !projection_paths.is_empty() {
        if let Ok(mut active_paths) = app_handle.state::<AppState>().reward_relic_paths.lock() {
            if last_projection.is_none_or(|instant| instant.elapsed() > Duration::from_secs(30)) {
                active_paths.clear();
            }
            active_paths.extend(projection_paths);
        }
        *last_projection = Some(Instant::now());
    }
    if chunk.contains("ProjectionRewardChoice.lua: Relic reward screen shut down") {
        if let Ok(mut active_paths) = app_handle.state::<AppState>().reward_relic_paths.lock() {
            active_paths.clear();
        }
        *last_projection = None;
        hide_reward_overlay(app_handle);
    }
    tail.push_str(chunk);
    if reward_log_contains_reward_screen(tail) {
        let realtime_active = app_handle
            .state::<AppState>()
            .reward_realtime_active
            .load(Ordering::Acquire);
        if !realtime_active
            && last_emitted.is_none_or(|instant| instant.elapsed() >= REWARD_LOG_DEBOUNCE)
        {
            if let Err(error) = app_handle.emit("relic-reward-screen", ()) {
                tracing::warn!(
                    event = "relic_reward_screen_event_failed",
                    error = %error,
                    "reward screen was detected but UI event failed"
                );
            }
            *last_emitted = Some(Instant::now());
        }
        // Маркеры от одного экрана приходят несколько раз. Удаляем их даже во время
        // cooldown, иначе сохранённая строка повторно запустит OCR через восемь секунд.
        tail.clear();
    }
    *tail = tail
        .chars()
        .rev()
        .take(512)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
}

fn spawn_reward_log_watcher(app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let Some(path) = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|base| base.join("Warframe").join("EE.log"))
        else {
            return;
        };
        let mut interval = tokio::time::interval(REWARD_LOG_POLL_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut offset = None;
        let mut tail = String::new();
        let mut last_emitted: Option<Instant> = None;
        let mut last_projection: Option<Instant> = None;
        let mut trade_machine = trade_log::TradeMachine::default();
        let mut trade_line_tail = String::new();
        let watcher_started = Instant::now();
        // Торги могли завершиться после запуска Warframe, но до запуска PlatScope.
        // Дочитываем текущий EE.log с начала; стабильный fingerprint выше не даст
        // повторно добавить те же сделки после перезапуска приложения.
        let mut reward_live_from = None;
        loop {
            interval.tick().await;
            let Ok(metadata) = fs::metadata(&path) else {
                offset = None;
                reward_live_from = None;
                tail.clear();
                trade_machine = trade_log::TradeMachine::default();
                trade_line_tail.clear();
                continue;
            };
            let file_len = metadata.len();
            let Some(current_offset) = offset else {
                offset = Some(0);
                reward_live_from = Some(file_len);
                tracing::info!(
                    event = "trade_log_backfill_started",
                    bytes = file_len,
                    "reading the current EE.log session for completed trades"
                );
                continue;
            };
            if file_len < current_offset {
                offset = Some(0);
                reward_live_from = Some(0);
                tail.clear();
                trade_machine = trade_log::TradeMachine::default();
                trade_line_tail.clear();
                continue;
            }
            if file_len == current_offset {
                continue;
            }
            let read_path = path.clone();
            let read = tauri::async_runtime::spawn_blocking(move || {
                read_reward_log_chunk(&read_path, current_offset, file_len)
            })
            .await;
            let Ok(Ok((new_offset, chunk))) = read else {
                continue;
            };
            offset = Some(new_offset);
            let now_ms = u64::try_from(watcher_started.elapsed().as_millis()).unwrap_or(u64::MAX);
            handle_trade_log_chunk(
                &app_handle,
                &mut trade_machine,
                &mut trade_line_tail,
                &chunk,
                now_ms,
            );
            // Старые маркеры наград не должны повторно открывать OCR/оверлей при
            // запуске PlatScope. В реальном времени обрабатываем только байты,
            // дописанные после подключения наблюдателя.
            let live_from = reward_live_from.unwrap_or(current_offset);
            if new_offset > live_from {
                let skip = usize::try_from(live_from.saturating_sub(current_offset))
                    .unwrap_or(usize::MAX)
                    .min(chunk.len());
                if let Some(live_chunk) = chunk.get(skip..) {
                    handle_reward_markers(
                        &app_handle,
                        live_chunk,
                        &mut tail,
                        &mut last_emitted,
                        &mut last_projection,
                    );
                }
            }
        }
    });
}

fn spawn_reward_realtime_watcher(app_handle: AppHandle) {
    tauri::async_runtime::spawn_blocking(move || {
        let executable = match find_reward_ocr_executable(&app_handle) {
            Ok(executable) => executable,
            Err(error) => {
                tracing::warn!(
                    event = "reward_realtime_watcher_missing",
                    error = %error,
                    "real-time reward trigger is unavailable"
                );
                return;
            }
        };
        let mut child = match start_reward_realtime_process(&app_handle, &executable) {
            Ok(child) => child,
            Err(error) => {
                tracing::warn!(
                    event = "reward_realtime_watcher_start_failed",
                    error = %error,
                    "real-time reward trigger could not start"
                );
                return;
            }
        };
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            return;
        };

        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let Ok(event) = serde_json::from_str::<RewardTriggerEvent>(&line) else {
                continue;
            };
            handle_reward_trigger_event(&app_handle, event);
        }

        app_handle
            .state::<AppState>()
            .reward_realtime_active
            .store(false, Ordering::Release);
        let status = child.wait().ok().and_then(|result| result.code());
        tracing::warn!(
            event = "reward_realtime_watcher_stopped",
            exit_code = status,
            "real-time reward trigger stopped; EE.log fallback remains active"
        );
    });
}

fn start_reward_realtime_process(
    app_handle: &AppHandle,
    executable: &Path,
) -> Result<std::process::Child, String> {
    let watcher_payload = app_handle
        .state::<AppState>()
        .database
        .lock()
        .ok()
        .and_then(|database| build_reward_watcher_request(&database).ok())
        .and_then(|request| serde_json::to_vec(&request).ok());
    let mut command = Command::new(executable);
    command
        .arg("--watch-warframe-log")
        .arg(std::process::id().to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if watcher_payload.is_some() {
        command.arg("--visual-fallback").stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    hide_process_window(&mut command);
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    if let Some(payload) = watcher_payload {
        child
            .stdin
            .take()
            .ok_or_else(|| "reward watcher stdin is unavailable".to_owned())?
            .write_all(&payload)
            .map_err(|error| error.to_string())?;
    }
    Ok(child)
}

fn handle_reward_trigger_event(app_handle: &AppHandle, event: RewardTriggerEvent) {
    match event.event_type.as_str() {
        "ready" => {
            app_handle
                .state::<AppState>()
                .reward_realtime_active
                .store(true, Ordering::Release);
            tracing::info!(
                event = "reward_realtime_watcher_ready",
                shared_listener_already_existed = event.already_exists.unwrap_or(false),
                "real-time Warframe reward trigger is ready"
            );
        }
        "reward" => {
            tracing::info!(
                event = "relic_reward_screen_detected_realtime",
                source = event.source.as_deref().unwrap_or("dbwin"),
                "reward screen detected"
            );
            if let Err(error) = app_handle.emit("relic-reward-screen", ()) {
                tracing::warn!(
                    event = "relic_reward_screen_event_failed",
                    error = %error,
                    "real-time reward screen event failed"
                );
            }
        }
        "projection" => {
            let Some(path) = event.path else {
                return;
            };
            if let Ok(mut relic_paths) = app_handle.state::<AppState>().reward_relic_paths.lock() {
                if event.reset.unwrap_or(false) {
                    relic_paths.clear();
                }
                relic_paths.insert(path);
            }
        }
        "projection_clear" => {
            if let Ok(mut relic_paths) = app_handle.state::<AppState>().reward_relic_paths.lock() {
                relic_paths.clear();
            }
            hide_reward_overlay(app_handle);
        }
        _ => {}
    }
}

fn reward_log_contains_reward_screen(log: &str) -> bool {
    log.contains("Got rewards") || log.contains("ProjectionRewardChoice.lua: Missing icon data!")
}

fn reward_log_projection_paths(log: &str) -> HashSet<String> {
    const PREFIX: &str = "/Lotus/Types/Game/Projections/";
    let mut paths = HashSet::new();
    for line in log.lines() {
        let Some(start) = line.find(PREFIX) else {
            continue;
        };
        let candidate = &line[start..];
        let end = candidate
            .find(|character: char| character == ')' || character.is_whitespace())
            .unwrap_or(candidate.len());
        let path = &candidate[..end];
        let ignored_extension = Path::new(path).extension().is_some_and(|extension| {
            extension.eq_ignore_ascii_case("png") || extension.eq_ignore_ascii_case("lua")
        });
        if path.len() > PREFIX.len() && !ignored_extension {
            paths.insert(path.to_owned());
        }
    }
    paths
}

fn read_reward_log_chunk(
    path: &Path,
    offset: u64,
    file_len: u64,
) -> Result<(u64, String), std::io::Error> {
    let bytes_to_read = file_len.saturating_sub(offset).min(REWARD_LOG_READ_LIMIT);
    let mut file = fs::File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = Vec::with_capacity(usize::try_from(bytes_to_read).unwrap_or(0));
    file.take(bytes_to_read).read_to_end(&mut bytes)?;
    Ok((
        offset.saturating_add(bytes_to_read),
        String::from_utf8_lossy(&bytes).into_owned(),
    ))
}

async fn refresh_market_in_background(app_handle: &AppHandle, state: &AppState, refresh_hours: u8) {
    match state
        .market_data_service
        .refresh_if_due(&state.database, refresh_hours)
        .await
    {
        Ok(Some(outcome)) if !outcome.stale => {
            tracing::info!(
                event = "background_market_refresh_finished",
                provider = ?outcome.snapshot.provider,
                source_date = %outcome.snapshot.source_date,
                "background bulk refresh promoted a valid snapshot"
            );
            if let Err(error) = app_handle.emit("market-data-updated", outcome) {
                tracing::warn!(
                    event = "market_refresh_event_failed",
                    error = %error,
                    "bulk snapshot was promoted but UI event failed"
                );
            }
            match state.history_service.bootstrap(&state.database).await {
                Ok(history) => tracing::info!(
                    event = "post_refresh_history_bootstrap_finished",
                    imported_days = history.imported_days,
                    coverage_days = history.coverage.day_count,
                    failures = history.failures.len(),
                    "history bootstrap followed successful background market refresh"
                ),
                Err(error) => tracing::warn!(
                    event = "post_refresh_history_bootstrap_failed",
                    error = %error,
                    "market refresh succeeded but history bootstrap failed"
                ),
            }
        }
        Ok(Some(outcome)) => tracing::warn!(
            event = "background_market_refresh_used_lkg",
            source_date = %outcome.snapshot.source_date,
            "background bulk refresh failed and preserved LKG"
        ),
        Ok(None) => {}
        Err(error) => tracing::warn!(
            event = "background_market_refresh_failed",
            error = %error,
            "background bulk refresh failed without blocking startup"
        ),
    }
}

async fn refresh_game_metadata_in_background(app_handle: &AppHandle, state: &AppState) {
    match state
        .game_metadata_service
        .refresh_if_due(&state.database, GAME_METADATA_REFRESH_HOURS)
        .await
    {
        Ok(Some(outcome)) if !outcome.stale => {
            tracing::info!(
                event = "background_game_metadata_refresh_finished",
                set_count = outcome.metadata.set_count,
                relic_count = outcome.metadata.relic_count,
                riven_disposition_count = outcome.metadata.riven_disposition_count,
                "background game metadata refresh promoted a valid snapshot"
            );
            if let Err(error) = app_handle.emit("game-metadata-updated", outcome) {
                tracing::warn!(
                    event = "game_metadata_refresh_event_failed",
                    error = %error,
                    "game metadata was promoted but UI event failed"
                );
            }
        }
        Ok(Some(outcome)) => tracing::warn!(
            event = "background_game_metadata_refresh_used_lkg",
            fetched_at = %outcome.metadata.fetched_at,
            "background game metadata refresh failed and preserved LKG"
        ),
        Ok(None) => {}
        Err(error) => tracing::warn!(
            event = "background_game_metadata_refresh_failed",
            error = %error,
            "background game metadata refresh failed without affecting pricing"
        ),
    }
}

/// Запускает Tauri shell и владеет жизненным циклом desktop-приложения.
///
/// # Panics
///
/// Завершает процесс, если Tauri runtime не может быть создан или запущен.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .register_asynchronous_uri_scheme_protocol(
            COMPONENT_IMAGE_PROTOCOL,
            serve_component_image_protocol,
        )
        .setup(|app| {
            let data_directory = app.path().local_data_dir()?.join("PlatScope");
            fs::create_dir_all(&data_directory)?;
            let logging_guard = init_logging(&data_directory.join("logs"))?;
            let database_path = data_directory.join("platscope.db");
            let database = Database::open(&database_path)?;
            let reward_database = Database::open(&database_path)?;
            let market_data_service = MarketDataService::production()?;
            let live_pricing_service = LivePricingService::production()?;
            let history_service = HistoryService::production()?;
            let game_metadata_service = GameMetadataService::production()?;
            let resource_converter_service = ResourceConverterService::production()?;
            let account_device_id =
                if let Some(value) = database.get_setting::<String>(ACCOUNT_DEVICE_ID_KEY)? {
                    value
                } else {
                    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
                    let value = format!("platscope-desktop-{nonce:x}");
                    database.set_setting(ACCOUNT_DEVICE_ID_KEY, &value)?;
                    value
                };
            let account_service = AccountService::production(account_device_id)?;

            tracing::info!(
                event = "foundation_ready",
                schema_version = database.schema_version()?,
                "PlatScope foundation initialized"
            );

            app.manage(AppState {
                database: Mutex::new(database),
                reward_database: Mutex::new(reward_database),
                market_data_service,
                live_pricing_service,
                history_service,
                game_metadata_service,
                resource_converter_service,
                account_service,
                trade_reconciliation_lock: tokio::sync::Mutex::new(()),
                read_only_inventory_scanner: Arc::new(ReadOnlyInventoryScanner::new()),
                reward_scan_in_flight: AtomicBool::new(false),
                reward_realtime_active: AtomicBool::new(false),
                reward_relic_paths: Mutex::new(HashSet::new()),
                latest_reward_scan: Mutex::new(None),
                reward_overlay_generation: AtomicU64::new(0),
                data_directory,
                _logging_guard: logging_guard,
            });

            spawn_history_bootstrap(app.handle().clone());
            spawn_market_refresh_scheduler(app.handle().clone());
            spawn_pending_trade_reconciliation(app.handle().clone());
            spawn_reward_log_watcher(app.handle().clone());
            spawn_reward_realtime_watcher(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            foundation_status,
            diagnostics_status,
            export_diagnostics_report,
            refresh_market_data,
            refresh_game_metadata,
            insights,
            resource_converter,
            open_market_items,
            account_status,
            account_connect,
            account_disconnect,
            account_create_listing,
            account_update_listing,
            account_delete_listing,
            trade_events,
            trade_sales_summary,
            trade_event_reconciled,
            trade_event_ignore,
            trade_event_restore,
            trade_event_retry,
            search_market,
            scan_relic_rewards,
            preview_reward_overlay,
            latest_relic_rewards,
            price_current_variant,
            live_price_current_variant,
            market_history,
            bootstrap_history,
            scan_read_only_inventory,
            load_inventory,
            set_inventory_keep_copies,
            sell_now,
            sell_now_live,
            load_settings,
            save_settings
        ])
        .run(tauri::generate_context!())
        .expect("unable to run PlatScope desktop application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use platscope_core::{InventorySummary, InventoryViewItem};
    use platscope_domain::{InventorySnapshotMetadata, InventorySource, VaultStatus};

    fn listing_inventory(sellable_quantity: u32) -> InventoryView {
        InventoryView {
            metadata: InventorySnapshotMetadata {
                source: InventorySource::TestFixture,
                observed_at: Utc::now(),
                schema_version: 2,
                item_count: 1,
                checksum_sha256: "inventory".into(),
            },
            keep_copies: 1,
            mod_usage_scanned: true,
            summary: InventorySummary {
                owned_quantity: 4,
                sellable_quantity: u64::from(sellable_quantity),
                resolved_rows: 1,
                attention_rows: 0,
            },
            items: vec![InventoryViewItem {
                canonical_game_id: "/Lotus/Test/Mod".into(),
                item_id: Some("item-id".into()),
                bulk_tradable: true,
                display_name: "Тестовый мод".into(),
                image_url: None,
                tags: vec!["mod".into()],
                key: Some(
                    MarketVariantKey::new(
                        "test_mod",
                        platscope_domain::Platform::Pc,
                        Some(5),
                        None::<String>,
                    )
                    .expect("key")
                    .with_charges(Some(2)),
                ),
                rank: Some(5),
                subtype: None,
                owned_quantity: 4,
                tradeable_quantity: 4,
                untradeable_quantity: 0,
                unknown_quantity: 0,
                leveled_quantity: 4,
                equipped_quantity: 0,
                equipped_placements: Vec::new(),
                sellable_quantity,
                resolution: InventoryResolution::Resolved,
                vault_status: VaultStatus::Unknown,
            }],
        }
    }

    fn listing_intent(quantity: u32, per_trade: u32) -> SellListingIntent {
        SellListingIntent {
            item_id: "item-id".into(),
            quantity,
            per_trade,
            rank: Some(5),
            charges: Some(2),
            subtype: None,
            amber_stars: None,
            cyan_stars: None,
        }
    }

    fn existing_sell_order(quantity: u32) -> AccountOrder {
        AccountOrder {
            id: "existing".into(),
            item_id: Some("item-id".into()),
            order_type: AccountOrderType::Sell,
            platinum: 10,
            quantity,
            per_trade: Some(1),
            rank: Some(5),
            charges: Some(2),
            subtype: None,
            amber_stars: None,
            cyan_stars: None,
            visible: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn set_listing_fixture() -> (InventoryView, SellListingIntent, PrimeSetDefinition) {
        let mut inventory = listing_inventory(0);
        inventory.metadata.item_count = 2;
        inventory.summary.owned_quantity = 6;
        inventory.summary.sellable_quantity = 6;
        inventory.summary.resolved_rows = 2;
        inventory.items = [
            ("set_part_a", "part-a-id", 2_u32),
            ("set_part_b", "part-b-id", 4_u32),
        ]
        .into_iter()
        .map(|(slug, item_id, quantity)| InventoryViewItem {
            canonical_game_id: format!("/Lotus/Test/{slug}"),
            item_id: Some(item_id.into()),
            bulk_tradable: false,
            display_name: slug.into(),
            image_url: None,
            tags: vec!["prime".into()],
            key: Some(
                MarketVariantKey::new(slug, platscope_domain::Platform::Pc, None, None::<String>)
                    .expect("component key"),
            ),
            rank: None,
            subtype: None,
            owned_quantity: quantity,
            tradeable_quantity: quantity,
            untradeable_quantity: 0,
            unknown_quantity: 0,
            leveled_quantity: 0,
            equipped_quantity: 0,
            equipped_placements: Vec::new(),
            sellable_quantity: quantity,
            resolution: InventoryResolution::Resolved,
            vault_status: VaultStatus::Unknown,
        })
        .collect();
        let intent = SellListingIntent {
            item_id: "set-id".into(),
            quantity: 2,
            per_trade: 1,
            rank: None,
            charges: None,
            subtype: None,
            amber_stars: None,
            cyan_stars: None,
        };
        let definition = PrimeSetDefinition {
            set_slug: "test_prime_set".into(),
            set_game_ref: "/Lotus/Test/Set".into(),
            display_name_en: "Test Prime Set".into(),
            vault_status: VaultStatus::Unknown,
            components: vec![
                platscope_domain::PrimeSetComponentDefinition {
                    slug: "set_part_a".into(),
                    game_ref: "/Lotus/Test/set_part_a".into(),
                    required_quantity: 1,
                    ducats: Some(15),
                    image_url: None,
                },
                platscope_domain::PrimeSetComponentDefinition {
                    slug: "set_part_b".into(),
                    game_ref: "/Lotus/Test/set_part_b".into(),
                    required_quantity: 2,
                    ducats: Some(45),
                    image_url: None,
                },
            ],
        };
        (inventory, intent, definition)
    }

    #[test]
    fn listing_validation_reserves_existing_orders_and_checks_lot_size() {
        let inventory = listing_inventory(3);
        assert!(
            validate_sell_listing_inventory(
                &listing_intent(1, 1),
                &inventory,
                &[existing_sell_order(2)],
                None,
                None,
                &HashMap::new(),
            )
            .is_ok()
        );
        assert!(
            validate_sell_listing_inventory(
                &listing_intent(2, 1),
                &inventory,
                &[existing_sell_order(2)],
                None,
                None,
                &HashMap::new(),
            )
            .is_err()
        );
        assert!(
            validate_sell_listing_inventory(
                &listing_intent(3, 2),
                &inventory,
                &[],
                None,
                None,
                &HashMap::new(),
            )
            .is_err()
        );
    }

    #[test]
    fn listing_validation_requires_the_exact_charged_variant() {
        let inventory = listing_inventory(3);
        let mut wrong = listing_intent(1, 1);
        wrong.charges = Some(1);
        assert!(
            validate_sell_listing_inventory(&wrong, &inventory, &[], None, None, &HashMap::new(),)
                .is_err()
        );
    }

    #[test]
    fn set_listing_validation_uses_components_and_reserves_component_orders() {
        let (inventory, intent, definition) = set_listing_fixture();
        assert!(
            validate_sell_listing_inventory(
                &intent,
                &inventory,
                &[],
                None,
                Some(&definition),
                &HashMap::new(),
            )
            .is_ok()
        );

        let component_order = AccountOrder {
            id: "component-order".into(),
            item_id: Some("part-b-id".into()),
            order_type: AccountOrderType::Sell,
            platinum: 5,
            quantity: 2,
            per_trade: None,
            rank: None,
            charges: None,
            subtype: None,
            amber_stars: None,
            cyan_stars: None,
            visible: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        assert!(
            validate_sell_listing_inventory(
                &intent,
                &inventory,
                &[component_order],
                None,
                Some(&definition),
                &HashMap::new(),
            )
            .is_err()
        );
    }

    #[test]
    fn listing_validation_does_not_reuse_parts_reserved_by_set_orders() {
        let (inventory, intent, definition) = set_listing_fixture();
        let set_reservations = HashMap::from([
            ("part-a-id".to_owned(), 1_u32),
            ("part-b-id".to_owned(), 2_u32),
        ]);
        assert!(
            validate_sell_listing_inventory(
                &intent,
                &inventory,
                &[],
                None,
                Some(&definition),
                &set_reservations,
            )
            .is_err()
        );

        let component_intent = SellListingIntent {
            item_id: "part-a-id".into(),
            quantity: 1,
            per_trade: 1,
            rank: None,
            charges: None,
            subtype: None,
            amber_stars: None,
            cyan_stars: None,
        };
        assert!(
            validate_sell_listing_inventory(
                &component_intent,
                &inventory,
                &[],
                None,
                None,
                &HashMap::from([("part-a-id".to_owned(), 2_u32)]),
            )
            .is_err()
        );
    }

    #[test]
    fn old_settings_receive_current_overlay_defaults() {
        let settings: AppSettings = serde_json::from_str(
            r#"{"language":"russian","platform":"pc","crossplay":true,"bulk_refresh_hours":4,"live_quote_ttl_seconds":90,"keep_inventory_copies":1}"#,
        )
        .expect("old settings remain compatible");
        assert_eq!(settings.reward_overlay_scale_percent, 100);
        assert_eq!(settings.reward_overlay_offset_x_percent, 0);
        assert_eq!(settings.reward_overlay_offset_y_percent, 0);
    }

    #[test]
    fn settings_validation_enforces_operational_bounds() {
        assert!(validate_app_settings(&AppSettings::default()).is_ok());

        for bulk_refresh_hours in [0, 25] {
            let settings = AppSettings {
                bulk_refresh_hours,
                ..AppSettings::default()
            };
            assert!(validate_app_settings(&settings).is_err());
        }

        for live_quote_ttl_seconds in [14, 601] {
            let settings = AppSettings {
                live_quote_ttl_seconds,
                ..AppSettings::default()
            };
            assert!(validate_app_settings(&settings).is_err());
        }

        let settings = AppSettings {
            keep_inventory_copies: 11,
            ..AppSettings::default()
        };
        assert!(validate_app_settings(&settings).is_err());

        for reward_overlay_scale_percent in [69, 141] {
            let settings = AppSettings {
                reward_overlay_scale_percent,
                ..AppSettings::default()
            };
            assert!(validate_app_settings(&settings).is_err());
        }

        for offset in [-41, 41] {
            let settings = AppSettings {
                reward_overlay_offset_x_percent: offset,
                ..AppSettings::default()
            };
            assert!(validate_app_settings(&settings).is_err());
        }
    }

    #[test]
    fn diagnostic_report_omits_local_path_and_credentials() {
        let status = DiagnosticsStatus {
            generated_at: Utc::now(),
            foundation: FoundationStatus {
                app_name: "PlatScope",
                app_version: "0.1.0",
                database_path: r"C:\Users\SecretName\AppData\Local\PlatScope\platscope.db".into(),
                schema_version: 10,
                offline_ready: true,
                market_snapshot: None,
                catalog_item_count: Some(3_840),
                history_coverage: HistoryCoverage {
                    oldest_date: None,
                    newest_date: None,
                    day_count: 0,
                },
                inventory_item_count: Some(3),
            },
            providers: Vec::new(),
        };

        let report = safe_diagnostics_report(status);
        let json = serde_json::to_string(&report).expect("safe report serializes");
        assert!(!json.contains("SecretName"));
        assert!(!json.contains("databasePath"));
        assert!(!json.contains("password"));
        assert!(!json.contains("token"));
        assert!(!json.contains("nonce"));
        assert!(json.contains("\"reportVersion\":1"));

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is valid")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("platscope-report-test-{suffix}"));
        let (path, bytes) =
            write_safe_diagnostics_report(&directory, &report).expect("safe report is written");
        let persisted = fs::read_to_string(&path).expect("safe report is readable");
        assert_eq!(u64::try_from(persisted.len()).expect("length fits"), bytes);
        assert!(!persisted.contains("SecretName"));
        assert_eq!(path.parent(), Some(directory.as_path()));
        fs::remove_file(path).expect("test report is removed");
        fs::remove_dir(directory).expect("test directory is removed");
    }

    #[test]
    fn market_links_accept_only_bounded_canonical_slugs() {
        let slugs = validate_market_slugs(vec![
            "nyx_prime_systems".into(),
            "nyx_prime_systems".into(),
            "nyx_prime_chassis".into(),
        ])
        .expect("canonical slugs are accepted");
        assert_eq!(slugs, ["nyx_prime_systems", "nyx_prime_chassis"]);
        assert!(validate_market_slugs(vec!["https://example.com".into()]).is_err());
        assert!(validate_market_slugs(vec!["../secret".into()]).is_err());
        assert!(validate_market_slugs(Vec::new()).is_err());
    }

    #[test]
    fn market_item_links_always_use_russian_locale() {
        assert_eq!(
            market_item_url("carrier_prime_cerebrum"),
            "https://warframe.market/ru/items/carrier_prime_cerebrum"
        );
    }

    #[test]
    fn component_images_use_a_validated_local_protocol() {
        let remote = "https://cdn.warframestat.us/img/GenericGunPrimeBarrel.png";
        let local = component_image_protocol_url(remote).expect("known CDN image is accepted");
        assert!(local.ends_with("/GenericGunPrimeBarrel.png"));
        assert!(local.contains(COMPONENT_IMAGE_PROTOCOL));
        assert!(component_image_protocol_url("https://example.com/barrel.png").is_none());
        assert!(
            component_image_protocol_url("https://cdn.warframestat.us/img/../secret.png").is_none()
        );

        let invalid = component_image_response(Path::new("unused"), "/../secret.png");
        assert_eq!(invalid.status(), tauri::http::StatusCode::BAD_REQUEST);
        assert!(valid_component_png(&[
            0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A
        ]));
        assert!(!valid_component_png(b"not a png"));
    }

    #[test]
    fn reward_log_markers_match_current_warframe_messages() {
        assert!(reward_log_contains_reward_screen(
            "Script [Info]: Got rewards; waiting for choice"
        ));
        assert!(reward_log_contains_reward_screen(
            "ProjectionRewardChoice.lua: Missing icon data!"
        ));
        assert!(!reward_log_contains_reward_screen(
            "Script [Info]: Mission complete"
        ));
    }

    #[test]
    fn reward_log_extracts_only_projection_resource_paths() {
        let paths = reward_log_projection_paths(
            "ResourceLoader (/Lotus/Types/Game/Projections/T1VoidProjectionGaussPrimeBPlatinum) Found\n\
             Spot-loading /Lotus/Types/Game/Projections/T1VoidProjectionLavosPrimeABronze\n\
             Spot-loading /Lotus/Types/Game/Projections/ProjectionIcon.png",
        );
        assert_eq!(paths.len(), 2);
        assert!(
            paths.contains("/Lotus/Types/Game/Projections/T1VoidProjectionGaussPrimeBPlatinum")
        );
        assert!(paths.contains("/Lotus/Types/Game/Projections/T1VoidProjectionLavosPrimeABronze"));
    }

    #[test]
    fn reward_set_component_names_stay_compact_and_readable() {
        let definition = PrimeSetDefinition {
            set_slug: "nyx_prime_set".into(),
            set_game_ref: String::new(),
            display_name_en: "Nyx Prime Set".into(),
            vault_status: platscope_domain::VaultStatus::Unknown,
            components: Vec::new(),
        };
        let catalog = HashMap::from([
            (
                "nyx_prime_set".into(),
                ("Никс Прайм: Комплект".into(), None),
            ),
            (
                "nyx_prime_neuroptics_blueprint".into(),
                ("Никс Прайм: Нейрооптика (Чертеж)".into(), None),
            ),
            (
                "nyx_prime_blueprint".into(),
                ("Никс Прайм (Чертеж)".into(), None),
            ),
        ]);
        assert_eq!(
            reward_set_component_name("nyx_prime_neuroptics_blueprint", &definition, &catalog),
            "Нейрооптика"
        );
        assert_eq!(
            reward_set_component_name("nyx_prime_blueprint", &definition, &catalog),
            "Чертёж"
        );
        assert_eq!(
            reward_set_component_name("nyx_prime_upper_limb", &definition, &HashMap::new()),
            "Часть комплекта"
        );
    }

    #[test]
    fn reward_ocr_catalog_accepts_only_non_empty_russian_names() {
        assert_eq!(
            russian_reward_ocr_name(Some("  Ивара Прайм: Каркас  ".into())).as_deref(),
            Some("Ивара Прайм: Каркас")
        );
        assert_eq!(russian_reward_ocr_name(Some("   ".into())), None);
        assert_eq!(russian_reward_ocr_name(None), None);
    }

    fn reward_choice_for_ranking(
        slot: u8,
        confidence: f64,
        choice_value: Option<f64>,
        ducats: Option<u32>,
    ) -> RelicRewardChoice {
        RelicRewardChoice {
            slot,
            raw_text: String::new(),
            confidence,
            item_id: Some(format!("item-{slot}")),
            slug: Some(format!("reward-{slot}")),
            display_name: Some(format!("Reward {slot}")),
            market: None,
            ducats,
            owned_quantity: Some(0),
            set: None,
            completes_set: None,
            choice_value,
            recommended: false,
        }
    }

    #[test]
    fn reward_recommendation_rejects_uncertain_ocr_even_with_a_high_price() {
        let mut rewards = [
            reward_choice_for_ranking(0, 0.60, Some(1_000.0), Some(100)),
            reward_choice_for_ranking(1, 0.95, Some(20.0), Some(15)),
        ];

        mark_recommended_reward(&mut rewards);

        assert!(!rewards[0].recommended);
        assert!(rewards[1].recommended);
    }

    #[test]
    fn reward_value_does_not_treat_low_confidence_as_a_full_price() {
        assert_eq!(
            credible_reward_value(PriceConfidence::Low, Some(500.0), None, None),
            None
        );
        assert_eq!(
            credible_reward_value(PriceConfidence::Medium, Some(20.0), None, None),
            Some(20.0)
        );
    }

    #[test]
    fn reward_recommendation_uses_ducats_as_a_price_tie_breaker() {
        let mut rewards = [
            reward_choice_for_ranking(0, 0.95, Some(20.0), Some(15)),
            reward_choice_for_ranking(1, 0.95, Some(20.0), Some(100)),
        ];

        mark_recommended_reward(&mut rewards);

        assert!(!rewards[0].recommended);
        assert!(rewards[1].recommended);
    }

    #[test]
    fn reward_set_progress_excludes_copies_used_by_previous_sets() {
        assert_eq!(next_set_owned_quantity(2, 1, 1), 1);
        assert_eq!(next_set_owned_quantity(5, 2, 2), 1);
        assert_eq!(next_set_owned_quantity(1, 1, 2), 0);
    }

    #[test]
    fn reward_overlay_tracks_the_warframe_card_block() {
        let rect = WarframeWindowRect {
            x: 1920,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let settings = AppSettings::default();
        let four = reward_overlay_geometry(&rect, &settings, 4, 4);
        assert_eq!(
            four,
            RewardOverlayGeometry {
                x: 2234,
                y: 430,
                width: 1292,
                height: 400,
            }
        );
        let three = reward_overlay_geometry(&rect, &settings, 3, 4);
        assert_eq!((three.x, three.width), (2396, 968));
        let two = reward_overlay_geometry(&rect, &settings, 2, 4);
        assert_eq!((two.x, two.width), (2558, 644));
    }

    #[test]
    fn reward_overlay_scales_by_game_height_on_ultrawide() {
        let rect = WarframeWindowRect {
            x: 0,
            y: 120,
            width: 3440,
            height: 1440,
        };
        let overlay = reward_overlay_geometry(&rect, &AppSettings::default(), 4, 4);
        assert_eq!(overlay.width, 1723);
        assert_eq!(overlay.height, 533);
        assert_eq!(overlay.x, 858);
        assert_eq!(overlay.y, 693);
    }

    #[test]
    fn reward_overlay_applies_saved_scale_and_offsets() {
        let rect = WarframeWindowRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let settings = AppSettings {
            reward_overlay_scale_percent: 80,
            reward_overlay_offset_x_percent: 10,
            reward_overlay_offset_y_percent: -10,
            ..AppSettings::default()
        };
        let overlay = reward_overlay_geometry(&rect, &settings, 4, 4);
        assert_eq!(overlay.width, 1034);
        assert_eq!(overlay.height, 320);
        assert_eq!(overlay.x, 635);
        assert_eq!(overlay.y, 322);
    }

    #[test]
    fn reward_overlay_keeps_contents_inside_at_small_resolution_and_eighty_six_percent() {
        let rect = WarframeWindowRect {
            x: 0,
            y: 0,
            width: 1224,
            height: 878,
        };
        let settings = AppSettings {
            reward_overlay_scale_percent: 86,
            ..AppSettings::default()
        };
        let scale = reward_overlay_scale_factor(rect.width, rect.height, 4, &settings);
        let overlay = reward_overlay_geometry(&rect, &settings, 4, 4);
        assert!((scale - 0.699_148).abs() < 0.000_001);
        assert_eq!(overlay.width, 903);
        assert_eq!(overlay.height, 280);
        assert_eq!(overlay.x, 160);
        assert_eq!(overlay.y, 350);
    }

    #[test]
    fn reward_overlay_adds_space_for_a_fifth_set_part() {
        let rect = WarframeWindowRect {
            x: 0,
            y: 0,
            width: 1224,
            height: 878,
        };
        let settings = AppSettings {
            reward_overlay_scale_percent: 86,
            ..AppSettings::default()
        };

        let four_parts = reward_overlay_geometry(&rect, &settings, 4, 4);
        let five_parts = reward_overlay_geometry(&rect, &settings, 4, 5);

        assert_eq!(reward_overlay_reference_height(4), 400);
        assert_eq!(reward_overlay_reference_height(5), 448);
        assert_eq!(four_parts.height, 280);
        assert_eq!(five_parts.height, 313);
        assert_eq!(five_parts.width, four_parts.width);
        assert_eq!(five_parts.x, four_parts.x);
        assert_eq!(five_parts.y, four_parts.y);
    }

    fn automatic_trade_order(
        item_id: &str,
        quantity: u32,
        occurred_at: DateTime<Utc>,
    ) -> AccountOrder {
        AccountOrder {
            id: format!("order-{item_id}"),
            item_id: Some(item_id.to_owned()),
            order_type: AccountOrderType::Sell,
            platinum: 25,
            quantity,
            per_trade: None,
            rank: None,
            charges: None,
            subtype: None,
            amber_stars: None,
            cyan_stars: None,
            visible: true,
            created_at: occurred_at - chrono::Duration::hours(1),
            updated_at: occurred_at - chrono::Duration::seconds(1),
        }
    }

    fn automatic_trade_event(items: Vec<TradeItem>, occurred_at: DateTime<Utc>) -> TradeEvent {
        TradeEvent {
            id: 7,
            occurred_at,
            partner: Some("MarketTenno".into()),
            platinum_given: 0,
            platinum_received: 25,
            given_items: items,
            received_items: Vec::new(),
            status: TradeEventStatus::Pending,
            matched_order_id: None,
            reconciliation_json: None,
        }
    }

    #[test]
    fn automatic_trade_plan_closes_instead_of_deleting_order() {
        let occurred_at = Utc::now();
        let order = automatic_trade_order("item-123", 3, occurred_at);
        let account = AccountView {
            connected: true,
            profile: None,
            orders: vec![order.clone()],
            order_items: HashMap::from([(
                "item-123".into(),
                AccountOrderItemView {
                    slug: "strun_prime_stock".into(),
                    display_name: "Стран Прайм: Приклад".into(),
                    display_name_en: "Strun Prime Stock".into(),
                    image_url: None,
                    item_kind: MarketItemKind::Standard,
                    set_components: Vec::new(),
                },
            )]),
        };
        let event = automatic_trade_event(
            vec![TradeItem {
                name: "Стран Прайм: Приклад".into(),
                quantity: 1,
            }],
            occurred_at,
        );

        let actions = plan_automatic_trade_reconciliation(&event, &account)
            .expect("sale matches exactly one order");

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].kind, AutomaticTradeActionKind::Close);
        assert_eq!(actions[0].before.id, order.id);
        assert_eq!(actions[0].sold_quantity, 1);
    }

    #[test]
    fn automatic_trade_plan_recognizes_complete_set_from_components() {
        let occurred_at = Utc::now();
        let order = automatic_trade_order("hildryn-set", 1, occurred_at);
        let component = |slug: &str, ru: &str, en: &str| AccountSetComponentView {
            slug: slug.into(),
            required_quantity: 1,
            display_name: ru.into(),
            display_name_en: en.into(),
        };
        let components = vec![
            component(
                "hildryn_prime_blueprint",
                "Хильдрин Прайм",
                "Hildryn Prime Blueprint",
            ),
            component(
                "hildryn_prime_chassis",
                "Хильдрин Прайм: Каркас",
                "Hildryn Prime Chassis Blueprint",
            ),
            component(
                "hildryn_prime_neuroptics",
                "Хильдрин Прайм: Нейрооптика",
                "Hildryn Prime Neuroptics Blueprint",
            ),
            component(
                "hildryn_prime_systems",
                "Хильдрин Прайм: Система",
                "Hildryn Prime Systems Blueprint",
            ),
        ];
        let account = AccountView {
            connected: true,
            profile: None,
            orders: vec![order],
            order_items: HashMap::from([(
                "hildryn-set".into(),
                AccountOrderItemView {
                    slug: "hildryn_prime_set".into(),
                    display_name: "Хильдрин Прайм: Комплект".into(),
                    display_name_en: "Hildryn Prime Set".into(),
                    image_url: None,
                    item_kind: MarketItemKind::Standard,
                    set_components: components,
                },
            )]),
        };
        let event = automatic_trade_event(
            vec![
                TradeItem {
                    name: "Хильдрин Прайм".into(),
                    quantity: 1,
                },
                TradeItem {
                    name: "Хильдрин Прайм: Каркас".into(),
                    quantity: 1,
                },
                TradeItem {
                    name: "Хильдрин Прайм: Нейрооптика".into(),
                    quantity: 1,
                },
                TradeItem {
                    name: "Хильдрин Прайм: Система".into(),
                    quantity: 1,
                },
            ],
            occurred_at,
        );

        let actions = plan_automatic_trade_reconciliation(&event, &account)
            .expect("all components form one complete set");

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].kind, AutomaticTradeActionKind::Close);
        assert_eq!(actions[0].item_name, "Хильдрин Прайм: Комплект");
        assert_eq!(actions[0].sold_quantity, 1);
    }
}
