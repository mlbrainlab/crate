//! Extracting (all, the selection, or one item to open it), the done popup,
//! and testing the archive. All of it runs on worker threads.

use crate::i18n::{tr, trf, trn};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate_core::destination::{self, Destination};
use crate_core::{
    Archive, CancelToken, Conflict, Error, ExtractOptions, ExtractReport, Progress, Resolution,
    Selection,
};

use super::{
    PROGRESS_INTERVAL, PasswordFor, View, cache_dir, open_extracted, open_with_default_app, post,
    sentence,
};
use crate::model;

/// `Selection::All` for an empty set, else those paths.
pub(super) fn selection_of(paths: &HashSet<String>) -> Selection {
    if paths.is_empty() {
        Selection::All
    } else {
        let mut paths: Vec<String> = paths.iter().cloned().collect();
        paths.sort();
        Selection::Paths(paths)
    }
}

fn is_permission(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem
    )
}

/// Sends throttled progress to the UI from a worker.
fn progress_sender() -> impl FnMut(&Progress) {
    let mut last: Option<Instant> = None;
    move |p: &Progress| {
        let finished = p.bytes_done >= p.bytes_total && p.files_done >= p.files_total;
        if !finished && last.is_some_and(|t| t.elapsed() < PROGRESS_INTERVAL) {
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

/// One extraction the user asked for. Kept, so Retry, Extract anyway and
/// Extract to Downloads can run it again.
#[derive(Clone)]
pub(super) struct Request {
    pub selection: Selection,
    /// How many items were picked (0: everything), for the done message.
    pub picked: usize,
    pub dest: Destination,
    pub options: ExtractOptions,
    pub open_after: bool,
    pub trash_after: bool,
    /// Also unpack archives found among the extracted files.
    pub nested: bool,
}

impl View {
    /// The primary button: everything, or the selection, next to the archive.
    pub(super) fn extract(&self) {
        let request = self.default_request(None);
        self.extract_request(request);
    }

    /// Ctrl+E: everything next to the archive, whatever is selected.
    pub(super) fn extract_all(&self) {
        let mut request = self.default_request(None);
        request.selection = Selection::All;
        request.picked = 0;
        self.extract_request(request);
    }

    /// Runs `request`, asking for a password first when it needs one.
    pub(super) fn extract_request(&self, request: Request) {
        if self.needs_password(&request.selection) {
            self.state.borrow_mut().pending_request = Some(request);
            self.ask_password(PasswordFor::Extract, "");
        } else {
            self.run(request);
        }
    }

    /// Where Extract all goes inside `parent`: the smart folder, or (with
    /// that preference off) always a folder named after the archive.
    pub(super) fn suggest_in(&self, parent: &Path, s: &super::State) -> destination::Destination {
        if s.prefs.smart_folder {
            destination::suggest_in(parent, &self.path, &s.entries)
        } else {
            destination::fresh_folder(parent, &destination::folder_name(&self.path))
        }
    }

    /// The selection (or everything), into `parent` or next to the archive.
    fn default_request(&self, parent: Option<&Path>) -> Request {
        self.refresh_destination();
        let s = self.state.borrow();
        let dest = match parent {
            Some(p) => self.suggest_in(p, &s),
            None => s.dest.clone().expect("set with the listing"),
        };
        Request {
            selection: selection_of(&s.selection.selected),
            picked: s.selection.selected.len(),
            dest,
            options: ExtractOptions::default(),
            open_after: false,
            trash_after: false,
            nested: false,
        }
    }

    pub(super) fn extract_to_downloads(&self) {
        let request = self.default_request(Some(&crate::settings::downloads_dir()));
        self.extract_request(request);
    }

    pub(super) fn needs_password(&self, selection: &Selection) -> bool {
        let s = self.state.borrow();
        let Some(archive) = &s.archive else {
            return false;
        };
        !archive.has_password()
            && s.entries
                .iter()
                .any(|e| e.encrypted && !e.is_dir() && selection.includes(&e.path))
    }

    /// "Extract anyway" after a likely-bomb refusal.
    pub(super) fn force_extract(&self) {
        self.g(|g| g.set_show_error(false));
        let last = self.state.borrow().last_request.clone();
        if let Some(mut request) = last.filter(|_| self.state.borrow().can_force) {
            request.options.limits.max_ratio = None;
            self.run(request);
        }
    }

    pub(super) fn retry(&self) {
        self.g(|g| g.set_show_error(false));
        let last = self.state.borrow().last_request.clone();
        if let Some(request) = last {
            self.run(request);
        }
    }

    pub(super) fn retry_in_downloads(&self) {
        self.g(|g| g.set_show_error(false));
        let last = self.state.borrow().last_request.clone();
        if let Some(mut request) = last {
            let s = self.state.borrow();
            request.dest = self.suggest_in(&crate::settings::downloads_dir(), &s);
            drop(s);
            self.run(request);
        }
    }

    /// Extracts on a worker. Name clashes under "Ask" come back to the UI
    /// as a question; the worker waits for the answer.
    pub(super) fn run(&self, request: Request) {
        if self.state.borrow().archive.is_none() {
            return;
        }
        let cancel = CancelToken::new();
        let op = self.start_op(
            crate::ops::Kind::Extract,
            trf("Extracting {name}", &[("name", &self.file_name())]),
            trf(
                "→ {folder}",
                &[("folder", &model::display_folder(&request.dest.folder))],
            ),
            &cancel,
        );
        let mut s = self.state.borrow_mut();
        let Some(mut archive) = s.archive.take() else {
            return;
        };
        s.cancel = Some(cancel.clone());
        s.last_request = Some(request.clone());
        let (reply, answers) = mpsc::channel::<(Resolution, bool)>();
        s.conflict_reply = Some(reply);
        drop(s);

        self.g(|g| {
            g.set_progress(0.0);
            g.set_current_file("".into());
            g.set_busy(true);
        });
        std::thread::spawn(move || {
            let started = Instant::now();
            let mut send = progress_sender();
            let job = op.clone();
            let mut on_progress = |p: &Progress| {
                job.progress(p);
                send(p);
            };
            let mut for_all = None;
            let mut asked = 0;
            let mut resolve = |c: &Conflict| {
                if let Some(r) = for_all {
                    return r;
                }
                asked += 1;
                let (c, n) = (c.clone(), asked);
                post(move |v| v.ask_conflict(c, n));
                match answers.recv() {
                    Ok((r, all)) => {
                        if all {
                            for_all = Some(r);
                        }
                        r
                    }
                    // The window closed: leave existing files alone.
                    Err(_) => Resolution::Skip,
                }
            };
            let result = archive.extract_with(
                &request.selection,
                &request.dest.dest,
                &request.options,
                &mut on_progress,
                &mut resolve,
                &cancel,
            );
            let nested = (request.nested && result.is_ok())
                .then(|| unpack_nested(&archive, &request, &cancel));
            let elapsed = started.elapsed();
            post(move |v| v.extracted(archive, result, request, elapsed, nested, &op));
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
            s.conflict_reply = None;
            s.quit_when_idle
        };
        self.g(|g| {
            g.set_busy(false);
            g.set_opening("".into());
            g.set_conflict_show(false);
        });
        if quit {
            let _ = slint::quit_event_loop();
        }
        quit
    }

    fn ask_conflict(&self, c: Conflict, n: usize) {
        let when = |t: Option<std::time::SystemTime>| t.map(model::date_text).unwrap_or_default();
        let meta = |size: u64, t| {
            let date = when(t);
            if date.is_empty() {
                model::size_text(size)
            } else {
                trf(
                    "{size} · {date}",
                    &[("size", &model::size_text(size)), ("date", &date)],
                )
            }
        };
        let newer =
            matches!((c.incoming_modified, c.existing_modified), (Some(a), Some(b)) if a > b);
        let name = c.path.rsplit('/').next().unwrap_or(&c.path).to_owned();
        self.g(|g| {
            g.set_conflict_title(
                trf(
                    "Conflict {n} · {path}",
                    &[("n", &n.to_string()), ("path", &c.path)],
                )
                .into(),
            );
            g.set_conflict_name(name.into());
            g.set_conflict_existing(meta(c.existing_size, c.existing_modified).into());
            g.set_conflict_incoming(meta(c.incoming_size, c.incoming_modified).into());
            g.set_conflict_incoming_newer(newer);
            g.set_conflict_show(true);
        });
    }

    pub(super) fn conflict_answer(&self, answer: i32, for_all: bool) {
        self.g(|g| g.set_conflict_show(false));
        let resolution = match answer {
            1 => Resolution::KeepBoth,
            2 => Resolution::Replace,
            _ => Resolution::Skip,
        };
        if let Some(reply) = &self.state.borrow().conflict_reply {
            let _ = reply.send((resolution, for_all));
        }
    }

    fn extracted(
        &self,
        archive: Archive,
        result: crate_core::Result<ExtractReport>,
        request: Request,
        elapsed: Duration,
        nested: Option<crate_core::Result<crate_core::nested::NestedReport>>,
        op: &crate::ops::Op,
    ) {
        self.state.borrow_mut().can_force = false;
        let name = self.file_name();
        let folder = model::display_folder(&request.dest.folder);
        match &result {
            Ok(_) => self.finish_op(
                op,
                crate::ops::Outcome::Done,
                &trf(
                    "Extracted {name} → {folder}",
                    &[("name", &name), ("folder", &folder)],
                ),
                Some(&request.dest.folder),
            ),
            Err(Error::Cancelled) => self.finish_op(
                op,
                crate::ops::Outcome::Cancelled,
                &trf("Cancelled: {name}", &[("name", &name)]),
                None,
            ),
            Err(e) => self.finish_op(
                op,
                crate::ops::Outcome::Failed,
                &trf(
                    "Couldn’t extract {name}: {error}",
                    &[("name", &name), ("error", &e.to_string())],
                ),
                None,
            ),
        }
        if self.finish_work(archive) {
            return;
        }
        let dest = request.dest.clone();
        match result {
            Ok(report) => {
                self.state.borrow_mut().done_folder = Some(dest.folder.clone());
                // The next "Extract all" must not reuse this folder.
                self.refresh_destination();
                if request.trash_after {
                    if request.open_after {
                        let _ = open_with_default_app(&dest.folder);
                    }
                    match trash::delete(&self.path) {
                        // The archive is gone: nothing left to show.
                        Ok(()) => return self.close_window(),
                        Err(e) => {
                            return self.show_error(
                                &tr("Couldn’t move the archive to the Trash"),
                                &trf(
                                    "The files were extracted. {error}",
                                    &[("error", &e.to_string())],
                                ),
                            );
                        }
                    }
                }
                if request.open_after && report.skipped.is_empty() {
                    self.open_folder();
                    return;
                }
                let items = report.files + report.dirs + report.links;
                let what = if request.picked > 0 {
                    model::count(request.picked as u64, "selected item")
                } else {
                    trf(
                        "{items} · {size}",
                        &[
                            ("items", &model::count(items, "item")),
                            ("size", &model::size_text(report.bytes)),
                        ],
                    )
                };
                let mut message = match model::duration_text(elapsed) {
                    Some(time) => trf(
                        "{what} extracted in {time}",
                        &[("what", &what), ("time", &time)],
                    ),
                    None => trf("{what} extracted", &[("what", &what)]),
                };
                if let Some(Ok(n)) = &nested {
                    if !n.unpacked.is_empty() {
                        message.push_str(&trn(
                            " · {n} archive inside unpacked",
                            " · {n} archives inside unpacked",
                            n.unpacked.len() as u64,
                        ));
                    }
                    if !n.kept.is_empty() {
                        message.push_str(&trn(
                            " · {n} archive inside kept as it was",
                            " · {n} archives inside kept as they were",
                            n.kept.len() as u64,
                        ));
                    }
                }
                let skipped = report.skipped.len() as u64;
                let exists = report
                    .skipped
                    .iter()
                    .filter(|s| s.reason == crate_core::SkipReason::Exists)
                    .count() as u64;
                let warning = match (skipped, exists) {
                    (0, _) => String::new(),
                    (n, e) if n == e => trn(
                        "{n} item already there, kept as it was",
                        "{n} items already there, kept as they were",
                        n,
                    ),
                    (n, _) => trn(
                        "{n} item skipped for your safety",
                        "{n} items skipped for your safety",
                        n,
                    ),
                };
                crate::about::count_success();
                self.g(|g| {
                    g.set_progress(1.0);
                    g.set_done_message(message.into());
                    g.set_done_folder(model::display_folder(&dest.folder).into());
                    g.set_done_warning(warning.into());
                    g.set_show_done(true);
                });
            }
            Err(Error::Cancelled) => {
                self.show_toast(&tr("Extraction cancelled; nothing was written"))
            }
            Err(Error::WrongPassword) => {
                if let Some(a) = self.state.borrow_mut().archive.as_mut() {
                    a.set_password(None);
                }
                self.state.borrow_mut().pending_request = Some(request);
                self.wrong_password(PasswordFor::Extract);
            }
            Err(e) => self.extract_failed(&e, &dest),
        }
    }

    /// The error popup, in the variant the error calls for (FileIssues.dc.html).
    fn extract_failed(&self, e: &Error, dest: &Destination) {
        let target = dest.dest.clone();
        let folder_name = target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| target.display().to_string());
        let (kind, title, message, detail) = match e {
            Error::SuspiciousRatio { ratio, .. } => (
                1,
                tr("This may be a decompression bomb"),
                trf(
                    "It would expand to {ratio}× its own size. Extract anyway only if you trust where it came from.",
                    &[("ratio", &format!("{ratio:.0}"))],
                ),
                String::new(),
            ),
            Error::SizeExceeded { .. } => (
                0,
                tr("This may be a decompression bomb"),
                sentence(&e.to_string()),
                String::new(),
            ),
            Error::NotEnoughSpace { needed, available } => (
                2,
                trf("Not enough space in {folder}", &[("folder", &folder_name)]),
                trf(
                    "Needs {needed}, {free} free. Checked before starting, so nothing was written.",
                    &[
                        ("needed", &model::size_text(*needed)),
                        ("free", &model::size_text(*available)),
                    ],
                ),
                String::new(),
            ),
            Error::Io(io) if is_permission(io) => (
                3,
                trf(
                    "Crate can’t write to {folder}",
                    &[("folder", &model::display_folder(&target))],
                ),
                tr("The folder is read-only or belongs to another user."),
                io.to_string(),
            ),
            _ => (
                0,
                tr("Couldn’t extract"),
                sentence(&e.to_string()),
                String::new(),
            ),
        };
        self.state.borrow_mut().can_force = kind == 1;
        self.g(|g| {
            g.set_error_kind(kind);
            g.set_error_title(title.into());
            g.set_error_message(message.into());
            g.set_error_detail(detail.into());
            g.set_error_can_force(kind == 1);
            g.set_show_error(true);
        });
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
            Err(e) => return self.show_error(&tr("Couldn’t open"), &e.to_string()),
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
                    if open_extracted(&target).is_err() {
                        self.show_toast(&tr("No app to open this file"));
                    } else {
                        self.watch(path, target);
                    }
                }
                None => {
                    let why = report
                        .skipped
                        .first()
                        .map_or(tr("it was skipped"), |s| s.reason.to_string());
                    self.show_error(
                        &tr("Couldn’t open"),
                        &trf("Skipped for your safety: {reason}", &[("reason", &why)]),
                    );
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
            Err(e) => self.show_error(&tr("Couldn’t open"), &e.to_string()),
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
            self.show_toast(&tr("Couldn’t open the file manager"));
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
            self.show_toast(&trf(
                "Opened {name}/ in the file manager",
                &[("name", &name)],
            ));
        } else {
            self.show_toast(&tr("Couldn’t open the file manager"));
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
                Some(Err(e)) => return self.show_error(&tr("Couldn’t test"), &e.to_string()),
                None => return,
            }
        };
        let cancel = CancelToken::new();
        self.state.borrow_mut().testing = Some(cancel.clone());
        let op = self.start_op(
            crate::ops::Kind::Test,
            trf("Testing {name}", &[("name", &self.file_name())]),
            String::new(),
            &cancel,
        );
        self.g(|g| {
            g.set_test_state(1);
            g.set_test_status(tr("Testing…").into());
        });
        std::thread::spawn(move || {
            let mut archive = copy;
            let started = Instant::now();
            let mut last: Option<Instant> = None;
            let result = archive.test(
                &mut |p| {
                    op.progress(p);
                    if last.is_some_and(|t| t.elapsed() < Duration::from_millis(100)) {
                        return;
                    }
                    last = Some(Instant::now());
                    let pct = (p.bytes_done * 100).checked_div(p.bytes_total).unwrap_or(0);
                    post(move |v| {
                        let text = trf("Testing… {pct}%", &[("pct", &pct.to_string())]);
                        v.g(|g| g.set_test_status(text.into()))
                    });
                },
                &cancel,
            );
            let took = started.elapsed().as_secs_f64();
            post(move |v| v.tested(result, took, &op));
        });
    }

    fn tested(
        &self,
        result: crate_core::Result<crate_core::TestReport>,
        took: f64,
        op: &crate::ops::Op,
    ) {
        self.state.borrow_mut().testing = None;
        let name = self.file_name();
        let (outcome, line) = match &result {
            Ok(r) if r.is_ok() => (
                crate::ops::Outcome::Done,
                trf("Tested {name}: OK", &[("name", &name)]),
            ),
            Ok(r) => (
                crate::ops::Outcome::Failed,
                trf(
                    &trn(
                        "Tested {name}: {n} file damaged",
                        "Tested {name}: {n} files damaged",
                        r.failed.len() as u64,
                    ),
                    &[("name", &name)],
                ),
            ),
            Err(Error::Cancelled) => (
                crate::ops::Outcome::Cancelled,
                trf("Cancelled: {name}", &[("name", &name)]),
            ),
            Err(e) => (
                crate::ops::Outcome::Failed,
                trf(
                    "Couldn’t test {name}: {error}",
                    &[("name", &name), ("error", &e.to_string())],
                ),
            ),
        };
        self.finish_op(op, outcome, &line, None);
        if result.is_ok() {
            self.tested_at(took);
        }
        match result {
            Ok(report) if report.is_ok() => {
                let note = match report.skipped.len() {
                    0 => String::new(),
                    n => trn(
                        " · {n} file not tested (encrypted)",
                        " · {n} files not tested (encrypted)",
                        n as u64,
                    ),
                };
                crate::about::count_success();
                self.g(|g| {
                    g.set_test_state(2);
                    g.set_test_status(trf("Archive OK{note}", &[("note", &note)]).into());
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
                    g.set_test_status(trn("{n} file damaged", "{n} files damaged", n).into());
                });
                self.show_error(
                    &tr("Archive is damaged"),
                    &trf(
                        &trn(
                            "{n} of {total} failed the check:\n{list}",
                            "{n} of {total} failed the check:\n{list}",
                            n,
                        ),
                        &[
                            ("total", &model::thousands(report.files + n)),
                            ("list", &list.join("\n")),
                        ],
                    ),
                );
            }
            Err(Error::Cancelled) => self.g(|g| g.set_test_state(0)),
            Err(e) => {
                self.g(|g| {
                    g.set_test_state(3);
                    g.set_test_status(tr("Damaged").into());
                });
                self.show_error(&tr("Archive is damaged"), &e.to_string());
            }
        }
    }
}

/// "Also unpack archives found inside": the archives among the entries just
/// extracted, where they landed.
fn unpack_nested(
    archive: &Archive,
    request: &Request,
    cancel: &CancelToken,
) -> crate_core::Result<crate_core::nested::NestedReport> {
    let files: Vec<PathBuf> = archive
        .entries()
        .iter()
        .filter(|e| e.kind == crate_core::EntryKind::File)
        .filter(|e| request.selection.includes(&e.path))
        .filter(|e| crate_core::nested::is_archive_name(&e.path))
        .filter_map(|e| {
            let rel = crate_core::safety::safe_relative_path(&e.path).ok()?;
            Some(if request.options.keep_structure {
                request.dest.dest.join(rel)
            } else {
                request.dest.dest.join(rel.file_name()?)
            })
        })
        .collect();
    crate_core::nested::unpack(&files, cancel)
}
