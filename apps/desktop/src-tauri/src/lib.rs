#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use platscope_core::{
    AccountOrder, AccountService, AccountView, AppSettings, CreateListingInput,
    DEFAULT_MARKET_SEARCH_LIMIT, GameMetadataRefreshOutcome, GameMetadataService,
    HistoryBootstrapOutcome, HistoryService, InsightsService, InsightsView, InventoryService,
    InventoryView, LivePricingResult, LivePricingService, LiveSellNowResult, LoggingGuard,
    MarketBrowserService, MarketDataService, MarketHistoryView, MarketRefreshOutcome,
    MarketSearchResult, PriceRecommendation, PricingService, SETTINGS_KEY, SellNowService,
    SellNowView, UpdateListingInput, enrich_account_view, init_logging,
};
use platscope_domain::{MarketItemKind, MarketVariantKey};
use platscope_readonly_scan::inventory::InventoryScanner as ReadOnlyInventoryScanner;
use platscope_storage::{Database, HistoryCoverage, MarketSnapshotSummary, ProviderHealth};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

struct AppState {
    database: Mutex<Database>,
    market_data_service: MarketDataService,
    live_pricing_service: LivePricingService,
    history_service: HistoryService,
    game_metadata_service: GameMetadataService,
    account_service: AccountService,
    read_only_inventory_scanner: Arc<ReadOnlyInventoryScanner>,
    companion_import_tracker: Mutex<CompanionImportTracker>,
    data_directory: PathBuf,
    _logging_guard: LoggingGuard,
}

const ACCOUNT_DEVICE_ID_KEY: &str = "account.device_id";
const MAX_COMPANION_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_COMPANION_PATH_BYTES: usize = 4_096;
const COMPANION_POLL_INTERVAL: Duration = Duration::from_secs(3);
const BULK_REFRESH_CHECK_INTERVAL: Duration = Duration::from_secs(5 * 60);
const GAME_METADATA_REFRESH_HOURS: u16 = 24;
const COMPANION_MANUAL_STABILITY_DELAY: Duration = Duration::from_millis(750);

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompanionFileSample {
    path: PathBuf,
    bytes: u64,
    modified_at: SystemTime,
}

#[derive(Debug, Default)]
struct CompanionImportTracker {
    path: Option<PathBuf>,
    candidate: Option<CompanionFileSample>,
    attempted: Option<CompanionFileSample>,
    last_imported_at: Option<DateTime<Utc>>,
    last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum CompanionImportState {
    Disabled,
    NeedsPath,
    Missing,
    Stabilizing,
    UpToDate,
    Imported,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CompanionImportStatus {
    state: CompanionImportState,
    last_imported_at: Option<DateTime<Utc>>,
    last_error: Option<String>,
}

#[derive(Debug)]
struct CompanionPollOutcome {
    status: CompanionImportStatus,
    imported: bool,
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

#[tauri::command]
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

#[tauri::command]
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

#[tauri::command]
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

fn companion_status(
    tracker: &CompanionImportTracker,
    state: CompanionImportState,
) -> CompanionImportStatus {
    CompanionImportStatus {
        state,
        last_imported_at: tracker.last_imported_at,
        last_error: (state == CompanionImportState::Error)
            .then(|| tracker.last_error.clone())
            .flatten(),
    }
}

fn compact_companion_error(error: impl std::fmt::Display) -> String {
    error.to_string().chars().take(240).collect()
}

fn configured_companion_path(settings: &AppSettings) -> Result<Option<PathBuf>, &'static str> {
    if !settings.inventory_companion_enabled {
        return Ok(None);
    }
    let Some(raw_path) = settings
        .inventory_companion_path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
    else {
        return Err("Choose an inventory JSON file before enabling automatic import.");
    };
    if raw_path.len() > MAX_COMPANION_PATH_BYTES {
        return Err("The companion file path is too long.");
    }
    let path = PathBuf::from(raw_path);
    if !path.is_absolute() {
        return Err("Use an absolute path to the companion inventory JSON file.");
    }
    Ok(Some(path))
}

fn reset_companion_tracker(tracker: &mut CompanionImportTracker, path: Option<PathBuf>) {
    if tracker.path != path {
        tracker.path = path;
        tracker.candidate = None;
        tracker.attempted = None;
        tracker.last_imported_at = None;
        tracker.last_error = None;
    }
}

fn read_companion_file_sample(path: &Path) -> Result<Option<CompanionFileSample>, String> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(compact_companion_error(error)),
    };
    if !metadata.is_file() {
        return Err("The configured path is not a file.".into());
    }
    if metadata.len() > MAX_COMPANION_FILE_BYTES {
        return Err("The companion inventory file exceeds 8 MiB.".into());
    }
    let modified_at = metadata.modified().map_err(compact_companion_error)?;
    Ok(Some(CompanionFileSample {
        path: path.to_owned(),
        bytes: metadata.len(),
        modified_at,
    }))
}

