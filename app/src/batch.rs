//! Batch extract (`design/canvas/Batch.dc.html`): several archives, one after
//! another, each into its own folder (the smart-folder rule from
//! Preferences) or all into one new folder. An archive that needs a password
//! waits for it without holding up the others. Every write goes through
//! `crate_core::Archive::extract`, so the safety checks and limits apply.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate_core::destination::{self, Destination};
use crate_core::{Archive, CancelToken, Error, ExtractOptions, IfExists, OpenOptions, Selection};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::i18n::{tr, trf, trn};
use crate::ops::{Kind, Op, Outcome as OpOutcome};
use crate::prefs::Prefs;
use crate::{BatchItem, BatchState, BatchUi, MainWindow, model};

thread_local! {
    static BATCH: RefCell<Option<Rc<Batch>>> = const { RefCell::new(None) };
}

/// Runs `f` with the batch on the UI thread. Callable from any thread.
fn post(f: impl FnOnce(&Batch) + Send + 'static) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(b) = BATCH.with(|b| b.borrow().clone()) {
            f(&b);
        }
    });
}

struct Item {
    path: PathBuf,
    size: u64,
    state: BatchState,
    password: Option<String>,
    /// Where it went (or goes): shown as "name/".
    folder: String,
    /// The folder to open afterwards.
    opened: Option<PathBuf>,
    status: String,
    progress: f32,
    /// Volumes the user located elsewhere.
    volumes: Vec<PathBuf>,
    /// Failed because this volume is missing.
    missing: Option<String>,
    cancel: CancelToken,
    /// The job in the Operations panel: queued, then running.
    op: Option<Arc<Op>>,
}

struct State {
    items: Vec<Item>,
    mode: i32,
    started: Option<Instant>,
    running: Option<usize>,
    /// "All into one folder": the folder, made when the batch starts.
    one_folder: Option<Destination>,
    cancelled_all: bool,
}

pub struct Batch {
    ui: slint::Weak<MainWindow>,
    state: RefCell<State>,
    prefs: Prefs,
    /// Redraws while running (a Pause from the Operations panel shows).
    timer: slint::Timer,
}

/// True for the files Crate can open, judging by the name.
pub fn is_archive(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|n| crate_core::nested::is_archive_name(&n.to_string_lossy()))
}

/// Shows the batch sheet for `archives` on Home. `start_now`: skip the ready
/// step (`--extract-each`).
pub fn start(ui: &MainWindow, archives: Vec<PathBuf>, start_now: bool) {
    let prefs = Prefs::load();
    let items = archives
        .into_iter()
        .map(|path| {
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            Item {
                folder: format!("{}/", destination::folder_name(&path)),
                path,
                size,
                state: BatchState::Waiting,
                password: None,
                opened: None,
                status: tr("Waiting"),
                progress: 0.0,
                volumes: Vec::new(),
                missing: None,
                cancel: CancelToken::new(),
                op: None,
            }
        })
        .collect();
    let batch = Rc::new(Batch {
        ui: ui.as_weak(),
        state: RefCell::new(State {
            items,
            mode: 0,
            started: None,
            running: None,
            one_folder: None,
            cancelled_all: false,
        }),
        prefs,
        timer: slint::Timer::default(),
    });
    BATCH.with(|b| *b.borrow_mut() = Some(batch.clone()));
    connect(ui);
    let g = ui.global::<BatchUi>();
    g.set_trash(false);
    g.set_pw_row(-1);
    batch.refresh();
    g.set_show(true);
    if start_now {
        batch.begin();
    }
}

