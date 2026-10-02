//! Миниокно выбора реликвии и временный учёт подтверждённых открытий.
use std::collections::{HashMap, HashSet};

use platscope_core::{
    AppSettings, InventoryService, InventoryView, RelicEra, RelicSelectionBaseConsumption,
    RelicSelectionConsumption, RelicSelectionRequest, RelicSelectionReward, RelicSelectionRow,
    RelicSelectionService, SETTINGS_KEY,
};
use platscope_domain::MarketVariantKey;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Listener, Manager, PhysicalPosition, PhysicalSize, State};

use crate::{AppState, localize_component_image_url, reward_ocr};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Trigger {
    pub phase: String,
    pub session_id: String,
    pub selection_id: u64,
    pub era: Option<RelicEra>,
    pub screen_era: Option<RelicEra>,
    pub relic_name: Option<String>,
    pub refinement: Option<String>,
    pub mission_name: Option<String>,
    pub reason: Option<String>,
    pub foreground: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OverlayView {
    preview: bool,
    status: String,
    message: Option<String>,
    mission_name: Option<String>,
    era: Option<RelicEra>,
    inventory_available: bool,
    selected: Option<RelicSelectionRow>,
    selected_relic_name: Option<String>,
    selected_remaining_quantity: Option<u32>,
    selected_refinement_known: bool,
    selected_browsing: bool,
    selected_rewards: Vec<RelicSelectionReward>,
    recommendations: Vec<RelicSelectionRow>,
    opened_this_session: u32,
    last_opened_relic_name: Option<String>,
    overlay_scale: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Consumption {
    Base(String),
    Exact(MarketVariantKey),
}

#[derive(Default)]
pub(crate) struct Runtime {
    session_id: String,
    selection_id: u64,
    active: bool,
    foreground: bool,
    generation: u64,
    refreshing: bool,
    preview_settings: Option<AppSettings>,
    preview_token: u64,
    era: Option<RelicEra>,
    screen_era: Option<RelicEra>,
    selected_browsing: bool,
    mission_name: Option<String>,
    selected_name: Option<String>,
    selected_refinement: Option<String>,
    opened: HashSet<u64>,
    opened_this_session: u32,
    last_opened_name: Option<String>,
    account_key: Option<String>,
    inventory_checksum: Option<String>,
    physical: HashMap<Consumption, u32>,
    pending: HashMap<Consumption, u32>,
    latest: Option<OverlayView>,
}

fn base_name(slug: &str) -> Option<String> {
    let name = slug.strip_suffix("_relic")?;
    let (era, code) = name.split_once('_')?;
    if !matches!(era, "lith" | "meso" | "neo" | "axi" | "requiem")
        || code.is_empty()
        || !code
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return None;
    }
    Some(format!("{era} {code}").to_ascii_lowercase())
}

impl Runtime {
    fn can_show(&self) -> bool {
        (self.active && self.foreground) || self.preview_settings.is_some()
    }
    fn begin_session(&mut self, session_id: String) {
        // Перезапуск слушателя не возвращает уже потраченные копии старого снимка.
        let same_process = self.session_id.split(':').next() == session_id.split(':').next();
        *self = Self {
            session_id,
            generation: self.generation.wrapping_add(1),
            refreshing: self.refreshing,
            preview_token: self.preview_token,
            account_key: self.account_key.take(),
            inventory_checksum: self.inventory_checksum.take(),
            physical: std::mem::take(&mut self.physical),
            pending: std::mem::take(&mut self.pending),
            opened_this_session: if same_process {
                self.opened_this_session
            } else {
                0
            },
            last_opened_name: if same_process {
                self.last_opened_name.take()
            } else {
                None
            },
            ..Default::default()
        };
    }
    fn selected_key(&self) -> Option<MarketVariantKey> {
        let name = self.selected_name.as_ref()?.to_ascii_lowercase();
        let refinement = self.selected_refinement.as_deref()?;
        self.physical.keys().find_map(|identity| match identity {
            Consumption::Exact(key)
                if base_name(&key.slug).as_ref() == Some(&name)
                    && key.subtype.as_deref() == Some(refinement) =>
            {
                Some(key.clone())
            }
            _ => None,
        })
    }

    fn reconcile(&mut self, inventory: &InventoryView) {
        let mut physical = HashMap::<Consumption, u32>::new();
        for item in &inventory.items {
            let Some(key) = item.key.as_ref() else {
                continue;
            };
            let Some(name) = base_name(&key.slug) else {
                continue;
            };
            for identity in [Consumption::Base(name), Consumption::Exact(key.clone())] {
                let quantity = physical.entry(identity).or_default();
                *quantity = quantity.saturating_add(item.owned_quantity);
            }
        }
        if self.account_key != inventory.reserve_account_key {
            self.pending.clear();
            self.opened.clear();
            self.opened_this_session = 0;
            self.last_opened_name = None;
        } else if self.inventory_checksum.as_deref() != Some(&inventory.metadata.checksum_sha256) {
            // Не списываем второй раз то, что уже отражено в новом снимке.
            // При неизменившемся количестве оставляем подтверждённую временную дельту.
            let mut exact_acknowledged = HashMap::<String, u32>::new();
            self.pending.retain(|identity, pending| {
                let Consumption::Exact(key) = identity else {
                    return true;
                };
                let before = self.physical.get(identity).copied().unwrap_or(0);
                let after = physical.get(identity).copied().unwrap_or(0);
                let acknowledged = (*pending).min(before.saturating_sub(after));
                if let Some(name) = base_name(&key.slug) {
                    let total = exact_acknowledged.entry(name).or_default();
                    *total = total.saturating_add(acknowledged);
                }
                *pending = pending.saturating_sub(acknowledged);
                *pending > 0
            });
            self.pending.retain(|identity, pending| {
                let Consumption::Base(name) = identity else {
                    return true;
                };
                let before = self.physical.get(identity).copied().unwrap_or(0);
                let after = physical.get(identity).copied().unwrap_or(0);
                let acknowledged = before
                    .saturating_sub(after)
                    .saturating_sub(exact_acknowledged.get(name).copied().unwrap_or(0));
                *pending = pending.saturating_sub(acknowledged);
                *pending > 0
            });
        }
        self.account_key.clone_from(&inventory.reserve_account_key);
        self.inventory_checksum = Some(inventory.metadata.checksum_sha256.clone());
        self.physical = physical;
    }

    fn record_opened(&mut self, trigger: &Trigger) {
        if trigger.selection_id != self.selection_id || !self.opened.insert(trigger.selection_id) {
            return;
        }
        let Some(name) = trigger.relic_name.as_ref().or(self.selected_name.as_ref()) else {
            return;
        };
        self.opened_this_session = self.opened_this_session.saturating_add(1);
        self.last_opened_name = Some(name.clone());
        // Без привязки и исходного количества неизвестную дельту не вычитаем из снимка.
        if self.account_key.is_none() {
            return;
        }
        let identity = self.selected_key().map_or_else(
            || Consumption::Base(name.to_ascii_lowercase()),
            Consumption::Exact,
        );
        if self.physical.contains_key(&identity) {
            let pending = self.pending.entry(identity).or_default();
            *pending = pending.saturating_add(1);
        }
    }

    fn request(&self) -> RelicSelectionRequest {
        let mut request = RelicSelectionRequest {
            era: match self.era {
                None => self.screen_era,
                Some(RelicEra::Omnia) => self
                    .screen_era
                    .filter(|era| {
                        matches!(
                            era,
                            RelicEra::Lith | RelicEra::Meso | RelicEra::Neo | RelicEra::Axi
                        )
                    })
                    .or(self.era),
                known => known,
            },
            mission_name: self.mission_name.clone(),
            selected_key: self.selected_key(),
            selected_relic_name: self.selected_name.clone(),
            baseline_inventory_checksum: self.inventory_checksum.clone(),
            baseline_reserve_account_key: self.account_key.clone(),
            ..Default::default()
        };
        for (identity, quantity) in &self.pending {
            match identity {
                Consumption::Base(name) => {
                    request.base_consumed.push(RelicSelectionBaseConsumption {
                        relic_name: name.clone(),
                        quantity: *quantity,
                    })
                }
                Consumption::Exact(key) => request.consumed.push(RelicSelectionConsumption {
                    key: key.clone(),
                    quantity: *quantity,
                }),
            }
        }
        request
    }
}

pub(crate) fn handle_trigger(app: &AppHandle, trigger: Trigger) {
    if trigger.session_id.is_empty() || trigger.session_id.len() > 128 {
        return;
    }
    let state = app.state::<AppState>();
    let Ok(mut runtime) = state.relic_selection.lock() else {
        return;
    };
    if trigger.phase == "browse"
        && (runtime.session_id != trigger.session_id
            || !runtime.active
            || runtime.selection_id != trigger.selection_id)
    {
        return;
    }
    if runtime.session_id != trigger.session_id {
        runtime.begin_session(trigger.session_id.clone());
    }
    runtime.generation = runtime.generation.wrapping_add(1);
    runtime.preview_settings = None;
    if trigger.phase == "visibility" {
        runtime.foreground = trigger.foreground.unwrap_or(false);
    } else {
        runtime.era = trigger.era;
        runtime.screen_era = trigger.screen_era;
        runtime.mission_name.clone_from(&trigger.mission_name);
        match trigger.phase.as_str() {
            "open" => {
                runtime.selection_id = trigger.selection_id;
                runtime.active = true;
                runtime.selected_name.clone_from(&trigger.relic_name);
                runtime.selected_refinement.clone_from(&trigger.refinement);
                runtime.selected_browsing = false;
            }
            "browse" if runtime.active && runtime.selection_id == trigger.selection_id => {
                runtime.selected_name.clone_from(&trigger.relic_name);
                runtime.selected_refinement.clone_from(&trigger.refinement);
                runtime.selected_browsing = trigger.relic_name.is_some();
            }
            "selected" => {
                runtime.selection_id = trigger.selection_id;
                runtime.selected_name.clone_from(&trigger.relic_name);
                runtime.selected_refinement.clone_from(&trigger.refinement);
                runtime.active = false;
                runtime.selected_browsing = false;
            }
            "opened" => {
                runtime.record_opened(&trigger);
                runtime.active = false;
            }
            "closed" | "reward" | "reset" => {
                runtime.active = false;
            }
            "mission" if trigger.reason.as_deref() == Some("mission_ended") => {
                runtime.active = false;
                runtime.selection_id = 0;
                runtime.opened.clear();
                runtime.opened_this_session = 0;
                runtime.last_opened_name = None;
                runtime.selected_name = None;
                runtime.selected_refinement = None;
                // Не теряем ещё не отражённые в инвентаре открытия при выходе из миссии.
            }
            _ => {}
        }
    }
    let show = runtime.active && runtime.foreground;
    drop(runtime);
    if !show {
        hide(app)
    } else {
        if trigger.phase == "open" {
            hide(app);
        }
        refresh(app)
    }
}

fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("relic-selection-overlay") {
        let _ = window.hide();
    }
}

