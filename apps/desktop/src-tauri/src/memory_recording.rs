//! Запись ограниченных исследований памяти только по явному запуску пользователем.
use crate::AppState;
use chrono::Utc;
use platscope_readonly_scan::{research, scan::find_wf_pid};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

const MAX_BYTES: u64 = 100 * 1024 * 1024;
const MAX_DURATION: Duration = Duration::from_secs(3600);

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct View {
    active: bool,
    scanning: bool,
    path: Option<String>,
    started_at: Option<String>,
    samples: u64,
    bytes: u64,
    changed_fields: Vec<String>,
    error: Option<String>,
    stop_reason: Option<String>,
}

struct Session {
    id: String,
    file: File,
    pid: u32,
    started: Instant,
    due: Instant,
    previous: BTreeMap<String, Value>,
}

#[derive(Default)]
pub(crate) struct Recorder {
    session: Option<Session>,
    status: View,
}

impl Recorder {
    fn append(&mut self, entry: &Value) -> Result<(), String> {
        let session = self.session.as_mut().ok_or("Запись не запущена.")?;
        let mut bytes = serde_json::to_vec(entry).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        if self.status.bytes + bytes.len() as u64 > MAX_BYTES {
            return Err("Достигнут предел записи 100 МиБ.".into());
        }
        session
            .file
            .write_all(&bytes)
            .and_then(|()| session.file.sync_data())
            .map_err(|e| format!("Не удалось сохранить запись: {e}"))?;
        self.status.bytes += bytes.len() as u64;
        Ok(())
    }

    fn start(&mut self, directory: &Path, pid: u32) -> Result<(), String> {
        if self.session.is_some() {
            return Err("Запись уже идёт.".into());
        }
        std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        let id = Utc::now().format("%Y%m%dT%H%M%S%9fZ").to_string();
        let path = directory.join(format!("warframe-memory-{id}.jsonl"));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        self.status = View {
            active: true,
            path: Some(path.display().to_string()),
            started_at: Some(Utc::now().to_rfc3339()),
            ..View::default()
        };
        self.session = Some(Session {
            id,
            file,
            pid,
            started: Instant::now(),
            due: Instant::now(),
            previous: BTreeMap::new(),
        });
        if let Err(error) = self.append(&json!({"kind":"session_start", "format":1, "at":Utc::now(), "intervalSeconds":30, "maxMinutes":60, "description":"Фильтрованные схемы игровых JSON-полей и примеры значений. Не полный дамп. Владение и актуальность найденных объектов не установлены. Изменения схемы не равны событиям игры."})) {
            self.status.error = Some(error.clone()); self.session = None; self.status.active = false; return Err(error);
        }
        Ok(())
    }

    pub(crate) fn stop(&mut self, reason: &str) {
        if self.session.is_none() {
            return;
        }
        if let Err(error) = self.append(&json!({"kind":"session_end", "at":Utc::now(), "samples":self.status.samples, "reason":reason})) { self.status.error = Some(error); }
        self.session = None;
        self.status.active = false;
        self.status.scanning = false;
        self.status.stop_reason = Some(reason.into());
    }

    fn accept(&mut self, id: &str, report: &Value) {
        let Some(session) = self.session.as_mut().filter(|s| s.id == id) else {
            return;
        };
        let mut current = BTreeMap::new();
        if let Some(results) = report["results"].as_object() {
            for (key, result) in results {
                current.insert(
                    key.clone(),
                    json!({"schema":result["schema"],"suitDetails":result["suitDetails"]}),
                );
            }
        }
        if let Some(resources) = report["resourcePaths"].as_object() {
            current.insert(
                "resourcePaths".into(),
                json!(resources.keys().collect::<Vec<_>>()),
            );
        }
        let changed: Vec<_> = current
            .iter()
            .filter(|(key, value)| session.previous.get(*key) != Some(*value))
            .map(|(key, _)| key.clone())
            .collect();
        session.previous = current;
        session.due = Instant::now() + Duration::from_secs(30);
        let entry = json!({"kind":"snapshot", "sequence":self.status.samples+1, "at":Utc::now(), "changedFields":changed, "report":report});
        if let Err(error) = self.append(&entry) {
            self.status.error = Some(error);
            self.stop("Запись остановлена из-за ошибки или ограничения размера.");
            return;
        }
        self.status.samples += 1;
        self.status.changed_fields = changed;
        self.status.scanning = false;
    }
}

pub(crate) fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let task_app = app.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || poll(&task_app)).await;
        }
    });
}

pub(crate) fn on_close(window: &tauri::Window, event: &tauri::WindowEvent) {
    if window.label() == "main"
        && matches!(event, tauri::WindowEvent::CloseRequested { .. })
        && let Some(state) = window.try_state::<AppState>()
        && let Ok(mut recorder) = state.memory_recording.lock()
    {
        recorder.stop("PlatScope завершает работу.");
    }
}

