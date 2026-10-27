//! `crate-app`: command line, startup probe and window setup.
// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use slint::ComponentHandle;
use slint::winit_030::WinitWindowAccessor;

use crate_app::{ArchiveWindow, MainWindow, begin_window_drag, instance, platform, view};

const USAGE: &str = "\
Usage: crate-app [archive]
       crate-app --compress <files and folders…>
       crate-app --extract-each <archives…>

With an archive, opens it in the mini view (the same archive twice brings
the open window to the front). With --compress, opens the New archive
dialog for those files. With --extract-each, extracts every archive into
its own folder next to it (Batch extract). Otherwise, opens Home.

  --startup-probe   log startup timings after the first useful frame and exit";

/// Launch options read from the command line.
struct Launch {
    /// The archive to show in the mini view.
    archive: Option<PathBuf>,
    /// `--compress <files…>`: open the New archive dialog for these.
    compress: Vec<PathBuf>,
    /// `--extract-each <archives…>`: Batch extract these.
    extract_each: Vec<PathBuf>,
    /// `--startup-probe`: log the startup timeline and exit once the window
    /// shows something useful (for the mini view: the listing).
    probe: bool,
}

impl Launch {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut launch = Launch {
            archive: None,
            compress: Vec::new(),
            extract_each: Vec::new(),
            probe: false,
        };
        let mut compressing = false;
        let mut extracting = false;
        for arg in args {
            match arg.as_str() {
                "--compress" => compressing = true,
                "--extract-each" => extracting = true,
                a if compressing => launch
                    .compress
                    .push(PathBuf::from(a.strip_prefix("file://").unwrap_or(a))),
                a if extracting => launch
                    .extract_each
                    .push(PathBuf::from(a.strip_prefix("file://").unwrap_or(a))),
                "--startup-probe" => launch.probe = true,
                // Kept from M0 so old bench commands still run.
                "--mini" => {}
                a if a.starts_with("--") => return Err(format!("unknown option `{a}`")),
                // Desktop files may pass file:// URIs.
                a if launch.archive.is_none() => {
                    launch.archive = Some(PathBuf::from(a.strip_prefix("file://").unwrap_or(a)))
                }
                a => return Err(format!("only one archive per window (got `{a}` too)")),
            }
        }
        Ok(launch)
    }
}

/// Connects a window's header callbacks to the real window. Both window
/// components expose the same `start-drag` callback.
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
    }};
}