pub(crate) fn close(app: &AppHandle) {
    let state = app.state::<AppState>();
    if let Ok(mut runtime) = state.relic_selection.lock() {
        runtime.generation = runtime.generation.wrapping_add(1);
        runtime.active = false;
        runtime.latest = None;
        runtime.preview_settings = None;
    }
    hide(app);
}

pub(crate) fn stop(app: &AppHandle) {
    close(app);
    if let Ok(mut runtime) = app.state::<AppState>().relic_selection.lock() {
        runtime.foreground = false;
    }
}

pub(crate) fn spawn(app: AppHandle) {
    for event in [
        "inventory-updated",
        "market-data-updated",
        "game-metadata-updated",
    ] {
        let app_handle = app.clone();
        app.listen(event, move |_| {
            let state = app_handle.state::<AppState>();
            if let Ok(mut runtime) = state.relic_selection.lock() {
                runtime.generation = runtime.generation.wrapping_add(1);
            }
            refresh(&app_handle);
        });
    }
}

fn refresh(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Ok(mut runtime) = state.relic_selection.lock() else {
        return;
    };
    if !runtime.can_show() || runtime.refreshing {
        return;
    }
    runtime.refreshing = true;
    let generation = runtime.generation;
    drop(runtime);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let builder_app = app.clone();
        let result =
            tauri::async_runtime::spawn_blocking(move || build_view(&builder_app, generation))
                .await;
        if let Ok(Ok(Some((mut view, settings)))) = result {
            let rect = if view.preview {
                preview_rect(&app)
            } else {
                reward_ocr::warframe_window_rect(&app).await
            };
            if let Ok(rect) = rect {
                let scale = (f64::from(rect.height) / 1080.0).min(f64::from(rect.width) / 1280.0)
                    * f64::from(settings.relic_selection_overlay_scale_percent)
                    / 100.0;
                let state = app.state::<AppState>();
                if let Ok(mut runtime) = state.relic_selection.lock()
                    && runtime.generation == generation
                    && runtime.can_show()
                    && let Some(window) = app.get_webview_window("relic-selection-overlay")
                {
                    view.overlay_scale = scale;
                    runtime.latest = Some(view.clone());
                    let _ = app.emit("relic-selection-updated", &view);
                    let width = overlay_width(&rect, scale);
                    let height = (760.0 * scale).round().max(1.0) as u32;
                    let margin = (16.0 * scale).round() as i32;
                    let x = rect.x + i32::try_from(rect.width.saturating_sub(width)).unwrap_or(0)
                        - margin;
                    let y =
                        rect.y + i32::try_from(rect.height.saturating_sub(height) / 2).unwrap_or(0);
                    let _ = window.set_size(PhysicalSize::new(width, height));
                    let _ = window.set_position(PhysicalPosition::new(x, y));
                    let _ = window.set_focusable(false);
                    let _ = window.set_ignore_cursor_events(true);
                    let _ = window.set_always_on_top(true);
                    let _ = window.show();
                }
            } else {
                invalidate_view(&app, generation);
            }
        } else {
            invalidate_view(&app, generation);
        }
        let state = app.state::<AppState>();
        let retry = if let Ok(mut runtime) = state.relic_selection.lock() {
            runtime.refreshing = false;
            runtime.generation != generation && runtime.can_show()
        } else {
            false
        };
        if retry {
            refresh(&app)
        }
    });
}

