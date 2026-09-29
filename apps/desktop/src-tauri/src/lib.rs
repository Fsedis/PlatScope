#![forbid(unsafe_code)]

mod app_lifecycle;
mod binary_recording;
mod dbwin_capture;
mod game_names;
mod inventory_refresh;
mod market_account;
mod market_presence;
mod market_profiles;
mod memory_recording;
mod mission_research;
mod reward_ocr;
mod squad;
mod trade_log;
mod trade_reconciliation;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use platscope_core::{
    AccountOrder, AccountOrderType, AccountService, AccountView, AppSettings, BountyHunterService,
    BountyHunterView, CreateListingInput, DEFAULT_MARKET_SEARCH_LIMIT, GameMetadataRefreshOutcome,
    GameMetadataService, HistoryBootstrapOutcome, HistoryService, InsightsService, InsightsView,
    InventoryService, InventoryView, LivePricingResult, LivePricingService, LiveSellNowResult,
    LoggingGuard, MarketBrowserService, MarketDataService, MarketHistoryView, MarketRefreshOutcome,
    MarketSearchResult, MasteryService, MasteryView, PersonalGoalsService, PersonalGoalsView,
    PriceRecommendation, PricingService, ResourceConverterService, ResourceConverterView,
    SETTINGS_KEY, SellNowService, SellNowView, UpdateListingInput, WorldActivityService,
    WorldActivityView, enrich_account_view, init_logging,
};
use platscope_domain::{InventoryResolution, MarketItemKind, MarketVariantKey, PrimeSetDefinition};
use platscope_readonly_scan::inventory::{
    InventoryScanner as ReadOnlyInventoryScanner, ReadOnlyScanResult,
};
use platscope_storage::{Database, HistoryCoverage, MarketSnapshotSummary, ProviderHealth};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

struct AppState {
    database: Mutex<Database>,
    // OCR наград чувствителен к задержкам: обновление рынка может удерживать основной
    // mutex БД дольше, чем открыт экран выбора. Отдельное WAL-чтение не блокирует награды.
    reward_database: Mutex<Database>,
    // Сводка читает справочники независимо от длительного обновления рынка и OCR.
    world_database: Mutex<Database>,
    inventory_database: Mutex<Database>,
    game_names: Mutex<game_names::Cache>,
    inventory_refresh: inventory_refresh::InventoryRefreshService,
    market_data_service: MarketDataService,
    live_pricing_service: LivePricingService,
    history_service: HistoryService,
    game_metadata_service: GameMetadataService,
    resource_converter_service: ResourceConverterService,
    bounty_hunter_service: BountyHunterService,
    world_activity_service: WorldActivityService,
    account_service: AccountService,
    trade_reconciliation_lock: tokio::sync::Mutex<()>,
    read_only_inventory_scanner: Arc<ReadOnlyInventoryScanner>,
    reward_scan_in_flight: AtomicBool,
    reward_realtime_active: AtomicBool,
    dbwin_capture: Mutex<dbwin_capture::Recorder>,
    squad: Mutex<squad::Service>,
    memory_recording: Mutex<memory_recording::Recorder>,
    binary_recording: Mutex<binary_recording::Recorder>,
    mission_research: Mutex<mission_research::Service>,
    reward_relic_paths: Mutex<HashSet<String>>,
    latest_reward_scan: Mutex<Option<reward_ocr::RelicRewardScanView>>,
    reward_overlay_generation: AtomicU64,
    data_directory: PathBuf,
    _logging_guard: LoggingGuard,
}

