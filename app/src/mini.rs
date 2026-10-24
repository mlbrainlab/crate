//! The mini view's behavior. Archive work runs on worker threads; results come
//! back to the UI thread through `slint::invoke_from_event_loop`, where the one
//! `Mini` controller lives (thread-local, so workers never touch UI state).

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate_core::destination::{self, Destination};
use crate_core::{
    Archive, CancelToken, Entry, Error, ExtractOptions, ExtractReport, NameCodec, OpenOptions,
    Progress, Selection,
};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::model;
use crate::{MiniView, Row};

thread_local! {
    static MINI: RefCell<Option<Rc<Mini>>> = const { RefCell::new(None) };
}

/// Runs `f` with the controller on the UI thread. Callable from any thread.
pub fn post(f: impl FnOnce(&Mini) + Send + 'static) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(mini) = MINI.with(|m| m.borrow().clone()) {
            f(&mini);
        }
    });
}

/// How often a running extraction updates the progress bar.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(33);
const TOAST_TIME: Duration = Duration::from_millis(2200);

#[derive(Clone, Copy, PartialEq, Eq)]
enum PasswordFor {
    /// The file list is encrypted: needed to open the archive at all.
    Open,
    /// Some selected entries (or the one being opened) are encrypted:
    /// needed before extracting.
    Extract,
}

pub struct Mini {
    ui: slint::Weak<MiniView>,
    path: PathBuf,
    state: RefCell<State>,
    /// Called once the listing (or an error) is on screen; the startup probe
    /// uses it.
    on_listed: RefCell<Option<Box<dyn FnOnce()>>>,
    toast_timer: slint::Timer,
}

#[derive(Default)]
struct State {
    /// `None` while a worker thread has it.
    archive: Option<Archive>,
    entries: Vec<Entry>,
    rows: Vec<model::Row>,
    selection: model::Picker,
    query: String,
    dest: Option<Destination>,
    cancel: Option<CancelToken>,
    password_for: Option<PasswordFor>,
    wrong_passwords: u32,
    /// The encoding names were decoded with, when the user picked one.
    forced_codec: Option<NameCodec>,
    picker: Vec<NameCodec>,
    picker_index: usize,
    /// Extraction of the selection was refused as a likely bomb; "Extract
    /// anyway" runs it again without the ratio check.
    can_force: bool,
    quit_when_idle: bool,
    done_folder: Option<PathBuf>,
    /// An item to open once the password is in.
    pending_open: Option<String>,
    /// Where double-clicked items are extracted to; removed when Crate exits.
    open_dir: Option<tempfile::TempDir>,
}

impl Mini {
    /// Creates the controller for `ui` and starts reading `path`.
    pub fn start(ui: &MiniView, path: PathBuf, on_listed: Option<Box<dyn FnOnce()>>) {
        let mini = Rc::new(Mini {
            ui: ui.as_weak(),
            path: path.clone(),
            state: RefCell::default(),
            on_listed: RefCell::new(on_listed),
            toast_timer: slint::Timer::default(),
        });
        MINI.with(|m| *m.borrow_mut() = Some(mini.clone()));
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        ui.set_archive_name(name.into());
        connect(ui);
        mini.open(None);
    }

    fn ui(&self) -> MiniView {
        self.ui
            .upgrade()
            .expect("the mini view outlives its controller")
    }

    // ----- Opening -------------------------------------------------------

    fn open(&self, password: Option<String>) {
        let path = self.path.clone();
        std::thread::spawn(move || {
            let result = Archive::open_with(
                &path,
                &OpenOptions {
                    password,
                    ..OpenOptions::default()
                },
            );
            post(move |m| m.opened(result));
        });
    }

    fn opened(&self, result: crate_core::Result<Archive>) {
        let ui = self.ui();
        ui.set_password_checking(false);
        match result {
            Ok(archive) => {
                ui.set_show_password(false);
                ui.set_format_name(archive.format().name().into());
                ui.set_status("ready".into());
                self.state.borrow_mut().archive = Some(archive);
                self.refresh_listing();
            }
            Err(Error::PasswordRequired) => {
                ui.set_status("locked".into());
                ui.set_status_text("Enter the password to see what’s inside.".into());
                self.ask_password(PasswordFor::Open, "");
            }
            Err(Error::WrongPassword) => {
                ui.set_status("locked".into());
                self.wrong_password(PasswordFor::Open);
            }
            Err(e) => {
                ui.set_status("failed".into());
                ui.set_status_text(sentence(&e.to_string()).into());
            }
        }
        if let Some(listed) = self.on_listed.borrow_mut().take() {
            listed();
        }
    }

