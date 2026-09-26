//! Автозапуск сцены по EE.log и ручной разбор записей. Длительные операции вне UI.
// Tauri передаёт состояние и аргументы команды по значению.
#![allow(clippy::needless_pass_by_value)]
use crate::AppState;
use chrono::Utc;
use platscope_readonly_scan::{
    scan::find_wf_pid,
    spatial::{self, AnalysisProgress, Scene},
};
use serde::Serialize;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager, State};

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)] // Независимые флаги IPC: игра, чтение, отмена и обновление.
pub(crate) struct Status {
    busy: bool,
    cancelling: bool,
    phase: String,
    error: Option<String>,
    revision: u64,
    game_running: bool,
    tracking: bool,
    scanned_bytes: u64,
    auto_start: bool,
    in_orbiter: bool,
}
#[derive(Default)]
struct Inner {
    status: Status,
    scene: Option<Scene>,
    scene_epoch: u64,
    local_pose_reader: Option<spatial::LocalPoseReader>,
    previous_objects: Option<(u64, Vec<spatial::SceneObject>)>,
    generation: u64,
    live_pid: Option<u32>,
    desired_live: bool,
    discovery_rules: Arc<spatial::DiscoveryRules>,
    auto_paused: bool,
}
#[derive(Default)]
pub(crate) struct Service {
    inner: Arc<Mutex<Inner>>,
    cancel: Arc<AtomicBool>,
    log_tail: String,
    log_ready: bool,
    loading_location: bool,
    in_orbiter: bool,
    last_load: Option<String>,
    auto_after: Option<Instant>,
}
enum Source {
    Live(u32),
    Archive(PathBuf, u64),
}

