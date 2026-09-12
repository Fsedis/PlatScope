//! Ручное чтение сцены и локальных записей. Длительные операции выполняются вне UI.
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
    time::Duration,
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
}
#[derive(Default)]
struct Inner {
    status: Status,
    scene: Option<Scene>,
    generation: u64,
    live_pid: Option<u32>,
    desired_live: bool,
}
#[derive(Default)]
pub(crate) struct Service {
    inner: Arc<Mutex<Inner>>,
    cancel: Arc<AtomicBool>,
    log_tail: String,
}
enum Source {
    Live(u32),
    Archive(PathBuf, u64),
}

impl Service {
    fn status(&self) -> Result<Status, String> {
        let mut status = self.inner.lock().map_err(|e| e.to_string())?.status.clone();
        status.game_running = find_wf_pid().is_some();
        Ok(status)
    }
    fn start(&mut self, source: Source) -> Result<Status, String> {
        let generation = {
            let mut inner = self.inner.lock().map_err(|e| e.to_string())?;
            if inner.status.busy {
                return Err("Дождитесь завершения текущего чтения или отмените его.".into());
            }
            inner.desired_live = matches!(&source, Source::Live(_));
            inner.generation += 1;
            inner.status.busy = true;
            inner.status.tracking = false;
            inner.status.cancelling = false;
            inner.status.error = None;
            inner.status.phase = "Подготовка чтения".into();
            inner.status.scanned_bytes = 0;
            inner.generation
        };
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
                let result = match source {
                    Source::Live(pid) => spatial::analyze_live(pid, &cancel, report),
                    Source::Archive(path, sequence) => {
                        spatial::analyze_archive(&path, sequence, &cancel, report)
                    }
                };
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
                                inner.scene = Some(scene);
                                inner.live_pid = pid;
                                inner.status.revision += 1;
                                inner.status.phase = "Чтение завершено".into();
                            }
                            Err(error) => {
                                inner.desired_live = false;
                                inner.status.error = Some(error);
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
                inner.status.revision += 1;
            }
            inner.status.phase = "Миссия сменилась. Запустите карту для новой миссии.".into();
        }
    }
    pub(crate) fn feed_log(&mut self, chunk: &str) {
        self.log_tail.push_str(chunk);
        if let Some(end) = self.log_tail.rfind('\n') {
            if self.log_tail[..end].lines().any(mission_changed) {
                self.invalidate();
            }
            self.log_tail.drain(..=end);
        }
        if self.log_tail.len() > 8192 {
            self.log_tail.clear();
        }
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
                    let sample = shared.lock().ok().and_then(|s| {
                        (s.generation == generation && s.status.tracking)
                            .then(|| s.scene.clone().map(|scene| (scene, s.status.revision)))
                            .flatten()
                    });
                    let Some((scene, revision)) = sample else {
                        break;
                    };
                    let result = spatial::refresh_live(pid, &scene, &cancel);
                    if let Ok(mut inner) = shared.lock() {
                        if inner.generation != generation || cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        match result {
                            Ok(scene) => {
                                if inner.status.revision == revision {
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
                    for _ in 0..10 {
                        if cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(100));
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
fn mission_changed(line: &str) -> bool {
    let Some((timestamp, message)) = line.trim().split_once(' ') else {
        return false;
    };
    timestamp.contains('.')
        && timestamp.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        && timestamp.parse::<f64>().is_ok_and(f64::is_finite)
        && message.trim_start() == "Sys [Info]: FSM::LoadLevel"
}

pub(crate) fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let state = app.state::<AppState>();
            let Ok(mut service) = state.mission_research.lock() else {
                continue;
            };
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
    let service = state.mission_research.lock().map_err(|e| e.to_string())?;
    Ok(service
        .inner
        .lock()
        .map_err(|e| e.to_string())?
        .scene
        .clone())
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
        .start(Source::Live(pid))
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
        .start(Source::Archive(path, sequence))
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
pub(crate) async fn mission_research_export(
    format: String,
    app: AppHandle,
) -> Result<String, String> {
    let state = app.state::<AppState>();
    let scene = state
        .mission_research
        .lock()
        .map_err(|e| e.to_string())?
        .inner
        .lock()
        .map_err(|e| e.to_string())?
        .scene
        .clone()
        .ok_or("Сначала прочитайте сцену.")?;
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
        scene.meshes[0].faces[0][2] = 99;
        assert!(obj(&scene).is_err());
        let directory = std::env::temp_dir().join(format!(
            "platscope-invalid-obj-{}",
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        assert!(!directory.exists());
        assert!(export(&directory, "obj", &scene).is_err());
        assert!(!directory.exists());
        scene.meshes[0].faces[0] = vec![0, 1];
        assert!(obj(&scene).is_err());
        scene.meshes[0].faces[0] = vec![0, 1, 2];
        scene.meshes[0].vertices[0][0] = f32::NAN;
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
    fn level_change_stops_tracking_even_with_split_line() {
        let mut service = Service::default();
        service.inner.lock().unwrap().status.tracking = true;
        service.inner.lock().unwrap().desired_live = true;
        service.feed_log("123.000 Sys [Info]: FSM::Load");
        assert!(service.inner.lock().unwrap().status.tracking);
        service.feed_log("Level\n");
        assert!(!service.inner.lock().unwrap().status.tracking);
        assert!(service.cancel.load(Ordering::Relaxed));
        assert!(!service.inner.lock().unwrap().desired_live);
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