    /// Re-reads entries from the archive (after opening or "Fix names").
    fn refresh_listing(&self) {
        let ui = self.ui();
        let mut s = self.state.borrow_mut();
        let Some(archive) = &s.archive else { return };
        let entries = archive.entries().to_vec();
        let guessed = archive.name_encoding().filter(|e| e.guessed);
        ui.set_summary(model::summary(&entries, archive.file_size()).into());
        ui.set_item_count(entries.len() as i32);
        ui.set_encrypted_count(model::encrypted_count(&entries) as i32);
        ui.set_show_banner(guessed.is_some());
        if let Some(e) = guessed {
            ui.set_banner_text(format!("Names were read as {}.", e.codec.label()).into());
        }
        ui.set_encoding_chip(s.forced_codec.map(|c| c.label()).unwrap_or_default().into());
        s.entries = entries;
        s.selection.clear();
        s.selection.rows_changed();
        drop(s);
        self.refresh_destination();
        self.refresh_rows();
    }

    fn refresh_destination(&self) {
        let mut s = self.state.borrow_mut();
        let dest = destination::suggest(&self.path, &s.entries);
        let ui = self.ui();
        ui.set_dest_name(dest.name.clone().into());
        ui.set_dest_label(model::display_folder(&dest.folder).into());
        s.dest = Some(dest);
    }

    fn refresh_rows(&self) {
        let mut s = self.state.borrow_mut();
        s.rows = if s.query.trim().is_empty() {
            model::top_level(&s.entries)
        } else {
            model::search(&s.entries, &s.query)
        };
        s.selection.rows_changed();
        drop(s);
        self.show_rows();
    }

    fn show_rows(&self) {
        let s = self.state.borrow();
        let rows: Vec<Row> = s
            .rows
            .iter()
            .map(|r| Row {
                name: r.name.clone().into(),
                ext: r.ext.clone().into(),
                meta: r.meta.clone().into(),
                kind: r.tile as i32,
                selected: s.selection.selected.contains(&r.path),
            })
            .collect();
        let ui = self.ui();
        ui.set_rows(ModelRc::new(VecModel::from(rows)));
        ui.set_selected_count(s.selection.selected.len() as i32);
        ui.set_cursor(s.selection.cursor.map_or(-1, |c| c as i32));
    }

    /// Changes the selection, unless an extraction is running.
    fn select(&self, change: impl FnOnce(&mut model::Picker, &[model::Row])) {
        {
            let mut s = self.state.borrow_mut();
            if s.cancel.is_some() {
                return;
            }
            let State {
                selection, rows, ..
            } = &mut *s;
            change(selection, rows);
        }
        self.show_rows();
    }

    fn search(&self, query: &str) {
        self.state.borrow_mut().query = query.to_owned();
        self.refresh_rows();
    }

    // ----- Extracting ----------------------------------------------------

    fn extract(&self) {
        let needs_password = {
            let s = self.state.borrow();
            let Some(archive) = &s.archive else { return };
            !archive.has_password()
                && model::selection_needs_password(&s.entries, &s.selection.selected)
        };
        if needs_password {
            self.ask_password(PasswordFor::Extract, "");
        } else {
            self.run_extract(false);
        }
    }

    fn run_extract(&self, force: bool) {
        self.refresh_destination();
        let mut s = self.state.borrow_mut();
        let Some(mut archive) = s.archive.take() else {
            return;
        };
        let dest = s.dest.clone().expect("destination is set with the listing");
        let selection = if s.selection.selected.is_empty() {
            Selection::All
        } else {
            let mut paths: Vec<String> = s.selection.selected.iter().cloned().collect();
            paths.sort();
            Selection::Paths(paths)
        };
        let selected = s.selection.selected.len();
        let cancel = CancelToken::new();
        s.cancel = Some(cancel.clone());
        drop(s);

        let ui = self.ui();
        ui.set_progress(0.0);
        ui.set_current_file("".into());
        ui.set_busy(true);

        let mut options = ExtractOptions::default();
        if force {
            options.limits.max_ratio = None;
        }
        std::thread::spawn(move || {
            let started = Instant::now();
            let mut last: Option<Instant> = None;
            let mut on_progress = |p: &Progress| {
                if last.is_some_and(|t| t.elapsed() < PROGRESS_INTERVAL) {
                    return;
                }
                last = Some(Instant::now());
                let fraction = if p.bytes_total > 0 {
                    p.bytes_done as f32 / p.bytes_total as f32
                } else if p.files_total > 0 {
                    p.files_done as f32 / p.files_total as f32
                } else {
                    0.0
                };
                let current = p.current.clone();
                post(move |m| m.progress(fraction, &current));
            };
            let result =
                archive.extract(&selection, &dest.dest, &options, &mut on_progress, &cancel);
            let elapsed = started.elapsed();
            post(move |m| m.extracted(archive, result, dest, selected, elapsed));
        });
    }

