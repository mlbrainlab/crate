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
                begin_window_drag(&ui);
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
        // Straight to winit: Wayland never reports "restored", so Slint's cached
        // minimized/maximized state can go stale and skip the request.
        ui.on_minimize(move || {
            if let Some(ui) = weak.upgrade() {
                ui.window().with_winit_window(|w| w.set_minimized(true));
            }
        });
        let weak = ui.as_weak();
        ui.on_toggle_maximize(move || {
            if let Some(ui) = weak.upgrade() {
                ui.window()
                    .with_winit_window(|w| w.set_maximized(!w.is_maximized()));
            }
        });
        probe_startup(ui.window(), &launch, started, pre_main);
        ui.run()
    }
}

/// Hands an interactive move to the window manager. The compositor keeps the
/// button release that ends the move, so tell Slint the button is up; otherwise
/// the header keeps the pointer grab and every later click lands on it.
/// The release is sent from a zero-delay timer because Slint ignores events
/// dispatched while it is still handling the pointer move that started the drag.
fn begin_window_drag<C: ComponentHandle + 'static>(ui: &C) {
    ui.window().with_winit_window(|w| {
        let _ = w.drag_window();
    });
    let weak = ui.as_weak();
    slint::Timer::single_shot(Duration::ZERO, move || {
        if let Some(ui) = weak.upgrade() {
            ui.window()
                .dispatch_event(slint::platform::WindowEvent::PointerReleased {
                    position: slint::LogicalPosition::new(-1.0, -1.0),
                    button: slint::platform::PointerEventButton::Left,
                });
        }
    });
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
    use std::rc::Rc;

    fn main_window() -> MainWindow {
        i_slint_backend_testing::init_no_event_loop();
        let ui = MainWindow::new().unwrap();
        ui.set_custom_frame(true);
        ui.window().set_size(LogicalSize::new(1000.0, 680.0));
        ui.show().unwrap();
        ui
    }

    const LEFT: PointerEventButton = PointerEventButton::Left;

    fn event(ui: &MainWindow, event: WindowEvent) {
        ui.window().dispatch_event(event);
    }

    fn click(ui: &MainWindow, x: f32, y: f32) {
        let position = LogicalPosition::new(x, y);
        event(ui, WindowEvent::PointerMoved { position });
        event(
            ui,
            WindowEvent::PointerPressed {
                position,
                button: LEFT,
            },
        );
        event(
            ui,
            WindowEvent::PointerReleased {
                position,
                button: LEFT,
            },
        );
    }

    /// Press on the header and move: what a user does to drag the window. Like a
    /// real compositor, never deliver the release that ends the move.
    fn drag_header(ui: &MainWindow, x: f32) {
        let position = LogicalPosition::new(x, 28.0);
        event(ui, WindowEvent::PointerMoved { position });
        event(
            ui,
            WindowEvent::PointerPressed {
                position,
                button: LEFT,
            },
        );
        let position = LogicalPosition::new(x + 20.0, 30.0);
        event(ui, WindowEvent::PointerMoved { position });
        // Let begin_window_drag's zero-delay timer run.
        slint::platform::update_timers_and_animations();
    }

    /// Records calls to the header callbacks, wired like `main` wires them.
    fn record(ui: &MainWindow) -> Rc<std::cell::RefCell<Vec<&'static str>>> {
        let log = Rc::new(std::cell::RefCell::new(Vec::new()));
        let (l, weak) = (log.clone(), ui.as_weak());
        ui.on_start_drag(move || {
            l.borrow_mut().push("drag");
            begin_window_drag(&weak.unwrap());
        });
        let l = log.clone();
        ui.on_toggle_maximize(move || l.borrow_mut().push("maximize"));
        let l = log.clone();
        ui.on_minimize(move || l.borrow_mut().push("minimize"));
        let l = log.clone();
        ui.on_close_window(move || l.borrow_mut().push("close"));
        log
    }

    // Button centers: 12 px right margin, 24 px buttons, 8 px gaps, in the 56 px header.
    const CLOSE_X: f32 = 1000.0 - 12.0 - 12.0;
    const MAXIMIZE_X: f32 = CLOSE_X - 32.0;
    const MINIMIZE_X: f32 = MAXIMIZE_X - 32.0;

    #[test]
    fn dragging_the_header_starts_a_window_drag() {
        let ui = main_window();
        let log = record(&ui);
        for x in [40.0, 300.0, 480.0, 850.0] {
            drag_header(&ui, x);
        }
        assert_eq!(*log.borrow(), ["drag"; 4]);
    }

    #[test]
    fn a_click_on_the_header_is_not_a_drag() {
        let ui = main_window();
        let log = record(&ui);
        click(&ui, 300.0, 28.0);
        assert!(log.borrow().is_empty());
    }

    #[test]
    fn double_click_on_the_header_toggles_maximize() {
        let ui = main_window();
        let log = record(&ui);
        click(&ui, 300.0, 28.0);
        click(&ui, 300.0, 28.0);
        assert_eq!(*log.borrow(), ["maximize"]);
    }

    #[test]
    fn window_buttons_work_after_a_drag() {
        let ui = main_window();
        let log = record(&ui);
        drag_header(&ui, 300.0);
        click(&ui, MINIMIZE_X, 28.0);
        click(&ui, MAXIMIZE_X, 28.0);
        click(&ui, CLOSE_X, 28.0);
        assert_eq!(*log.borrow(), ["drag", "minimize", "maximize", "close"]);
    }
}