impl Service {
    fn status(&self) -> Result<Status, String> {
        let inner = self.inner.lock().map_err(|e| e.to_string())?;
        let mut status = inner.status.clone();
        status.auto_start = !inner.auto_paused;
        status.in_orbiter = self.in_orbiter;
        drop(inner);
        status.game_running = find_wf_pid().is_some();
        Ok(status)
    }
    fn start(&mut self, source: Source, profile_dir: PathBuf) -> Result<Status, String> {
        let generation = {
            let mut inner = self.inner.lock().map_err(|e| e.to_string())?;
            if inner.status.busy {
                return Err("Дождитесь завершения текущего чтения или отмените его.".into());
            }
            inner.desired_live = matches!(&source, Source::Live(_));
            inner.auto_paused = !inner.desired_live;
            inner.generation += 1;
            inner.status.busy = true;
            inner.status.tracking = false;
            inner.status.cancelling = false;
            inner.status.error = None;
            inner.status.phase = "Подготовка чтения".into();
            inner.status.scanned_bytes = 0;
            inner.generation
        };
        self.auto_after = None;
        self.cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
        let (shared, cancel) = (self.inner.clone(), self.cancel.clone());
        let spawned = std::thread::Builder::new()
            .name("mission-analysis".into())
            .spawn(move || {
                let report = |progress: AnalysisProgress| {
                    if let Ok(mut inner) = shared.lock()
                        && inner.generation == generation
                    {
                        inner.status.phase = progress.stage;
                        inner.status.scanned_bytes = progress.scanned_bytes;
                    }
                };
                let pid = match &source {
                    Source::Live(pid) => Some(*pid),
                    Source::Archive(..) => None,
                };
                let analyze = |pack: &spatial::ProfilePack| match &source {
                    Source::Live(pid) => {
                        spatial::analyze_live_with_profiles_detailed(*pid, pack, &cancel, &report)
                    }
                    Source::Archive(path, sequence) => {
                        spatial::analyze_archive_with_profiles_detailed(
                            path, *sequence, pack, &cancel, &report,
                        )
                    }
                };
                let mut result = spatial::ProfilePack::latest_cached(&profile_dir)
                    .map_err(spatial::AnalysisFailure::Other)
                    .and_then(|pack| analyze(&pack));
                if let Err(spatial::AnalysisFailure::UnsupportedBuild(error)) = &result
                    && !cancel.load(Ordering::Relaxed)
                {
                    report(AnalysisProgress {
                        stage: "Проверка обновления профиля карты".into(),
                        ..Default::default()
                    });
                    match spatial::ProfilePack::download_newer(&profile_dir) {
                        Ok(Some(pack)) => result = analyze(&pack),
                        Ok(None) => {}
                        Err(update_error) => {
                            result = Err(spatial::AnalysisFailure::Other(format!(
                                "{error}. Обновление профиля недоступно: {update_error}"
                            )));
                        }
                    }
                }
                if let Ok(mut inner) = shared.lock()
                    && inner.generation == generation
                {
                    inner.status.busy = false;
                    inner.status.cancelling = false;
                    if cancel.load(Ordering::Relaxed) {
                        inner.status.phase =
                            "Чтение отменено. Предыдущий результат сохранён.".into();
                    } else {
                        match result {
                            Ok(scene) => {
                                inner.status.scanned_bytes = scene.stats.scanned_bytes;
                                inner.scene_epoch += 1;
                                inner.previous_objects = None;
                                inner.local_pose_reader =
                                    spatial::LocalPoseReader::from_scene(&scene);
                                inner.scene = Some(scene);
                                inner.live_pid = pid;
                                inner.status.revision += 1;
                                inner.status.phase = "Чтение завершено".into();
                            }
                            Err(error) => {
                                inner.desired_live = false;
                                inner.status.error = Some(error.to_string());
                                inner.status.phase = "Не удалось прочитать сцену".into();
                            }
                        }
                    }
                }
            });
        if let Err(error) = spawned {
            let mut inner = self.inner.lock().map_err(|e| e.to_string())?;
            inner.status.busy = false;
            inner.status.error = Some(error.to_string());
            return Err(error.to_string());
        }
        self.status()
    }
    fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Ok(mut inner) = self.inner.lock() {
            inner.status.cancelling = inner.status.busy;
            inner.status.tracking = false;
            inner.desired_live = false;
            inner.auto_paused = true;
        }
    }
    pub(crate) fn invalidate(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            if !inner.desired_live {
                return;
            }
            self.cancel.store(true, Ordering::Relaxed);
            inner.status.tracking = false;
            inner.status.cancelling = inner.status.busy;
            inner.live_pid = None;
            inner.desired_live = false;
            if inner
                .scene
                .as_ref()
                .is_some_and(|scene| scene.source == "live")
            {
                inner.scene = None;
                inner.local_pose_reader = None;
                inner.scene_epoch += 1;
                inner.previous_objects = None;
                inner.status.revision += 1;
            }
            inner.status.phase = "Локация меняется. Ожидаем завершения загрузки.".into();
        }
    }
    pub(crate) fn feed_log(&mut self, chunk: &str) {
        self.log_tail.push_str(chunk);
        if let Some(end) = self.log_tail.rfind('\n') {
            let lines: Vec<_> = self.log_tail[..end].lines().map(str::to_owned).collect();
            for line in lines {
                // Основной уровень объявляется до FSM::LoadLevel. Фоновые сцены
                // перелёта и обычная загрузка ресурсов не меняют назначение перехода.
                if let Some(path) = log_message(&line).and_then(|message| {
                    message
                        .strip_prefix("Game [Info]: FrameworkCmd::OpenLevel - ")
                        .or_else(|| message.strip_prefix("Game [Info]: Level="))
                        .or_else(|| {
                            message
                                .strip_prefix("Sys [Info]: Client finished loading ")
                                .and_then(|path| {
                                    path.strip_suffix(". Sending CMSG_LOAD_COMPLETE to server.")
                                })
                                .filter(|path| path.starts_with("/Lotus/Levels/"))
                        })
                }) {
                    let path = path.trim();
                    self.in_orbiter = path == "/Lotus/Levels/Proc/PlayerShip"
                        || path.starts_with("/Lotus/Levels/Proc/PlayerShip/");
                    if self.in_orbiter {
                        self.auto_after = None;
                    }
                }
                if mission_changed(&line) && self.last_load.as_deref() != Some(line.trim()) {
                    self.last_load = Some(line.trim().to_owned());
                    self.loading_location = true;
                    self.auto_after = None;
                    self.invalidate();
                } else if level_loaded(&line) && self.loading_location {
                    self.loading_location = false;
                    self.auto_after =
                        (!self.in_orbiter).then(|| Instant::now() + Duration::from_secs(3));
                }
            }
            self.log_tail.drain(..=end);
        }
        if self.log_tail.len() > 8192 {
            self.log_tail.clear();
        }
    }
    pub(crate) fn set_log_ready(&mut self, ready: bool) {
        if ready && !self.log_ready && self.auto_after.is_some() {
            // История журнала не запускает поиск каждой старой миссии.
            self.auto_after = Some(Instant::now() + Duration::from_secs(3));
        }
        self.log_ready = ready;
    }
    pub(crate) fn reset_log(&mut self) {
        self.invalidate();
        self.log_tail.clear();
        self.last_load = None;
        self.log_ready = false;
        self.loading_location = false;
        self.in_orbiter = false;
        self.auto_after = None;
    }
    fn take_auto_request(&mut self, now: Instant) -> bool {
        if !self.log_ready
            || self.loading_location
            || self.in_orbiter
            || self.auto_after.is_none_or(|at| at > now)
        {
            return false;
        }
        let Ok(inner) = self.inner.lock() else {
            return false;
        };
        if inner.status.busy {
            return false;
        }
        self.auto_after = None;
        !inner.auto_paused && !inner.status.tracking
    }
    fn track(&mut self, enabled: bool) -> Result<Status, String> {
        if !enabled {
            self.stop();
            return self.status();
        }
        let (generation, pid) = {
            let mut inner = self.inner.lock().map_err(|e| e.to_string())?;
            if inner.status.busy {
                return Err("Сначала дождитесь чтения сцены.".into());
            }
            if inner.status.tracking {
                drop(inner);
                return self.status();
            }
            let pid = inner
                .live_pid
                .ok_or("Сначала прочитайте текущую сцену из игры.")?;
            if inner.scene.as_ref().is_none_or(|s| s.source != "live") {
                return Err("Обновление доступно только для сцены из игры.".into());
            }
            inner.generation += 1;
            inner.status.tracking = true;
            inner.desired_live = true;
            inner.auto_paused = false;
            inner.status.error = None;
            (inner.generation, pid)
        };
        self.cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
        let (shared, cancel) = (self.inner.clone(), self.cancel.clone());
        let spawned = std::thread::Builder::new()
            .name("mission-position-refresh".into())
            .spawn(move || {
                while !cancel.load(Ordering::Relaxed) {
                    let tick_started = Instant::now();
                    let sample = shared.lock().ok().and_then(|s| {
                        (s.generation == generation && s.status.tracking)
                            .then(|| {
                                s.scene.clone().map(|scene| {
                                    (scene, s.status.revision, s.discovery_rules.clone())
                                })
                            })
                            .flatten()
                    });
                    let Some((scene, revision, rules)) = sample else {
                        break;
                    };
                    let result = spatial::refresh_live_filtered(pid, &scene, &rules, &cancel);
                    if let Ok(mut inner) = shared.lock() {
                        if inner.generation != generation || cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        match result {
                            Ok(scene) => {
                                if inner.status.revision == revision {
                                    inner.previous_objects =
                                        inner.scene.as_ref().map(|s| (revision, s.objects.clone()));
                                    inner.local_pose_reader =
                                        spatial::LocalPoseReader::from_scene(&scene);
                                    inner.scene = Some(scene);
                                    inner.status.revision += 1;
                                }
                            }
                            Err(error) => {
                                inner.status.error = Some(error);
                                inner.status.tracking = false;
                                inner.desired_live = false;
                                inner.live_pid = None;
                                if let Some(scene) = &mut inner.scene {
                                    scene.complete = false;
                                    scene.warnings.push(
                                        "Обновление объектов остановлено. Прочитайте сцену заново."
                                            .into(),
                                    );
                                }
                                inner.status.revision += 1;
                                break;
                            }
                        }
                    }
                    while let Some(remaining) =
                        Duration::from_secs(1).checked_sub(tick_started.elapsed())
                    {
                        if cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        std::thread::sleep(remaining.min(Duration::from_millis(100)));
                    }
                }
            });
        if let Err(error) = spawned {
            self.inner
                .lock()
                .map_err(|e| e.to_string())?
                .status
                .tracking = false;
            return Err(error.to_string());
        }
        self.status()
    }
}

