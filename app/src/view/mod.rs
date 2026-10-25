//! Behavior of an archive window (mini and full view). Archive work runs on
//! worker threads; results come back to the UI thread through
//! `slint::invoke_from_event_loop`, where the one `View` lives (thread-local,
//! so workers never touch UI state).

mod browse;
mod extract;
mod listing;
mod names;
mod password;

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use crate_core::destination::Destination;
use crate_core::{Archive, CancelToken, Entry, NameCodec};
use slint::{ComponentHandle, Model, SharedString, VecModel};

use crate::settings::Settings;
use crate::tree::{NodeId, ROOT, SortKey, Tree};
use crate::{ArchiveUi, ArchiveWindow, FileRow, Row, model};

thread_local! {
    static VIEW: RefCell<Option<Rc<View>>> = const { RefCell::new(None) };
}

/// Runs `f` with the controller on the UI thread. Callable from any thread.
pub fn post(f: impl FnOnce(&View) + Send + 'static) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(view) = VIEW.with(|v| v.borrow().clone()) {
            f(&view);
        }
    });
}

/// Runs `f` with the controller right away (on the UI thread).
fn now(f: impl FnOnce(&View)) {
    if let Some(v) = VIEW.with(|v| v.borrow().clone()) {
        f(&v);
    }
}

/// Drops the controller, which removes the temporary copies of opened items
/// and saves what should be remembered.
pub fn shutdown() {
    if let Some(v) = VIEW.with(|v| v.borrow_mut().take()) {
        v.remember_size();
    }
}

/// How often running work updates the UI.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(33);
const TOAST_TIME: Duration = Duration::from_millis(2200);
const MINI_SIZE: (f32, f32) = (520.0, 620.0);
const FULL_SIZE: (f32, f32) = (1180.0, 720.0);

#[derive(Clone, Copy, PartialEq, Eq)]
enum PasswordFor {
    /// The file list is encrypted: needed to open the archive at all.
    Open,
    /// Some selected entries (or the one being opened) are encrypted.
    Extract,
}

pub struct View {
    ui: slint::Weak<ArchiveWindow>,
    path: PathBuf,
    state: RefCell<State>,
    /// Called once the listing (or an error) is on screen; the startup probe
    /// uses it.
    on_listed: RefCell<Option<Box<dyn FnOnce()>>>,
    toast_timer: slint::Timer,
    rows_model: Rc<VecModel<Row>>,
    files_model: Rc<VecModel<FileRow>>,
}

#[derive(Default)]
struct State {
    /// `None` while a worker thread has it, or before it is listed.
    archive: Option<Archive>,
    entries: Arc<Vec<Entry>>,
    /// Set while a slow listing streams in; cancelling stops it.
    streaming: Option<CancelToken>,
    /// Built when the full view is first shown, then kept up to date.
    tree: Option<Tree>,

    // The list on screen (mini rows or full-view nodes) and its selection.
    mini_rows: Vec<model::Row>,
    full_rows: Vec<NodeId>,
    row_paths: Vec<String>,
    selection: model::Picker,
    query: String,

    // Full view
    folder: NodeId,
    expanded: HashSet<NodeId>,
    tree_rows: Vec<NodeId>,
    sort: SortKey,
    descending: bool,
    testing: Option<CancelToken>,

    // Extraction
    dest: Option<Destination>,
    cancel: Option<CancelToken>,
    can_force: bool,
    quit_when_idle: bool,
    done_folder: Option<PathBuf>,
    pending_open: Option<String>,
    open_dir: Option<tempfile::TempDir>,

    // Passwords and names
    password_for: Option<PasswordFor>,
    wrong_passwords: u32,
    forced_codec: Option<NameCodec>,
    codecs: Vec<NameCodec>,
    codec_index: usize,

    clip_serial: i32,
    settings: Settings,
}

impl View {
    /// Creates the controller for `ui` and starts reading `path`.
    pub fn start(ui: &ArchiveWindow, path: PathBuf, on_listed: Option<Box<dyn FnOnce()>>) {
        let view = Rc::new(View {
            ui: ui.as_weak(),
            path: path.clone(),
            state: RefCell::new(State {
                expanded: HashSet::from([ROOT]),
                settings: Settings::load(),
                ..State::default()
            }),
            on_listed: RefCell::new(on_listed),
            toast_timer: slint::Timer::default(),
            rows_model: Rc::new(VecModel::default()),
            files_model: Rc::new(VecModel::default()),
        });
        VIEW.with(|v| *v.borrow_mut() = Some(view.clone()));
        let g = ui.global::<ArchiveUi>();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        g.set_archive_name(name.into());
        g.set_rows(view.rows_model.clone().into());
        g.set_files(view.files_model.clone().into());
        g.set_preview_enabled(false);
        connect(ui);
        view.open(None);
    }

