//! Управление локальной двоичной записью без блокировки интерфейса.
use crate::AppState;
use chrono::Utc;
use platscope_readonly_scan::{
    binary_snapshot::{Archive, Process, Progress},
    scan::find_wf_pid,
};
use serde::Serialize;
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct View {
    active: bool,
    stopping: bool,
    scanning: bool,
    path: Option<String>,
    samples: u64,
    bytes: u64,
    read_bytes: u64,
    holes: u64,
    last_complete: Option<bool>,
    error: Option<String>,
    stop_reason: Option<String>,
}

#[derive(Default)]
pub(crate) struct Recorder {
    view: Arc<Mutex<View>>,
    cancel: Arc<AtomicBool>,
    sample: Arc<AtomicBool>,
}

impl Recorder {
    fn status(&self) -> Result<View, String> {
        Ok(self.view.lock().map_err(|e| e.to_string())?.clone())
    }

    fn start(&mut self, root: &Path, limit_gib: u64) -> Result<View, String> {
        if self.status()?.active {
            return Err("Двоичная запись уже идёт или завершается.".into());
        }
        if ![8, 16, 32].contains(&limit_gib) {
            return Err("Выберите предел 8, 16 или 32 ГиБ.".into());
        }
        let pid = find_wf_pid().ok_or("Запустите Warframe перед записью.")?;
        // Фиксируем экземпляр процесса до запуска потока; в потоке проверяем время его создания.
        let created = Process::open(pid)
            .map_err(|e| format!("Не удалось открыть игру: {e}"))?
            .created;
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        if platscope_readonly_scan::binary_snapshot::free_bytes(root).map_err(|e| e.to_string())?
            < 3 * 1024 * 1024 * 1024
        {
            return Err("Для записи нужно не менее 3 ГиБ свободного места.".into());
        }
        let path = root.join(format!(
            "warframe-binary-{}",
            Utc::now().format("%Y%m%dT%H%M%S%9fZ")
        ));
        *self.view.lock().map_err(|e| e.to_string())? = View {
            active: true,
            scanning: true,
            path: Some(path.display().to_string()),
            ..View::default()
        };
        self.cancel = Arc::new(AtomicBool::new(false));
        self.sample = Arc::new(AtomicBool::new(false));
        let (cancel, sample, view) = (self.cancel.clone(), self.sample.clone(), self.view.clone());
        let spawned = std::thread::Builder::new()
            .name("memory-binary-recording".into())
            .spawn(move || {
                let result = record(&path, pid, &created, limit_gib, &cancel, &sample, &view);
                if let Ok(mut status) = view.lock() {
                    status.active = false;
                    status.scanning = false;
                    status.stopping = false;
                    match result {
                        Ok(reason) => status.stop_reason = Some(reason),
                        Err(error) => status.error = Some(error),
                    }
                }
            });
        if let Err(error) = spawned {
            if let Ok(mut view) = self.view.lock() {
                view.active = false;
                view.scanning = false;
                view.error = Some(error.to_string());
            }
            return Err(error.to_string());
        }
        self.status()
    }

    pub(crate) fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Ok(mut view) = self.view.lock() {
            view.stopping = view.active;
        }
    }
}