// Подгрузка комнаты LS_CREATE_EX не означает переход в другую миссию.
fn log_message(line: &str) -> Option<&str> {
    let Some((timestamp, message)) = line.trim().split_once(' ') else {
        return None;
    };
    (timestamp.contains('.')
        && timestamp.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        && timestamp.parse::<f64>().is_ok_and(f64::is_finite))
    .then_some(message.trim_start())
}
fn mission_changed(line: &str) -> bool {
    log_message(line) == Some("Sys [Info]: FSM::LoadLevel")
}
fn level_loaded(line: &str) -> bool {
    log_message(line) == Some("Game [Info]: Level loader: LS_POST_CREATE -> LS_COMPLETE")
}

pub(crate) fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let state = app.state::<AppState>();
            let Ok(mut service) = state.mission_research.lock() else {
                continue;
            };
            let Some(pid) = find_wf_pid() else {
                service.reset_log();
                continue;
            };
            if service.take_auto_request(Instant::now()) {
                // Одна попытка на локацию. Ошибка не запускает цикл тяжёлых повторов.
                let _ = service.start(Source::Live(pid), profile_root(&state));
            }
            let ready = service.inner.lock().ok().and_then(|inner| {
                (inner.desired_live && !inner.status.busy && !inner.status.tracking)
                    .then_some(inner.live_pid)
            });
            if let Some(Some(_)) = ready {
                let _ = service.track(true);
            }
        }
    });
}