const ACCOUNT_DEVICE_ID_KEY: &str = "account.device_id";
const BULK_REFRESH_CHECK_INTERVAL: Duration = Duration::from_secs(5 * 60);
const GAME_METADATA_REFRESH_HOURS: u16 = 24;
const MAX_MARKET_ITEMS_PER_OPEN: usize = 6;
const MAX_MARKET_SLUG_BYTES: usize = 96;
const REWARD_LOG_POLL_INTERVAL: Duration = Duration::from_millis(350);
const REWARD_LOG_READ_LIMIT: u64 = 256 * 1024;
const WFCD_IMAGE_BASE_URL: &str = "https://cdn.warframestat.us/img/";
const MARKET_THUMB_BASE_URL: &str = "https://warframe.market/static/assets/items/images/";
const COMPONENT_IMAGE_PROTOCOL: &str = "component-image";
const COMPONENT_IMAGE_CACHE_DIRECTORY: &str = "component-images";
const MAX_COMPONENT_IMAGE_BYTES: usize = 1024 * 1024;
static COMPONENT_IMAGE_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn hide_process_window(command: &mut Command) {
    command.creation_flags(CREATE_NO_WINDOW);
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
async fn scan_read_only_inventory(app: AppHandle) -> Result<InventoryView, String> {
    inventory_refresh::InventoryRefreshService::manual(&app).await
}

fn perform_inventory_scan(app: &AppHandle, pid: u32, epoch: u64) -> Result<InventoryView, String> {
    let state = app.state::<AppState>();
    let scan_result = state
        .read_only_inventory_scanner
        .scan(Some(pid), None)
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
    } = scan_result;
    if platscope_readonly_scan::scan::find_wf_pid() != Some(pid)
        || !state.inventory_refresh.session_is_current(pid, epoch)
    {
        return Err("inventory_session_changed".into());
    }
    let response_bytes = bytes.len();
    let raw_json = String::from_utf8(bytes)
        .map_err(|_| "Digital Extremes returned non-UTF-8 inventory JSON".to_owned())?;
    let settings = state
        .inventory_database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .get_setting::<AppSettings>(SETTINGS_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    let view = localize_inventory_images(
        InventoryService::import_read_only_scan_json(
            &state.inventory_database,
            &raw_json,
            &settings,
        )
        .map_err(|error| error.to_string())?,
    );
    // История не блокирует торговый инвентарь. Привязка к checksum не даст
    // показать старый аккаунт при смене снимка или неудачной записи кэша.
    if MasteryService::capture(
        &state.inventory_database,
        &raw_json,
        &scan_info.account_id,
        &view.metadata.checksum_sha256,
    )
    .is_err()
    {
        tracing::warn!(
            event = "mastery_cache_failed",
            "mastery history was not cached"
        );
    }
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
        "read-only Warframe inventory scan imported"
    );
    app.emit("inventory-updated", ())
        .map_err(|error| error.to_string())?;
    Ok(view)
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
fn load_mastery(state: State<'_, AppState>) -> Result<MasteryView, String> {
    MasteryService::view(&state.database)
        .map(|mut view| {
            for item in &mut view.items {
                localize_component_image_url(&mut item.image_url);
            }
            view
        })
        .map_err(|_| "unable to load saved mastery history".to_owned())
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
fn personal_goals(state: State<'_, AppState>) -> Result<PersonalGoalsView, String> {
    let settings = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .get_setting::<AppSettings>(SETTINGS_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    PersonalGoalsService::view(&state.database, &settings)
        .map(localize_personal_goal_images)
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)]
fn acknowledge_personal_goal_completions(
    completions: Vec<platscope_core::PersonalGoalCompletion>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    PersonalGoalsService::acknowledge_completions(&state.database, &completions)
        .map_err(|error| error.to_string())
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)]
fn set_personal_goal(
    set_slug: String,
    enabled: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    PersonalGoalsService::set_goal(&state.database, &set_slug, enabled)
        .map_err(|error| error.to_string())?;
    app.emit("inventory-updated", ())
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

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri deserializes command values by ownership.
fn market_history_batch(
    keys: Vec<MarketVariantKey>,
    state: State<'_, AppState>,
) -> Result<platscope_core::MarketAnalyticsBatch, String> {
    HistoryService::view_batch(&state.database, &keys).map_err(|error| error.to_string())
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
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
async fn world_activity(
    force_refresh: bool,
    state: State<'_, AppState>,
) -> Result<WorldActivityView, String> {
    state
        .world_activity_service
        .view(&state.world_database, force_refresh)
        .await
        .map(localize_world_activity_images)
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
async fn bounty_hunter(
    force_refresh: bool,
    state: State<'_, AppState>,
) -> Result<Option<BountyHunterView>, String> {
    let settings = state
        .database
        .try_lock()
        .map_err(|_| "market data is being updated; retry shortly".to_owned())?
        .get_setting::<AppSettings>(SETTINGS_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    state
        .bounty_hunter_service
        .view(&state.database, &settings, force_refresh)
        .await
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
    trade_reconciliation::spawn_pending_trade_reconciliation(app_handle);
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

// Общий резерв копий подтверждается предупреждением в интерфейсе.
// Ограничения обмена, надетые копии и личные цели остаются обязательными.
fn inventory_listing_quantity(item: &platscope_core::InventoryViewItem) -> u32 {
    if item.resolution != InventoryResolution::Resolved {
        return 0;
    }
    item.owned_quantity
        .saturating_sub(
            item.untradeable_quantity
                .saturating_add(item.unknown_quantity)
                .saturating_add(item.equipped_quantity),
        )
        .min(
            item.tradeable_quantity
                .saturating_sub(item.equipped_quantity),
        )
        .min(
            item.tradeable_quantity
                .saturating_sub(item.personal_reserved_quantity),
        )
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
            total.saturating_add(inventory_listing_quantity(item))
        });
    if available == 0 {
        return Err(
            "Этот точный вариант сейчас нельзя продать: проверьте наличие, возможность обмена и личные цели."
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
                total.saturating_add(inventory_listing_quantity(item))
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
            "Для продажи доступно полных комплектов: {available_sets}. Проверьте наличие деталей, личные цели и активные ордера."
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
    expected_order: Option<AccountOrder>,
    confirmed: bool,
    state: State<'_, AppState>,
) -> Result<AccountOrder, String> {
    input.validate().map_err(|error| error.to_string())?;
    let _trade_guard = state.trade_reconciliation_lock.lock().await;
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
        .update_listing_if_unchanged(
            &id,
            &input,
            expected_order.as_ref().or(Some(current)),
            confirmed,
        )
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
    expected_order: Option<AccountOrder>,
    confirmed: bool,
    state: State<'_, AppState>,
) -> Result<AccountOrder, String> {
    let _trade_guard = state.trade_reconciliation_lock.lock().await;
    let order = state
        .account_service
        .delete_listing_if_unchanged(&id, expected_order.as_ref(), confirmed)
        .await
        .map_err(|error| error.to_string())?;
    tracing::info!(
        event = "wfm_listing_deleted",
        "explicit WFM listing delete completed"
    );
    Ok(order)
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

fn reward_market_image_url(thumb: &str) -> String {
    if thumb.starts_with("https://") || thumb.starts_with("http://") {
        thumb.to_owned()
    } else {
        format!("https://warframe.market/static/assets/{thumb}")
    }
}

fn component_image_file_name(remote_url: &str) -> Option<&str> {
    let file_name = remote_url.strip_prefix(WFCD_IMAGE_BASE_URL)?;
    valid_image_file_name(file_name, "png").then_some(file_name)
}

fn valid_image_file_name(file_name: &str, extension: &str) -> bool {
    !file_name.is_empty()
        && file_name.len() <= 128
        && Path::new(file_name)
            .extension()
            .is_some_and(|actual| actual.eq_ignore_ascii_case(extension))
        && file_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn market_thumb_parts(remote_url: &str) -> Option<(&str, &str)> {
    let path = remote_url.strip_prefix(MARKET_THUMB_BASE_URL)?;
    let (locale, file_name) = path.split_once("/thumbs/")?;
    (matches!(locale, "en" | "ru")
        && (valid_image_file_name(file_name, "png") || valid_image_file_name(file_name, "webp")))
    .then_some((locale, file_name))
}

fn market_thumb_protocol_url(remote_url: &str) -> Option<String> {
    let (locale, file_name) = market_thumb_parts(remote_url)?;
    Some(format!(
        "http://{COMPONENT_IMAGE_PROTOCOL}.localhost/market/{locale}/{file_name}"
    ))
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

fn localize_personal_goal_images(mut view: PersonalGoalsView) -> PersonalGoalsView {
    for choice in &mut view.catalog {
        localize_component_image_url(&mut choice.image_url);
    }
    for goal in &mut view.goals {
        localize_component_image_url(&mut goal.set.image_url);
        for part in &mut goal.parts {
            localize_component_image_url(&mut part.image_url);
        }
    }
    view
}

fn localize_world_activity_images(mut view: WorldActivityView) -> WorldActivityView {
    for offer in view
        .baro_offers
        .iter_mut()
        .chain(&mut view.resurgence_offers)
    {
        localize_component_image_url(&mut offer.image_url);
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
    let path = request_path.trim_start_matches('/');
    let image = if let Some(market_path) = path.strip_prefix("market/") {
        market_path.split_once('/').and_then(|(locale, file_name)| {
            let remote_url = format!("{MARKET_THUMB_BASE_URL}{locale}/thumbs/{file_name}");
            market_thumb_parts(&remote_url)?;
            let cache_name = format!("market-{locale}-{file_name}");
            let content_type = if file_name.to_ascii_lowercase().ends_with(".webp") {
                "image/webp"
            } else {
                "image/png"
            };
            Some((remote_url, cache_name, content_type))
        })
    } else {
        component_image_file_name(&format!("{WFCD_IMAGE_BASE_URL}{path}"))
            .filter(|file_name| *file_name == path)
            .map(|file_name| {
                (
                    format!("{WFCD_IMAGE_BASE_URL}{file_name}"),
                    file_name.to_owned(),
                    "image/png",
                )
            })
    };
    let Some((remote_url, cache_name, content_type)) = image else {
        return component_image_http_response(
            tauri::http::StatusCode::BAD_REQUEST,
            b"invalid component image".to_vec(),
            "text/plain; charset=utf-8",
        );
    };
    match load_component_image(cache_directory, &cache_name, &remote_url, content_type) {
        Ok(image) => {
            component_image_http_response(tauri::http::StatusCode::OK, image, content_type)
        }
        Err(error) => {
            tracing::warn!(
                event = "component_image_load_failed",
                file_name = cache_name,
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
    let cache_control = if status.is_success() {
        "public, max-age=604800, immutable"
    } else {
        "no-store"
    };
    response.headers_mut().insert(
        tauri::http::header::CACHE_CONTROL,
        tauri::http::HeaderValue::from_static(cache_control),
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

fn load_component_image(
    cache_directory: &Path,
    file_name: &str,
    remote_url: &str,
    content_type: &str,
) -> Result<Vec<u8>, String> {
    fs::create_dir_all(cache_directory).map_err(|error| error.to_string())?;
    let cache_file = cache_directory.join(file_name);
    if let Ok(cached) = fs::read(&cache_file)
        && valid_component_image(&cached, content_type)
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
        .get(remote_url)
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
    if !valid_component_image(&image, content_type) {
        return Err("component image has an invalid format or size".to_owned());
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

fn valid_component_image(image: &[u8], content_type: &str) -> bool {
    match content_type {
        "image/png" => valid_component_png(image),
        "image/webp" => {
            image.len() >= 12
                && image.len() <= MAX_COMPONENT_IMAGE_BYTES
                && image.starts_with(b"RIFF")
                && image[8..12] == *b"WEBP"
                && u32::from_le_bytes([image[4], image[5], image[6], image[7]]) as usize + 8
                    == image.len()
        }
        _ => false,
    }
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

fn reset_log_consumers(app: &AppHandle) {
    if let Ok(mut service) = app.state::<AppState>().mission_research.lock() {
        service.reset_log();
    }
    app.state::<AppState>().inventory_refresh.reset_log();
    if let Ok(mut squad) = app.state::<AppState>().squad.lock() {
        squad.reset();
    }
}

#[allow(clippy::too_many_lines)]
fn spawn_game_log_watcher(app_handle: AppHandle) {
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
        let mut log_created = None;
        loop {
            interval.tick().await;
            let Ok(metadata) = fs::metadata(&path) else {
                if offset.is_some() {
                    reset_log_consumers(&app_handle);
                }
                offset = None;
                log_created = None;
                reward_live_from = None;
                tail.clear();
                trade_machine = trade_log::TradeMachine::default();
                trade_line_tail.clear();
                continue;
            };
            let file_len = metadata.len();
            let Some(current_offset) = offset else {
                offset = Some(0);
                log_created = metadata.created().ok();
                reward_live_from = Some(file_len);
                tracing::info!(
                    event = "trade_log_backfill_started",
                    bytes = file_len,
                    "reading the current EE.log session for completed trades"
                );
                continue;
            };
            if file_len < current_offset || metadata.created().ok() != log_created {
                reset_log_consumers(&app_handle);
                log_created = metadata.created().ok();
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
            if let Ok(mut service) = app_handle.state::<AppState>().mission_research.lock() {
                service.feed_log(&chunk);
                service.set_log_ready(new_offset == file_len);
            }
            if let Ok(mut squad) = app_handle.state::<AppState>().squad.lock() {
                squad.feed(&chunk);
                squad.set_log_ready(new_offset == file_len);
            }
            let now_ms = u64::try_from(watcher_started.elapsed().as_millis()).unwrap_or(u64::MAX);
            trade_reconciliation::handle_trade_log_chunk(
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
                    app_handle
                        .state::<AppState>()
                        .inventory_refresh
                        .feed(live_chunk);
                    reward_ocr::handle_reward_markers(
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

fn desktop_builder() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .on_window_event(|window, event| {
            dbwin_capture::on_close(window, event);
            memory_recording::on_close(window, event);
            binary_recording::on_close(window, event);
            mission_research::on_close(window, event);
            app_lifecycle::handle_window_event(window, event);
        })
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
}

/// Запускает Tauri shell и владеет жизненным циклом desktop-приложения.
///
/// # Panics
///
/// Завершает процесс, если Tauri runtime не может быть создан или запущен.
#[allow(clippy::too_many_lines)] // Единый список сервисов и команд оболочки.
pub fn run() {
    desktop_builder()
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
            let world_database = Database::open(&database_path)?;
            let inventory_database = Database::open(&database_path)?;
            let inventory_refresh =
                inventory_refresh::InventoryRefreshService::new(&inventory_database);
            let market_data_service = MarketDataService::production()?;
            let live_pricing_service = LivePricingService::production()?;
            let history_service = HistoryService::production()?;
            let game_metadata_service = GameMetadataService::production()?;
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

            let squad = squad::service(
                database
                    .get_setting::<bool>("squad.enabled.v1")?
                    .unwrap_or(true),
            );
            app.manage(AppState {
                database: Mutex::new(database),
                reward_database: Mutex::new(reward_database),
                world_database: Mutex::new(world_database),
                inventory_database: Mutex::new(inventory_database),
                game_names: Mutex::new(game_names::Cache::default()),
                inventory_refresh,
                market_data_service,
                live_pricing_service,
                history_service,
                game_metadata_service,
                resource_converter_service: ResourceConverterService::production()?,
                bounty_hunter_service: BountyHunterService::production()?,
                world_activity_service: WorldActivityService::production()?,
                account_service,
                trade_reconciliation_lock: tokio::sync::Mutex::new(()),
                read_only_inventory_scanner: Arc::new(ReadOnlyInventoryScanner::new()),
                reward_scan_in_flight: AtomicBool::new(false),
                reward_realtime_active: AtomicBool::new(false),
                dbwin_capture: Mutex::new(dbwin_capture::Recorder::default()),
                memory_recording: Mutex::new(memory_recording::Recorder::default()),
                binary_recording: Mutex::new(binary_recording::Recorder::default()),
                mission_research: Mutex::new(mission_research::Service::default()),
                squad,
                reward_relic_paths: Mutex::new(HashSet::new()),
                latest_reward_scan: Mutex::new(None),
                reward_overlay_generation: AtomicU64::new(0),
                data_directory,
                _logging_guard: logging_guard,
            });

            spawn_history_bootstrap(app.handle().clone());
            spawn_market_refresh_scheduler(app.handle().clone());
            trade_reconciliation::spawn_pending_trade_reconciliation(app.handle().clone());
            spawn_game_log_watcher(app.handle().clone());
            inventory_refresh::spawn(app.handle().clone());
            reward_ocr::spawn_reward_realtime_watcher(app.handle().clone());
            dbwin_capture::spawn_limit_check(app.handle().clone());
            squad::spawn(app.handle().clone());
            memory_recording::spawn(app.handle().clone());
            mission_research::spawn(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            foundation_status,
            mission_research::mission_research_status,
            mission_research::mission_research_scene,
            mission_research::mission_research_update,
            mission_research::mission_research_pose,
            mission_research::mission_research_archives,
            mission_research::mission_research_scan_live,
            mission_research::mission_research_analyze_archive,
            mission_research::mission_research_cancel,
            mission_research::mission_research_track,
            mission_research::mission_research_filters,
            mission_research::mission_research_export,
            squad::squad_status,
            memory_recording::memory_recording_status,
            memory_recording::memory_recording_start,
            memory_recording::memory_recording_stop,
            memory_recording::memory_recording_sample,
            memory_recording::memory_recording_folder,
            binary_recording::binary_recording_status,
            binary_recording::binary_recording_start,
            binary_recording::binary_recording_stop,
            binary_recording::binary_recording_sample,
            binary_recording::binary_recording_folder,
            squad::squad_read_configurations,
            squad::squad_set_enabled,
            squad::squad_refresh,
            squad::squad_save_build,
            squad::squad_edit_build,
            squad::squad_delete_build,
            squad::squad_export_text,
            diagnostics_status,
            dbwin_capture::dbwin_capture_status,
            dbwin_capture::start_dbwin_capture,
            dbwin_capture::stop_dbwin_capture,
            dbwin_capture::mark_dbwin_capture,
            dbwin_capture::open_dbwin_capture_folder,
            export_diagnostics_report,
            refresh_market_data,
            refresh_game_metadata,
            insights,
            resource_converter,
            bounty_hunter,
            world_activity,
            open_market_items,
            account_status,
            market_presence::account_presence,
            market_presence::account_set_presence,
            market_account::account_set_orders_visibility,
            market_profiles::open_trade_partner_profile,
            market_profiles::open_market_user_profile,
            market_profiles::market_trade_events,
            account_connect,
            account_disconnect,
            account_create_listing,
            account_update_listing,
            account_delete_listing,
            trade_reconciliation::trade_events,
            trade_reconciliation::trade_sales_summary,
            trade_reconciliation::trade_event_reconciled,
            trade_reconciliation::trade_event_ignore,
            trade_reconciliation::trade_event_restore,
            trade_reconciliation::trade_event_retry,
            search_market,
            reward_ocr::scan_relic_rewards,
            reward_ocr::preview_reward_overlay,
            reward_ocr::latest_relic_rewards,
            price_current_variant,
            live_price_current_variant,
            market_history,
            market_history_batch,
            bootstrap_history,
            scan_read_only_inventory,
            inventory_refresh::inventory_refresh_status,
            inventory_refresh::set_inventory_auto_refresh,
            load_inventory,
            load_mastery,
            set_inventory_keep_copies,
            personal_goals,
            set_personal_goal,
            acknowledge_personal_goal_completions,
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
                personal_reserved_quantity: 0,
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
            personal_reserved_quantity: 0,
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
    fn listing_validation_allows_keep_copies_but_protects_actual_stock() {
        let mut inventory = listing_inventory(0);
        inventory.keep_copies = 10;
        assert!(
            validate_sell_listing_inventory(
                &listing_intent(4, 1),
                &inventory,
                &[],
                None,
                None,
                &HashMap::new()
            )
            .is_ok()
        );
        assert!(
            validate_sell_listing_inventory(
                &listing_intent(5, 1),
                &inventory,
                &[],
                None,
                None,
                &HashMap::new()
            )
            .is_err()
        );
        // Изменяемый ордер не резервирует копии сам у себя.
        assert!(
            validate_sell_listing_inventory(
                &listing_intent(4, 1),
                &inventory,
                &[existing_sell_order(3)],
                Some("existing"),
                None,
                &HashMap::new()
            )
            .is_ok()
        );
        inventory.items[0].equipped_quantity = 1;
        assert_eq!(inventory_listing_quantity(&inventory.items[0]), 3);
        inventory.items[0].personal_reserved_quantity = 2;
        assert_eq!(inventory_listing_quantity(&inventory.items[0]), 2);
        inventory.items[0].resolution = InventoryResolution::AmbiguousItem;
        assert_eq!(inventory_listing_quantity(&inventory.items[0]), 0);
    }

    #[test]
    fn set_listing_validation_allows_parts_from_keep_copies() {
        let (mut inventory, intent, definition) = set_listing_fixture();
        for item in &mut inventory.items {
            item.sellable_quantity = 0;
        }
        assert!(
            validate_sell_listing_inventory(
                &intent,
                &inventory,
                &[],
                None,
                Some(&definition),
                &HashMap::new()
            )
            .is_ok()
        );
        inventory.items[0].personal_reserved_quantity = 1;
        assert!(
            validate_sell_listing_inventory(
                &intent,
                &inventory,
                &[],
                None,
                Some(&definition),
                &HashMap::new()
            )
            .is_err()
        );
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
                &listing_intent(3, 1),
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
    fn component_images_use_a_validated_local_protocol() {
        let remote = "https://cdn.warframestat.us/img/GenericGunPrimeBarrel.png";
        let local = component_image_protocol_url(remote).expect("known CDN image is accepted");
        assert!(local.ends_with("/GenericGunPrimeBarrel.png"));
        assert!(local.contains(COMPONENT_IMAGE_PROTOCOL));
        assert!(component_image_protocol_url("https://example.com/barrel.png").is_none());
        assert!(
            component_image_protocol_url("https://cdn.warframestat.us/img/../secret.png").is_none()
        );
        let market = "https://warframe.market/static/assets/items/images/en/thumbs/intensify.123.128x128.webp";
        assert_eq!(
            market_thumb_protocol_url(market).as_deref(),
            Some("http://component-image.localhost/market/en/intensify.123.128x128.webp")
        );
        assert!(
            market_thumb_protocol_url("https://evil.example/items/images/en/thumbs/mod.webp")
                .is_none()
        );
        assert!(
            market_thumb_protocol_url(
                "https://warframe.market/static/assets/items/images/en/thumbs/../mod.webp"
            )
            .is_none()
        );

        let invalid = component_image_response(Path::new("unused"), "/../secret.png");
        assert_eq!(invalid.status(), tauri::http::StatusCode::BAD_REQUEST);
        let invalid_market =
            component_image_response(Path::new("unused"), "/market/en/../mod.webp");
        assert_eq!(
            invalid_market.status(),
            tauri::http::StatusCode::BAD_REQUEST
        );
        assert!(valid_component_png(&[
            0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A
        ]));
        assert!(!valid_component_png(b"not a png"));
        assert!(valid_component_image(b"RIFF\x04\0\0\0WEBP", "image/webp"));
        assert!(!valid_component_image(b"RIFF\x05\0\0\0WEBP", "image/webp"));
    }
}