fn record(
    path: &Path,
    pid: u32,
    created: &str,
    limit_gib: u64,
    cancel: &AtomicBool,
    sample: &AtomicBool,
    view: &Mutex<View>,
) -> Result<String, String> {
    let process = Process::open(pid).map_err(|e| e.to_string())?;
    if process.created != created {
        return Err("Процесс игры сменился до начала записи.".into());
    }
    let mut archive = Archive::create(path, &process, limit_gib * 1024 * 1024 * 1024)
        .map_err(|e| e.to_string())?;
    let started = Instant::now();
    let reason = loop {
        if cancel.load(Ordering::Relaxed) {
            break "Остановлено пользователем.".into();
        }
        if !process.alive() {
            break "Процесс игры завершился.".into();
        }
        if started.elapsed() >= Duration::from_secs(20 * 60) {
            break "Достигнут предел записи 20 минут.".into();
        }
        sample.store(false, Ordering::Relaxed);
        if let Ok(mut status) = view.lock() {
            status.scanning = true;
            status.read_bytes = 0;
            status.holes = 0;
        }
        let snapshot = archive.capture(&process, cancel, |progress: Progress| {
            if let Ok(mut status) = view.lock() {
                status.bytes = progress.stored_bytes;
                status.read_bytes = progress.read_bytes;
                status.holes = progress.holes;
            }
        });
        match snapshot {
            Ok(snapshot) => {
                if let Ok(mut status) = view.lock() {
                    status.samples = snapshot.sequence;
                    status.last_complete = Some(snapshot.complete);
                    status.scanning = false;
                }
                if let Some(reason) = snapshot.reason {
                    break reason;
                }
            }
            Err(error) => {
                let reason = if cancel.load(Ordering::Relaxed) {
                    "Остановлено пользователем.".to_owned()
                } else {
                    format!("Запись прервана: {error}")
                };
                if !cancel.load(Ordering::Relaxed)
                    && let Ok(mut status) = view.lock()
                {
                    status.error = Some(reason.clone());
                }
                break reason;
            }
        }
        let wait_started = Instant::now();
        while wait_started.elapsed() < Duration::from_secs(45)
            && !cancel.load(Ordering::Relaxed)
            && !sample.load(Ordering::Relaxed)
            && process.alive()
        {
            std::thread::sleep(Duration::from_millis(100));
        }
    };
    archive
        .finish(&reason)
        .map_err(|e| format!("Не удалось завершить запись: {e}. Готовые снимки доступны."))?;
    if let Ok(mut status) = view.lock() {
        status.bytes = archive.bytes();
    }
    Ok(reason)
}

pub(crate) fn on_close(window: &tauri::Window, event: &tauri::WindowEvent) {
    if window.label() == "main"
        && matches!(event, tauri::WindowEvent::CloseRequested { .. })
        && let Some(state) = window.try_state::<AppState>()
        && let Ok(recorder) = state.binary_recording.lock()
    {
        recorder.stop();
    }
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn binary_recording_status(state: State<'_, AppState>) -> Result<View, String> {
    state
        .binary_recording
        .lock()
        .map_err(|e| e.to_string())?
        .status()
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn binary_recording_start(
    state: State<'_, AppState>,
    limit_gib: u64,
) -> Result<View, String> {
    state
        .binary_recording
        .lock()
        .map_err(|e| e.to_string())?
        .start(
            &state.data_directory.join("diagnostics/binary-memory"),
            limit_gib,
        )
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn binary_recording_stop(state: State<'_, AppState>) -> Result<View, String> {
    let recorder = state.binary_recording.lock().map_err(|e| e.to_string())?;
    recorder.stop();
    recorder.status()
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn binary_recording_sample(state: State<'_, AppState>) -> Result<View, String> {
    let recorder = state.binary_recording.lock().map_err(|e| e.to_string())?;
    let view = recorder.status()?;
    if !view.active || view.stopping || view.scanning {
        return Err("Дождитесь завершения текущего снимка или начните запись.".into());
    }
    recorder.sample.store(true, Ordering::Relaxed);
    Ok(view)
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn binary_recording_folder(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let root = state.data_directory.join("diagnostics/binary-memory");
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(root.display().to_string(), None::<String>)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_remains_pending_until_worker_finishes_and_blocks_restart() {
        let mut recorder = Recorder::default();
        recorder.view.lock().unwrap().active = true;
        recorder.view.lock().unwrap().scanning = true;
        recorder.stop();
        let view = recorder.status().unwrap();
        assert!(view.active && view.stopping);
        assert!(recorder.cancel.load(Ordering::Relaxed));
        assert!(
            recorder
                .start(Path::new("unused"), 16)
                .unwrap_err()
                .contains("завершается")
        );
    }

    #[test]
    fn invalid_size_is_rejected_before_accessing_game_or_files() {
        let mut recorder = Recorder::default();
        assert!(
            recorder
                .start(Path::new("unused"), 0)
                .unwrap_err()
                .contains("8, 16 или 32")
        );
        assert!(!recorder.status().unwrap().active);
    }
}
