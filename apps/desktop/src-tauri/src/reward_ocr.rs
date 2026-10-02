//! Распознавание наград, данные оверлея и наблюдение за экраном выбора.

mod process;

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use platscope_core::{
    AppSettings, InsightsService, InsightsView, InventoryService, InventoryView, MarketSearchRow,
    PricingService, SETTINGS_KEY, SetComponentInsight,
};
use platscope_domain::{
    GameMetadataSnapshot, MarketItemKind, MarketVariantKey, PriceConfidence, PrimeSetDefinition,
    VaultStatus,
};
use platscope_storage::Database;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State};

use crate::{
    AppState, component_image_protocol_url, hide_process_window, reward_market_image_url,
    validate_app_settings,
};

const REWARD_OCR_EXECUTABLE: &str = "platscope-reward-ocr.exe";
const REWARD_OCR_TIMEOUT: Duration = Duration::from_secs(15);
const WARFRAME_WINDOW_TIMEOUT: Duration = Duration::from_secs(5);
const REWARD_LOG_DEBOUNCE: Duration = Duration::from_secs(8);
const REWARD_OVERLAY_VISIBLE_FOR: Duration = Duration::from_secs(18);
const MIN_REWARD_RECOMMENDATION_OCR_CONFIDENCE: f64 = 0.75;

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
    debug_capture_supported: Option<bool>,
    process_id: Option<u32>,
    received_at: Option<DateTime<Utc>>,
    message: Option<String>,
    raw_base64: Option<String>,
}

struct RewardScanGuard<'a>(&'a AtomicBool);