fn archive_root(state: &AppState) -> PathBuf {
    state
        .data_directory
        .join("diagnostics")
        .join("binary-memory")
}
fn profile_root(state: &AppState) -> PathBuf {
    state.data_directory.join("spatial-profiles")
}
fn archive_path(root: &Path, id: &str) -> Result<PathBuf, String> {
    if !id.starts_with("warframe-binary-")
        || !matches!(
            Path::new(id).components().collect::<Vec<_>>().as_slice(),
            [Component::Normal(_)]
        )
        || id.contains(['/', '\\', ':'])
    {
        return Err("Некорректное имя записи.".into());
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let path = root.join(id).canonicalize().map_err(|e| e.to_string())?;
    if path.parent() != Some(root.as_path()) || !path.is_dir() {
        return Err("Запись находится вне папки диагностики.".into());
    }
    Ok(path)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ArchiveView {
    id: String,
    label: String,
    created_at: String,
    size_bytes: u64,
    snapshots: Vec<spatial::ArchiveSnapshot>,
}

#[tauri::command]
pub(crate) fn mission_research_status(state: State<'_, AppState>) -> Result<Status, String> {
    state
        .mission_research
        .lock()
        .map_err(|e| e.to_string())?
        .status()
}
#[tauri::command]
pub(crate) fn mission_research_scene(state: State<'_, AppState>) -> Result<Option<Scene>, String> {
    localized_scene(&state)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SceneUpdate {
    revision: u64,
    epoch: u64,
    reset_objects: bool,
    geometry_included: bool,
    removed: Vec<String>,
    scene: Option<Scene>,
}

fn scene_update(
    inner: &Inner,
    after_revision: Option<u64>,
    geometry_epoch: Option<u64>,
) -> SceneUpdate {
    let geometry_included = geometry_epoch != Some(inner.scene_epoch);
    let previous = inner
        .previous_objects
        .as_ref()
        .filter(|(revision, _)| Some(*revision) == after_revision && !geometry_included);
    let mut scene = inner.scene.clone();
    let mut removed = Vec::new();
    if let Some(scene) = &mut scene {
        if !geometry_included {
            scene.meshes = Default::default();
        }
        if let Some((_, objects)) = previous {
            let before: std::collections::HashMap<_, _> =
                objects.iter().map(|o| (o.key.as_str(), o)).collect();
            let now: std::collections::HashSet<_> =
                scene.objects.iter().map(|o| o.key.as_str()).collect();
            removed = objects
                .iter()
                .filter(|o| !now.contains(o.key.as_str()))
                .map(|o| o.key.clone())
                .collect();
            scene
                .objects
                .retain(|o| before.get(o.key.as_str()).is_none_or(|old| *old != o));
        }
    }
    SceneUpdate {
        revision: inner.status.revision,
        epoch: inner.scene_epoch,
        reset_objects: previous.is_none(),
        geometry_included,
        removed,
        scene,
    }
}

#[tauri::command]
pub(crate) fn mission_research_update(
    state: State<'_, AppState>,
    after_revision: Option<u64>,
    geometry_epoch: Option<u64>,
) -> Result<SceneUpdate, String> {
    let mut update = {
        let service = state.mission_research.lock().map_err(|e| e.to_string())?;
        let inner = service.inner.lock().map_err(|e| e.to_string())?;
        scene_update(&inner, after_revision, geometry_epoch)
    };
    if let Some(scene) = &mut update.scene {
        localize_scene(scene, &crate::game_names::get(&state));
    }
    Ok(update)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PoseUpdate {
    epoch: u64,
    pose: Option<spatial::LocalPose>,
}

#[tauri::command]
pub(crate) async fn mission_research_pose(
    state: State<'_, AppState>,
    epoch: u64,
) -> Result<PoseUpdate, String> {
    let shared = state
        .mission_research
        .lock()
        .map_err(|e| e.to_string())?
        .inner
        .clone();
    let sample = {
        let inner = shared.lock().map_err(|e| e.to_string())?;
        if inner.scene_epoch != epoch || !inner.status.tracking {
            return Ok(PoseUpdate { epoch, pose: None });
        }
        inner
            .local_pose_reader
            .clone()
            .map(|reader| (reader, inner.generation))
    };
    let Some((reader, generation)) = sample else {
        return Ok(PoseUpdate { epoch, pose: None });
    };
    let pose = tauri::async_runtime::spawn_blocking(move || reader.read().ok())
        .await
        .map_err(|e| e.to_string())?;
    let inner = shared.lock().map_err(|e| e.to_string())?;
    let valid =
        inner.scene_epoch == epoch && inner.generation == generation && inner.status.tracking;
    Ok(PoseUpdate {
        epoch,
        pose: if valid { pose } else { None },
    })
}

fn localized_scene(state: &AppState) -> Result<Option<Scene>, String> {
    let service = state.mission_research.lock().map_err(|e| e.to_string())?;
    let mut scene = service
        .inner
        .lock()
        .map_err(|e| e.to_string())?
        .scene
        .clone();
    drop(service);
    if let Some(scene) = &mut scene {
        localize_scene(scene, &crate::game_names::get(state));
    }
    Ok(scene)
}

fn localize_scene(scene: &mut Scene, names: &crate::game_names::GameNames) {
    for object in &mut scene.objects {
        if let Some(path) = &object.item_path
            && let Some((ru, en, _)) = names.lookup(path)
        {
            if !ru.is_empty() {
                object.label.clone_from(ru);
            } else if !en.is_empty() {
                object.label.clone_from(en);
            }
            if !en.is_empty() {
                object.name_en.clone_from(en);
            }
        }
    }
}
#[tauri::command]
pub(crate) async fn mission_research_archives(app: AppHandle) -> Result<Vec<ArchiveView>, String> {
    let root = archive_root(&app.state::<AppState>());
    tauri::async_runtime::spawn_blocking(move || list_archives(&root))
        .await
        .map_err(|e| e.to_string())?
}
fn list_archives(root: &Path) -> Result<Vec<ArchiveView>, String> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let entries = fs::read_dir(root).map_err(|e| e.to_string())?;
    let mut archives = Vec::new();
    for entry in entries.take(1000) {
        let entry = entry.map_err(|e| e.to_string())?;
        let id = entry.file_name().to_string_lossy().into_owned();
        let Ok(path) = archive_path(root, &id) else {
            continue;
        };
        let Ok(snapshots) = spatial::list_archive(&path) else {
            continue;
        };
        if snapshots.is_empty() {
            continue;
        }
        let created_at = snapshots[0].started_at.clone();
        let size_bytes = fs::metadata(path.join("blocks.bin")).map_or(0, |m| m.len());
        archives.push(ArchiveView {
            label: id.clone(),
            id,
            created_at,
            size_bytes,
            snapshots,
        });
    }
    archives.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(archives)
}
#[tauri::command]
pub(crate) fn mission_research_scan_live(state: State<'_, AppState>) -> Result<Status, String> {
    let pid = find_wf_pid().ok_or("Запустите Warframe перед чтением сцены.")?;
    state
        .mission_research
        .lock()
        .map_err(|e| e.to_string())?
        .start(Source::Live(pid), profile_root(&state))
}
#[tauri::command]
pub(crate) fn mission_research_analyze_archive(
    id: String,
    sequence: u64,
    state: State<'_, AppState>,
) -> Result<Status, String> {
    let path = archive_path(&archive_root(&state), &id)?;
    state
        .mission_research
        .lock()
        .map_err(|e| e.to_string())?
        .start(Source::Archive(path, sequence), profile_root(&state))
}
#[tauri::command]
pub(crate) fn mission_research_cancel(state: State<'_, AppState>) -> Result<Status, String> {
    let service = state.mission_research.lock().map_err(|e| e.to_string())?;
    service.stop();
    service.status()
}
#[tauri::command]
pub(crate) fn mission_research_track(
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<Status, String> {
    state
        .mission_research
        .lock()
        .map_err(|e| e.to_string())?
        .track(enabled)
}
#[tauri::command]
pub(crate) fn mission_research_filters(
    keys: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let rules = spatial::DiscoveryRules::parse(keys)?;
    let service = state.mission_research.lock().map_err(|e| e.to_string())?;
    service
        .inner
        .lock()
        .map_err(|e| e.to_string())?
        .discovery_rules = Arc::new(rules);
    Ok(())
}
#[tauri::command]
pub(crate) async fn mission_research_export(
    format: String,
    app: AppHandle,
) -> Result<String, String> {
    let state = app.state::<AppState>();
    let scene = localized_scene(&state)?.ok_or("Сначала прочитайте сцену.")?;
    let root = state.data_directory.join("mission-exports");
    tauri::async_runtime::spawn_blocking(move || export(&root, &format, &scene))
        .await
        .map_err(|e| e.to_string())?
}
fn export(root: &Path, format: &str, scene: &Scene) -> Result<String, String> {
    let bytes = match format {
        "json" => serde_json::to_vec_pretty(scene).map_err(|e| e.to_string())?,
        "obj" => obj(scene)?.into_bytes(),
        _ => return Err("Поддерживаются JSON и OBJ.".into()),
    };
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let path = root.join(format!(
        "mission-{}.{}",
        Utc::now().format("%Y%m%dT%H%M%S%9fZ"),
        format
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}
fn obj(scene: &Scene) -> Result<String, String> {
    use std::fmt::Write as _;
    let mut text =
        String::from("# PlatScope: исследуемая геометрия; покрытие уровня не гарантировано.\n");
    writeln!(
        text,
        "# source={} complete={} capturedAt={}",
        scene.source, scene.complete, scene.captured_at
    )
    .map_err(|e| e.to_string())?;
    let mut offset = 1_usize;
    for (index, mesh) in scene.meshes.iter().enumerate() {
        writeln!(text, "o level_{}", index + 1).map_err(|e| e.to_string())?;
        for v in &mesh.vertices {
            if !v.iter().all(|v| v.is_finite()) {
                return Err("Некорректная координата геометрии.".into());
            }
            writeln!(text, "v {} {} {}", v[0], v[1], v[2]).map_err(|e| e.to_string())?;
        }
        for face in &mesh.faces {
            if face.len() < 3 || face.iter().any(|i| *i as usize >= mesh.vertices.len()) {
                return Err("Некорректная грань геометрии.".into());
            }
            text.push('f');
            for i in face {
                write!(text, " {}", offset + *i as usize).map_err(|e| e.to_string())?;
            }
            text.push('\n');
        }
        offset = offset
            .checked_add(mesh.vertices.len())
            .ok_or("Слишком большая геометрия")?;
    }
    Ok(text)
}
pub(crate) fn on_close(window: &tauri::Window, event: &tauri::WindowEvent) {
    if window.label() == "main"
        && matches!(event, tauri::WindowEvent::CloseRequested { .. })
        && let Some(state) = window.try_state::<AppState>()
        && let Ok(service) = state.mission_research.lock()
    {
        service.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample_scene(source: &str) -> Scene {
        serde_json::from_value(serde_json::json!({
            "format":1,"source":source,"startedAt":"2026-09-12T06:14:44Z",
            "capturedAt":"2026-09-12T06:15:51Z","complete":false,"profile":"test-profile",
            "objects":[{"key":"object-test","kind":"feather","label":"Перо","nameEn":"Voidplume",
                "itemPath":"/Lotus/Test","position":[1,2,3],"typeNames":["PickUp *"],
                "availability":"unknown","details":[]}],
            "meshes":[
                {"key":"a","vertices":[[0,0,0],[1,0,0],[0,1,0]],"faces":[[0,1,2]],"adjacency":[]},
                {"key":"b","vertices":[[2,0,0],[3,0,0],[2,1,0]],"faces":[[0,2,1]],"adjacency":[]}
            ],"warnings":["Покрытие неполное"],"stats":{"scannedBytes":123,"objectCount":1,"meshCount":2,"vertexCount":6,"faceCount":2}
        })).unwrap()
    }

    #[test]
    fn scene_delta_preserves_geometry_and_recovers_missed_revision() {
        let before = sample_scene("live");
        let mut scene = before.clone();
        let mut added = scene.objects[0].clone();
        added.key = "added".into();
        scene.objects[0].position[0] += 1.0;
        scene.objects.push(added);
        let mut removed = before.objects[0].clone();
        removed.key = "removed".into();
        let mut baseline = before.objects.clone();
        baseline.push(removed);
        let mut inner = Inner {
            scene: Some(scene),
            scene_epoch: 3,
            previous_objects: Some((7, baseline)),
            ..Inner::default()
        };
        inner.status.revision = 8;
        let delta = scene_update(&inner, Some(7), Some(3));
        assert!(!delta.reset_objects && !delta.geometry_included);
        assert_eq!(delta.removed, ["removed"]);
        assert_eq!(delta.scene.unwrap().objects.len(), 2);
        assert!(
            scene_update(&inner, Some(7), Some(3))
                .scene
                .unwrap()
                .meshes
                .is_empty()
        );
        let recovered = scene_update(&inner, Some(6), Some(3));
        assert!(recovered.reset_objects && !recovered.geometry_included);
        assert_eq!(recovered.scene.unwrap().objects.len(), 2);
        let initial = scene_update(&inner, None, None);
        assert!(initial.reset_objects && initial.geometry_included);
        assert!(Arc::ptr_eq(&initial.scene.unwrap().meshes, &before.meshes));
        inner.previous_objects = Some((8, inner.scene.as_ref().unwrap().objects.clone()));
        assert!(
            scene_update(&inner, Some(8), Some(3))
                .scene
                .unwrap()
                .objects
                .is_empty()
        );
    }

    #[test]
    fn scene_localization_preserves_coordinates_and_identity() {
        let mut scene = sample_scene("live");
        scene.objects[0].item_path = Some("/Lotus/StoreItems/Test".into());
        let before = scene.objects[0].clone();
        localize_scene(&mut scene, &crate::game_names::test_names());
        assert_eq!(scene.objects[0].label, "Чертёж");
        assert_eq!(scene.objects[0].name_en, "Test");
        assert_eq!(
            scene.objects[0].position.map(f32::to_bits),
            before.position.map(f32::to_bits)
        );
        assert_eq!(scene.objects[0].key, before.key);
        assert_eq!(scene.objects[0].kind, before.kind);
        assert_eq!(scene.objects[0].availability, before.availability);
    }

    #[test]
    fn obj_keeps_provenance_and_offsets_indices_between_meshes() {
        let text = obj(&sample_scene("archive")).unwrap();
        assert!(text.contains("source=archive complete=false capturedAt=2026-09-12T06:15:51Z"));
        assert!(text.contains("o level_1\n"));
        assert!(text.contains("o level_2\n"));
        assert!(text.contains("f 1 2 3\n"));
        assert!(text.contains("f 4 6 5\n"));
        assert_eq!(text.lines().filter(|l| l.starts_with("v ")).count(), 6);
    }

    #[test]
    fn obj_rejects_invalid_geometry_before_export_creates_a_directory() {
        let mut scene = sample_scene("archive");
        Arc::make_mut(&mut scene.meshes)[0].faces[0][2] = 99;
        assert!(obj(&scene).is_err());
        let directory = std::env::temp_dir().join(format!(
            "platscope-invalid-obj-{}",
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        assert!(!directory.exists());
        assert!(export(&directory, "obj", &scene).is_err());
        assert!(!directory.exists());
        Arc::make_mut(&mut scene.meshes)[0].faces[0] = vec![0, 1];
        assert!(obj(&scene).is_err());
        Arc::make_mut(&mut scene.meshes)[0].faces[0] = vec![0, 1, 2];
        Arc::make_mut(&mut scene.meshes)[0].vertices[0][0] = f32::NAN;
        assert!(obj(&scene).is_err());
    }

    #[test]
    fn json_export_preserves_unknown_availability_and_warnings() {
        let directory = std::env::temp_dir().join(format!(
            "platscope-scene-export-{}",
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let filename = export(&directory, "json", &sample_scene("archive")).unwrap();
        let restored: Scene = serde_json::from_slice(&fs::read(&filename).unwrap()).unwrap();
        assert_eq!(restored.source, "archive");
        assert!(!restored.complete);
        assert_eq!(restored.objects[0].availability, "unknown");
        assert_eq!(restored.warnings, ["Покрытие неполное"]);
        assert_eq!(restored.meshes.len(), 2);
        fs::remove_file(filename).unwrap();
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn level_change_does_not_cancel_archive_analysis_and_stop_keeps_previous_scene() {
        let mut service = Service::default();
        {
            let mut inner = service.inner.lock().unwrap();
            inner.scene = Some(sample_scene("archive"));
            inner.status.busy = true;
            inner.status.revision = 7;
            inner.desired_live = false;
        }
        service.feed_log("Level loader: LS_PREPARE -> LS_CREATE_EX\n");
        assert!(!service.cancel.load(Ordering::Relaxed));
        assert_eq!(service.inner.lock().unwrap().status.revision, 7);
        service.stop();
        let inner = service.inner.lock().unwrap();
        assert!(service.cancel.load(Ordering::Relaxed));
        assert!(inner.status.busy);
        assert!(inner.status.cancelling);
        assert!(!inner.desired_live);
        assert_eq!(inner.scene.as_ref().unwrap().source, "archive");
        assert_eq!(inner.scene.as_ref().unwrap().objects.len(), 1);
    }

    #[test]
    fn rejects_paths_outside_archive_directory() {
        for id in [
            "../warframe-binary-test",
            "warframe-binary-a/../b",
            "warframe-binary-a\\b",
            "warframe-binary-a:stream",
            "C:\\warframe-binary-a",
        ] {
            assert!(archive_path(Path::new("."), id).is_err());
        }
    }
    #[test]
    fn location_auto_start_waits_for_loading_and_ignores_history_rooms_and_stop() {
        let mut service = Service::default();
        // История нескольких миссий не должна порождать несколько запусков.
        service.feed_log("1.000 Sys [Info]: FSM::LoadLevel\n2.000 Game [Info]: Level loader: LS_POST_CREATE -> LS_COMPLETE\n3.000 Sys [Info]: FSM::Load");
        assert!(!service.take_auto_request(Instant::now() + Duration::from_secs(30)));
        service.feed_log("Level\n4.000 Game [Info]: Level loader: LS_POST_CREATE -> LS_COMPLETE\n");
        service.set_log_ready(true);
        assert!(!service.take_auto_request(Instant::now()));
        assert!(service.take_auto_request(Instant::now() + Duration::from_secs(4)));
        assert!(!service.take_auto_request(Instant::now() + Duration::from_secs(5)));
        // Повтор завершения загрузки и поток комнат не запрашивают новую карту.
        service.feed_log("4.000 Game [Info]: Level loader: LS_POST_CREATE -> LS_COMPLETE\n5.000 Game [Info]: Level loader: LS_PREPARE -> LS_CREATE_EX\n6.000 Game [Info]: Level loader: LS_POST_CREATE -> LS_COMPLETE\n7.000 Chat [Info]: Sys [Info]: FSM::LoadLevel\n");
        assert!(!service.take_auto_request(Instant::now() + Duration::from_secs(30)));
        {
            let mut inner = service.inner.lock().unwrap();
            inner.desired_live = true;
            inner.status.busy = true;
            inner.status.tracking = true;
            inner.scene = Some(sample_scene("live"));
        }
        service.feed_log("8.000 Sys [Info]: FSM::Load");
        assert!(service.inner.lock().unwrap().status.tracking);
        service.feed_log("Level\n9.000 Game [Info]: Level loader: LS_POST_CREATE -> LS_COMPLETE\n");
        assert!(!service.inner.lock().unwrap().status.tracking);
        assert!(!service.inner.lock().unwrap().desired_live);
        assert!(service.cancel.load(Ordering::Relaxed));
        assert!(service.inner.lock().unwrap().scene.is_none());
        assert!(
            !service.take_auto_request(Instant::now() + Duration::from_secs(30)),
            "Ждём выхода отменённого рабочего потока"
        );
        service.inner.lock().unwrap().status.busy = false;
        assert!(service.take_auto_request(Instant::now() + Duration::from_secs(30)));
        // Реальный порядок EE.log: путь Орбитера раньше FSM, фон перелёта позже.
        service.feed_log("100780.492 Game [Info]: FrameworkCmd::OpenLevel - /Lotus/Levels/Proc/PlayerShip\n100780.742 Game [Info]: Level=/Lotus/Levels/Proc/PlayerShip/AeAg.lp\n100780.766 Sys [Info]: FSM::LoadLevel\n100781.383 Sys [Info]: RegionMgrImpl::SetLevel /Lotus/Levels/Episodes/LisetInFlightAtmosphere.level\n100784.258 Game [Info]: Level loader: LS_POST_CREATE -> LS_COMPLETE\n");
        service.set_log_ready(false);
        service.set_log_ready(true);
        assert!(service.in_orbiter);
        assert!(service.auto_after.is_none());
        assert!(!service.take_auto_request(Instant::now() + Duration::from_secs(30)));
        service.feed_log("100799.023 Game [Info]: FrameworkCmd::OpenLevel - /Lotus/Levels/Proc/Grineer/GrineerOceanExterminateAnywhere\n100799.312 Game [Info]: Level=/Lotus/Levels/Proc/Grineer/GrineerOceanExterminateAnywhere/DREISLKUKJ+7uqqoAA.lp\n100799.352 Sys [Info]: FSM::LoadLevel\n100810.000 Game [Info]: Level loader: LS_POST_CREATE -> LS_COMPLETE\n100811.000 Chat [Info]: Game [Info]: Level=/Lotus/Levels/Proc/PlayerShip/AeAg.lp\n");
        assert!(!service.in_orbiter);
        assert!(service.take_auto_request(Instant::now() + Duration::from_secs(30)));
        // Подключение к хосту: путь получен в подтверждении клиентской загрузки,
        // без локальных OpenLevel/Level=. Фон и строки чата не меняют локацию.
        service.feed_log("12.000 Game [Info]: Level=/Lotus/Levels/Proc/PlayerShip/AeAv.lp\n13.000 Sys [Info]: FSM::LoadLevel\n14.000 Sys [Info]: Client finished loading /Lotus/Levels/Proc/Orokin/OrokinTowerDerelictCapture/example.lp. Sending CMSG_LOAD_COMPLETE to ser");
        assert!(service.in_orbiter);
        service.feed_log("ver.\n15.000 Sys [Info]: RegionMgrImpl::SetLevel /Lotus/Levels/Episodes/LisetInFlight.level\n16.000 Game [Info]: Level loader: LS_POST_CREATE -> LS_COMPLETE\n17.000 Chat [Info]: Sys [Info]: Client finished loading /Lotus/Levels/Proc/PlayerShip/AeAv.lp. Sending CMSG_LOAD_COMPLETE to server.\n");
        assert!(!service.in_orbiter);
        assert!(service.take_auto_request(Instant::now() + Duration::from_secs(30)));
        service.feed_log("18.000 Sys [Info]: FSM::LoadLevel\n19.000 Sys [Info]: Client finished loading /Lotus/Levels/Proc/PlayerShip/AeAv.lp. Sending CMSG_LOAD_COMPLETE to server.\n20.000 Game [Info]: Level loader: LS_POST_CREATE -> LS_COMPLETE\n");
        assert!(service.in_orbiter);
        assert!(!service.take_auto_request(Instant::now() + Duration::from_secs(30)));
        service.feed_log("20.500 Game [Info]: Level=/Lotus/Levels/Proc/Orokin/OrokinTowerDerelictCapture/example.lp\n");
        service.stop();
        service.feed_log("10.000 Sys [Info]: FSM::LoadLevel\n11.000 Game [Info]: Level loader: LS_POST_CREATE -> LS_COMPLETE\n");
        assert!(
            !service.take_auto_request(Instant::now() + Duration::from_secs(30)),
            "Ручная остановка запрещает автоматический перезапуск"
        );
        service.reset_log();
        assert!(!service.log_ready && service.last_load.is_none() && service.auto_after.is_none());
    }
    #[test]
    fn streaming_rooms_never_clears_live_scene_or_requests_full_scan() {
        let mut service = Service::default();
        {
            let mut inner = service.inner.lock().unwrap();
            inner.scene = Some(sample_scene("live"));
            inner.live_pid = Some(123);
            inner.desired_live = true;
            inner.status.tracking = true;
            inner.status.revision = 9;
        }
        for _ in 0..100 {
            service.feed_log("123.000 Game [Info]: Level loader: LS_PREPARE -> LS_CREATE_EX\n123.500 Game [Info]: Level loader: LS_POST_CREATE -> LS_COMPLETE\n");
        }
        let inner = service.inner.lock().unwrap();
        assert!(inner.status.tracking);
        assert!(!inner.status.busy);
        assert_eq!(inner.status.revision, 9);
        assert!(inner.scene.is_some());
        assert!(!service.cancel.load(Ordering::Relaxed));
        assert!(!mission_changed(
            "123.000 Chat [Info]: Sys [Info]: FSM::LoadLevel"
        ));
    }
}