    fn ui(&self) -> ArchiveWindow {
        self.ui
            .upgrade()
            .expect("the window outlives its controller")
    }

    /// Runs `f` with the window's shared state.
    fn g<R>(&self, f: impl FnOnce(&ArchiveUi) -> R) -> R {
        let ui = self.ui();
        let g = ui.global::<ArchiveUi>();
        f(&g)
    }

    // ----- Toast, clipboard, errors --------------------------------------

    fn show_toast(&self, text: &str) {
        self.g(|g| g.set_toast(text.into()));
        let weak = self.ui.clone();
        self.toast_timer
            .start(slint::TimerMode::SingleShot, TOAST_TIME, move || {
                if let Some(ui) = weak.upgrade() {
                    ui.global::<ArchiveUi>().set_toast(SharedString::new());
                }
            });
    }

    fn copy_to_clipboard(&self, text: &str, toast: &str) {
        let serial = {
            let mut s = self.state.borrow_mut();
            s.clip_serial += 1;
            s.clip_serial
        };
        self.g(|g| {
            g.set_clip_text(text.into());
            g.set_clip_serial(serial);
        });
        self.show_toast(toast);
    }

    fn show_error(&self, title: &str, message: &str) {
        self.g(|g| {
            g.set_error_title(title.into());
            g.set_error_message(sentence(message).into());
            g.set_error_can_force(false);
            g.set_show_error(true);
        });
    }

    // ----- Window --------------------------------------------------------

    /// Closes the window. Running work is cancelled first and the app quits
    /// once it has cleaned up, so no partial files stay behind.
    pub fn close_window(&self) {
        let _ = self.ui().hide();
        let mut s = self.state.borrow_mut();
        if let Some(t) = &s.streaming {
            t.cancel();
        }
        if let Some(t) = &s.testing {
            t.cancel();
        }
        match &s.cancel {
            Some(cancel) => {
                cancel.cancel();
                s.quit_when_idle = true;
            }
            None => {
                let _ = slint::quit_event_loop();
            }
        }
    }

    /// Brings the window to the front (the archive was opened again).
    pub fn raise(&self) {
        use slint::winit_030::WinitWindowAccessor;
        self.ui().window().with_winit_window(|w| {
            w.set_minimized(false);
            w.focus_window();
        });
    }

    fn minimize(&self) {
        use slint::winit_030::WinitWindowAccessor;
        self.ui()
            .window()
            .with_winit_window(|w| w.set_minimized(true));
    }

    fn toggle_maximize(&self) {
        use slint::winit_030::WinitWindowAccessor;
        self.ui()
            .window()
            .with_winit_window(|w| w.set_maximized(!w.is_maximized()));
    }

    fn is_maximized(&self) -> bool {
        use slint::winit_030::WinitWindowAccessor;
        self.ui()
            .window()
            .with_winit_window(|w| w.is_maximized())
            .unwrap_or(false)
    }

    /// Mini ↔ full. The full view's size is remembered between runs.
    fn toggle_full(&self) {
        let full = !self.g(|g| g.get_full());
        if !full {
            self.remember_size();
            if self.is_maximized() {
                self.toggle_maximize();
            }
        }
        let (w, h) = if full {
            let s = self.state.borrow();
            (
                s.settings.get("full_width").unwrap_or(FULL_SIZE.0),
                s.settings.get("full_height").unwrap_or(FULL_SIZE.1),
            )
        } else {
            MINI_SIZE
        };
        {
            let mut s = self.state.borrow_mut();
            // Positions mean something else in the other list.
            s.selection.rows_changed();
        }
        self.g(|g| g.set_full(full));
        self.ui().window().set_size(slint::LogicalSize::new(w, h));
        self.refresh_rows();
    }

    /// Saves the full view's size, if that's what is showing.
    fn remember_size(&self) {
        if !self.g(|g| g.get_full()) || self.is_maximized() {
            return;
        }
        let size = self.ui().window().size();
        let scale = self.ui().window().scale_factor();
        let mut s = self.state.borrow_mut();
        s.settings
            .set("full_width", (size.width as f32 / scale).round());
        s.settings
            .set("full_height", (size.height as f32 / scale).round());
        s.settings.save();
    }
}