fn connect(ui: &MainWindow) {
    let g = ui.global::<BatchUi>();
    g.on_start(|| with(|b| b.begin()));
    g.on_set_mode(|m| {
        with(|b| {
            b.state.borrow_mut().mode = m;
            b.refresh();
        })
    });
    g.on_row_action(|i| with(|b| b.row_action(i as usize)));
    g.on_submit_password(|| with(|b| b.submit_password()));
    g.on_cancel_password(|| {
        with(|b| {
            b.g(|g| g.set_pw_row(-1));
        })
    });
    g.on_skip_problems(|| with(|b| b.skip_problems()));
    g.on_cancel_all(|| with(|b| b.cancel_all()));
    let weak = ui.as_weak();
    g.on_background(move || {
        if let Some(ui) = weak.upgrade() {
            use slint::winit_030::WinitWindowAccessor;
            ui.window().with_winit_window(|w| w.set_minimized(true));
        }
    });
    g.on_close(|| with(|b| b.close()));
    g.on_open_folder(|| {
        with(|b| {
            let folder = b.state.borrow().one_folder.as_ref().map(|d| d.folder.clone());
            if let Some(f) = folder {
                let _ = crate::home::open_path(&f);
            }
        })
    });
}

/// A missing volume of row `index` found at `part` (what "Locate…" does
/// after its file picker).
pub fn located(index: usize, part: PathBuf) {
    with(|b| b.located(index, part));
}

fn with(f: impl FnOnce(&Batch)) {
    if let Some(b) = BATCH.with(|b| b.borrow().clone()) {
        f(&b);
    }
}

