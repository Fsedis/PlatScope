//! Локальная запись DBWIN по явному запросу. Сырые строки не попадают в tracing
//! и обычный диагностический отчёт. В памяти хранится только хвост текущей сессии.
use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::AppState;

const MAX_BYTES: u64 = 100 * 1024 * 1024;
const MAX_DURATION: Duration = Duration::from_secs(2 * 60 * 60);
const PREVIEW_ROWS: usize = 100;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Entry {
    sequence: u64,
    received_at: DateTime<Utc>,
    elapsed_ms: u64,
    kind: String,
    process_id: Option<u32>,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    raw_base64: Option<String>,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)] // Независимые признаки транспорта и записи в контракте интерфейса.
pub(crate) struct CaptureStatus {
    connected: bool,
    supported: bool,
    shared_listener: bool,
    active: bool,
    path: Option<String>,
    started_at: Option<DateTime<Utc>>,
    stopped_at: Option<DateTime<Utc>>,
    messages: u64,
    markers: u64,
    bytes: u64,
    error: Option<String>,
    stop_reason: Option<String>,
    entries: VecDeque<Entry>,
}

#[derive(Default)]
pub(crate) struct Recorder {
    status: CaptureStatus,
    file: Option<File>,
    started: Option<Instant>,
    sequence: u64,
}

impl Recorder {
    pub(crate) fn connected(&mut self, supported: bool, shared: bool) {
        self.status.connected = true;
        self.status.supported = supported;
        self.status.shared_listener = shared;
    }

    pub(crate) fn disconnected(&mut self) {
        self.status.connected = false;
        self.stop("Слушатель Warframe завершился. Перезапустите PlatScope для новой записи.");
    }

    fn start(&mut self, directory: &Path) -> Result<(), String> {
        if self.status.active {
            return Err("Запись уже идёт.".into());
        }
        if !self.status.connected || !self.status.supported {
            return Err("Слушатель записи недоступен. Нужен обновлённый OCR-модуль; перезапустите PlatScope.".into());
        }
        fs::create_dir_all(directory).map_err(|error| error.to_string())?;
        let now = Utc::now();
        let path = directory.join(format!(
            "warframe-dbwin-{}.jsonl",
            now.format("%Y%m%dT%H%M%S%9fZ")
        ));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| format!("Не удалось создать журнал: {error}"))?;
        self.status = CaptureStatus {
            connected: self.status.connected,
            supported: self.status.supported,
            shared_listener: self.status.shared_listener,
            active: true,
            path: Some(path.display().to_string()),
            started_at: Some(now),
            ..CaptureStatus::default()
        };
        self.file = Some(file);
        self.started = Some(Instant::now());
        self.sequence = 0;
        self.append("session_start", None, now, "Начало записи сообщений Warframe (DBWIN). Формат 1; время UTC; rawBase64 содержит исходные байты сообщения.", None);
        if let Some(error) = &self.status.error {
            return Err(error.clone());
        }
        Ok(())
    }

    pub(crate) fn record(
        &mut self,
        pid: u32,
        received_at: DateTime<Utc>,
        message: &str,
        raw: &str,
    ) {
        self.check_limit();
        if !self.status.active {
            return;
        }
        // Сообщение могло ждать в канале до начала сессии.
        if self
            .status
            .started_at
            .is_some_and(|start| received_at < start)
        {
            return;
        }
        if self.append("message", Some(pid), received_at, message, Some(raw)) {
            self.status.messages += 1;
        }
    }

    fn mark(&mut self, message: &str) -> Result<(), String> {
        self.check_limit();
        if !self.status.active {
            return Err("Сначала начните запись.".into());
        }
        let message = message.trim();
        if message.is_empty() || message.chars().count() > 500 {
            return Err("Введите отметку от 1 до 500 символов.".into());
        }
        if !self.append("marker", None, Utc::now(), message, None) {
            return Err(self
                .status
                .error
                .clone()
                .unwrap_or_else(|| "Запись остановлена.".into()));
        }
        self.status.markers += 1;
        Ok(())
    }

    fn append(
        &mut self,
        kind: &str,
        pid: Option<u32>,
        received_at: DateTime<Utc>,
        message: &str,
        raw: Option<&str>,
    ) -> bool {
        if self.file.is_none() {
            return false;
        }
        let entry = Entry {
            sequence: self.sequence + 1,
            received_at,
            elapsed_ms: u64::try_from(self.started.map_or(0, |start| start.elapsed().as_millis()))
                .unwrap_or(u64::MAX),
            kind: kind.into(),
            process_id: pid,
            message: message.into(),
            raw_base64: raw.map(str::to_owned),
        };
        let result = (|| -> Result<u64, String> {
            let mut bytes = serde_json::to_vec(&entry).map_err(|error| error.to_string())?;
            bytes.push(b'\n');
            let size = u64::try_from(bytes.len()).map_err(|error| error.to_string())?;
            if self.status.bytes + size > MAX_BYTES && kind != "session_stop" {
                return Err("Достигнут предел журнала — 100 МиБ. Начните новую сессию.".into());
            }
            self.file
                .as_mut()
                .ok_or("Журнал закрыт")?
                .write_all(&bytes)
                .map_err(|error| error.to_string())?;
            Ok(size)
        })();
        match result {
            Ok(bytes) => {
                self.sequence += 1;
                self.status.bytes += bytes;
                // Исходные байты нужны только в файле; предпросмотр не дублирует их.
                self.status.entries.push_back(Entry {
                    raw_base64: None,
                    ..entry
                });
                while self.status.entries.len() > PREVIEW_ROWS {
                    self.status.entries.pop_front();
                }
                true
            }
            Err(error) => {
                self.status.error = Some(format!("Запись прекращена: {error}"));
                self.status.active = false;
                self.status.stopped_at = Some(Utc::now());
                self.file = None;
                false
            }
        }
    }

    fn stop(&mut self, reason: &str) {
        if !self.status.active {
            return;
        }
        self.append("session_stop", None, Utc::now(), reason, None);
        if let Some(file) = self.file.take()
            && let Err(error) = file.sync_all()
        {
            self.status.error = Some(format!("Не удалось завершить сохранение: {error}"));
        }
        self.status.active = false;
        self.status.stopped_at = Some(Utc::now());
        self.status.stop_reason = Some(reason.into());
    }

    fn check_limit(&mut self) {
        if self.status.active
            && self
                .started
                .is_some_and(|start| start.elapsed() >= MAX_DURATION)
        {
            self.stop("Запись остановлена через 2 часа. Можно начать новую сессию.");
        }
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.stop("PlatScope завершает работу.");
    }
}

