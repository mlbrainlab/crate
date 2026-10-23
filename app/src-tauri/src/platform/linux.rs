use tauri::{Manager, Runtime, WebviewWindowBuilder};

/// Linux draws its own header bar (design/DESIGN.md → Window frame), so the
/// window has no system decorations.
pub fn window_chrome<'a, R: Runtime, M: Manager<R>>(
    builder: WebviewWindowBuilder<'a, R, M>,
) -> WebviewWindowBuilder<'a, R, M> {
    builder.decorations(false)
}