fn invalidate_view(app: &AppHandle, generation: u64) {
    if let Ok(mut runtime) = app.state::<AppState>().relic_selection.lock()
        && runtime.generation == generation
    {
        runtime.latest = None;
        hide(app);
    }
}

fn overlay_width(rect: &reward_ocr::WarframeWindowRect, scale: f64) -> u32 {
    // Заголовок реликвии читается слева от 77,9% ширины игры. Миниокно
    // остаётся правее этой области даже при увеличенном шрифте.
    let margin = (16.0 * scale).round().max(0.0) as u32;
    let available = (f64::from(rect.width) * 0.221).floor() as u32;
    ((400.0 * scale).round().max(1.0) as u32).min(available.saturating_sub(margin).max(1))
}

fn build_view(
    app: &AppHandle,
    generation: u64,
) -> Result<Option<(OverlayView, AppSettings)>, String> {
    let state = app.state::<AppState>();
    let settings = state
        .reward_database
        .lock()
        .map_err(|_| "database unavailable")?
        .get_setting::<AppSettings>(SETTINGS_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    let preview_settings = state
        .relic_selection
        .lock()
        .map_err(|_| "selection unavailable")?
        .preview_settings
        .clone();
    let preview = preview_settings.is_some();
    let settings = preview_settings.unwrap_or(settings);
    let inventory = InventoryService::view(&state.reward_database, &settings)
        .map_err(|error| error.to_string())?;
    let mut request = {
        let mut runtime = state
            .relic_selection
            .lock()
            .map_err(|_| "selection unavailable")?;
        if runtime.generation != generation
            || (!runtime.active && runtime.preview_settings.is_none())
        {
            return Ok(None);
        }
        if let Some(inventory) = &inventory {
            runtime.reconcile(inventory);
        }
        runtime.request()
    };
    if preview {
        request.era = None;
    }
    let data = RelicSelectionService::view(&state.reward_database, &settings, &request)
        .map_err(|error| error.to_string())?;
    let mut runtime = state
        .relic_selection
        .lock()
        .map_err(|_| "selection unavailable")?;
    if runtime.generation != generation || (!runtime.active && runtime.preview_settings.is_none()) {
        return Ok(None);
    }
    if let Some(data) = &data
        && (Some(data.inventory_checksum.as_str())
            != request.baseline_inventory_checksum.as_deref()
            || data.reserve_account_key != request.baseline_reserve_account_key)
    {
        // Пока core читал данные, снимок сменился: сначала согласуем временные списания.
        runtime.generation = runtime.generation.wrapping_add(1);
        return Ok(None);
    }
    let mut view = OverlayView {
        preview,
        status: if data.is_some() { "ok" } else { "needs_data" }.into(),
        message: if data.is_none() {
            Some("Обновите инвентарь и данные реликвий в PlatScope.".into())
        } else {
            None
        },
        mission_name: if preview {
            Some("Предпросмотр".into())
        } else {
            runtime.mission_name.clone()
        },
        era: request.era,
        inventory_available: inventory.is_some(),
        selected: None,
        selected_relic_name: runtime.selected_name.clone(),
        selected_remaining_quantity: None,
        selected_refinement_known: false,
        selected_browsing: runtime.selected_browsing,
        selected_rewards: vec![],
        recommendations: vec![],
        opened_this_session: runtime.opened_this_session,
        last_opened_relic_name: runtime.last_opened_name.clone(),
        overlay_scale: 1.0,
    };
    if let Some(data) = data {
        if data.has_uncertain_stock && data.recommendations.is_empty() {
            view.message = Some("Остаток есть, но улучшения нужно уточнить в инвентаре.".into());
        }
        view.selected = data.selected;
        view.selected_remaining_quantity = data.selected_remaining_quantity;
        view.selected_refinement_known = data.selected_refinement_known;
        view.selected_rewards = data.selected_rewards;
        view.recommendations = data.recommendations;
    }
    if request.era.is_none() && !preview {
        view.recommendations.clear();
        view.message = Some("Эра открытой вкладки пока не определена.".into());
    }
    for row in view
        .selected
        .iter_mut()
        .chain(view.recommendations.iter_mut())
    {
        localize_component_image_url(&mut row.image_url);
        for reward in &mut row.rewards {
            localize_component_image_url(&mut reward.image_url);
        }
    }
    for reward in &mut view.selected_rewards {
        localize_component_image_url(&mut reward.image_url);
    }
    Ok(Some((view, settings)))
}

fn preview_rect(app: &AppHandle) -> Result<reward_ocr::WarframeWindowRect, String> {
    let window = app
        .get_webview_window("main")
        .ok_or("Окно PlatScope не найдено.")?;
    let monitor = window
        .current_monitor()
        .map_err(|error| error.to_string())?
        .or(window
            .primary_monitor()
            .map_err(|error| error.to_string())?)
        .ok_or("Не удалось определить экран предпросмотра.")?;
    Ok(reward_ocr::WarframeWindowRect {
        x: monitor.position().x,
        y: monitor.position().y,
        width: monitor.size().width,
        height: monitor.size().height,
    })
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub(crate) async fn preview_relic_selection_overlay(
    settings: AppSettings,
    duration_seconds: Option<u64>,
    app: AppHandle,
) -> Result<OverlayView, String> {
    crate::validate_app_settings(&settings).map_err(str::to_owned)?;
    let (generation, preview_token) = {
        let state = app.state::<AppState>();
        let mut runtime = state
            .relic_selection
            .lock()
            .map_err(|_| "selection unavailable")?;
        runtime.generation = runtime.generation.wrapping_add(1);
        runtime.preview_settings = Some(settings.clone());
        runtime.preview_token = runtime.preview_token.wrapping_add(1);
        (runtime.generation, runtime.preview_token)
    };
    let preview_result: Result<OverlayView, String> = async {
        let builder = app.clone();
        let (mut view, _) =
            tauri::async_runtime::spawn_blocking(move || build_view(&builder, generation))
                .await
                .map_err(|error| error.to_string())??
                .ok_or("Данные изменились; повторите предпросмотр.")?;
        let rect = preview_rect(&app)?;
        let scale = (f64::from(rect.height) / 1080.0).min(f64::from(rect.width) / 1280.0)
            * f64::from(settings.relic_selection_overlay_scale_percent)
            / 100.0;
        view.overlay_scale = scale;
        {
            let state = app.state::<AppState>();
            let mut runtime = state
                .relic_selection
                .lock()
                .map_err(|_| "selection unavailable")?;
            if runtime.generation != generation || runtime.preview_settings.is_none() {
                return Err("Предпросмотр уже закрыт.".into());
            }
            runtime.latest = Some(view.clone());
            app.emit("relic-selection-updated", &view)
                .map_err(|error| error.to_string())?;
            let window = app
                .get_webview_window("relic-selection-overlay")
                .ok_or("Окно подсказки не найдено.")?;
            let width = overlay_width(&rect, scale);
            let height = (760.0 * scale).round().max(1.0) as u32;
            let x = rect.x + i32::try_from(rect.width.saturating_sub(width)).unwrap_or(0)
                - (16.0 * scale).round() as i32;
            let y = rect.y + i32::try_from(rect.height.saturating_sub(height) / 2).unwrap_or(0);
            window
                .set_size(PhysicalSize::new(width, height))
                .map_err(|error| error.to_string())?;
            window
                .set_position(PhysicalPosition::new(x, y))
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
        }
        let expiry_app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(
                duration_seconds.unwrap_or(120).clamp(10, 300),
            ))
            .await;
            let state = expiry_app.state::<AppState>();
            if let Ok(mut runtime) = state.relic_selection.lock()
                && runtime.preview_token == preview_token
                && runtime.preview_settings.is_some()
            {
                runtime.generation = runtime.generation.wrapping_add(1);
                runtime.preview_settings = None;
                runtime.latest = None;
                hide(&expiry_app);
            }
        });
        Ok(view)
    }
    .await;
    if preview_result.is_err() {
        let state = app.state::<AppState>();
        if let Ok(mut runtime) = state.relic_selection.lock()
            && runtime.preview_token == preview_token
            && runtime.preview_settings.is_some()
        {
            runtime.generation = runtime.generation.wrapping_add(1);
            runtime.preview_settings = None;
            runtime.latest = None;
            hide(&app);
        }
    }
    preview_result
}