fn with_recorder(
    state: &AppState,
    operation: impl FnOnce(&mut Recorder) -> Result<(), String>,
) -> Result<CaptureStatus, String> {
    let mut recorder = state
        .dbwin_capture
        .lock()
        .map_err(|_| "Состояние записи недоступно".to_owned())?;
    recorder.check_limit();
    operation(&mut recorder)?;
    Ok(recorder.status.clone())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn dbwin_capture_status(state: State<'_, AppState>) -> Result<CaptureStatus, String> {
    with_recorder(&state, |_| Ok(()))
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn start_dbwin_capture(state: State<'_, AppState>) -> Result<CaptureStatus, String> {
    with_recorder(&state, |recorder| {
        recorder.start(&state.data_directory.join("diagnostics").join("dbwin"))
    })
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn stop_dbwin_capture(state: State<'_, AppState>) -> Result<CaptureStatus, String> {
    with_recorder(&state, |recorder| {
        recorder.stop("Запись остановлена пользователем.");
        Ok(())
    })
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn mark_dbwin_capture(
    state: State<'_, AppState>,
    message: String,
) -> Result<CaptureStatus, String> {
    with_recorder(&state, |recorder| recorder.mark(&message))
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn open_dbwin_capture_folder(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let directory = state.data_directory.join("diagnostics").join("dbwin");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    app.opener()
        .open_path(directory.display().to_string(), None::<&str>)
        .map_err(|error| error.to_string())
}

pub(crate) fn spawn_limit_check(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if let Ok(mut recorder) = app.state::<AppState>().dbwin_capture.lock() {
                recorder.check_limit();
            }
        }
    });
}

pub(crate) fn on_close(window: &tauri::Window, event: &tauri::WindowEvent) {
    if window.label() == "main"
        && matches!(event, tauri::WindowEvent::CloseRequested { .. })
        && let Some(state) = window.try_state::<AppState>()
        && let Ok(mut recorder) = state.dbwin_capture.lock()
    {
        recorder.stop("PlatScope завершает работу.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_records_unknown_lines_markers_and_raw_bytes_but_not_idle_messages() {
        let directory = std::env::temp_dir().join(format!(
            "platscope-dbwin-{}",
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let mut recorder = Recorder::default();
        assert!(recorder.start(&directory).is_err());
        recorder.connected(true, false);
        recorder.record(42, Utc::now(), "до начала", "");
        recorder.start(&directory).unwrap();
        assert!(recorder.start(&directory).is_err());
        let text = "Неизвестное событие\n\"Прайм\" \\ путь";
        recorder.record(42, Utc::now(), text, "AAEC/w==");
        recorder.mark("Открыл инвентарь").unwrap();
        recorder.stop("Тест завершён");
        recorder.record(42, Utc::now(), "после остановки", "");
        let contents = fs::read_to_string(recorder.status.path.as_ref().unwrap()).unwrap();
        let entries: Vec<serde_json::Value> = contents
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[1]["message"], text);
        assert_eq!(entries[1]["rawBase64"], "AAEC/w==");
        assert_eq!(entries[2]["kind"], "marker");
        assert_eq!(entries[3]["sequence"], 4);
        assert_eq!(recorder.status.messages, 1);
        assert!(recorder.mark("поздно").is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn preview_is_bounded_and_disconnect_preserves_file() {
        let directory = std::env::temp_dir().join(format!(
            "platscope-dbwin-ring-{}",
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let mut recorder = Recorder::default();
        recorder.connected(true, true);
        recorder.start(&directory).unwrap();
        for _ in 0..150 {
            recorder.record(42, Utc::now(), "test", "dGVzdA==");
        }
        assert_eq!(recorder.status.entries.len(), PREVIEW_ROWS);
        assert!(
            recorder
                .status
                .entries
                .iter()
                .all(|entry| entry.raw_base64.is_none())
        );
        recorder.disconnected();
        assert!(!recorder.status.active);
        assert!(!recorder.status.connected);
        assert_eq!(
            fs::read_to_string(recorder.status.path.as_ref().unwrap())
                .unwrap()
                .lines()
                .count(),
            152
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn duration_and_size_limits_stop_recording() {
        let directory = std::env::temp_dir().join(format!(
            "platscope-dbwin-limit-{}",
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let mut recorder = Recorder::default();
        recorder.connected(true, false);
        recorder.start(&directory).unwrap();
        recorder.started = Instant::now().checked_sub(MAX_DURATION);
        recorder.check_limit();
        assert!(!recorder.status.active);
        recorder.start(&directory).unwrap();
        recorder.status.bytes = MAX_BYTES;
        recorder.record(42, Utc::now(), "test", "");
        assert!(!recorder.status.active);
        assert!(recorder.status.error.is_some());
        assert_eq!(recorder.status.messages, 0);
        fs::remove_dir_all(directory).unwrap();
    }
}
