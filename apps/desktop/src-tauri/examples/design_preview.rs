//! Проверка Windows `WebView2` без нативного IPC, аккаунта и игровых наблюдателей.
//! Нужен Vite на 127.0.0.1:1420. Закрытие любого окна завершает предпросмотр.
#![windows_subsystem = "windows"]

#[cfg(all(windows, debug_assertions))]
fn main() -> wry::Result<()> {
    use tao::{
        dpi::LogicalSize,
        event::{Event, WindowEvent},
        event_loop::{ControlFlow, EventLoop},
        window::WindowBuilder,
    };
    let event_loop = EventLoop::new();
    let profile =
        std::env::temp_dir().join(format!("platscope-design-preview-{}", std::process::id()));
    let mut context = wry::WebContext::new(Some(profile));
    let main = WindowBuilder::new()
        .with_title("PlatScope — проверка дизайна")
        .with_inner_size(LogicalSize::new(1180.0, 760.0))
        .with_focused(false)
        .build(&event_loop)
        .expect("Не удалось создать тестовое окно");
    let _main_webview = wry::WebViewBuilder::new_with_web_context(&mut context)
        .with_url("http://127.0.0.1:1420/?mock=1&mockTradeShift=1&mockOrders=27")
        .build(&main)?;
    let overlay = WindowBuilder::new()
        .with_title("PlatScope — проверка карточек")
        .with_inner_size(LogicalSize::new(1292.0, 448.0))
        .with_decorations(false)
        .with_focused(false)
        .build(&event_loop)
        .expect("Не удалось создать окно карточек");
    let _overlay_webview = wry::WebViewBuilder::new_with_web_context(&mut context)
        .with_url("http://127.0.0.1:1420/?mock=1&overlay=1")
        .build(&overlay)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(600);
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(deadline);
        if std::time::Instant::now() >= deadline
            || matches!(
                event,
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                }
            )
        {
            *control_flow = ControlFlow::Exit;
        }
    });
}

#[cfg(not(all(windows, debug_assertions)))]
fn main() {
    // Предпросмотр доступен только в отладочной сборке Windows.
}