    fn progress(&self, fraction: f32, current: &str) {
        let ui = self.ui();
        if ui.get_busy() {
            ui.set_progress(fraction.clamp(0.0, 1.0));
            ui.set_current_file(current.into());
        }
    }

    fn extracted(
        &self,
        archive: Archive,
        result: crate_core::Result<ExtractReport>,
        dest: Destination,
        selected: usize,
        elapsed: Duration,
    ) {
        let ui = self.ui();
        let quit = {
            let mut s = self.state.borrow_mut();
            s.archive = Some(archive);
            s.cancel = None;
            s.can_force = false;
            s.quit_when_idle
        };
        ui.set_busy(false);
        if quit {
            let _ = slint::quit_event_loop();
            return;
        }
        match result {
            Ok(report) => {
                ui.set_progress(1.0);
                let items = report.files + report.dirs + report.links;
                let what = if selected > 0 {
                    model::count(selected as u64, "selected item")
                } else {
                    format!(
                        "{} · {}",
                        model::count(items, "item"),
                        model::size_text(report.bytes)
                    )
                };
                let message = match model::duration_text(elapsed) {
                    Some(time) => format!("{what} extracted in {time}"),
                    None => format!("{what} extracted"),
                };
                ui.set_done_message(message.into());
                ui.set_done_folder(model::display_folder(&dest.folder).into());
                let skipped = report.skipped.len() as u64;
                ui.set_done_warning(if skipped == 0 {
                    "".into()
                } else {
                    format!("{} skipped for your safety", model::count(skipped, "item")).into()
                });
                self.state.borrow_mut().done_folder = Some(dest.folder);
                ui.set_show_done(true);
                // The next "Extract all" must not reuse this folder.
                self.refresh_destination();
            }
            Err(Error::Cancelled) => self.show_toast("Extraction cancelled; nothing was written"),
            Err(Error::WrongPassword) => {
                if let Some(a) = self.state.borrow_mut().archive.as_mut() {
                    a.set_password(None);
                }
                self.wrong_password(PasswordFor::Extract);
            }
            Err(e) => {
                let (title, message, can_force) = match &e {
                    Error::SuspiciousRatio { ratio, .. } => (
                        "This may be a decompression bomb",
                        format!(
                            "It would expand to {ratio:.0}× its own size. Extract anyway only if you trust where it came from."
                        ),
                        true,
                    ),
                    Error::SizeExceeded { .. } => (
                        "This may be a decompression bomb",
                        sentence(&e.to_string()),
                        false,
                    ),
                    Error::NotEnoughSpace { .. } => {
                        ("Not enough free space", sentence(&e.to_string()), false)
                    }
                    _ => ("Couldn’t extract", sentence(&e.to_string()), false),
                };
                self.state.borrow_mut().can_force = can_force;
                ui.set_error_title(title.into());
                ui.set_error_message(message.into());
                ui.set_error_can_force(can_force);
                ui.set_show_error(true);
            }
        }
    }

    // ----- Opening an item ------------------------------------------------

    /// Double-click: extract one row to a private temporary folder and open
    /// it with the default app (a folder opens in the file manager).
    fn open_row(&self, index: usize) {
        let (path, needs_password) = {
            let s = self.state.borrow();
            let (Some(row), Some(archive)) = (s.rows.get(index), &s.archive) else {
                return; // no such row, or a worker has the archive
            };
            let one = HashSet::from([row.path.clone()]);
            let needs =
                !archive.has_password() && model::selection_needs_password(&s.entries, &one);
            (row.path.clone(), needs)
        };
        if needs_password {
            self.state.borrow_mut().pending_open = Some(path);
            self.ask_password(PasswordFor::Extract, "");
        } else {
            self.run_open(path);
        }
    }