#[tauri::command]
pub(crate) fn close_relic_selection_preview(app: AppHandle) {
    let state = app.state::<AppState>();
    if let Ok(mut runtime) = state.relic_selection.lock()
        && runtime.preview_settings.is_some()
    {
        runtime.generation = runtime.generation.wrapping_add(1);
        runtime.preview_settings = None;
        runtime.latest = None;
        hide(&app);
    }
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn latest_relic_selection(
    state: State<'_, AppState>,
) -> Result<Option<OverlayView>, String> {
    state
        .relic_selection
        .lock()
        .map(|runtime| runtime.latest.clone())
        .map_err(|_| "relic selection state unavailable".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmed_openings_are_not_counted_twice_after_refresh_or_account_change() {
        let name = Consumption::Base("lith t14".into());
        let mut runtime = Runtime {
            selection_id: 1,
            account_key: Some("one".into()),
            inventory_checksum: Some("before".into()),
            selected_name: Some("Lith T14".into()),
            physical: HashMap::from([(name.clone(), 5)]),
            ..Default::default()
        };
        let trigger = Trigger {
            phase: "opened".into(),
            session_id: "game".into(),
            selection_id: 1,
            era: Some(RelicEra::Lith),
            screen_era: None,
            relic_name: Some("Lith T14".into()),
            refinement: None,
            mission_name: None,
            reason: None,
            foreground: None,
        };
        runtime.record_opened(&trigger);
        runtime.record_opened(&trigger);
        assert_eq!(runtime.pending[&name], 1);
        assert_eq!(runtime.opened_this_session, 1);
        runtime.begin_session("game:listener-two".into());
        assert_eq!(runtime.pending[&name], 1);
        let mut inventory = InventoryView {
            metadata: platscope_domain::InventorySnapshotMetadata {
                source: platscope_domain::InventorySource::TestFixture,
                observed_at: chrono::Utc::now(),
                schema_version: 3,
                item_count: 0,
                checksum_sha256: "after".into(),
            },
            keep_copies: 1,
            reserve_account_key: Some("one".into()),
            mod_usage_scanned: true,
            summary: platscope_core::InventorySummary {
                owned_quantity: 0,
                sellable_quantity: 0,
                resolved_rows: 0,
                attention_rows: 0,
            },
            items: vec![],
        };
        // Новый достоверный ноль уже содержит потраченную копию; повторной дельты нет.
        runtime.reconcile(&inventory);
        assert!(runtime.pending.is_empty());
        inventory.reserve_account_key = Some("two".into());
        runtime.reconcile(&inventory);
        assert_eq!(runtime.opened_this_session, 0);
        assert!(runtime.pending.is_empty());

        // Один новый снимок отражает точное открытие, но ещё не неизвестное улучшение.
        let radiant_key = MarketVariantKey::new(
            "lith_t14_relic",
            platscope_domain::Platform::Pc,
            None,
            Some("radiant"),
        )
        .unwrap();
        let intact_key = MarketVariantKey::new(
            "lith_t14_relic",
            platscope_domain::Platform::Pc,
            None,
            Some("intact"),
        )
        .unwrap();
        let radiant = Consumption::Exact(radiant_key);
        let mut mixed = Runtime {
            account_key: Some("one".into()),
            inventory_checksum: Some("before-mixed".into()),
            physical: HashMap::from([
                (name.clone(), 6),
                (radiant.clone(), 1),
                (Consumption::Exact(intact_key.clone()), 5),
            ]),
            pending: HashMap::from([(name.clone(), 1), (radiant.clone(), 1)]),
            ..Default::default()
        };
        inventory.reserve_account_key = Some("one".into());
        inventory.metadata.checksum_sha256 = "exact-reflected".into();
        inventory.items = vec![platscope_core::InventoryViewItem {
            canonical_game_id: "/Lotus/Types/Game/Projections/LithT14Bronze".into(),
            item_id: None,
            bulk_tradable: true,
            display_name: "Lith T14".into(),
            image_url: None,
            tags: vec!["relic".into()],
            key: Some(intact_key),
            rank: None,
            subtype: Some("intact".into()),
            owned_quantity: 5,
            tradeable_quantity: 5,
            untradeable_quantity: 0,
            unknown_quantity: 0,
            leveled_quantity: 0,
            equipped_quantity: 0,
            equipped_placements: vec![],
            sellable_quantity: 4,
            personal_reserved_quantity: 0,
            keep_copies_override: None,
            resolution: platscope_domain::InventoryResolution::Resolved,
            vault_status: platscope_domain::VaultStatus::Unknown,
        }];
        mixed.reconcile(&inventory);
        assert!(!mixed.pending.contains_key(&radiant));
        assert_eq!(mixed.pending[&name], 1);
        assert_eq!(mixed.physical[&name], 5);
        inventory.metadata.checksum_sha256 = "same-count".into();
        mixed.reconcile(&inventory);
        assert_eq!(mixed.pending[&name], 1);
        inventory.metadata.checksum_sha256 = "both-reflected".into();
        inventory.items[0].owned_quantity = 4;
        mixed.reconcile(&inventory);
        assert!(mixed.pending.is_empty());
    }
}
