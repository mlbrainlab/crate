//! Extracting (all, the selection, or one item to open it), the done popup,
//! and testing the archive. All of it runs on worker threads.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate_core::destination::{self, Destination};
use crate_core::{Archive, CancelToken, Error, ExtractOptions, ExtractReport, Progress, Selection};

use super::{
    PROGRESS_INTERVAL, PasswordFor, View, cache_dir, open_with_default_app, post, sentence,
};
use crate::model;

/// Sends throttled progress to the UI from a worker.
fn progress_sender() -> impl FnMut(&Progress) {
    let mut last: Option<Instant> = None;
    move |p: &Progress| {
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
        post(move |v| v.progress(fraction, &current));
    }
}

impl View {
    /// The primary button: everything, or the selection, next to the archive.
    pub(super) fn extract(&self) {
        if self.needs_password_for_selection() {
            self.ask_password(PasswordFor::Extract, "");
        } else {
            self.run_extract(None, false);
        }
    }

    pub(super) fn extract_to_downloads(&self) {
        if self.needs_password_for_selection() {
            self.ask_password(PasswordFor::Extract, "");
            return;
        }
        let downloads = crate::settings::downloads_dir();
        let dest = {
            let s = self.state.borrow();
            destination::suggest_in(&downloads, &self.path, &s.entries)
        };
        self.run_extract(Some(dest), false);
    }

    pub(super) fn force_extract(&self) {
        self.g(|g| g.set_show_error(false));
        if self.state.borrow().can_force {
            self.run_extract(None, true);
        }
    }

    fn needs_password_for_selection(&self) -> bool {
        let s = self.state.borrow();
        let Some(archive) = &s.archive else {
            return false;
        };
        !archive.has_password()
            && model::selection_needs_password(&s.entries, &s.selection.selected)
    }

    /// Extracts the selection (or everything) into `dest` (default: next to
    /// the archive). `force` skips the decompression-bomb ratio check.
    pub(super) fn run_extract(&self, dest: Option<Destination>, force: bool) {
        if dest.is_none() {
            self.refresh_destination();
        }
        let mut s = self.state.borrow_mut();
        let Some(mut archive) = s.archive.take() else {
            return;
        };
        let dest = dest.unwrap_or_else(|| s.dest.clone().expect("set with the listing"));
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

        self.g(|g| {
            g.set_progress(0.0);
            g.set_current_file("".into());
            g.set_busy(true);
        });
        let mut options = ExtractOptions::default();
        if force {
            options.limits.max_ratio = None;
        }
        std::thread::spawn(move || {
            let started = Instant::now();
            let mut on_progress = progress_sender();
            let result =
                archive.extract(&selection, &dest.dest, &options, &mut on_progress, &cancel);
            let elapsed = started.elapsed();
            post(move |v| v.extracted(archive, result, dest, selected, elapsed));
        });
    }

    pub(super) fn progress(&self, fraction: f32, current: &str) {
        self.g(|g| {
            if g.get_busy() {
                g.set_progress(fraction.clamp(0.0, 1.0));
                g.set_current_file(current.into());
            }
        });
    }

    /// The worker hands the archive back. Returns true when the app should
    /// quit now (the window was closed meanwhile).
    fn finish_work(&self, archive: Archive) -> bool {
        let quit = {
            let mut s = self.state.borrow_mut();
            s.archive = Some(archive);
            s.cancel = None;
            s.quit_when_idle
        };
        self.g(|g| {
            g.set_busy(false);
            g.set_opening("".into());
        });
        if quit {
            let _ = slint::quit_event_loop();
        }
        quit
    }

    fn extracted(
        &self,
        archive: Archive,
        result: crate_core::Result<ExtractReport>,
        dest: Destination,
        selected: usize,
        elapsed: Duration,
    ) {
        self.state.borrow_mut().can_force = false;
        if self.finish_work(archive) {
            return;
        }
        match result {
            Ok(report) => {
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
                let skipped = report.skipped.len() as u64;
                self.g(|g| {
                    g.set_progress(1.0);
                    g.set_done_message(message.into());
                    g.set_done_folder(model::display_folder(&dest.folder).into());
                    g.set_done_warning(if skipped == 0 {
                        "".into()
                    } else {
                        format!("{} skipped for your safety", model::count(skipped, "item")).into()
                    });
                    g.set_show_done(true);
                });
                self.state.borrow_mut().done_folder = Some(dest.folder);
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
                self.g(|g| {
                    g.set_error_title(title.into());
                    g.set_error_message(message.into());
                    g.set_error_can_force(can_force);
                    g.set_show_error(true);
                });
            }
        }
    }

    pub(super) fn cancel_extract(&self) {
        if let Some(cancel) = &self.state.borrow().cancel {
            cancel.cancel();
        }
    }

    // ----- Opening an item ------------------------------------------------