impl<'a> RewardScanGuard<'a> {
    fn acquire(in_flight: &'a AtomicBool) -> Option<Self> {
        in_flight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self(in_flight))
    }
}

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
    vault_status: VaultStatus,
    set: Option<RewardSetOverview>,
    completes_set: Option<RewardSetCompletion>,
    choice_value: Option<f64>,
    recommended: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RelicRewardScanView {
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
pub(crate) struct WarframeWindowRect {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RewardOverlayGeometry {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State and AppHandle.
pub(crate) async fn scan_relic_rewards(
    image_path: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RelicRewardScanView, String> {
    let total_started = Instant::now();
    let Some(_scan_guard) = RewardScanGuard::acquire(&state.reward_scan_in_flight) else {
        tracing::info!(
            event = "reward_scan_skipped",
            reason = "already_in_flight",
            "duplicate reward OCR request skipped"
        );
        return Err("Распознавание наград уже выполняется.".to_owned());
    };
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
    let response = run_reward_ocr_process(&executable, &request).await?;
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
    publish_reward_scan(&app, &state, &settings, &view).await?;
    Ok(view)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri deserializes settings and injects handles by ownership.
pub(crate) async fn preview_reward_overlay(
    settings: AppSettings,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RelicRewardScanView, String> {
    validate_app_settings(&settings).map_err(str::to_owned)?;
    let rect = warframe_window_rect(&app).await?;
    let insights = InsightsService::view(&state.reward_database, &settings)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Сначала обновите данные предметов в настройках.".to_owned())?;
    let response = build_reward_preview_response(&insights, &rect)?;
    let view = build_reward_scan_view(&state, &settings, response)?;
    present_reward_scan(&app, &state, &settings, &view).await?;
    Ok(view)
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri command extractor owns State.
pub(crate) fn latest_relic_rewards(
    state: State<'_, AppState>,
) -> Result<Option<RelicRewardScanView>, String> {
    state
        .latest_reward_scan
        .lock()
        .map(|view| view.clone())
        .map_err(|_| "reward overlay state is unavailable".to_owned())
}

async fn publish_reward_scan(
    app: &AppHandle,
    state: &AppState,
    settings: &AppSettings,
    view: &RelicRewardScanView,
) -> Result<(), String> {
    *state
        .latest_reward_scan
        .lock()
        .map_err(|_| "reward overlay state is unavailable".to_owned())? = Some(view.clone());
    present_reward_scan(app, state, settings, view).await
}

async fn present_reward_scan(
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
    if let Err(error) = show_reward_overlay(app, settings, view.rewards.len(), max_set_parts).await
    {
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
    // Список деталей полных сетов не равен списку наград: например, рецепт
    // Акбронко может не собраться из каталога, но его чертёж выпадает из реликвий.
    // Используем все известные награды, в том числе из реликвий других игроков.
    let reward_slugs: HashSet<_> = metadata
        .relics
        .iter()
        .flat_map(|relic| &relic.rewards)
        .filter_map(|reward| reward.reward_slug.as_deref())
        .chain(metadata.prime_parts.iter().map(|part| part.slug.as_str()))
        .collect();
    let mut result = Vec::new();
    for item in market_catalog.items.into_iter().filter(|item| {
        reward_slugs.contains(item.slug.as_str())
            || (item.tags.iter().any(|tag| tag == "prime")
                && item
                    .tags
                    .iter()
                    .any(|tag| tag == "component" || tag == "blueprint")
                && !item.tags.iter().any(|tag| tag == "set"))
    }) {
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
    let vault_status = reward.slug.as_deref().map_or(VaultStatus::Unknown, |slug| {
        reward_vault_status(metadata, slug)
    });
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
        vault_status,
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
            display_name_en: String::new(),
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

fn reward_vault_status(metadata: Option<&GameMetadataSnapshot>, reward_slug: &str) -> VaultStatus {
    let Some(metadata) = metadata else {
        return VaultStatus::Unknown;
    };
    let set_statuses = metadata
        .prime_sets
        .iter()
        .filter(|set| {
            set.components
                .iter()
                .any(|component| component.slug == reward_slug)
        })
        .map(|set| set.vault_status);
    let part_statuses = metadata
        .prime_parts
        .iter()
        .filter(|part| part.slug == reward_slug)
        .map(|part| part.vault_status);
    let mut status = None;
    for candidate in set_statuses.chain(part_statuses) {
        match status {
            Some(current) if current != candidate => return VaultStatus::Unknown,
            None => status = Some(candidate),
            _ => {}
        }
    }
    status.unwrap_or(VaultStatus::Unknown)
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

pub(crate) async fn warframe_window_rect(app: &AppHandle) -> Result<WarframeWindowRect, String> {
    let executable = find_reward_ocr_executable(app)?;
    let mut command = tokio::process::Command::new(&executable);
    command
        .current_dir(executable.parent().unwrap_or_else(|| Path::new(".")))
        .arg("--warframe-window-rect");
    let output = process::run(&mut command, None, WARFRAME_WINDOW_TIMEOUT)
        .await
        .map_err(|error| format!("Не удалось определить окно Warframe: {error}"))?;
    serde_json::from_slice(&output.stdout).map_err(|error| {
        let stderr = String::from_utf8_lossy(&output.stderr);
        format!("Окно Warframe недоступно: {error}. {stderr}")
    })
}

async fn show_reward_overlay(
    app: &AppHandle,
    settings: &AppSettings,
    cards: usize,
    max_set_parts: usize,
) -> Result<(), String> {
    let rect = warframe_window_rect(app).await?;
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

async fn run_reward_ocr_process(
    executable: &Path,
    request: &RewardOcrRequest,
) -> Result<RewardOcrResponse, String> {
    let mut command = tokio::process::Command::new(executable);
    command.current_dir(executable.parent().unwrap_or_else(|| Path::new(".")));
    let payload = serde_json::to_vec(request).map_err(|error| error.to_string())?;
    let output = process::run(&mut command, Some(payload), REWARD_OCR_TIMEOUT)
        .await
        .map_err(|error| match error {
            process::ProcessFailure::TimedOut { .. } => {
                tracing::warn!(event = "reward_ocr_timeout", timeout_seconds = REWARD_OCR_TIMEOUT.as_secs(), "OCR-помощник остановлен по тайм-ауту");
                "Распознавание наград не завершилось за 15 секунд. OCR-помощник остановлен; повторите попытку.".to_owned()
            }
            error => format!("Не удалось выполнить распознавание наград: {error}"),
        })?;
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

pub(crate) fn handle_reward_markers(
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

pub(crate) fn spawn_reward_realtime_watcher(app_handle: AppHandle) {
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
            if event.event_type == "relic_selection" {
                if let Ok(trigger) = serde_json::from_str(&line) {
                    crate::relic_selection_overlay::handle_trigger(&app_handle, trigger);
                }
                continue;
            }
            handle_reward_trigger_event(&app_handle, event);
        }

        app_handle
            .state::<AppState>()
            .reward_realtime_active
            .store(false, Ordering::Release);
        crate::relic_selection_overlay::stop(&app_handle);
        if let Ok(mut recorder) = app_handle.state::<AppState>().dbwin_capture.lock() {
            recorder.disconnected();
        }
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
        .arg("--forward-debug-lines")
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
            if let Ok(mut recorder) = app_handle.state::<AppState>().dbwin_capture.lock() {
                recorder.connected(
                    event.debug_capture_supported.unwrap_or(false),
                    event.already_exists.unwrap_or(false),
                );
            }
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
        "debug_line" => {
            if let (Some(pid), Some(received_at), Some(message), Some(raw)) = (
                event.process_id,
                event.received_at,
                event.message,
                event.raw_base64,
            ) && let Ok(mut recorder) = app_handle.state::<AppState>().dbwin_capture.lock()
            {
                recorder.record(pid, received_at, &message, &raw);
            }
        }
        "reward" => {
            crate::relic_selection_overlay::close(app_handle);
            app_handle.state::<AppState>().inventory_refresh.after_relic_round();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ocr_timeout_kills_helper_and_allows_next_scan() {
        let system_directory =
            PathBuf::from(std::env::var_os("SystemRoot").expect("Windows")).join("System32");
        let powershell = system_directory.join("WindowsPowerShell/v1.0/powershell.exe");
        let in_flight = AtomicBool::new(false);
        // Проверяем и зависание после получения запроса, и помощник, который
        // вообще не читает stdin: большой каталог заполняет системный канал.
        let cases = [
            (
                "[void][Console]::In.ReadToEnd(); Start-Sleep -Seconds 60",
                vec![b'x'],
            ),
            ("Start-Sleep -Seconds 60", vec![b'x'; 2 * 1024 * 1024]),
        ];
        for (script, input) in cases {
            let pid = {
                let _guard = RewardScanGuard::acquire(&in_flight).expect("Первый запуск разрешён");
                assert!(RewardScanGuard::acquire(&in_flight).is_none());
                let mut command = tokio::process::Command::new(&powershell);
                command.args(["-NoProfile", "-NonInteractive", "-Command", script]);
                let started = Instant::now();
                let error = process::run(&mut command, Some(input), Duration::from_millis(750))
                    .await
                    .expect_err("Зависший помощник должен завершиться по тайм-ауту");
                assert!(started.elapsed() < Duration::from_secs(6));
                let process::ProcessFailure::TimedOut { pid: Some(pid) } = error else {
                    panic!("Ожидался тайм-аут запущенного процесса: {error}");
                };
                pid
            };
            assert!(!in_flight.load(Ordering::Acquire));
            // Проверяем завершение настоящего дочернего процесса, а не только
            // возврат ошибки ожидания. Никакие окна приложения не открываются.
            let mut query = tokio::process::Command::new(system_directory.join("tasklist.exe"));
            query.args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"]);
            let processes = process::run(&mut query, None, Duration::from_secs(5))
                .await
                .expect("Список процессов доступен");
            assert!(processes.status.success());
            assert!(
                !String::from_utf8_lossy(&processes.stdout)
                    .lines()
                    .any(|line| line.split(',').nth(1) == Some(format!("\"{pid}\"").as_str()))
            );

            let _guard = RewardScanGuard::acquire(&in_flight).expect("Повторный запуск разрешён");
            let mut next = tokio::process::Command::new(&powershell);
            next.args(["-NoProfile", "-NonInteractive", "-Command",
                "[void][Console]::In.ReadToEnd(); [Console]::Out.Write('{\"status\":\"ok\",\"rewards\":[]}')"]);
            let output = process::run(&mut next, Some(b"{}".to_vec()), Duration::from_secs(5))
                .await
                .expect("Следующий помощник успешно отвечает");
            let response: RewardOcrResponse = serde_json::from_slice(&output.stdout)
                .expect("Следующий ответ читается штатным протоколом OCR");
            assert!(output.status.success());
            assert_eq!(response.status, "ok");
        }
    }

    #[test]
    fn reward_log_markers_match_current_warframe_messages() {
        assert!(!reward_log_contains_reward_screen(
            "Script [Info]: VoidProjections: OpenVoidProjectionRewardScreen"
        ));
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
    fn reward_ocr_catalog_accepts_only_non_empty_russian_names() {
        assert_eq!(
            russian_reward_ocr_name(Some("  Ивара Прайм: Каркас  ".into())).as_deref(),
            Some("Ивара Прайм: Каркас")
        );
        assert_eq!(russian_reward_ocr_name(Some("   ".into())), None);
        assert_eq!(russian_reward_ocr_name(None), None);
    }

    #[test]
    fn reward_ocr_catalog_keeps_rewards_missing_from_complete_prime_sets() {
        let missing_rewards = [
            ("akbronco_prime_blueprint", "Акбронко Прайм (Чертеж)"),
            ("akbronco_prime_link", "Акбронко Прайм: Связь"),
            ("aklex_prime_blueprint", "Аклекс Прайм (Чертеж)"),
            ("aklex_prime_link", "Аклекс Прайм: Связь"),
            ("akvasto_prime_blueprint", "Аквасто Прайм (Чертеж)"),
            ("akvasto_prime_link", "Аквасто Прайм: Связь"),
            ("akmagnus_prime_blueprint", "Акмагнус Прайм (Чертеж)"),
            ("akmagnus_prime_link", "Акмагнус Прайм: Связь"),
            ("kavasa_prime_buckle", "Каваса Прайм: Застежка"),
            ("kavasa_prime_band", "Каваса Прайм: Лента"),
            (
                "kavasa_prime_kubrow_collar_blueprint",
                "Ошейник Кубрау: Каваса Прайм (Чертеж)",
            ),
            ("lohk", "Лок"),
        ];
        let mut items: Vec<_> = missing_rewards
            .iter()
            .copied()
            .chain([
                ("bronco_prime_blueprint", "Бронко Прайм (Чертеж)"),
                ("akbronco_prime_set", "Акбронко Прайм: Комплект"),
                ("english_only", ""),
            ])
            .map(|(slug, name)| {
                serde_json::json!({
                    "item_id": slug, "slug": slug, "display_name_en": slug,
                    "display_name_ru": name, "subtypes": [], "tags": []
                })
            })
            .collect();
        for (slug, name) in [
            (
                "citrine_prime_systems_blueprint",
                "Цитрина Прайм: Система (Чертеж)",
            ),
            ("steflos_prime_barrel", "Стефлос Прайм: Ствол"),
            ("corufell_prime_handle", "Коруфелл Прайм: Рукоять"),
        ] {
            items.push(serde_json::json!({
                "item_id": slug, "slug": slug, "display_name_en": slug,
                "display_name_ru": name, "subtypes": [],
                "tags": ["prime", "component"]
            }));
        }
        let catalog: platscope_domain::ItemCatalog = serde_json::from_value(serde_json::json!({
            "metadata": {
                "provider": "relics_run", "fetched_at": Utc::now(), "schema_version": 1,
                "item_count": items.len(), "checksum_sha256": "reward-regression"
            },
            "items": items
        }))
        .unwrap();
        let rewards: Vec<_> = missing_rewards
            .iter()
            .map(|(slug, name)| {
                serde_json::json!({
                    "rewardSlug": slug, "rewardGameRef": format!("/Lotus/Test/{slug}"),
                    "displayNameEn": name,
                    "chancePercent": 100.0 / f64::from(u32::try_from(missing_rewards.len()).unwrap())
                })
            })
            .collect();
        // Старый кэш: рецепт Акбронко не поддержан, его деталей в primeParts нет.
        let metadata: GameMetadataSnapshot = serde_json::from_value(serde_json::json!({
            "metadata": {
                "source": "wfcd_warframe_items", "fetchedAt": Utc::now(), "schemaVersion": 8,
                "setCount": 0, "relicCount": 1, "primePartCount": 1,
                "checksumSha256": "old-reward-cache"
            },
            "primeSets": [], "rivenDispositions": [],
            "primeParts": [{
                "slug": "bronco_prime_blueprint", "gameRef": "/Lotus/Test/Bronco",
                "ducats": 15, "vaultStatus": "available"
            }],
            "relics": [{
                "relicSlug": "axi_test_relic", "relicGameRef": "/Lotus/Test/OtherPlayersRelic",
                "displayNameEn": "Axi Test", "refinement": "intact", "vaultStatus": "vaulted",
                "rewards": rewards
            }]
        }))
        .unwrap();
        assert_eq!(
            reward_vault_status(Some(&metadata), "bronco_prime_blueprint"),
            VaultStatus::Available
        );
        assert_eq!(
            reward_vault_status(Some(&metadata), "akbronco_prime_blueprint"),
            VaultStatus::Unknown
        );
        let mut conflicting_metadata = metadata.clone();
        conflicting_metadata.prime_sets.push(PrimeSetDefinition {
            set_slug: "bronco_prime_set".into(),
            set_game_ref: "/Lotus/Test/BroncoSet".into(),
            display_name_en: "Bronco Prime Set".into(),
            vault_status: VaultStatus::Vaulted,
            components: vec![platscope_domain::PrimeSetComponentDefinition {
                slug: "bronco_prime_blueprint".into(),
                game_ref: "/Lotus/Test/Bronco".into(),
                required_quantity: 1,
                ducats: Some(15),
                image_url: None,
            }],
        });
        assert_eq!(
            reward_vault_status(Some(&conflicting_metadata), "bronco_prime_blueprint"),
            VaultStatus::Unknown
        );
        let mut database = Database::open_in_memory().unwrap();
        database.promote_catalog(&catalog).unwrap();
        database.promote_game_metadata(&metadata).unwrap();
        for active in [
            HashSet::new(),
            HashSet::from(["/Lotus/Test/OtherPlayersRelic".into()]),
        ] {
            let result = build_reward_ocr_catalog(&database, &active).unwrap();
            for (slug, name) in missing_rewards {
                let reward = result.iter().find(|item| item.slug == slug).unwrap();
                assert_eq!(reward.item_id, slug);
                assert_eq!(reward.name, name);
            }
            for slug in [
                "citrine_prime_systems_blueprint",
                "steflos_prime_barrel",
                "corufell_prime_handle",
            ] {
                assert!(result.iter().any(|item| item.slug == slug), "{slug}");
            }
            assert!(
                result
                    .iter()
                    .any(|item| item.slug == "bronco_prime_blueprint")
            );
            assert!(!result.iter().any(|item| item.slug.ends_with("_set")));
            assert!(!result.iter().any(|item| item.slug == "english_only"));
            assert_eq!(
                result
                    .iter()
                    .filter(|item| item.slug == "forma_blueprint")
                    .count(),
                2
            );
            if !active.is_empty() {
                assert_ne!(result[0].slug, "bronco_prime_blueprint");
            }
        }
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
            vault_status: VaultStatus::Unknown,
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
}
