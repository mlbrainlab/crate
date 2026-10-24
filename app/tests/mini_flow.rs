//! Drives the real mini view (controller, worker threads and all) on Slint's
//! software renderer: open, select, extract, passwords, encodings, errors.
//! Screenshots of each state go to `target/ui-snapshots/` for a visual check.

#[path = "../../crates/crate-core/tests/support/mod.rs"]
mod support;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate_app::{MiniView, mini::Mini};
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

struct Harness {
    queue: Queue,
    current: Current,
    shots: PathBuf,
}

impl Harness {
    fn new() -> Self {
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
    fn open(&self, archive: &Path) -> MiniView {
        let ui = MiniView::new().unwrap();
        self.window()
            .dispatch_event(WindowEvent::ScaleFactorChanged {
                scale_factor: SCALE,
            });
        ui.window().set_size(slint::LogicalSize::new(WIDTH, HEIGHT));
        Mini::start(&ui, archive.to_owned(), None);
        ui.show().unwrap();
        ui
    }

    fn window(&self) -> Rc<MinimalSoftwareWindow> {
        self.current.borrow().clone().expect("a view is open")
    }

    /// Runs posted jobs and timers until `done` holds.
    fn until(&self, what: &str, done: impl Fn() -> bool) {
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

    /// Lets animations finish, then writes a PNG of the window.
    fn shot(&self, name: &str) {
        let settle = Instant::now() + Duration::from_millis(200);
        self.until("animations", || Instant::now() > settle);
        self.shot_now(name);
    }

    fn shot_now(&self, name: &str) {
        let (w, h) = ((WIDTH * SCALE) as usize, (HEIGHT * SCALE) as usize);
        let mut pixels = vec![PremultipliedRgbaColor::default(); w * h];
        let window = self.window();
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

fn rows(ui: &MiniView) -> Vec<String> {
    ui.get_rows().iter().map(|r| r.name.to_string()).collect()
}

/// One test, because the platform and the controller live on this thread.
#[test]
fn mini_view_flows() {
    let h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let path = |name: &str| dir.path().join(name);

    // Open, select, extract: the smart folder keeps the archive's own folder.
    support::sample_zip(&path("project-assets.zip"));
    let ui = h.open(&path("project-assets.zip"));
    h.until("listing", || ui.get_status() == "ready");
    assert_eq!(rows(&ui), ["project"]);
    assert_eq!(ui.get_dest_name(), "project");
    assert_eq!(ui.get_format_name(), "ZIP");
    h.shot("01-ready");
    ui.invoke_search("readme".into());
    assert_eq!(rows(&ui), ["project/README.md"]);
    ui.invoke_toggle_row(0);
    assert_eq!(ui.get_selected_count(), 1);
    h.shot("02-search-selected");
    ui.invoke_extract();
    h.until("done popup", || ui.get_show_done());
    h.shot("03-done");
    assert!(path("project/README.md").is_file());
    assert!(!path("project/run.sh").exists(), "only the selection");
    assert!(
        ui.get_done_message()
            .starts_with("1 selected item extracted")
    );
    ui.invoke_back_to_archive();
    // Now "project/" exists, so everything goes into a new folder.
    assert_eq!(ui.get_dest_name(), "project-assets");
    ui.invoke_search("".into());
    assert_eq!(ui.get_selected_count(), 1, "selection survives the search");
    ui.invoke_search("readme".into());
    ui.invoke_toggle_row(0);
    assert_eq!(ui.get_selected_count(), 0);
    ui.invoke_extract();
    h.until("done popup", || ui.get_show_done());
    assert!(path("project-assets/project/run.sh").is_file());

    // Dark mode follows the same tokens.
    ui.global::<crate_app::Palette>()
        .set_color_scheme(slint::language::ColorScheme::Dark);
    ui.invoke_toggle_row(0);
    ui.invoke_extract();
    h.until("done popup", || ui.get_show_done());
    h.shot("11-dark-done");
    ui.invoke_back_to_archive();
    h.shot("12-dark");
    ui.global::<crate_app::Palette>()
        .set_color_scheme(slint::language::ColorScheme::Light);

    // Encrypted entries: ask for the password, show a wrong one, then extract.
    support::encrypted_zip(&path("secret.zip"));
    let ui = h.open(&path("secret.zip"));
    h.until("listing", || ui.get_status() == "ready");
    assert_eq!(ui.get_encrypted_count(), 1);
    ui.invoke_extract();
    assert!(ui.get_show_password());
    ui.invoke_unlock("wrong".into());
    h.until("password check", || !ui.get_password_checking());
    assert_eq!(ui.get_password_error(), "Incorrect password");
    h.shot("04-password-wrong");
    ui.invoke_unlock("secret".into());
    h.until("done popup", || ui.get_show_done());
    assert_eq!(
        std::fs::read_to_string(path("secret/secret.txt")).unwrap(),
        "secret"
    );

    // An encrypted file list: locked until the password is given.
    support::encrypted_7z(&path("names.7z"), true);
    let ui = h.open(&path("names.7z"));
    h.until("password prompt", || ui.get_show_password());
    assert_eq!(ui.get_status(), "locked");
    ui.invoke_unlock("nope".into());
    h.until("second prompt", || ui.get_password_error() != "");
    ui.invoke_cancel_password();
    h.shot("05-locked");
    ui.invoke_ask_password();
    ui.invoke_unlock("secret".into());
    h.until("listing", || ui.get_status() == "ready");
    assert_eq!(rows(&ui), ["secret.txt"]);

    // Old Arabic names: banner, picker, fixed names.
    support::arabic_cp1256_zip(&path("arabic.zip"));
    let ui = h.open(&path("arabic.zip"));
    h.until("listing", || ui.get_status() == "ready");
    assert!(ui.get_show_banner());
    assert!(ui.get_banner_text().contains("Windows-1256"));
    h.shot("06-banner");
    ui.invoke_open_picker();
    assert!(ui.get_show_picker());
    let labels: Vec<String> = ui
        .get_codec_labels()
        .iter()
        .map(|l| l.to_string())
        .collect();
    assert_eq!(
        labels[ui.get_codec_index() as usize],
        "Arabic (Windows-1256)"
    );
    assert_eq!(ui.get_codec_suggested(), ui.get_codec_index());
    h.shot("07-picker");
    let cp437 = labels.iter().position(|l| l.contains("CP437")).unwrap();
    ui.invoke_pick_codec(cp437 as i32);
    ui.invoke_cancel_picker();
    assert!(
        rows(&ui).iter().any(|r| r == "الصور"),
        "cancel keeps the names"
    );
    ui.invoke_open_picker();
    let arabic = ui.get_codec_index();
    ui.invoke_pick_codec(arabic);
    ui.invoke_apply_codec();
    assert!(!ui.get_show_banner());
    assert_eq!(ui.get_encoding_chip(), "Arabic (Windows-1256)");

    // Damaged archive.
    support::truncated_tar_gz(&path("cut.tar.gz"));
    let ui = h.open(&path("cut.tar.gz"));
    h.until("error state", || ui.get_status() == "failed");
    h.shot("08-failed");

    // Likely bomb: refused, with "Extract anyway".
    support::bomb_zip(&path("bomb.zip"));
    let ui = h.open(&path("bomb.zip"));
    h.until("listing", || ui.get_status() == "ready");
    ui.invoke_extract();
    h.until("error popup", || ui.get_show_error());
    assert!(ui.get_error_can_force());
    h.shot("09-bomb");
    ui.invoke_dismiss_error();

    // Cancel a large extraction: nothing stays behind.
    big_zip(&path("big.zip"));
    let ui = h.open(&path("big.zip"));
    h.until("listing", || ui.get_status() == "ready");
    ui.invoke_extract();
    h.until("progress", || ui.get_progress() > 0.05);
    h.shot_now("10-progress");
    ui.invoke_cancel_extract();
    h.until("cancelled", || !ui.get_busy());
    assert!(
        ui.get_toast().contains("cancelled"),
        "toast {:?} done {} error {:?}",
        ui.get_toast(),
        ui.get_show_done(),
        ui.get_error_message()
    );
    assert!(!path("big").exists());
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(".crate-extract-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

/// 48 × 8 MiB of zeros, stored: big enough to watch the progress.
fn big_zip(path: &Path) {
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let chunk = vec![0u8; 1 << 20];
    for i in 0..48 {
        zip.start_file(format!("part{i:02}.bin"), stored).unwrap();
        for _ in 0..8 {
            zip.write_all(&chunk).unwrap();
        }
    }
    zip.finish().unwrap();
}