fn poll(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Ok(mut recorder) = state.memory_recording.lock() else {
        return;
    };
    let Some(session) = recorder.session.as_ref() else {
        return;
    };
    if session.started.elapsed() >= MAX_DURATION {
        recorder.stop("Достигнут предел записи 60 минут.");
        return;
    }
    let pid = session.pid;
    let id = session.id.clone();
    if find_wf_pid() != Some(pid) {
        recorder.stop("Игра завершилась или процесс изменился.");
        return;
    }
    if session.due > Instant::now() {
        return;
    }
    recorder.status.scanning = true;
    drop(recorder);
    let result = research::capture(pid);
    let Ok(mut recorder) = state.memory_recording.lock() else {
        return;
    };
    if recorder.session.as_ref().is_none_or(|s| s.id != id) {
        return;
    }
    if find_wf_pid() != Some(pid) {
        recorder.stop("Игра завершилась или процесс изменился.");
        return;
    }
    match result {
        Ok(report) => recorder.accept(&id, &report),
        Err(error) => {
            recorder.status.error = Some(error.to_string());
            recorder.stop("Не удалось прочитать память игры.");
        }
    }
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn memory_recording_status(state: State<'_, AppState>) -> Result<View, String> {
    Ok(state
        .memory_recording
        .lock()
        .map_err(|e| e.to_string())?
        .status
        .clone())
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn memory_recording_start(state: State<'_, AppState>) -> Result<View, String> {
    let pid = find_wf_pid().ok_or("Запустите Warframe перед записью.")?;
    let mut recorder = state.memory_recording.lock().map_err(|e| e.to_string())?;
    recorder.start(&state.data_directory.join("diagnostics/memory"), pid)?;
    Ok(recorder.status.clone())
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn memory_recording_stop(state: State<'_, AppState>) -> Result<View, String> {
    let mut recorder = state.memory_recording.lock().map_err(|e| e.to_string())?;
    recorder.stop("Остановлено пользователем.");
    Ok(recorder.status.clone())
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn memory_recording_sample(state: State<'_, AppState>) -> Result<View, String> {
    let mut recorder = state.memory_recording.lock().map_err(|e| e.to_string())?;
    let session = recorder.session.as_mut().ok_or("Сначала начните запись.")?;
    session.due = Instant::now();
    recorder.append(&json!({"kind":"manual_sample_requested", "at":Utc::now()}))?;
    Ok(recorder.status.clone())
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn memory_recording_folder(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let directory = state.data_directory.join("diagnostics/memory");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(directory.display().to_string(), None::<String>)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Ручная проверка требует запущенного Warframe"]
    fn live_recording_probe() {
        let pid = find_wf_pid().unwrap();
        let mut recorder = Recorder::default();
        recorder
            .start(Path::new("../../../target/memory-recording-preview"), pid)
            .unwrap();
        let id = recorder.session.as_ref().unwrap().id.clone();
        let report = research::capture(pid).unwrap();
        assert!(report["results"].is_object());
        recorder.accept(&id, &report);
        recorder.stop("Проверка записи на запущенной игре завершена.");
        assert_eq!(recorder.status.samples, 1);
        assert!(recorder.status.error.is_none());
        let data = std::fs::read_to_string(recorder.status.path.as_ref().unwrap()).unwrap();
        assert_eq!(data.lines().count(), 3);
        println!(
            "Сохранён полный журнал из трёх записей: {} байт; обход {} мс; завершён {}",
            data.len(),
            report["elapsedMs"],
            report["complete"]
        );
    }
    #[test]
    fn durable_recording_has_footer_and_rejects_late_samples() {
        let directory = std::env::temp_dir().join(format!(
            "platscope-memory-test-{}",
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let mut recorder = Recorder::default();
        recorder.start(&directory, 1).unwrap();
        let id = recorder.session.as_ref().unwrap().id.clone();
        let report = json!({"results":{"Suits":{"schema":{"kind":"array"}}}});
        recorder.accept(&id, &report);
        recorder.accept(&id, &report);
        assert!(recorder.status.changed_fields.is_empty());
        assert_eq!(recorder.status.samples, 2);
        recorder.stop("Тест завершён");
        recorder.accept(&id, &report);
        assert_eq!(recorder.status.samples, 2);
        let path = recorder.status.path.clone().unwrap();
        let lines: Vec<Value> = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 4);
        assert_eq!(lines.last().unwrap()["kind"], "session_end");
        recorder.start(&directory, 1).unwrap();
        recorder.accept(&id, &report);
        assert_eq!(recorder.status.samples, 0);
        recorder.stop("Тест завершён");
        std::fs::remove_file(recorder.status.path.unwrap()).unwrap();
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
