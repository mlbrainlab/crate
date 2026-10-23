use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

mod platform;

/// Launch options read from the command line.
struct Launch {
    /// `--mini`: open the mini view (520 × 620) instead of the full window.
    mini: bool,
    /// `--startup-probe`: log startup timings and exit once the window is visible.
    probe: bool,
}

/// Startup timeline, measured from the first line of `main`.
struct Startup {
    started: Instant,
    window_built: Mutex<Option<Duration>>,
    ready: Mutex<Option<Duration>>,
}

/// Called by the web UI once it has painted its first frame. The window starts
/// hidden so the user never sees an empty white rectangle.
#[tauri::command]
fn app_ready(window: WebviewWindow, startup: State<Startup>) -> tauri::Result<()> {
    *startup.ready.lock().unwrap() = Some(startup.started.elapsed());
    window.show()
}

/// Called by the web UI on the first frame after the window was shown.
#[tauri::command]
fn app_visible(signal: String, startup: State<Startup>, launch: State<Launch>) {
    if !launch.probe {
        return;
    }
    let visible = startup.started.elapsed();
    let ms = |d: Option<Duration>| d.unwrap_or_default().as_secs_f64() * 1e3;
    eprintln!(
        "crate-startup view={} window_ms={:.1} ready_ms={:.1} visible_ms={:.1} signal={signal}",
        if launch.mini { "mini" } else { "main" },
        ms(*startup.window_built.lock().unwrap()),
        ms(*startup.ready.lock().unwrap()),
        ms(Some(visible)),
    );
    // Skip teardown so a benchmark loop measures startup, not exit.
    std::process::exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(started: Instant) {
    let args: Vec<String> = std::env::args().collect();
    let launch = Launch {
        mini: args.iter().any(|a| a == "--mini"),
        probe: args.iter().any(|a| a == "--startup-probe"),
    };

    tauri::Builder::default()
        .manage(Startup {
            started,
            window_built: Mutex::new(None),
            ready: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![app_ready, app_visible])
        .setup(move |app| {
            let (url, title, size) = if launch.mini {
                ("index.html?view=mini", "Crate", (520.0, 620.0))
            } else {
                ("index.html", "Crate", (1000.0, 680.0))
            };
            let builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::App(url.into()))
                .title(title)
                .inner_size(size.0, size.1)
                .min_inner_size(480.0, 400.0)
                .visible(false);
            let window = platform::window_chrome(builder).build()?;
            let startup = app.state::<Startup>();
            *startup.window_built.lock().unwrap() = Some(startup.started.elapsed());
            app.manage(launch);

            // Safety net: never leave the user with an invisible app if the UI fails to load.
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(3));
                if !window.is_visible().unwrap_or(true) {
                    let _ = window.show();
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
