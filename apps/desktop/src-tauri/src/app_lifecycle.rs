use tauri::{Manager, Window, WindowEvent};

pub(crate) fn handle_window_event(window: &Window, event: &WindowEvent) {
    // Скрытый оверлей тоже считается открытым окном. Закрытие главного окна
    // должно завершать весь runtime, иначе приложение и OCR остаются в фоне.
    if window.label() == "main" && matches!(event, WindowEvent::CloseRequested { .. }) {
        window.app_handle().exit(0);
    }
}