    fn run_open(&self, path: String) {
        let dest = match self.new_open_folder() {
            Ok(d) => d,
            Err(e) => return self.show_error("Couldn’t open", &e.to_string()),
        };
        let mut s = self.state.borrow_mut();
        let Some(mut archive) = s.archive.take() else {
            return;
        };
        let cancel = CancelToken::new();
        s.cancel = Some(cancel.clone());
        drop(s);
        let name = path
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(&path)
            .to_owned();
        let ui = self.ui();
        ui.set_opening(name.into());
        ui.set_progress(0.0);
        ui.set_current_file("".into());
        ui.set_busy(true);
        std::thread::spawn(move || {
            let mut last: Option<Instant> = None;
            let mut on_progress = |p: &Progress| {
                if last.is_some_and(|t| t.elapsed() < PROGRESS_INTERVAL) {
                    return;
                }
                last = Some(Instant::now());
                let fraction = p.bytes_done as f32 / p.bytes_total.max(1) as f32;
                post(move |m| m.progress(fraction, ""));
            };
            let result = archive.extract(
                &Selection::Paths(vec![path.clone()]),
                &dest,
                &ExtractOptions::default(),
                &mut on_progress,
                &cancel,
            );
            post(move |m| m.opened_item(archive, result, dest, path));
        });
    }

    /// A new, empty folder under `~/.cache/crate/` for one opened item.
    fn new_open_folder(&self) -> std::io::Result<PathBuf> {
        let mut s = self.state.borrow_mut();
        if s.open_dir.is_none() {
            let root = cache_dir().join("crate");
            std::fs::create_dir_all(&root)?;
            s.open_dir = Some(tempfile::Builder::new().prefix("open-").tempdir_in(root)?);
        }
        let session = s.open_dir.as_ref().expect("just created").path();
        Ok(tempfile::Builder::new()
            .prefix("item-")
            .tempdir_in(session)?
            .keep())
    }

    fn opened_item(
        &self,
        archive: Archive,
        result: crate_core::Result<ExtractReport>,
        dest: PathBuf,
        path: String,
    ) {
        let ui = self.ui();
        let quit = {
            let mut s = self.state.borrow_mut();
            s.archive = Some(archive);
            s.cancel = None;
            s.quit_when_idle
        };
        ui.set_busy(false);
        ui.set_opening("".into());
        ui.set_progress(0.0);
        if quit {
            let _ = slint::quit_event_loop();
            return;
        }
        let target = crate_core::safety::safe_relative_path(&path)
            .ok()
            .map(|rel| dest.join(rel));
        match result {
            Ok(report) => match target.filter(|t| t.exists()) {
                Some(target) => {
                    if open_with_default_app(&target).is_err() {
                        self.show_toast("No app to open this file");
                    }
                }
                None => {
                    let why = report
                        .skipped
                        .first()
                        .map_or("it was skipped".to_owned(), |s| s.reason.to_string());
                    self.show_error("Couldn’t open", &format!("Skipped for your safety: {why}"));
                }
            },
            Err(Error::Cancelled) => {}
            Err(Error::WrongPassword) => {
                if let Some(a) = self.state.borrow_mut().archive.as_mut() {
                    a.set_password(None);
                }
                self.state.borrow_mut().pending_open = Some(path);
                self.wrong_password(PasswordFor::Extract);
            }
            Err(e) => self.show_error("Couldn’t open", &e.to_string()),
        }
    }

    fn show_error(&self, title: &str, message: &str) {
        let ui = self.ui();
        ui.set_error_title(title.into());
        ui.set_error_message(sentence(message).into());
        ui.set_error_can_force(false);
        ui.set_show_error(true);
    }

    fn cancel_extract(&self) {
        if let Some(cancel) = &self.state.borrow().cancel {
            cancel.cancel();
        }
    }

    // ----- Done popup ----------------------------------------------------

    fn open_done_folder(&self) -> bool {
        let Some(folder) = self.state.borrow().done_folder.clone() else {
            return false;
        };
        open_with_default_app(&folder).is_ok()
    }

    fn open_folder_and_close(&self) {
        if self.open_done_folder() {
            self.close_window();
        } else {
            self.show_toast("Couldn’t open the file manager");
        }
    }

