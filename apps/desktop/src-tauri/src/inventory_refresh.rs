//! Общая очередь чтения игры. EE.log задаёт поводы для инвентаря,
//! серверный Expiry — срок следующего чтения магазина Норы.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use platscope_core::ResourceConverterService;
use platscope_domain::NightwaveVendorSnapshot;
use platscope_readonly_scan::scan::find_wf_pid;
use platscope_storage::Database;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{AppState, InventoryView, perform_inventory_scan};

const ENABLED_KEY: &str = "inventory.auto_refresh.enabled.v1";
const NORA_RETRY_KEY: &str = "nightwave.auto_refresh.retry.v1";
const MIN_INTERVAL_MS: u64 = 30_000;
const MAX_LINE_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Job {
    Inventory,
    Nightwave,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)] // Независимые признаки настройки, процесса и двух источников.
pub(crate) struct RefreshStatus {
    enabled: bool,
    running: Option<Job>,
    waiting_for_game: bool,
    inventory_error: bool,
    nightwave_error: bool,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct NoraRetry {
    attempts: u32,
    after: Option<DateTime<Utc>>,
}

#[allow(clippy::struct_excessive_bools)] // Ортогональные состояния очереди и построчного декодера.
struct Queue {
    enabled: bool,
    pid: Option<u32>,
    epoch: u64,
    loading: bool,
    settle_until: u64,
    pending: Option<u64>,
    last_inventory_start: Option<u64>,
    inventory_attempts: u32,
    retry_after: u64,
    running: Option<Job>,
    inventory_error: bool,
    nightwave_error: bool,
    vendor: Option<NightwaveVendorSnapshot>,
    nora_retry: NoraRetry,
    // Ручное обновление также может заполнить отсутствующий кэш Норы один раз.
    manual_vendor_requested: bool,
    tail: String,
    dropping_line: bool,
}

impl Queue {
    fn new(enabled: bool, vendor: Option<NightwaveVendorSnapshot>, nora_retry: NoraRetry) -> Self {
        Self {
            enabled,
            vendor,
            nora_retry,
            pid: None,
            epoch: 0,
            loading: false,
            settle_until: 0,
            pending: None,
            last_inventory_start: None,
            inventory_attempts: 0,
            retry_after: 0,
            running: None,
            inventory_error: false,
            nightwave_error: false,
            manual_vendor_requested: false,
            tail: String::new(),
            dropping_line: false,
        }
    }

    fn status(&self) -> RefreshStatus {
        RefreshStatus {
            enabled: self.enabled,
            running: self.running,
            waiting_for_game: self.pid.is_none(),
            inventory_error: self.inventory_error,
            nightwave_error: self.nightwave_error,
        }
    }

    fn reset_log(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.loading = false;
        self.pending = None;
        self.tail.clear();
        self.dropping_line = false;
        self.inventory_attempts = 0;
        self.retry_after = 0;
    }

    fn observe_process(&mut self, pid: Option<u32>, now: u64) {
        if self.pid != pid {
            self.reset_log();
            self.pid = pid;
            self.settle_until = now + 10_000;
            if self.enabled && pid.is_some() {
                self.pending = Some(self.settle_until);
            }
        }
    }

    fn set_enabled(&mut self, enabled: bool, now: u64) {
        if self.enabled == enabled {
            return;
        }
        self.enabled = enabled;
        if !enabled {
            self.manual_vendor_requested = false;
        }
        self.pending = if enabled && self.pid.is_some() {
            Some(now + 3_000)
        } else {
            None
        };
        self.inventory_attempts = 0;
        self.inventory_error = false;
    }

    fn feed(&mut self, chunk: &str, now: u64) {
        // Только завершённые строки; длинная/обрезанная строка отбрасывается целиком.
        for segment in chunk.split_inclusive('\n') {
            if self.tail.len() + segment.len() > MAX_LINE_BYTES {
                self.dropping_line = true;
            }
            if !self.dropping_line {
                self.tail.push_str(segment);
            }
            if segment.ends_with('\n') {
                let marker = if self.dropping_line {
                    None
                } else {
                    Marker::parse(&self.tail)
                };
                self.tail.clear();
                self.dropping_line = false;
                if let Some(marker) = marker {
                    self.signal(marker, now);
                }
            }
        }
    }

