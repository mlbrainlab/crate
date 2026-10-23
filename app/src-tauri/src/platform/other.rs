use tauri::{Manager, Runtime, WebviewWindowBuilder};

/// Windows and macOS keep the native title bar (M7/M8).
pub fn window_chrome<'a, R: Runtime, M: Manager<R>>(
    builder: WebviewWindowBuilder<'a, R, M>,
) -> WebviewWindowBuilder<'a, R, M> {
    builder
}

/// Time spent before `main`; only measured on Linux for now.
pub fn process_age() -> Option<std::time::Duration> {
    None
}