    /// Extract one row to a private temporary folder and open it with the
    /// default app (a folder opens in the file manager).
    pub(super) fn open_row(&self, index: usize) {
        let (path, needs_password) = {
            let s = self.state.borrow();
            let (Some(path), Some(archive)) = (s.row_paths.get(index), &s.archive) else {
                return; // no such row, or a worker has the archive
            };
            let one = HashSet::from([path.clone()]);
            let needs =
                !archive.has_password() && model::selection_needs_password(&s.entries, &one);
            (path.clone(), needs)
        };
        if needs_password {
            self.state.borrow_mut().pending_open = Some(path);
            self.ask_password(PasswordFor::Extract, "");
        } else {
            self.run_open(path);
        }
    }

    pub(super) fn run_open(&self, path: String) {
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
        let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
        self.g(|g| {
            g.set_opening(name.into());
            g.set_progress(0.0);
            g.set_current_file("".into());
            g.set_busy(true);
        });
        std::thread::spawn(move || {
            let mut on_progress = progress_sender();
            let result = archive.extract(
                &Selection::Paths(vec![path.clone()]),
                &dest,
                &ExtractOptions::default(),
                &mut on_progress,
                &cancel,
            );
            post(move |v| v.opened_item(archive, result, dest, path));
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
        if self.finish_work(archive) {
            return;
        }
        self.g(|g| g.set_progress(0.0));
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

    // ----- Done popup ----------------------------------------------------

    fn open_done_folder(&self) -> bool {
        let Some(folder) = self.state.borrow().done_folder.clone() else {
            return false;
        };
        open_with_default_app(&folder).is_ok()
    }

    pub(super) fn open_folder_and_close(&self) {
        if self.open_done_folder() {
            self.close_window();
        } else {
            self.show_toast("Couldn’t open the file manager");
        }
    }

    pub(super) fn open_folder(&self) {
        self.back_to_archive();
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

    pub(super) fn back_to_archive(&self) {
        self.g(|g| {
            g.set_show_done(false);
            g.set_progress(0.0);
        });
    }

    // ----- Test ----------------------------------------------------------

    /// Reads every file and checks it, on a copy of the archive handle, so
    /// browsing goes on meanwhile.
    pub(super) fn test_archive(&self) {
        let copy = {
            let s = self.state.borrow();
            if s.testing.is_some() {
                return;
            }
            match s.archive.as_ref().map(Archive::try_clone) {
                Some(Ok(a)) => a,
                Some(Err(e)) => return self.show_error("Couldn’t test", &e.to_string()),
                None => return,
            }
        };
        let cancel = CancelToken::new();
        self.state.borrow_mut().testing = Some(cancel.clone());
        self.g(|g| {
            g.set_test_state(1);
            g.set_test_status("Testing…".into());
        });
        std::thread::spawn(move || {
            let mut archive = copy;
            let mut last: Option<Instant> = None;
            let result = archive.test(
                &mut |p| {
                    if last.is_some_and(|t| t.elapsed() < Duration::from_millis(100)) {
                        return;
                    }
                    last = Some(Instant::now());
                    let pct = (p.bytes_done * 100).checked_div(p.bytes_total).unwrap_or(0);
                    post(move |v| v.g(|g| g.set_test_status(format!("Testing… {pct}%").into())));
                },
                &cancel,
            );
            post(move |v| v.tested(result));
        });
    }

    fn tested(&self, result: crate_core::Result<crate_core::TestReport>) {
        self.state.borrow_mut().testing = None;
        match result {
            Ok(report) if report.is_ok() => {
                let note = match report.skipped.len() {
                    0 => String::new(),
                    n => format!(
                        " · {} not tested (encrypted)",
                        model::count(n as u64, "file")
                    ),
                };
                self.g(|g| {
                    g.set_test_state(2);
                    g.set_test_status(format!("Archive OK{note}").into());
                });
            }
            Ok(report) => {
                let n = report.failed.len() as u64;
                let list: Vec<String> = report
                    .failed
                    .iter()
                    .take(8)
                    .map(|(path, why)| format!("{path} · {why}"))
                    .collect();
                self.g(|g| {
                    g.set_test_state(3);
                    g.set_test_status(format!("{} damaged", model::count(n, "file")).into());
                });
                self.show_error(
                    "Archive is damaged",
                    &format!(
                        "{} of {} failed the check:\n{}",
                        model::count(n, "file"),
                        model::thousands(report.files + n),
                        list.join("\n")
                    ),
                );
            }
            Err(Error::Cancelled) => self.g(|g| g.set_test_state(0)),
            Err(e) => {
                self.g(|g| {
                    g.set_test_state(3);
                    g.set_test_status("Damaged".into());
                });
                self.show_error("Archive is damaged", &e.to_string());
            }
        }
    }
}