    fn signal(&mut self, marker: Marker, now: u64) {
        let delay = match marker {
            Marker::Loading => {
                self.loading = true;
                return;
            }
            Marker::Loaded if self.loading => {
                self.loading = false;
                10_000
            }
            Marker::Loaded => return, // Внутренние подуровни не являются переходом игрока.
            Marker::Synced => {
                self.loading = false;
                3_000
            }
            Marker::Saved => 10_000,
        };
        self.settle_until = now + delay;
        if self.enabled {
            self.pending = Some(self.settle_until);
            if self.inventory_attempts >= 3 {
                // Новое событие игры разрешает новую серию, но не отменяет паузу.
                self.inventory_attempts = 0;
            }
        }
    }

    fn next(&self, now: u64, utc: DateTime<Utc>) -> Option<Job> {
        if self.running.is_some() || self.pid.is_none() || self.loading || now < self.settle_until {
            return None;
        }
        let cooled = self
            .last_inventory_start
            .is_none_or(|last| now >= last + MIN_INTERVAL_MS);
        if self.enabled
            && cooled
            && now >= self.retry_after
            && self.pending.is_some_and(|due| now >= due)
        {
            return Some(Job::Inventory);
        }
        if (self.enabled || self.manual_vendor_requested)
            && !vendor_is_current(self.vendor.as_ref(), utc)
            && self.nora_retry.after.is_none_or(|after| utc >= after)
        {
            return Some(Job::Nightwave);
        }
        None
    }

    fn start(&mut self, job: Job, now: u64) -> u64 {
        self.running = Some(job);
        if job == Job::Inventory {
            self.pending = None;
            self.last_inventory_start = Some(now);
        }
        self.epoch
    }

    fn finish_inventory(&mut self, success: bool, now: u64, epoch: u64) {
        self.running = None;
        if epoch != self.epoch {
            return;
        }
        self.inventory_error = !success;
        if success {
            self.inventory_attempts = 0;
            self.retry_after = 0;
        } else {
            self.inventory_attempts += 1;
            self.retry_after = now
                + match self.inventory_attempts {
                    1 => 30_000,
                    2 => 90_000,
                    _ => 300_000,
                };
            if self.enabled && self.inventory_attempts < 3 {
                self.pending = Some(self.pending.unwrap_or(self.retry_after));
            } else if !self.enabled {
                self.pending = None;
            } else if self.pending.is_some() {
                // Событие, пришедшее во время последней попытки, не теряется.
                // Оно разрешит новую серию только ПОСЛЕ защитной паузы.
                self.inventory_attempts = 0;
            }
        }
    }

    fn finish_vendor(&mut self, vendor: Option<NightwaveVendorSnapshot>, utc: DateTime<Utc>) {
        self.running = None;
        if let Some(vendor) = vendor {
            self.vendor = Some(vendor);
            self.nora_retry = NoraRetry::default();
            self.nightwave_error = false;
            self.manual_vendor_requested = false;
        } else {
            self.nora_retry.attempts = self.nora_retry.attempts.saturating_add(1);
            let seconds = match self.nora_retry.attempts {
                1 => 60,
                2 => 300,
                3 => 900,
                _ => 3600,
            };
            self.nora_retry.after = Some(utc + chrono::Duration::seconds(seconds));
            self.nightwave_error = true;
        }
    }
}

fn vendor_is_current(vendor: Option<&NightwaveVendorSnapshot>, now: DateTime<Utc>) -> bool {
    vendor.is_some_and(|vendor| {
        vendor.observed_at <= now && now < vendor.expires_at && !vendor.offers.is_empty()
    })
}

#[derive(Clone, Copy)]
enum Marker {
    Loading,
    Loaded,
    Synced,
    Saved,
}

impl Marker {
    fn parse(line: &str) -> Option<Self> {
        let (stamp, message) = line.trim().split_once(' ')?;
        if !stamp.contains('.')
            || !stamp
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b'.')
            || !stamp.parse::<f64>().ok()?.is_finite()
        {
            return None;
        }
        let message = message.trim_start();
        match message {
            "Sys [Info]: FSM::LoadLevel" => Some(Self::Loading),
            "Game [Info]: Level loader: LS_POST_CREATE -> LS_COMPLETE" => Some(Self::Loaded),
            "Script [Info]: Hub.lua: Inventory sync done" => Some(Self::Synced),
            _ => {
                let name = message.strip_prefix("Sys [Info]: Committing ")?;
                let name = name
                    .strip_suffix("'s inventory to DB")
                    .or_else(|| name.strip_suffix("'s inventory checkpoint to DB"))?;
                if name.is_empty() || name.len() > 128 {
                    None
                } else {
                    Some(Self::Saved)
                }
            }
        }
    }
}