    fn open_folder(&self) {
        let ui = self.ui();
        ui.set_show_done(false);
        ui.set_progress(0.0);
        if self.open_done_folder() {
            let name = self
                .state
                .borrow()
                .done_folder
                .as_ref()
                .and_then(|f| f.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.show_toast(&format!("Opened {name}/ in the file manager"));
        } else {
            self.show_toast("Couldn’t open the file manager");
        }
    }

    fn back_to_archive(&self) {
        let ui = self.ui();
        ui.set_show_done(false);
        ui.set_progress(0.0);
    }

    fn show_toast(&self, text: &str) {
        self.ui().set_toast(text.into());
        let weak = self.ui.clone();
        self.toast_timer
            .start(slint::TimerMode::SingleShot, TOAST_TIME, move || {
                if let Some(ui) = weak.upgrade() {
                    ui.set_toast(SharedString::new());
                }
            });
    }

    // ----- Passwords -----------------------------------------------------

    fn ask_password(&self, purpose: PasswordFor, error: &str) {
        self.state.borrow_mut().password_for = Some(purpose);
        let ui = self.ui();
        ui.set_password_error(error.into());
        ui.set_password_checking(false);
        ui.set_show_password(true);
    }

    fn wrong_password(&self, purpose: PasswordFor) {
        let attempts = {
            let mut s = self.state.borrow_mut();
            s.wrong_passwords += 1;
            s.wrong_passwords
        };
        let error = if attempts == 1 {
            "Incorrect password".to_owned()
        } else {
            format!("Incorrect password · {attempts} attempts")
        };
        self.ask_password(purpose, &error);
    }

    fn unlock(&self, password: String) {
        let purpose = self.state.borrow().password_for;
        let ui = self.ui();
        ui.set_password_checking(true);
        ui.set_password_error("".into());
        match purpose {
            Some(PasswordFor::Open) => {
                ui.set_status("loading".into());
                self.open(Some(password));
            }
            Some(PasswordFor::Extract) => {
                let Some(mut archive) = self.state.borrow_mut().archive.take() else {
                    return;
                };
                std::thread::spawn(move || {
                    let ok = archive.check_password(&password, &CancelToken::new());
                    post(move |m| m.password_checked(archive, ok));
                });
            }
            None => ui.set_show_password(false),
        }
    }

    fn password_checked(&self, archive: Archive, ok: crate_core::Result<bool>) {
        self.state.borrow_mut().archive = Some(archive);
        let ui = self.ui();
        ui.set_password_checking(false);
        match ok {
            Ok(true) => {
                ui.set_show_password(false);
                self.state.borrow_mut().password_for = None;
                let pending = self.state.borrow_mut().pending_open.take();
                match pending {
                    Some(path) => self.run_open(path),
                    None => self.run_extract(false),
                }
            }
            Ok(false) => self.wrong_password(PasswordFor::Extract),
            Err(e) => {
                ui.set_show_password(false);
                ui.set_error_title("Couldn’t check the password".into());
                ui.set_error_message(sentence(&e.to_string()).into());
                ui.set_error_can_force(false);
                ui.set_show_error(true);
            }
        }
    }

    fn cancel_password(&self) {
        let mut s = self.state.borrow_mut();
        s.password_for = None;
        s.pending_open = None;
        drop(s);
        self.ui().set_show_password(false);
    }

    // ----- Filename encoding ---------------------------------------------

    fn open_picker(&self) {
        let (codecs, current, guessed) = {
            let s = self.state.borrow();
            let Some(archive) = &s.archive else { return };
            let current = archive.name_encoding().map(|e| e.codec);
            let guessed = current.filter(|_| s.forced_codec.is_none());
            let mut codecs = NameCodec::common();
            if let Some(c) = current
                && !codecs.contains(&c)
            {
                codecs.insert(0, c);
            }
            (codecs, current, guessed)
        };
        let index = current
            .and_then(|c| codecs.iter().position(|&x| x == c))
            .unwrap_or(0);
        let ui = self.ui();
        let labels: Vec<SharedString> = codecs.iter().map(|c| c.label().into()).collect();
        ui.set_codec_labels(ModelRc::new(VecModel::from(labels)));
        ui.set_codec_suggested(
            guessed
                .and_then(|c| codecs.iter().position(|&x| x == c))
                .map_or(-1, |i| i as i32),
        );
        {
            let mut s = self.state.borrow_mut();
            s.picker = codecs;
        }
        self.pick_codec(index);
        ui.set_show_picker(true);
    }

    /// Decodes the names with the picked encoding to preview them.
    fn pick_codec(&self, index: usize) {
        let preview = {
            let mut s = self.state.borrow_mut();
            let Some(&codec) = s.picker.get(index) else {
                return;
            };
            s.picker_index = index;
            let Some(archive) = s.archive.as_mut() else {
                return;
            };
            archive.set_name_encoding(Some(codec));
            preview_names(archive.entries())
        };
        let ui = self.ui();
        ui.set_codec_index(index as i32);
        ui.set_codec_preview(ModelRc::new(VecModel::from(preview)));
    }

    fn apply_codec(&self) {
        {
            let mut s = self.state.borrow_mut();
            s.forced_codec = s.picker.get(s.picker_index).copied();
        }
        self.ui().set_show_picker(false);
        self.refresh_listing();
    }

    fn cancel_picker(&self) {
        {
            let mut s = self.state.borrow_mut();
            let forced = s.forced_codec;
            if let Some(archive) = s.archive.as_mut() {
                archive.set_name_encoding(forced);
            }
        }
        self.ui().set_show_picker(false);
    }

    // ----- Window --------------------------------------------------------

    /// Closes the window. A running extraction is cancelled first and the app
    /// quits once it has cleaned up, so no partial files stay behind.
    pub fn close_window(&self) {
        let ui = self.ui();
        let _ = ui.hide();
        let mut s = self.state.borrow_mut();
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
}

fn connect(ui: &MiniView) {
    fn with(f: impl Fn(&Mini) + 'static) -> impl Fn() + 'static {
        move || {
            if let Some(m) = MINI.with(|m| m.borrow().clone()) {
                f(&m);
            }
        }
    }
    ui.on_close_window(with(Mini::close_window));
    ui.on_extract(with(Mini::extract));
    ui.on_cancel_extract(with(Mini::cancel_extract));
    ui.on_open_folder_and_close(with(Mini::open_folder_and_close));
    ui.on_open_folder(with(Mini::open_folder));
    ui.on_back_to_archive(with(Mini::back_to_archive));
    ui.on_dismiss_error(with(|m| m.ui().set_show_error(false)));
    ui.on_force_extract(with(|m| {
        m.ui().set_show_error(false);
        if m.state.borrow().can_force {
            m.run_extract(true);
        }
    }));
    ui.on_cancel_password(with(Mini::cancel_password));
    ui.on_ask_password(with(|m| m.ask_password(PasswordFor::Open, "")));
    ui.on_open_picker(with(Mini::open_picker));
    ui.on_apply_codec(with(Mini::apply_codec));
    ui.on_cancel_picker(with(Mini::cancel_picker));
    ui.on_row_pressed(|i, ctrl, shift| {
        post_now(move |m| m.select(|p, rows| p.click(rows, i as usize, ctrl, shift)))
    });
    ui.on_row_activated(|i| post_now(move |m| m.open_row(i as usize)));
    ui.on_step(|delta, shift| post_now(move |m| m.select(|p, rows| p.step(rows, delta, shift))));
    ui.on_toggle_cursor(with(|m| m.select(|p, rows| p.toggle_cursor(rows))));
    ui.on_select_all(with(|m| m.select(|p, rows| p.select_all(rows))));
    ui.on_clear_selection(with(|m| m.select(|p, _| p.clear())));
    ui.on_search(|q| post_now(move |m| m.search(&q)));
    ui.on_unlock(|pw| post_now(move |m| m.unlock(pw.into())));
    ui.on_pick_codec(|i| post_now(move |m| m.pick_codec(i as usize)));
    ui.window().on_close_requested(|| {
        post_now(Mini::close_window);
        slint::CloseRequestResponse::KeepWindowShown
    });
}

/// Runs `f` with the controller right away (we're on the UI thread).
fn post_now(f: impl FnOnce(&Mini)) {
    if let Some(m) = MINI.with(|m| m.borrow().clone()) {
        f(&m);
    }
}

/// A few names that show what the encoding does: ones with non-ASCII
/// characters first.
fn preview_names(entries: &[Entry]) -> Vec<SharedString> {
    let mut names: Vec<&str> = entries
        .iter()
        .map(|e| e.path.as_str())
        .filter(|p| !p.is_ascii())
        .take(3)
        .collect();
    if names.is_empty() {
        names = entries.iter().map(|e| e.path.as_str()).take(3).collect();
    }
    names.into_iter().map(SharedString::from).collect()
}

/// Drops the controller, which removes the temporary copies of opened items.
pub fn shutdown() {
    MINI.with(|m| m.borrow_mut().take());
}

fn cache_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
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