impl Batch {
    fn g(&self, f: impl FnOnce(BatchUi<'_>)) {
        if let Some(ui) = self.ui.upgrade() {
            f(ui.global::<BatchUi>());
        }
    }

    /// Starts the queue: makes the one folder if needed, then the first item.
    fn begin(&self) {
        {
            let mut s = self.state.borrow_mut();
            if s.started.is_some() {
                return;
            }
            s.started = Some(Instant::now());
            if s.mode == 1 {
                let parent = s
                    .items
                    .first()
                    .and_then(|i| i.path.parent())
                    .filter(|p| !p.as_os_str().is_empty())
                    .map_or_else(|| PathBuf::from("."), Path::to_owned);
                let dest = destination::fresh_folder(&parent, &tr("Extracted"));
                let name = format!("{}/", dest.name);
                for item in &mut s.items {
                    item.folder = name.clone();
                }
                s.one_folder = Some(dest);
            }
            // Everything waits in the Operations panel's queue.
            for item in &mut s.items {
                item.op = Some(Op::queued(
                    Kind::Extract,
                    trf("Extract {name}", &[("name", &item.name())]),
                    trf("→ {folder}", &[("folder", &item.folder)]),
                    item.cancel.clone(),
                ));
            }
        }
        self.timer.start(slint::TimerMode::Repeated, Duration::from_millis(500), || {
            with(|b| b.refresh());
        });
        self.pump();
    }

    /// Starts the next waiting archive if nothing is running.
    fn pump(&self) {
        // Removed from the queue in the Operations panel.
        for item in &mut self.state.borrow_mut().items {
            if item.state == BatchState::Waiting && item.op.as_ref().is_some_and(|o| o.removed()) {
                item.op = None;
                item.state = BatchState::Cancelled;
                item.status = tr("Removed from the queue");
            }
        }
        let next = {
            let s = self.state.borrow();
            if s.running.is_some() || s.cancelled_all {
                None
            } else {
                s.items
                    .iter()
                    .position(|i| i.state == BatchState::Waiting)
            }
        };
        match next {
            Some(i) => self.run(i),
            None => self.refresh(),
        }
    }

    fn run(&self, index: usize) {
        let (path, password, volumes, mode, one, cancel, op) = {
            let mut s = self.state.borrow_mut();
            s.running = Some(index);
            let (mode, one) = (s.mode, s.one_folder.clone());
            let item = &mut s.items[index];
            item.state = BatchState::Running;
            item.progress = 0.0;
            item.missing = None;
            item.status = trf("Extracting {pct}%", &[("pct", "0")]);
            let title = trf("Extracting {name}", &[("name", &item.name())]);
            let op = match &item.op {
                Some(op) => {
                    op.begin(title);
                    op.clone()
                }
                None => Op::start(
                    Kind::Extract,
                    title,
                    trf("→ {folder}", &[("folder", &item.folder)]),
                    item.cancel.clone(),
                ),
            };
            item.op = Some(op.clone());
            (
                item.path.clone(),
                item.password.clone(),
                item.volumes.clone(),
                mode,
                one,
                item.cancel.clone(),
                op,
            )
        };
        self.refresh();
        let smart = self.prefs.smart_folder;
        let encoding = self.prefs.encoding;
        std::thread::spawn(move || {
            let outcome = extract_one(
                &path, password, volumes, mode, one, smart, encoding, &op, &cancel,
            );
            post(move |b| b.finished(index, outcome));
        });
    }

    fn progress(&self, index: usize, fraction: f32) {
        {
            let mut s = self.state.borrow_mut();
            let Some(item) = s.items.get_mut(index) else { return };
            if item.state != BatchState::Running {
                return;
            }
            item.progress = fraction;
            item.status = trf(
                "Extracting {pct}%",
                &[("pct", &((fraction * 100.0) as u32).to_string())],
            );
        }
        self.refresh();
    }

    fn finished(&self, index: usize, outcome: Outcome) {
        let trash = self
            .ui
            .upgrade()
            .is_some_and(|ui| ui.global::<BatchUi>().get_trash());
        let background = self
            .ui
            .upgrade()
            .is_some_and(|ui| crate::ops::in_background(ui.window()));
        {
            let mut s = self.state.borrow_mut();
            s.running = None;
            let cancelled_all = s.cancelled_all;
            let item = &mut s.items[index];
            item.progress = 0.0;
            // The Operations panel's history (a password still to come
            // isn't the end: it registers again then).
            if let Some(op) = item.op.take() {
                let name = item.name();
                op.notify.store(background, std::sync::atomic::Ordering::Relaxed);
                match &outcome {
                    Outcome::Done { dest, .. } => op.finish(
                        OpOutcome::Done,
                        &trf(
                            "Extracted {name} → {folder}",
                            &[("name", &name), ("folder", &model::display_folder(&dest.folder))],
                        ),
                        Some(&dest.folder),
                    ),
                    Outcome::Cancelled => op.finish(
                        OpOutcome::Cancelled,
                        &trf("Cancelled: {name}", &[("name", &name)]),
                        None,
                    ),
                    Outcome::Failed(e) => op.finish(
                        OpOutcome::Failed,
                        &trf("Couldn’t extract {name}: {error}", &[("name", &name), ("error", e)]),
                        None,
                    ),
                    Outcome::MissingVolume(part) => op.finish(
                        OpOutcome::Failed,
                        &trf("Couldn’t extract {name}: {error}", &[("name", &name), ("error", &trf("Missing {name}", &[("name", part)]))]),
                        None,
                    ),
                    Outcome::NeedsPassword { .. } => {}
                }
            }
            match outcome {
                Outcome::Done {
                    dest,
                    no_wrapper,
                    skipped,
                } => {
                    crate::about::count_success();
                    item.state = BatchState::Done;
                    item.progress = 1.0;
                    item.folder = format!("{}/", dest.name);
                    item.opened = Some(dest.folder);
                    let mut status = if no_wrapper {
                        tr("Done · single folder, no wrapper added")
                    } else {
                        tr("Done")
                    };
                    if skipped > 0 {
                        status.push_str(&trn(
                            " · {n} item skipped for your safety",
                            " · {n} items skipped for your safety",
                            skipped,
                        ));
                    }
                    if trash && let Err(e) = trash::delete(&item.path) {
                        status.push_str(&trf(
                            " · couldn’t move it to the Trash: {error}",
                            &[("error", &e.to_string())],
                        ));
                    }
                    item.status = status;
                }
                Outcome::NeedsPassword { wrong } => {
                    item.state = BatchState::NeedsPassword;
                    item.status = if wrong {
                        tr("Wrong password")
                    } else {
                        tr("Needs password")
                    };
                }
                Outcome::Cancelled => {
                    item.cancel = CancelToken::new();
                    item.state = if cancelled_all {
                        BatchState::Cancelled
                    } else {
                        BatchState::Waiting
                    };
                    item.status = if cancelled_all {
                        tr("Cancelled")
                    } else {
                        tr("Waiting")
                    };
                }
                Outcome::Failed(error) => {
                    item.state = BatchState::Failed;
                    item.status = error;
                }
                Outcome::MissingVolume(name) => {
                    item.state = BatchState::Failed;
                    item.status = trf("Missing {name}", &[("name", &name)]);
                    item.missing = Some(name);
                }
            }
        }
        self.pump();
    }

    fn row_action(&self, index: usize) {
        let state = self.state.borrow().items.get(index).map(|i| i.state);
        match state {
            Some(BatchState::Done) => {
                let folder = self.state.borrow().items[index].opened.clone();
                if let Some(f) = folder {
                    let _ = crate::home::open_path(&f);
                }
            }
            Some(BatchState::Running) => self.set_paused(index, true),
            Some(BatchState::Paused) => self.set_paused(index, false),
            Some(BatchState::NeedsPassword) => {
                let name = self.state.borrow().items[index].name();
                self.g(|g| {
                    g.set_pw_text("".into());
                    g.set_pw_error("".into());
                    g.set_pw_name(name.into());
                    g.set_pw_row(index as i32);
                });
            }
            Some(BatchState::Failed) if self.state.borrow().items[index].missing.is_some() => {
                let dir = self.state.borrow().items[index].path.parent().map(Path::to_owned);
                std::thread::spawn(move || {
                    let mut dialog = rfd::FileDialog::new();
                    if let Some(dir) = dir {
                        dialog = dialog.set_directory(dir);
                    }
                    if let Some(picked) = dialog.pick_file() {
                        post(move |b| b.located(index, picked));
                    }
                });
            }
            Some(BatchState::Failed) => {
                {
                    let mut s = self.state.borrow_mut();
                    let item = &mut s.items[index];
                    item.state = BatchState::Waiting;
                    item.status = tr("Waiting");
                }
                self.pump();
            }
            _ => {}
        }
    }

    /// A missing volume found elsewhere: try that archive again.
    fn located(&self, index: usize, part: PathBuf) {
        {
            let mut s = self.state.borrow_mut();
            let Some(item) = s.items.get_mut(index) else { return };
            item.volumes.push(part);
            item.state = BatchState::Waiting;
            item.status = tr("Waiting");
        }
        self.pump();
    }

    fn set_paused(&self, index: usize, paused: bool) {
        if let Some(op) = self.state.borrow().items.get(index).and_then(|i| i.op.clone()) {
            op.set_paused(paused);
        }
        self.refresh();
    }

    fn submit_password(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let g = ui.global::<BatchUi>();
        let index = g.get_pw_row();
        let password = g.get_pw_text().to_string();
        if index < 0 || password.is_empty() {
            return;
        }
        g.set_pw_row(-1);
        g.set_pw_text("".into());
        {
            let mut s = self.state.borrow_mut();
            let item = &mut s.items[index as usize];
            item.password = Some(password);
            item.state = BatchState::Waiting;
            item.status = tr("Waiting");
        }
        self.pump();
    }

    fn skip_problems(&self) {
        {
            let mut s = self.state.borrow_mut();
            for item in &mut s.items {
                if matches!(item.state, BatchState::NeedsPassword | BatchState::Failed) {
                    item.state = BatchState::Skipped;
                    item.status = tr("Skipped");
                }
            }
        }
        self.refresh();
    }

    fn cancel_all(&self) {
        {
            let mut s = self.state.borrow_mut();
            s.cancelled_all = true;
            for item in &mut s.items {
                item.cancel.cancel();
                if matches!(item.state, BatchState::Waiting | BatchState::NeedsPassword) {
                    item.state = BatchState::Cancelled;
                    item.status = tr("Cancelled");
                    item.op = None;
                }
            }
        }
        self.refresh();
    }

    fn close(&self) {
        for item in &mut self.state.borrow_mut().items {
            item.cancel.cancel();
            if item.state != BatchState::Running {
                item.op = None;
            }
        }
        self.timer.stop();
        self.g(|g| g.set_show(false));
        BATCH.with(|b| b.borrow_mut().take());
    }

    /// Pushes the whole state to the sheet.
    fn refresh(&self) {
        let s = self.state.borrow();
        let n = s.items.len() as u64;
        let count = |st: BatchState| s.items.iter().filter(|i| i.state == st).count() as u64;
        let (done, running, needs, failed, waiting) = (
            count(BatchState::Done),
            count(BatchState::Running) + count(BatchState::Paused),
            count(BatchState::NeedsPassword),
            count(BatchState::Failed),
            count(BatchState::Waiting),
        );
        let started = s.started.is_some();
        let finished = started && running == 0 && waiting == 0 && needs == 0 && failed == 0
            || started && s.cancelled_all && running == 0;
        if finished {
            self.timer.stop();
        }
        let title = if !started {
            trn("Extract {n} archive", "Extract {n} archives", n)
        } else if finished {
            if done == n {
                trn("Extracted {n} archive", "Extracted {n} archives", n)
            } else {
                trf(
                    &trn(
                        "Extracted {done} of {n} archive",
                        "Extracted {done} of {n} archives",
                        n,
                    ),
                    &[("done", &model::thousands(done))],
                )
            }
        } else {
            trn("Extracting {n} archive", "Extracting {n} archives", n)
        };
        let mut parts = Vec::new();
        if started {
            // Literal calls, so scripts/update-translations.sh finds them.
            let counts = [
                (done, trn("{n} done", "{n} done", done)),
                (running, trn("{n} running", "{n} running", running)),
                (needs, trn("{n} needs you", "{n} need you", needs)),
                (failed, trn("{n} failed", "{n} failed", failed)),
                (waiting, trn("{n} waiting", "{n} waiting", waiting)),
            ];
            parts.extend(counts.into_iter().filter(|(k, _)| *k > 0).map(|(_, t)| t));
        }
        // Overall: archive sizes as weights.
        let total: u64 = s.items.iter().map(|i| i.size.max(1)).sum();
        let done_weight: f64 = s
            .items
            .iter()
            .map(|i| match i.state {
                BatchState::Done | BatchState::Skipped | BatchState::Failed | BatchState::Cancelled => {
                    i.size.max(1) as f64
                }
                BatchState::Running | BatchState::Paused => i.size.max(1) as f64 * f64::from(i.progress),
                _ => 0.0,
            })
            .sum();
        let fraction = (done_weight / total as f64).clamp(0.0, 1.0);
        let overall_text = trf(
            "Overall · {done} of {total}",
            &[
                ("done", &model::size_text(done_weight as u64)),
                ("total", &model::size_text(total)),
            ],
        );
        let eta = match s.started {
            Some(t) if !finished && fraction > 0.02 => {
                let left = t.elapsed().mul_f64((1.0 - fraction) / fraction);
                model::duration_text(left)
                    .map(|d| trf("About {time} left", &[("time", &d)]))
                    .unwrap_or_default()
            }
            _ => String::new(),
        };
        let paused = |i: &Item| i.state == BatchState::Running && i.op.as_ref().is_some_and(|o| o.is_paused());
        let rows: Vec<BatchItem> = s
            .items
            .iter()
            .map(|i| BatchItem {
                name: i.name().into(),
                size: model::size_text(i.size).into(),
                folder: i.folder.clone().into(),
                state: if paused(i) { BatchState::Paused } else { i.state },
                status: if paused(i) { tr("Paused").into() } else { i.status.clone().into() },
                progress: i.progress,
                locate: i.missing.is_some(),
            })
            .collect();
        let mode = s.mode;
        drop(s);
        self.g(|g| {
            g.set_items(ModelRc::new(VecModel::from(rows)));
            g.set_started(started);
            g.set_finished(finished);
            g.set_title(title.into());
            g.set_counts(SharedString::from(parts.join(" · ")));
            g.set_mode(mode);
            g.set_overall(fraction as f32);
            g.set_overall_text(overall_text.into());
            g.set_eta(eta.into());
            g.set_any_needs(needs + failed > 0);
        });
    }
}

impl Item {
    fn name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

enum Outcome {
    Done {
        dest: Destination,
        /// The archive held one folder, extracted as it is.
        no_wrapper: bool,
        skipped: u64,
    },
    NeedsPassword {
        wrong: bool,
    },
    Cancelled,
    Failed(String),
    /// This volume of the set isn't there.
    MissingVolume(String),
}

#[allow(clippy::too_many_arguments)]
fn extract_one(
    path: &Path,
    password: Option<String>,
    volumes: Vec<PathBuf>,
    mode: i32,
    one: Option<Destination>,
    smart: bool,
    encoding: Option<crate_core::NameCodec>,
    op: &Op,
    cancel: &CancelToken,
) -> Outcome {
    let had_password = password.is_some();
    let options = OpenOptions {
        password,
        encoding,
        volumes,
    };
    let mut archive = match Archive::open_with(path, &options) {
        Ok(a) => a,
        Err(Error::MissingVolume { name, .. }) => return Outcome::MissingVolume(name),
        Err(Error::PasswordRequired) => return Outcome::NeedsPassword { wrong: false },
        Err(Error::WrongPassword) => return Outcome::NeedsPassword { wrong: true },
        Err(e) => return Outcome::Failed(e.to_string()),
    };
    if !had_password && archive.entries().iter().any(|e| e.encrypted) {
        return Outcome::NeedsPassword { wrong: false };
    }
    let (dest, options) = match (mode, one) {
        (1, Some(one)) => (
            one,
            ExtractOptions {
                if_exists: IfExists::KeepBoth,
                ..ExtractOptions::default()
            },
        ),
        _ => {
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map_or_else(|| PathBuf::from("."), Path::to_owned);
            let dest = if smart {
                destination::suggest_in(&parent, path, archive.entries())
            } else {
                destination::fresh_folder(&parent, &destination::folder_name(path))
            };
            (dest, ExtractOptions::default())
        }
    };
    let no_wrapper = mode != 1 && dest.dest != dest.folder;
    // Progress in whole percents (fewer UI updates); the job blocks in
    // `op.progress` while paused.
    let mut last = 0u32;
    let mut on_progress = |p: &crate_core::Progress| {
        op.progress(p);
        if let Some(pct) = (p.bytes_done * 100).checked_div(p.bytes_total) {
            let pct = pct as u32;
            if pct != last {
                last = pct;
                post(move |b| {
                    let running = b.state.borrow().running;
                    if let Some(i) = running {
                        b.progress(i, pct as f32 / 100.0);
                    }
                });
            }
        }
    };
    match archive.extract(&Selection::All, &dest.dest, &options, &mut on_progress, cancel) {
        Ok(report) => Outcome::Done {
            dest,
            no_wrapper,
            skipped: report.skipped.len() as u64,
        },
        Err(Error::Cancelled) => Outcome::Cancelled,
        Err(Error::MissingVolume { name, .. }) => Outcome::MissingVolume(name),
        Err(Error::PasswordRequired) => Outcome::NeedsPassword { wrong: false },
        Err(Error::WrongPassword) => Outcome::NeedsPassword { wrong: true },
        Err(e) => Outcome::Failed(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_names() {
        assert!(is_archive(Path::new("/a/B.ZIP")));
        assert!(is_archive(Path::new("photos.tar.zst")));
        assert!(!is_archive(Path::new("notes.txt")));
    }
}