fn poll_companion_file(
    settings: &AppSettings,
    tracker: &mut CompanionImportTracker,
    mut importer: impl FnMut(&str) -> Result<(), String>,
) -> CompanionPollOutcome {
    let path = match configured_companion_path(settings) {
        Ok(None) => {
            reset_companion_tracker(tracker, None);
            return CompanionPollOutcome {
                status: companion_status(tracker, CompanionImportState::Disabled),
                imported: false,
            };
        }
        Err(error) => {
            reset_companion_tracker(tracker, None);
            tracker.last_error = Some(error.into());
            let state = if settings
                .inventory_companion_path
                .as_deref()
                .is_none_or(|path| path.trim().is_empty())
            {
                CompanionImportState::NeedsPath
            } else {
                CompanionImportState::Error
            };
            return CompanionPollOutcome {
                status: companion_status(tracker, state),
                imported: false,
            };
        }
        Ok(Some(path)) => path,
    };
    reset_companion_tracker(tracker, Some(path.clone()));

    let sample = match read_companion_file_sample(&path) {
        Ok(None) => {
            tracker.candidate = None;
            tracker.attempted = None;
            tracker.last_error = None;
            return CompanionPollOutcome {
                status: companion_status(tracker, CompanionImportState::Missing),
                imported: false,
            };
        }
        Err(error) => {
            tracker.last_error = Some(error);
            return CompanionPollOutcome {
                status: companion_status(tracker, CompanionImportState::Error),
                imported: false,
            };
        }
        Ok(Some(sample)) => sample,
    };

    if tracker.candidate.as_ref() != Some(&sample) {
        tracker.candidate = Some(sample);
        tracker.last_error = None;
        return CompanionPollOutcome {
            status: companion_status(tracker, CompanionImportState::Stabilizing),
            imported: false,
        };
    }
    if tracker.attempted.as_ref() == Some(&sample) {
        let state = if tracker.last_error.is_some() {
            CompanionImportState::Error
        } else {
            CompanionImportState::UpToDate
        };
        return CompanionPollOutcome {
            status: companion_status(tracker, state),
            imported: false,
        };
    }

    let result = fs::read_to_string(&sample.path)
        .map_err(compact_companion_error)
        .and_then(|raw| importer(&raw));
    tracker.attempted = Some(sample);
    match result {
        Ok(()) => {
            tracker.last_imported_at = Some(Utc::now());
            tracker.last_error = None;
            CompanionPollOutcome {
                status: companion_status(tracker, CompanionImportState::Imported),
                imported: true,
            }
        }
        Err(error) => {
            tracker.last_error = Some(compact_companion_error(error));
            CompanionPollOutcome {
                status: companion_status(tracker, CompanionImportState::Error),
                imported: false,
            }
        }
    }
}

