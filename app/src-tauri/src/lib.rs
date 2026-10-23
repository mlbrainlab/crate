use tauri::{WebviewUrl, WebviewWindow, WebviewWindowBuilder};

mod platform;

/// Called by the web UI once it has painted its first frame. The window starts
/// hidden so the user never sees an empty white rectangle.
#[tauri::command]
fn app_ready(window: WebviewWindow) -> tauri::Result<()> {
    window.show()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![app_ready])
        .setup(|app| {
            let builder =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                    .title("Crate")
                    .inner_size(1000.0, 680.0)
                    .min_inner_size(520.0, 400.0)
                    .visible(false);
            platform::window_chrome(builder).build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
