//! Проверяет настоящее событие закрытия Tauri со скрытыми окнами, без БД и игры.
//! Отдельный процесс нужен, чтобы цикл окон Windows работал в главном потоке.
#![forbid(unsafe_code)]

#[path = "../src/app_lifecycle.rs"]
mod app_lifecycle;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use tauri::{Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};

fn main() {
    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    let profile = std::env::temp_dir().join(format!("platscope-lifecycle-{}", std::process::id()));
    let app = tauri::Builder::default()
        .on_window_event(app_lifecycle::handle_window_event)
        .setup(move |app| {
            for label in ["main", "reward-overlay", "preview"] {
                WebviewWindowBuilder::new(app, label, WebviewUrl::External("about:blank".parse()?))
                    .visible(false)
                    .focused(false)
                    .data_directory(profile.clone())
                    .build()?;
            }
            Ok(())
        })
        .build(context)
        .expect("Не удалось создать скрытые тестовые окна");

    let (finished_tx, finished_rx) = mpsc::channel();
    let handle = app.handle().clone();
    let watchdog = std::thread::spawn(move || {
        if finished_rx.recv_timeout(Duration::from_secs(10)).is_err() {
            // Без исправления скрытый оверлей удерживает цикл окон бесконечно.
            handle.exit(1);
        }
    });
    let exited_with_overlay = Arc::new(AtomicBool::new(false));
    let exit_observed = exited_with_overlay.clone();
    let exit_code = app.run_return(move |app, event| match event {
        RunEvent::Ready => {
            // Закрытие вспомогательного окна не должно завершать приложение.
            app.get_webview_window("preview")
                .expect("Вспомогательное окно существует")
                .close()
                .expect("Вспомогательное окно закрывается");
        }
        RunEvent::WindowEvent {
            label,
            event: tauri::WindowEvent::Destroyed,
            ..
        } if label == "preview" => {
            app.get_webview_window("main")
                .expect("Главное окно продолжает работать")
                .close()
                .expect("Главное окно закрывается");
        }
        RunEvent::ExitRequested { code: Some(0), .. } => {
            exit_observed.store(
                app.get_webview_window("reward-overlay").is_some(),
                Ordering::Release,
            );
        }
        _ => {}
    });
    let _ = finished_tx.send(());
    watchdog.join().expect("Проверка времени завершена");
    assert_eq!(
        exit_code, 0,
        "Закрытие главного окна должно завершать приложение"
    );
    assert!(
        exited_with_overlay.load(Ordering::Acquire),
        "Приложение должно завершаться даже при открытом скрытом оверлее"
    );
    println!("Закрытие главного окна завершает приложение; закрытие вспомогательного окна — нет.");
}