fn connect(ui: &ArchiveWindow) {
    fn with(f: impl Fn(&View) + 'static) -> impl Fn() + 'static {
        move || now(&f)
    }
    let g = ui.global::<ArchiveUi>();
    // Window
    g.on_start_drag(with(|v| crate::begin_window_drag(&v.ui())));
    g.on_minimize(with(View::minimize));
    g.on_toggle_maximize(with(View::toggle_maximize));
    g.on_close_window(with(View::close_window));
    g.on_toggle_full(with(View::toggle_full));
    ui.window().on_close_requested(|| {
        now(View::close_window);
        slint::CloseRequestResponse::KeepWindowShown
    });
    // Listing
    g.on_stop_reading(with(View::stop_reading));
    // List and selection
    g.on_search(|q| now(move |v| v.search(&q)));
    g.on_row_pressed(|i, ctrl, shift| {
        now(move |v| v.select(|p, rows| p.click(rows, i as usize, ctrl, shift)))
    });
    g.on_row_activated(|i| now(move |v| v.activate_row(i)));
    g.on_step(|delta, shift| now(move |v| v.select(|p, rows| p.step(rows, delta, shift))));
    g.on_toggle_cursor(with(|v| v.select(|p, rows| p.toggle_cursor(rows))));
    g.on_select_all(with(|v| v.select(|p, rows| p.select_all(rows))));
    g.on_clear_selection(with(|v| v.select(|p, _| p.clear())));
    // Full view
    g.on_open_folder_row(|i| now(move |v| v.open_tree_row(i as usize)));
    g.on_toggle_folder_row(|i| now(move |v| v.toggle_tree_row(i as usize)));
    g.on_open_crumb(|i| now(move |v| v.open_crumb(i as usize)));
    g.on_go_up(with(View::go_up));
    g.on_sort_by(|c| now(move |v| v.sort_by(c)));
    g.on_test_archive(with(View::test_archive));
    g.on_copy_path(with(View::copy_path));
    g.on_copy_checksum(with(View::copy_checksum));
    g.on_extract_to_downloads(with(View::extract_to_downloads));
    g.on_preview_current(with(|_| {}));
    // Extraction
    g.on_extract(with(View::extract));
    g.on_extract_dialog(with(View::extract));
    g.on_cancel_extract(with(View::cancel_extract));
    g.on_open_folder_and_close(with(View::open_folder_and_close));
    g.on_open_folder(with(View::open_folder));
    g.on_back_to_archive(with(View::back_to_archive));
    g.on_dismiss_error(with(|v| v.g(|g| g.set_show_error(false))));
    g.on_force_extract(with(View::force_extract));
    // Passwords
    g.on_unlock(|pw| now(move |v| v.unlock(pw.into())));
    g.on_cancel_password(with(View::cancel_password));
    g.on_ask_password(with(|v| v.ask_password(PasswordFor::Open, "")));
    // Names
    g.on_open_picker(with(View::open_picker));
    g.on_pick_codec(|i| now(move |v| v.pick_codec(i as usize)));
    g.on_apply_codec(with(View::apply_codec));
    g.on_cancel_picker(with(View::cancel_picker));
}

/// Opens a file with its default app, or a folder in the file manager
/// (`xdg-open` on Linux).
pub type Opener = fn(&Path) -> std::io::Result<()>;

thread_local! {
    static OPENER: std::cell::Cell<Opener> = std::cell::Cell::new(|p| open::that_detached(p));
}

/// Replaces how files and folders are opened (tests record them instead).
pub fn set_opener(opener: Opener) {
    OPENER.with(|o| o.set(opener));
}

fn open_with_default_app(path: &Path) -> std::io::Result<()> {
    OPENER.with(|o| o.get())(path)
}

fn cache_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
}

/// Updates the rows of `model` whose `selected` flag changed.
fn sync_selected<T: Clone + 'static>(
    model: &VecModel<T>,
    paths: &[String],
    selected: &HashSet<String>,
    flag: impl Fn(&mut T) -> &mut bool,
) {
    for (i, path) in paths.iter().enumerate() {
        let Some(mut row) = model.row_data(i) else {
            break;
        };
        let want = selected.contains(path);
        if *flag(&mut row) != want {
            *flag(&mut row) = want;
            model.set_row_data(i, row);
        }
    }
}

/// Error texts start lowercase; dialogs want a sentence.
fn sentence(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) => {
            let mut s: String = c.to_uppercase().collect();
            s.push_str(chars.as_str());
            if !s.ends_with('.') {
                s.push('.');
            }
            s
        }
        None => String::new(),
    }
}