fn poll_companion_inventory(state: &AppState) -> Result<CompanionPollOutcome, String> {
    let settings = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .get_setting::<AppSettings>(SETTINGS_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    let mut tracker = state
        .companion_import_tracker
        .lock()
        .map_err(|_| "companion import state is unavailable".to_owned())?;
    Ok(poll_companion_file(&settings, &mut tracker, |raw| {
        let view = InventoryService::import_companion_json(&state.database, raw, &settings)
            .map_err(|error| error.to_string())?;
        tracing::info!(
            event = "companion_inventory_imported",
            source_rows = view.metadata.item_count,
            resolved_rows = view.summary.resolved_rows,
            attention_rows = view.summary.attention_rows,
            "stable companion inventory snapshot imported"
        );
        Ok(())
    }))
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
fn companion_inventory_status(state: State<'_, AppState>) -> Result<CompanionImportStatus, String> {
    let settings = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .get_setting::<AppSettings>(SETTINGS_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    let mut tracker = state
        .companion_import_tracker
        .lock()
        .map_err(|_| "companion import state is unavailable".to_owned())?;
    let state = match configured_companion_path(&settings) {
        Ok(None) => {
            reset_companion_tracker(&mut tracker, None);
            CompanionImportState::Disabled
        }
        Err(_)
            if settings
                .inventory_companion_path
                .as_deref()
                .is_none_or(|path| path.trim().is_empty()) =>
        {
            reset_companion_tracker(&mut tracker, None);
            CompanionImportState::NeedsPath
        }
        Err(error) => {
            reset_companion_tracker(&mut tracker, None);
            tracker.last_error = Some(error.into());
            CompanionImportState::Error
        }
        Ok(Some(path)) if tracker.path.as_ref() != Some(&path) => {
            reset_companion_tracker(&mut tracker, Some(path));
            CompanionImportState::Stabilizing
        }
        Ok(Some(_)) if tracker.last_error.is_some() => CompanionImportState::Error,
        Ok(Some(_)) if tracker.last_imported_at.is_some() => CompanionImportState::UpToDate,
        Ok(Some(_)) => CompanionImportState::Stabilizing,
    };
    Ok(companion_status(&tracker, state))
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State and AppHandle.
async fn check_companion_inventory(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<CompanionImportStatus, String> {
    let mut outcome = poll_companion_inventory(&state)?;
    if outcome.status.state == CompanionImportState::Stabilizing {
        tokio::time::sleep(COMPANION_MANUAL_STABILITY_DELAY).await;
        outcome = poll_companion_inventory(&state)?;
    }
    if outcome.imported {
        app.emit("inventory-updated", ())
            .map_err(|error| error.to_string())?;
    }
    Ok(outcome.status)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri deserializes command values by ownership.
fn import_inventory_json(
    raw_json: String,
    state: State<'_, AppState>,
) -> Result<InventoryView, String> {
    let settings = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .get_setting::<AppSettings>(SETTINGS_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    let view = InventoryService::import_json(&state.database, &raw_json, &settings)
        .map_err(|error| error.to_string())?;
    tracing::info!(
        event = "inventory_import_finished",
        source_rows = view.metadata.item_count,
        resolved_rows = view.summary.resolved_rows,
        attention_rows = view.summary.attention_rows,
        "local inventory import finished"
    );
    Ok(view)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri owns command state and app handle.
async fn scan_read_only_inventory(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<InventoryView, String> {
    let scanner = Arc::clone(&state.read_only_inventory_scanner);
    let (bytes, scan_info) = tauri::async_runtime::spawn_blocking(move || scanner.scan(None, None))
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
    let view = InventoryService::import_read_only_scan_json(&state.database, &raw_json, &settings)
        .map_err(|error| error.to_string())?;
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

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
fn load_inventory(state: State<'_, AppState>) -> Result<Option<InventoryView>, String> {
    let settings = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .get_setting::<AppSettings>(SETTINGS_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    InventoryService::view(&state.database, &settings).map_err(|error| error.to_string())
}

#[tauri::command]
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
    InventoryService::view(&state.database, &settings).map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
fn sell_now(state: State<'_, AppState>) -> Result<Option<SellNowView>, String> {
    let settings = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .get_setting::<AppSettings>(SETTINGS_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    SellNowService::view(&state.database, &settings).map_err(|error| error.to_string())
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
    .map_err(|error| error.to_string())
}

#[tauri::command]
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
        .bootstrap(&state.database)
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

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
fn insights(state: State<'_, AppState>) -> Result<Option<InsightsView>, String> {
    let settings = state
        .database
        .lock()
        .map_err(|_| "database state is unavailable".to_owned())?
        .get_setting::<AppSettings>(SETTINGS_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    InsightsService::view(&state.database, &settings).map_err(|error| error.to_string())
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
    enrich_account_view(&state.database, settings.language, view).map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri deserializes credentials by ownership.
async fn account_connect(
    email: String,
    password: String,
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
        .map_err(|error| error.to_string())?;
    tracing::info!(event = "wfm_account_connected", "WFM account connected");
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

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri deserializes command values by ownership.
async fn account_create_listing(
    input: CreateListingInput,
    confirmed: bool,
    state: State<'_, AppState>,
) -> Result<AccountOrder, String> {
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
        .price_current_variant(&state.database, &key, item_kind, &settings)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri deserializes command values by ownership.
fn price_current_variant(
    key: MarketVariantKey,
    item_kind: MarketItemKind,
    state: State<'_, AppState>,
) -> Result<Option<PriceRecommendation>, String> {
    PricingService::price_current_variant(&state.database, &key, item_kind)
        .map_err(|error| error.to_string())
}

#[tauri::command]
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
    .map_err(|error| error.to_string())
}

#[tauri::command]
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

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri deserializes the command body by value.
fn save_settings(mut settings: AppSettings, state: State<'_, AppState>) -> Result<(), String> {
    validate_app_settings(&settings).map_err(str::to_owned)?;
    configured_companion_path(&settings).map_err(str::to_owned)?;
    settings.inventory_companion_path = settings
        .inventory_companion_path
        .take()
        .map(|path| path.trim().to_owned())
        .filter(|path| !path.is_empty());
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

fn spawn_companion_poller(app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(COMPANION_POLL_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let state = app_handle.state::<AppState>();
            match poll_companion_inventory(&state) {
                Ok(outcome) if outcome.imported => {
                    if let Err(error) = app_handle.emit("inventory-updated", ()) {
                        tracing::warn!(
                            event = "companion_inventory_event_failed",
                            error = %error,
                            "companion inventory imported but UI event failed"
                        );
                    }
                }
                Ok(_) => {}
                Err(error) => tracing::warn!(
                    event = "companion_inventory_poll_failed",
                    error = %error,
                    "companion inventory poll failed without replacing LKG"
                ),
            }
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
        .setup(|app| {
            let data_directory = app.path().local_data_dir()?.join("PlatScope");
            fs::create_dir_all(&data_directory)?;
            let logging_guard = init_logging(&data_directory.join("logs"))?;
            let database = Database::open(data_directory.join("platscope.db"))?;
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

            app.manage(AppState {
                database: Mutex::new(database),
                market_data_service,
                live_pricing_service,
                history_service,
                game_metadata_service,
                account_service,
                read_only_inventory_scanner: Arc::new(ReadOnlyInventoryScanner::new()),
                companion_import_tracker: Mutex::new(CompanionImportTracker::default()),
                data_directory,
                _logging_guard: logging_guard,
            });

            spawn_history_bootstrap(app.handle().clone());
            spawn_companion_poller(app.handle().clone());
            spawn_market_refresh_scheduler(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            foundation_status,
            diagnostics_status,
            export_diagnostics_report,
            refresh_market_data,
            refresh_game_metadata,
            insights,
            account_status,
            account_connect,
            account_disconnect,
            account_create_listing,
            account_update_listing,
            account_delete_listing,
            search_market,
            price_current_variant,
            live_price_current_variant,
            market_history,
            bootstrap_history,
            import_inventory_json,
            scan_read_only_inventory,
            companion_inventory_status,
            check_companion_inventory,
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
    use std::cell::Cell;

    fn companion_test_settings(path: &Path) -> AppSettings {
        AppSettings {
            inventory_companion_enabled: true,
            inventory_companion_path: Some(path.display().to_string()),
            ..AppSettings::default()
        }
    }

    fn temporary_companion_file(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is valid")
            .as_nanos();
        std::env::temp_dir().join(format!("platscope-{name}-{suffix}.json"))
    }

    #[test]
    fn old_settings_default_to_disabled_companion_import() {
        let settings: AppSettings = serde_json::from_str(
            r#"{"language":"russian","platform":"pc","crossplay":true,"bulk_refresh_hours":4,"live_quote_ttl_seconds":90,"keep_inventory_copies":1}"#,
        )
        .expect("old settings remain compatible");
        assert!(!settings.inventory_companion_enabled);
        assert!(settings.inventory_companion_path.is_none());
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
    }

    #[test]
    fn companion_path_must_be_absolute_when_enabled() {
        let settings = companion_test_settings(Path::new("relative/inventory.json"));
        assert!(configured_companion_path(&settings).is_err());
    }

    #[test]
    fn companion_poll_waits_for_stability_and_imports_once() {
        let path = temporary_companion_file("stable");
        fs::write(&path, r#"{"complete":true}"#).expect("fixture is written");
        let settings = companion_test_settings(&path);
        let mut tracker = CompanionImportTracker::default();
        let imports = Cell::new(0_u32);

        let first = poll_companion_file(&settings, &mut tracker, |_| {
            imports.set(imports.get() + 1);
            Ok(())
        });
        assert_eq!(first.status.state, CompanionImportState::Stabilizing);
        assert_eq!(imports.get(), 0);

        let second = poll_companion_file(&settings, &mut tracker, |_| {
            imports.set(imports.get() + 1);
            Ok(())
        });
        assert_eq!(second.status.state, CompanionImportState::Imported);
        assert!(second.imported);
        assert_eq!(imports.get(), 1);

        let third = poll_companion_file(&settings, &mut tracker, |_| {
            imports.set(imports.get() + 1);
            Ok(())
        });
        assert_eq!(third.status.state, CompanionImportState::UpToDate);
        assert_eq!(imports.get(), 1);
        fs::remove_file(path).expect("fixture is removed");
    }

    #[test]
    fn failed_companion_sample_is_not_retried_until_it_changes() {
        let path = temporary_companion_file("invalid");
        fs::write(&path, "invalid").expect("fixture is written");
        let settings = companion_test_settings(&path);
        let mut tracker = CompanionImportTracker::default();
        let attempts = Cell::new(0_u32);

        let _ = poll_companion_file(&settings, &mut tracker, |_| Ok(()));
        let failed = poll_companion_file(&settings, &mut tracker, |_| {
            attempts.set(attempts.get() + 1);
            Err("schema drift".into())
        });
        assert_eq!(failed.status.state, CompanionImportState::Error);
        let repeated = poll_companion_file(&settings, &mut tracker, |_| {
            attempts.set(attempts.get() + 1);
            Err("must not retry".into())
        });
        assert_eq!(repeated.status.state, CompanionImportState::Error);
        assert_eq!(attempts.get(), 1);
        fs::remove_file(path).expect("fixture is removed");
    }

    #[test]
    fn diagnostic_report_omits_local_path_and_credentials() {
        let status = DiagnosticsStatus {
            generated_at: Utc::now(),
            foundation: FoundationStatus {
                app_name: "PlatScope",
                app_version: "0.1.0",
                database_path: r"C:\Users\SecretName\AppData\Local\PlatScope\platscope.db".into(),
                schema_version: 9,
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
}
