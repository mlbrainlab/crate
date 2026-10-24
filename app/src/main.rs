//! Crate desktop app (Slint UI). Archive work lives in `crate-core`.
// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::time::{Duration, Instant};

use slint::ComponentHandle;
use slint::winit_030::WinitWindowAccessor;

mod platform;

slint::include_modules!();

/// Launch options read from the command line.
struct Launch {
    /// `--mini`: open the mini view (520 × 620) instead of the full window.
    mini: bool,
    /// `--startup-probe`: log the startup timeline and exit after the first frame.
    probe: bool,
}

/// Connects a window's header callbacks to the real window. Both window
/// components expose the same `start-drag` / `close-window` callbacks.
macro_rules! connect_frame {
    ($ui:expr) => {{
        let ui = &$ui;
        ui.set_custom_frame(platform::CUSTOM_FRAME);
        let weak = ui.as_weak();
        ui.on_start_drag(move || {
            if let Some(ui) = weak.upgrade() {
                ui.window().with_winit_window(|w| {
                    let _ = w.drag_window();
                });
            }
        });
        let weak = ui.as_weak();
        ui.on_close_window(move || {
            if let Some(ui) = weak.upgrade() {
                let _ = ui.hide();
            }
        });
    }};
}

fn main() -> Result<(), slint::PlatformError> {
    // First thing in the process: the startup clock (docs/perf.md).
    let started = Instant::now();
    let pre_main = platform::process_age();
    let args: Vec<String> = std::env::args().collect();
    let launch = Launch {
        mini: args.iter().any(|a| a == "--mini"),
        probe: args.iter().any(|a| a == "--startup-probe"),
    };

    platform::select_backend()?;

    if launch.mini {
        let ui = MiniView::new()?;
        connect_frame!(ui);
        probe_startup(ui.window(), &launch, started, pre_main);
        ui.run()
    } else {
        let ui = MainWindow::new()?;
        connect_frame!(ui);
        let weak = ui.as_weak();
        ui.on_minimize(move || {
            if let Some(ui) = weak.upgrade() {
                ui.window().set_minimized(true);
            }
        });
        let weak = ui.as_weak();
        ui.on_toggle_maximize(move || {
            if let Some(ui) = weak.upgrade() {
                let w = ui.window();
                w.set_maximized(!w.is_maximized());
            }
        });
        probe_startup(ui.window(), &launch, started, pre_main);
        ui.run()
    }
}

/// With `--startup-probe`, log the startup timeline after the first rendered
/// frame and exit. scripts/startup-bench.sh parses this line.
fn probe_startup(
    window: &slint::Window,
    launch: &Launch,
    started: Instant,
    pre_main: Option<Duration>,
) {
    if !launch.probe {
        return;
    }
    let built = started.elapsed();
    let view = if launch.mini { "mini" } else { "main" };
    let mut done = false;
    let result = window.set_rendering_notifier(move |state, _| {
        if done || !matches!(state, slint::RenderingState::AfterRendering) {
            return;
        }
        done = true;
        let ms = |d: Duration| d.as_secs_f64() * 1e3;
        eprintln!(
            "crate-startup view={view} premain_ms={:.0} window_ms={:.1} ready_ms={:.1} visible_ms={:.1} signal=after-render",
            pre_main.map(ms).unwrap_or_default(),
            ms(built),
            ms(built),
            ms(started.elapsed()),
        );
        // Skip teardown so a benchmark loop measures startup, not exit.
        std::process::exit(0);
    });
    if let Err(err) = result {
        eprintln!("crate-startup: rendering notifier unavailable: {err:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::platform::{PointerEventButton, WindowEvent};
    use slint::{LogicalPosition, LogicalSize};
    use std::cell::Cell;
    use std::rc::Rc;

    fn main_window() -> MainWindow {
        i_slint_backend_testing::init_no_event_loop();
        let ui = MainWindow::new().unwrap();
        ui.set_custom_frame(true);
        ui.window().set_size(LogicalSize::new(1000.0, 680.0));
        ui.show().unwrap();
        ui
    }

    fn press(ui: &MainWindow, x: f32, y: f32) {
        let position = LogicalPosition::new(x, y);
        let button = PointerEventButton::Left;
        ui.window()
            .dispatch_event(WindowEvent::PointerMoved { position });
        ui.window()
            .dispatch_event(WindowEvent::PointerPressed { position, button });
        ui.window()
            .dispatch_event(WindowEvent::PointerReleased { position, button });
    }

    #[test]
    fn pressing_the_header_starts_a_window_drag() {
        let ui = main_window();
        let dragged = Rc::new(Cell::new(false));
        let d = dragged.clone();
        ui.on_start_drag(move || d.set(true));
        for x in [40.0, 300.0, 500.0, 850.0] {
            dragged.set(false);
            press(&ui, x, 28.0);
            assert!(dragged.get(), "no drag when pressing the header at x = {x}");
        }
    }

    #[test]
    fn close_button_sits_at_the_right_of_the_header() {
        let ui = main_window();
        let closed = Rc::new(Cell::new(false));
        let c = closed.clone();
        ui.on_close_window(move || c.set(true));
        // Close is the last 24 px button, 12 px from the right edge, centered in the 56 px header.
        press(&ui, 1000.0 - 12.0 - 12.0, 28.0);
        assert!(closed.get());
    }
}