fn main() -> std::process::ExitCode {
    // First thing in the process: the startup clock (docs/perf.md).
    let started = Instant::now();
    let pre_main = platform::process_age();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return std::process::ExitCode::SUCCESS;
    }
    let launch = match Launch::parse(&args) {
        Ok(l) => l,
        Err(msg) => {
            eprintln!("crate-app: {msg}\n\n{USAGE}");
            return std::process::ExitCode::from(2);
        }
    };
    let result = match &launch.archive {
        Some(archive) => run_archive(archive.clone(), &launch, started, pre_main),
        None => run_main(&launch, started, pre_main),
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("crate-app: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run_archive(
    archive: PathBuf,
    launch: &Launch,
    started: Instant,
    pre_main: Option<Duration>,
) -> Result<(), slint::PlatformError> {
    // One window per archive: if it's open already, raise that one instead.
    let _guard = match instance::claim(&archive, || view::post(view::View::raise)) {
        instance::Claim::Owner(guard) => Some(guard),
        instance::Claim::AlreadyOpen if !launch.probe => return Ok(()),
        instance::Claim::AlreadyOpen => None,
    };
    platform::select_backend()?;
    let ui = ArchiveWindow::new()?;
    ui.set_custom_frame(platform::CUSTOM_FRAME);
    let probe = launch
        .probe
        .then(|| Probe::new(ui.window(), "mini", started, pre_main));
    let on_listed = probe.map(|p| Box::new(move || p.listed()) as Box<dyn FnOnce()>);
    view::View::start(&ui, archive, on_listed);
    ui.show()?;
    // Run until quit rather than until the window hides: closing during an
    // extraction hides the window but waits for the cleanup.
    let result = slint::run_event_loop_until_quit();
    view::shutdown();
    result
}

fn run_main(
    launch: &Launch,
    started: Instant,
    pre_main: Option<Duration>,
) -> Result<(), slint::PlatformError> {
    platform::select_backend()?;
    let ui = MainWindow::new()?;
    connect_frame!(ui);
    let weak = ui.as_weak();
    ui.on_close_window(move || {
        if let Some(ui) = weak.upgrade() {
            let _ = ui.hide();
        }
    });
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
    crate_app::home::Home::start(&ui, launch.compress.clone());
    if !launch.extract_each.is_empty() {
        crate_app::batch::start(&ui, launch.extract_each.clone(), true);
    }
    // Files dropped on Home: winit sends one event per file, so collect them
    // until the burst ends.
    let dropped = std::rc::Rc::new(std::cell::RefCell::new(Vec::<PathBuf>::new()));
    let timer = std::rc::Rc::new(slint::Timer::default());
    ui.window().on_winit_window_event(move |_, event| {
        if let slint::winit_030::winit::event::WindowEvent::DroppedFile(path) = event {
            dropped.borrow_mut().push(path.clone());
            let dropped = dropped.clone();
            timer.start(
                slint::TimerMode::SingleShot,
                Duration::from_millis(80),
                move || {
                    let paths = std::mem::take(&mut *dropped.borrow_mut());
                    crate_app::home::post(move |home| home.open_paths(paths));
                },
            );
        }
        slint::winit_030::EventResult::Propagate
    });
    if launch.probe {
        // The full window has nothing to load: its first frame is the one.
        Probe::new(ui.window(), "main", started, pre_main).listed();
    }
    ui.run()
}

/// `--startup-probe`: logs the startup timeline once the first frame after
/// `listed()` is on screen, then exits. scripts/startup-bench.sh parses the
/// line; `visible_ms` is the number the ROADMAP budget is about.
#[derive(Clone)]
struct Probe(std::rc::Rc<ProbeState>);

struct ProbeState {
    view: &'static str,
    started: Instant,
    pre_main: Option<Duration>,
    built: Duration,
    first_frame: std::cell::Cell<Option<Duration>>,
    listed: std::cell::Cell<Option<Duration>>,
}

impl Probe {
    fn new(
        window: &slint::Window,
        view: &'static str,
        started: Instant,
        pre_main: Option<Duration>,
    ) -> Self {
        let probe = Probe(std::rc::Rc::new(ProbeState {
            view,
            started,
            pre_main,
            built: started.elapsed(),
            first_frame: None.into(),
            listed: None.into(),
        }));
        let p = probe.clone();
        let result = window.set_rendering_notifier(move |state, _| {
            if !matches!(state, slint::RenderingState::AfterRendering) {
                return;
            }
            let now = p.0.started.elapsed();
            if p.0.first_frame.get().is_none() {
                p.0.first_frame.set(Some(now));
            }
            if let Some(listed) = p.0.listed.get() {
                p.report(listed, now);
            }
        });
        if let Err(err) = result {
            eprintln!("crate-startup: rendering notifier unavailable: {err:?}");
        }
        probe
    }

    /// The window now holds its content; the next frame shows it.
    fn listed(&self) {
        self.0.listed.set(Some(self.0.started.elapsed()));
    }

    fn report(&self, listed: Duration, visible: Duration) {
        let s = &self.0;
        let ms = |d: Duration| d.as_secs_f64() * 1e3;
        eprintln!(
            "crate-startup view={} premain_ms={:.0} window_ms={:.1} first_frame_ms={:.1} ready_ms={:.1} visible_ms={:.1} signal=after-render",
            s.view,
            s.pre_main.map(ms).unwrap_or_default(),
            ms(s.built),
            ms(s.first_frame.get().unwrap_or(visible)),
            ms(listed),
            ms(visible),
        );
        // Skip teardown so a benchmark loop measures startup, not exit.
        std::process::exit(0);
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
