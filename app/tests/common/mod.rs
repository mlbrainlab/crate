//! Shared by the UI flow tests: a Slint platform that renders with the
//! software renderer and runs posted jobs on the test thread, so the real
//! controller (worker threads and all) can be driven without a display.
#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate_app::view::View;
use crate_app::{ArchiveUi, ArchiveWindow};
use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType,
};
use slint::platform::{EventLoopProxy, Platform, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, Model};

type Job = Box<dyn FnOnce() + Send>;

/// Queue that worker threads post into; the test thread runs the jobs.
#[derive(Clone, Default)]
struct Queue(Arc<Mutex<VecDeque<Job>>>);

impl EventLoopProxy for Queue {
    fn quit_event_loop(&self) -> Result<(), slint::EventLoopError> {
        Ok(())
    }

    fn invoke_from_event_loop(&self, job: Job) -> Result<(), slint::EventLoopError> {
        self.0.lock().unwrap().push_back(job);
        Ok(())
    }
}

/// The window of the view opened last.
type Current = Rc<RefCell<Option<Rc<MinimalSoftwareWindow>>>>;

struct TestPlatform {
    queue: Queue,
    current: Current,
}

impl Platform for TestPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        *self.current.borrow_mut() = Some(window.clone());
        Ok(window)
    }

    fn new_event_loop_proxy(&self) -> Option<Box<dyn EventLoopProxy>> {
        Some(Box::new(self.queue.clone()))
    }
}

const SCALE: f32 = 2.0;
const WIDTH: f32 = 520.0;
const HEIGHT: f32 = 620.0;

pub struct Harness {
    queue: Queue,
    current: Current,
    shots: PathBuf,
}

impl Harness {
    pub fn new() -> Self {
        let queue = Queue::default();
        let current = Current::default();
        slint::platform::set_platform(Box::new(TestPlatform {
            queue: queue.clone(),
            current: current.clone(),
        }))
        .unwrap();
        let shots = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/ui-snapshots");
        std::fs::create_dir_all(&shots).unwrap();
        Harness {
            queue,
            current,
            shots,
        }
    }

    /// Opens `archive` in a fresh mini view.
    pub fn open(&self, archive: &Path) -> ArchiveWindow {
        let ui = ArchiveWindow::new().unwrap();
        self.window()
            .dispatch_event(WindowEvent::ScaleFactorChanged {
                scale_factor: SCALE,
            });
        ui.window().set_size(slint::LogicalSize::new(WIDTH, HEIGHT));
        View::start(&ui, archive.to_owned(), None);
        ui.show().unwrap();
        ui
    }

    /// Opens Home with the New archive dialog for `sources` (empty: just Home).
    pub fn home(&self, sources: Vec<PathBuf>) -> crate_app::MainWindow {
        let ui = crate_app::MainWindow::new().unwrap();
        self.window()
            .dispatch_event(WindowEvent::ScaleFactorChanged {
                scale_factor: SCALE,
            });
        ui.window().set_size(slint::LogicalSize::new(1000.0, 720.0));
        crate_app::home::Home::start(&ui, sources);
        ui.show().unwrap();
        ui
    }

    pub fn window(&self) -> Rc<MinimalSoftwareWindow> {
        self.current.borrow().clone().expect("a view is open")
    }

    /// Runs posted jobs and timers until `done` holds.
    pub fn until(&self, what: &str, done: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !done() {
            assert!(Instant::now() < deadline, "timed out waiting for: {what}");
            let jobs: Vec<Job> = self.queue.0.lock().unwrap().drain(..).collect();
            for job in jobs {
                job();
            }
            slint::platform::update_timers_and_animations();
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Draws a frame (layout-driven callbacks run), without saving it.
    pub fn render(&self) {
        let window = self.window();
        let size = window.window().size();
        let (w, h) = (size.width as usize, size.height as usize);
        let mut pixels = vec![PremultipliedRgbaColor::default(); w * h];
        window.request_redraw();
        window.draw_if_needed(|renderer| {
            renderer.render(&mut pixels, w);
        });
        self.until("callbacks", || true);
        let jobs: Vec<Job> = self.queue.0.lock().unwrap().drain(..).collect();
        for job in jobs {
            job();
        }
    }

    /// Lets animations finish, then writes a PNG of the window.
    pub fn shot(&self, name: &str) {
        let settle = Instant::now() + Duration::from_millis(200);
        self.until("animations", || Instant::now() > settle);
        self.shot_now(name);
    }

    pub fn shot_now(&self, name: &str) {
        let window = self.window();
        let size = window.window().size();
        let (w, h) = (size.width as usize, size.height as usize);
        let mut pixels = vec![PremultipliedRgbaColor::default(); w * h];
        window.request_redraw();
        window.draw_if_needed(|renderer| {
            renderer.render(&mut pixels, w);
        });
        let file = std::fs::File::create(self.shots.join(format!("{name}.png"))).unwrap();
        let mut png = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
        png.set_color(png::ColorType::Rgba);
        let data: Vec<u8> = pixels
            .iter()
            .flat_map(|p| [p.red, p.green, p.blue, 255])
            .collect();
        png.write_header().unwrap().write_image_data(&data).unwrap();
    }
}

pub fn g(ui: &ArchiveWindow) -> ArchiveUi<'_> {
    ui.global::<ArchiveUi>()
}

/// Names in the mini list.
pub fn rows(ui: &ArchiveWindow) -> Vec<String> {
    g(ui)
        .get_rows()
        .iter()
        .map(|r| r.name.to_string())
        .collect()
}

/// Names in the full view's list.
pub fn files(ui: &ArchiveWindow) -> Vec<String> {
    g(ui)
        .get_files()
        .iter()
        .map(|r| r.name.to_string())
        .collect()
}

/// What would have been opened with the default app.
pub static OPENED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Points the cache at `cache` and records opens instead of launching apps.
pub fn record_opens(cache: &Path) {
    // SAFETY: set before any other thread starts; each test file is its own
    // process with a single test.
    unsafe { std::env::set_var("XDG_CACHE_HOME", cache) };
    crate_app::view::set_opener(|p| {
        OPENED.lock().unwrap().push(p.to_owned());
        Ok(())
    });
}