pub(crate) struct InventoryRefreshService {
    born: Instant,
    queue: Mutex<Queue>,
    // Захватывается ДО чтения памяти и освобождается ПОСЛЕ записи в БД и события UI.
    flight: tokio::sync::Mutex<()>,
}

impl InventoryRefreshService {
    pub(crate) fn new(database: &Database) -> Self {
        let enabled = database
            .get_setting(ENABLED_KEY)
            .ok()
            .flatten()
            .unwrap_or(true);
        let nora_retry = database
            .get_setting(NORA_RETRY_KEY)
            .ok()
            .flatten()
            .unwrap_or_default();
        Self {
            born: Instant::now(),
            queue: Mutex::new(Queue::new(enabled, None, nora_retry)),
            flight: tokio::sync::Mutex::new(()),
        }
    }

    fn now(&self) -> u64 {
        u64::try_from(self.born.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn queue(&self) -> std::sync::MutexGuard<'_, Queue> {
        self.queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn feed(&self, chunk: &str) {
        self.queue().feed(chunk, self.now());
    }
    pub(crate) fn reset_log(&self) {
        self.queue().reset_log();
    }

    pub(crate) fn session_is_current(&self, pid: u32, epoch: u64) -> bool {
        let queue = self.queue();
        queue.pid == Some(pid) && queue.epoch == epoch && !queue.loading
    }

    fn emit(&self, app: &AppHandle) {
        let _ = app.emit("inventory-refresh-status", self.queue().status());
    }

    pub(crate) async fn manual(app: &AppHandle) -> Result<InventoryView, String> {
        let state = app.state::<AppState>();
        let service = &state.inventory_refresh;
        let _guard = service
            .flight
            .try_lock()
            .map_err(|_| "inventory_scan_busy")?;
        let pid = find_wf_pid().ok_or("inventory_game_not_running")?;
        let epoch = {
            let mut queue = service.queue();
            queue.observe_process(Some(pid), service.now());
            if queue.loading {
                return Err("inventory_game_loading".into());
            }
            queue.inventory_attempts = 0;
            queue.start(Job::Inventory, service.now())
        };
        service.emit(app);
        let handle = app.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            perform_inventory_scan(&handle, pid, epoch)
        })
        .await
        .unwrap_or_else(|_| Err("Не удалось завершить чтение инвентаря.".into()));
        service
            .queue()
            .finish_inventory(result.is_ok(), service.now(), epoch);
        if result.is_ok() {
            let mut queue = service.queue();
            queue.manual_vendor_requested = !vendor_is_current(queue.vendor.as_ref(), Utc::now());
        }
        service.emit(app);
        result
    }
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri владеет аргументами команды.
pub(crate) fn inventory_refresh_status(state: State<'_, AppState>) -> RefreshStatus {
    state.inventory_refresh.queue().status()
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri владеет аргументами команды.
pub(crate) fn set_inventory_auto_refresh(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<RefreshStatus, String> {
    state
        .inventory_database
        .lock()
        .map_err(|_| "Настройка недоступна.")?
        .set_setting(ENABLED_KEY, &enabled)
        .map_err(|_| "Не удалось сохранить настройку.")?;
    state
        .inventory_refresh
        .queue()
        .set_enabled(enabled, state.inventory_refresh.now());
    state.inventory_refresh.emit(&app);
    Ok(state.inventory_refresh.queue().status())
}

pub(crate) fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let service = &state.inventory_refresh;
        // Успешная ротация переживает перезапуск, настройка UI не сбрасывает её.
        service.queue().vendor =
            ResourceConverterService::load_nightwave_vendor(&state.inventory_database)
                .ok()
                .flatten();
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut last_status = None;
        loop {
            interval.tick().await;
            let pid = find_wf_pid();
            service.queue().observe_process(pid, service.now());
            let status = service.queue().status();
            if last_status.as_ref() != Some(&status) {
                service.emit(&app);
                last_status = Some(status);
            }
            let Ok(_guard) = service.flight.try_lock() else {
                continue;
            };
            let Some((job, pid, epoch)) = ({
                let mut queue = service.queue();
                queue.next(service.now(), Utc::now()).and_then(|job| {
                    let pid = queue.pid?;
                    let epoch = queue.start(job, service.now());
                    Some((job, pid, epoch))
                })
            }) else {
                continue;
            };
            service.emit(&app);
            let handle = app.clone();
            match job {
                Job::Inventory => {
                    let result = tauri::async_runtime::spawn_blocking(move || {
                        perform_inventory_scan(&handle, pid, epoch)
                    })
                    .await;
                    service.queue().finish_inventory(
                        matches!(result, Ok(Ok(_))),
                        service.now(),
                        epoch,
                    );
                    if !matches!(result, Ok(Ok(_))) {
                        tracing::warn!(
                            event = "inventory_auto_refresh_failed",
                            "automatic inventory refresh did not publish a snapshot"
                        );
                    }
                }
                Job::Nightwave => {
                    let result = tauri::async_runtime::spawn_blocking(move || {
                        let state = handle.state::<AppState>();
                        let vendor = state.read_only_inventory_scanner.scan_nightwave(pid).ok()?;
                        if find_wf_pid() != Some(pid)
                            || !state.inventory_refresh.session_is_current(pid, epoch)
                        {
                            return None;
                        }
                        ResourceConverterService::cache_nightwave_vendor(
                            &state.inventory_database,
                            &vendor,
                        )
                        .ok()?;
                        let _ = handle.emit("nightwave-vendor-updated", ());
                        Some(vendor)
                    })
                    .await
                    .ok()
                    .flatten();
                    if let Some(vendor) = &result {
                        tracing::info!(event = "nightwave_rotation_updated", offers = vendor.offers.len(), expires_at = %vendor.expires_at, "Nightwave rotation cached");
                    } else {
                        tracing::warn!(
                            event = "nightwave_rotation_update_failed",
                            "Nightwave rotation remains due; retry is delayed"
                        );
                    }
                    service.queue().finish_vendor(result, Utc::now());
                    let retry = service.queue().nora_retry.clone();
                    if let Ok(database) = state.inventory_database.lock() {
                        let _ = database.set_setting(NORA_RETRY_KEY, &retry);
                    }
                }
            }
            service.emit(&app);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queue() -> Queue {
        let mut queue = Queue::new(true, None, NoraRetry::default());
        queue.pid = Some(1);
        queue.nora_retry.after = Some(Utc::now() + chrono::Duration::days(1));
        queue
    }
    fn next(queue: &Queue, now: u64) -> Option<Job> {
        queue.next(now, Utc::now())
    }

    #[test]
    fn mission_and_location_signals_coalesce() {
        let mut q = queue();
        q.signal(Marker::Saved, 0);
        q.signal(Marker::Loading, 1_000);
        assert_eq!(next(&q, 30_000), None);
        q.signal(Marker::Loaded, 31_000);
        q.signal(Marker::Synced, 32_000);
        assert_eq!(next(&q, 34_999), None);
        assert_eq!(next(&q, 35_000), Some(Job::Inventory));
        let epoch = q.start(Job::Inventory, 35_000);
        q.finish_inventory(true, 36_000, epoch);
        assert_eq!(next(&q, 100_000), None);
    }

    #[test]
    fn ignores_reopened_results_chat_and_sublevels() {
        let mut q = queue();
        q.feed("1.000 Script [Info]: EndOfMatch.lua: Mission Succeeded\n2.000 Chat [Info]: Hub.lua: Inventory sync done\n3.000 Game [Info]: Level loader: LS_POST_CREATE -> LS_COMPLETE\n", 0);
        assert_eq!(next(&q, 100_000), None);
        assert!(Marker::parse("junk Sys [Info]: FSM::LoadLevel").is_none());
    }

    #[test]
    fn waits_for_complete_line_and_bounds_memory() {
        let mut q = queue();
        q.feed("1.000 Script [Info]: Hub.lua: Inventory sync", 0);
        assert_eq!(next(&q, 20_000), None);
        q.feed(" done\r\n", 1_000);
        assert_eq!(next(&q, 4_000), Some(Job::Inventory));
        q.reset_log();
        q.feed(&"x".repeat(MAX_LINE_BYTES + 1), 5_000);
        q.feed("1.000 Script [Info]: Hub.lua: Inventory sync done\n", 5_000);
        assert_eq!(next(&q, 20_000), None);
        assert!(q.tail.is_empty());
    }

    #[test]
    fn event_during_scan_is_not_lost_and_cooldown_defers_it() {
        let mut q = queue();
        let epoch = q.start(Job::Inventory, 1_000);
        q.signal(Marker::Synced, 2_000);
        assert_eq!(next(&q, 100_000), None);
        q.finish_inventory(true, 3_000, epoch);
        assert_eq!(next(&q, 30_999), None);
        assert_eq!(next(&q, 31_000), Some(Job::Inventory));
    }

    #[test]
    fn failed_inventory_retries_three_times_without_losing_backoff() {
        let mut q = queue();
        for (index, start) in [0, 31_000, 122_000].into_iter().enumerate() {
            let epoch = q.start(Job::Inventory, start);
            q.finish_inventory(false, start + 1_000, epoch);
            assert_eq!(q.inventory_attempts, u32::try_from(index + 1).unwrap());
            assert_eq!(next(&q, start + 2_000), None);
        }
        assert_eq!(next(&q, 500_000), None);
        q.signal(Marker::Synced, 130_000);
        assert_eq!(next(&q, 140_000), None);
        assert_eq!(next(&q, 500_000), Some(Job::Inventory));
    }

    #[test]
    fn process_restart_invalidates_inflight_and_disabling_clears_pending() {
        let mut q = queue();
        let epoch = q.start(Job::Inventory, 0);
        q.observe_process(None, 1_000);
        q.finish_inventory(true, 2_000, epoch);
        assert!(!q.manual_vendor_requested);
        q.observe_process(Some(2), 3_000);
        assert_eq!(next(&q, 50_000), Some(Job::Inventory));
        q.set_enabled(false, 4_000);
        assert_eq!(next(&q, 100_000), None);
    }

    #[test]
    fn sanitized_real_mission_sequence_triggers_one_refresh() {
        let mut q = queue();
        // Временные метки реального EE.log; имя игрока заменено.
        q.feed("41964.883 Script [Info]: EndOfMatch.lua: Mission Succeeded\n41964.902 Sys [Info]: Committing Player's inventory to DB\n", 0);
        q.feed("41975.770 Sys [Info]: FSM::LoadLevel\n", 1_000);
        assert_eq!(next(&q, 20_000), None);
        q.feed("41978.700 Game [Info]: Level loader: LS_POST_CREATE -> LS_COMPLETE\n41979.359 Script [Info]: Hub.lua: Inventory sync done\n", 21_000);
        assert_eq!(next(&q, 24_000), Some(Job::Inventory));
        let epoch = q.start(Job::Inventory, 24_000);
        q.finish_inventory(true, 25_000, epoch);
        q.feed(
            "41999.883 Script [Info]: EndOfMatch.lua: Mission Succeeded\n",
            41_000,
        );
        assert_eq!(next(&q, 100_000), None);
    }

    #[test]
    fn checkpoint_is_a_fallback_without_hub_and_loading_still_blocks_it() {
        let mut q = queue();
        q.feed(
            "12.000 Sys [Info]: Committing Player's inventory checkpoint to DB\n",
            0,
        );
        assert_eq!(next(&q, 9_999), None);
        assert_eq!(next(&q, 10_000), Some(Job::Inventory));
        q.signal(Marker::Loading, 2_000);
        assert_eq!(next(&q, 100_000), None);
    }

    #[test]
    fn turning_auto_off_during_scan_does_not_restart_it_or_nora() {
        let mut q = queue();
        q.nora_retry = NoraRetry::default();
        let epoch = q.start(Job::Inventory, 0);
        q.signal(Marker::Synced, 1_000);
        q.set_enabled(false, 2_000);
        q.finish_inventory(true, 3_000, epoch);
        assert_eq!(next(&q, 100_000), None);
    }

    #[test]
    fn fresh_event_during_last_failed_attempt_waits_but_is_not_lost() {
        let mut q = queue();
        q.inventory_attempts = 2;
        let epoch = q.start(Job::Inventory, 0);
        q.signal(Marker::Synced, 1_000);
        q.finish_inventory(false, 2_000, epoch);
        assert_eq!(next(&q, 301_999), None);
        assert_eq!(next(&q, 302_000), Some(Job::Inventory));
    }

    #[test]
    fn nora_and_inventory_never_run_together_and_pending_inventory_survives() {
        let mut q = queue();
        q.nora_retry = NoraRetry::default();
        assert_eq!(next(&q, 0), Some(Job::Nightwave));
        q.start(Job::Nightwave, 0);
        q.signal(Marker::Synced, 1_000);
        assert_eq!(next(&q, 10_000), None);
        q.finish_vendor(Some(vendor(Utc::now())), Utc::now());
        assert_eq!(next(&q, 10_000), Some(Job::Inventory));
    }

    #[test]
    fn saved_setting_and_nora_retry_round_trip_through_database() {
        let database = Database::open_in_memory().unwrap();
        database.set_setting(ENABLED_KEY, &false).unwrap();
        let now = Utc::now();
        let retry = NoraRetry {
            attempts: 2,
            after: Some(now + chrono::Duration::minutes(5)),
        };
        database.set_setting(NORA_RETRY_KEY, &retry).unwrap();
        let service = InventoryRefreshService::new(&database);
        assert!(!service.queue().enabled);
        assert_eq!(service.queue().nora_retry.after, retry.after);
        let database = Mutex::new(database);
        ResourceConverterService::cache_nightwave_vendor(&database, &vendor(now)).unwrap();
        let saved = ResourceConverterService::load_nightwave_vendor(&database).unwrap();
        assert!(vendor_is_current(saved.as_ref(), now));
    }

    fn vendor(now: DateTime<Utc>) -> NightwaveVendorSnapshot {
        NightwaveVendorSnapshot {
            observed_at: now - chrono::Duration::hours(1),
            expires_at: now + chrono::Duration::hours(1),
            season_tag: "test".into(),
            vendor_type: "test".into(),
            offers: vec![platscope_domain::NightwaveVendorOffer {
                game_ref: "/test".into(),
                cred_cost: 20,
            }],
        }
    }

    #[test]
    fn nora_uses_persisted_expiry_not_inventory_events_or_weekday() {
        let now = Utc::now();
        let snapshot = vendor(now);
        let mut q = Queue::new(true, Some(snapshot.clone()), NoraRetry::default());
        q.pid = Some(1);
        assert_eq!(q.next(0, now), None);
        q.manual_vendor_requested = true;
        assert_eq!(
            q.next(100_000, snapshot.expires_at - chrono::Duration::seconds(1)),
            None
        );
        assert_eq!(q.next(100_000, snapshot.expires_at), Some(Job::Nightwave));
        assert_eq!(
            q.next(100_000, now + chrono::Duration::days(3)),
            Some(Job::Nightwave)
        );
    }

    #[test]
    fn nora_failure_does_not_mark_rotation_done_and_retry_survives_restart() {
        let now = Utc::now();
        let mut q = queue();
        q.finish_vendor(None, now);
        assert_eq!(q.next(0, now + chrono::Duration::seconds(59)), None);
        let mut restarted = Queue::new(true, None, q.nora_retry.clone());
        restarted.pid = Some(1);
        assert_eq!(restarted.next(0, now), None);
        assert_eq!(
            restarted.next(0, now + chrono::Duration::seconds(60)),
            Some(Job::Nightwave)
        );
        restarted.finish_vendor(Some(vendor(now)), now);
        assert_eq!(restarted.next(0, now), None);
        assert_eq!(restarted.nora_retry.attempts, 0);
    }
}
