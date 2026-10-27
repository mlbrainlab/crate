//! Operations (`design/canvas/Progress.dc.html`): every long job — extract,
//! test, create, convert, update, each archive of a batch — registers here,
//! so Home's Operations panel can show them all, although every archive
//! window is its own process.
//!
//! - A running job writes its state to `<runtime dir>/crate-ops/<id>.op`
//!   (key=value lines, at most every 250 ms) and removes it when it ends.
//! - The panel asks for Pause, Resume or Cancel by writing `<id>.ctl`; the
//!   job reads it from its progress callback (and blocks there while paused).
//! - Finished jobs go to `~/.local/state/crate/history` (the last 200).
//! - When a job ends while its window isn't in front, a desktop notification
//!   says so.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate_core::{CancelToken, Progress};

const WRITE_INTERVAL: Duration = Duration::from_millis(250);
const HISTORY_KEEP: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Extract,
    Test,
    Create,
    Convert,
    Update,
}

impl Kind {
    fn code(self) -> &'static str {
        match self {
            Kind::Extract => "extract",
            Kind::Test => "test",
            Kind::Create => "create",
            Kind::Convert => "convert",
            Kind::Update => "update",
        }
    }

    fn from_code(code: &str) -> Self {
        match code {
            "test" => Kind::Test,
            "create" => Kind::Create,
            "convert" => Kind::Convert,
            "update" => Kind::Update,
            _ => Kind::Extract,
        }
    }
}

/// What the panel shows for one job, as read from its state file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub id: String,
    pub kind: Option<Kind>,
    /// "Extracting invoices-q3.zip"
    pub title: String,
    /// "→ ~/Documents/Finance"
    pub detail: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub files_done: u64,
    pub files_total: u64,
    pub current: String,
    pub paused: bool,
    /// Waiting for its turn (a batch's later archives).
    pub queued: bool,
    /// Seconds since 1970 when it started.
    pub started: u64,
    /// Bytes per second, over the run so far.
    pub speed: u64,
}

impl Snapshot {
    pub fn fraction(&self) -> f32 {
        if self.bytes_total > 0 {
            (self.bytes_done as f64 / self.bytes_total as f64) as f32
        } else if self.files_total > 0 {
            self.files_done as f32 / self.files_total as f32
        } else {
            0.0
        }
    }

    fn write(&self) -> String {
        let mut out = String::new();
        let mut put = |k: &str, v: &dyn std::fmt::Display| out.push_str(&format!("{k}={v}\n"));
        put("kind", &self.kind.map_or("extract", Kind::code));
        put("title", &one_line(&self.title));
        put("detail", &one_line(&self.detail));
        put("bytes_done", &self.bytes_done);
        put("bytes_total", &self.bytes_total);
        put("files_done", &self.files_done);
        put("files_total", &self.files_total);
        put("current", &one_line(&self.current));
        put("paused", &u8::from(self.paused));
        put("queued", &u8::from(self.queued));
        put("started", &self.started);
        put("speed", &self.speed);
        out
    }

    fn read(id: &str, text: &str) -> Self {
        let mut s = Snapshot {
            id: id.to_owned(),
            ..Snapshot::default()
        };
        for (k, v) in text.lines().filter_map(|l| l.split_once('=')) {
            let n = || v.parse().unwrap_or(0);
            match k {
                "kind" => s.kind = Some(Kind::from_code(v)),
                "title" => s.title = v.to_owned(),
                "detail" => s.detail = v.to_owned(),
                "bytes_done" => s.bytes_done = n(),
                "bytes_total" => s.bytes_total = n(),
                "files_done" => s.files_done = n(),
                "files_total" => s.files_total = n(),
                "current" => s.current = v.to_owned(),
                "paused" => s.paused = v == "1",
                "queued" => s.queued = v == "1",
                "started" => s.started = n(),
                "speed" => s.speed = n(),
                _ => {}
            }
        }
        s
    }
}

fn one_line(s: &str) -> String {
    s.replace(['\n', '\r', '\t'], " ")
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `<runtime dir>/crate-ops`, made if needed.
pub fn ops_dir() -> PathBuf {
    let base = std::env::var_os("CRATE_OPS_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_RUNTIME_DIR").map(|d| PathBuf::from(d).join("crate-ops")))
        .unwrap_or_else(|| std::env::temp_dir().join(format!("crate-ops-{}", user_id())));
    let _ = std::fs::create_dir_all(&base);
    base
}

fn user_id() -> String {
    std::env::var("USER").unwrap_or_else(|_| "user".into())
}

fn history_file() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
    Some(state.join("crate").join("history"))
}

/// A running (or queued) job of this process.
pub struct Op {
    id: String,
    state: Mutex<Snapshot>,
    last_write: Mutex<Option<Instant>>,
    started: Instant,
    paused: AtomicBool,
    cancel: CancelToken,
    /// Set by the owner when its window isn't in front: notify at the end.
    pub notify: AtomicBool,
    done: AtomicBool,
}

static NEXT: AtomicU64 = AtomicU64::new(1);

impl Op {
    /// Registers a job; `cancel` is the token the job checks, so Cancel in
    /// the panel stops it.
    pub fn start(
        kind: Kind,
        title: String,
        detail: String,
        cancel: CancelToken,
    ) -> std::sync::Arc<Op> {
        Self::new(kind, title, detail, cancel, false)
    }

    /// Registers a job that waits for its turn ([`Op::begin`] starts it).
    pub fn queued(
        kind: Kind,
        title: String,
        detail: String,
        cancel: CancelToken,
    ) -> std::sync::Arc<Op> {
        Self::new(kind, title, detail, cancel, true)
    }

    fn new(
        kind: Kind,
        title: String,
        detail: String,
        cancel: CancelToken,
        queued: bool,
    ) -> std::sync::Arc<Op> {
        let id = format!(
            "{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let op = std::sync::Arc::new(Op {
            state: Mutex::new(Snapshot {
                id: id.clone(),
                kind: Some(kind),
                title,
                detail,
                queued,
                started: now_secs(),
                ..Snapshot::default()
            }),
            id,
            last_write: Mutex::new(None),
            started: Instant::now(),
            paused: AtomicBool::new(false),
            cancel,
            notify: AtomicBool::new(false),
            done: AtomicBool::new(false),
        });
        op.write();
        op
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    fn state_file(&self) -> PathBuf {
        ops_dir().join(format!("{}.op", self.id))
    }

    fn control_file(&self) -> PathBuf {
        ops_dir().join(format!("{}.ctl", self.id))
    }

    /// A queued job starts now, with a new title ("Extracting …").
    pub fn begin(&self, title: String) {
        {
            let mut s = self.state.lock().unwrap();
            s.queued = false;
            s.title = title;
            s.started = now_secs();
        }
        self.write();
    }

    /// Removed from the queue in the panel?
    pub fn removed(&self) -> bool {
        matches!(self.read_control().as_deref(), Some("cancel"))
    }

    fn write(&self) {
        if self.done.load(Ordering::Relaxed) {
            return;
        }
        let text = self.state.lock().unwrap().write();
        // Next to it, then renamed: the panel never reads half a file.
        let file = self.state_file();
        let tmp = file.with_extension("tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(tmp, file);
        }
        *self.last_write.lock().unwrap() = Some(Instant::now());
    }

    fn read_control(&self) -> Option<String> {
        let file = self.control_file();
        let text = std::fs::read_to_string(&file).ok()?;
        let _ = std::fs::remove_file(&file);
        Some(text.trim().to_owned())
    }

    /// Applies a Pause / Resume / Cancel the panel asked for.
    fn apply_control(&self) {
        match self.read_control().as_deref() {
            Some("pause") => self.set_paused(true),
            Some("resume") => self.set_paused(false),
            Some("cancel") => {
                self.cancel.cancel();
                self.set_paused(false);
            }
            _ => {}
        }
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
        self.state.lock().unwrap().paused = paused;
        self.write();
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    /// From the job's progress callback (any thread): records progress,
    /// takes requests from the panel, and blocks while paused.
    pub fn progress(&self, p: &Progress) {
        {
            let mut s = self.state.lock().unwrap();
            s.bytes_done = p.bytes_done;
            s.bytes_total = p.bytes_total;
            s.files_done = p.files_done;
            s.files_total = p.files_total;
            s.current = p.current.clone();
            let secs = self.started.elapsed().as_secs_f64();
            if secs > 0.5 {
                s.speed = (p.bytes_done as f64 / secs) as u64;
            }
        }
        let due = self
            .last_write
            .lock()
            .unwrap()
            .is_none_or(|t| t.elapsed() >= WRITE_INTERVAL);
        if due {
            self.apply_control();
            self.write();
        }
        while self.is_paused() && !self.cancel.is_cancelled() {
            std::thread::sleep(Duration::from_millis(150));
            self.apply_control();
        }
    }

    /// The job ended: unregisters it, writes the history line and, if
    /// [`Op::notify`] is set, sends a desktop notification. `open` is what
    /// "Open" / "Show in folder" opens afterwards.
    pub fn finish(&self, outcome: Outcome, line: &str, open: Option<&Path>) {
        if self.done.swap(true, Ordering::Relaxed) {
            return;
        }
        let _ = std::fs::remove_file(self.state_file());
        let _ = std::fs::remove_file(self.control_file());
        let kind = self.state.lock().unwrap().kind.unwrap_or(Kind::Extract);
        append_history(&History {
            time: now_secs(),
            kind,
            outcome,
            text: line.to_owned(),
            open: open.map(Path::to_owned),
        });
        if self.notify.load(Ordering::Relaxed) && outcome != Outcome::Cancelled {
            let title = self.state.lock().unwrap().title.clone();
            notify(&title, line);
        }
    }
}

impl Drop for Op {
    fn drop(&mut self) {
        // Never leave a stale entry behind.
        if !self.done.load(Ordering::Relaxed) {
            let _ = std::fs::remove_file(self.state_file());
            let _ = std::fs::remove_file(self.control_file());
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Done,
    Failed,
    Cancelled,
}

/// One finished job.
#[derive(Clone, Debug, PartialEq)]
pub struct History {
    pub time: u64,
    pub kind: Kind,
    pub outcome: Outcome,
    /// "Extracted invoices-q3.zip → ~/Documents/Finance"
    pub text: String,
    pub open: Option<PathBuf>,
}

fn append_history(h: &History) {
    let Some(file) = history_file() else { return };
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let outcome = match h.outcome {
        Outcome::Done => "done",
        Outcome::Failed => "failed",
        Outcome::Cancelled => "cancelled",
    };
    let line = format!(
        "{}\t{}\t{}\t{}\t{}\n",
        h.time,
        h.kind.code(),
        outcome,
        one_line(&h.text),
        h.open
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
    );
    let old = std::fs::read_to_string(&file).unwrap_or_default();
    let mut lines: Vec<&str> = old.lines().collect();
    let keep = lines.len().saturating_sub(HISTORY_KEEP - 1);
    lines.drain(..keep);
    let mut text = lines.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    text.push_str(&line);
    let tmp = file.with_extension("tmp");
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(tmp, &file);
    }
}

/// Finished jobs, newest first.
pub fn history() -> Vec<History> {
    let Some(file) = history_file() else {
        return Vec::new();
    };
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let mut out: Vec<History> = text
        .lines()
        .filter_map(|l| {
            let mut f = l.splitn(5, '\t');
            let time = f.next()?.parse().ok()?;
            let kind = Kind::from_code(f.next()?);
            let outcome = match f.next()? {
                "done" => Outcome::Done,
                "failed" => Outcome::Failed,
                _ => Outcome::Cancelled,
            };
            let text = f.next()?.to_owned();
            let open = f.next().filter(|p| !p.is_empty()).map(PathBuf::from);
            Some(History {
                time,
                kind,
                outcome,
                text,
                open,
            })
        })
        .collect();
    out.reverse();
    out
}

/// "Clear finished".
pub fn clear_history() {
    if let Some(file) = history_file() {
        let _ = std::fs::remove_file(file);
    }
}

/// Every registered job of every Crate process. Entries of processes that
/// are gone are removed.
pub fn running() -> Vec<Snapshot> {
    let Ok(dir) = std::fs::read_dir(ops_dir()) else {
        return Vec::new();
    };
    let mut out: Vec<Snapshot> = dir
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if path.extension().is_none_or(|x| x != "op") {
                return None;
            }
            let id = path.file_stem()?.to_string_lossy().into_owned();
            if !process_alive(&id) {
                let _ = std::fs::remove_file(&path);
                return None;
            }
            let text = std::fs::read_to_string(&path).ok()?;
            Some(Snapshot::read(&id, &text))
        })
        .collect();
    out.sort_by(|a, b| (a.queued, a.started, &a.id).cmp(&(b.queued, b.started, &b.id)));
    out
}

fn process_alive(id: &str) -> bool {
    let Some(pid) = id.split('-').next() else {
        return false;
    };
    if cfg!(target_os = "linux") {
        Path::new("/proc").join(pid).exists()
    } else {
        true
    }
}

/// The panel's Pause / Resume / Cancel (or remove from the queue) for job `id`.
pub fn request(id: &str, what: &str) {
    let dir = ops_dir();
    let _ = std::fs::write(dir.join(format!("{id}.ctl")), what);
    // A queued job disappears from the panel at once.
    if what == "cancel" {
        let file = dir.join(format!("{id}.op"));
        if std::fs::read_to_string(&file).is_ok_and(|t| t.contains("queued=1")) {
            let _ = std::fs::remove_file(file);
        }
    }
}

/// Is `window` in the background (minimized or not focused)? Only the real
/// window can tell; headless windows count as in front.
pub fn in_background(window: &slint::Window) -> bool {
    use slint::winit_030::WinitWindowAccessor;
    window
        .with_winit_window(|w| !w.has_focus() || w.is_minimized() == Some(true))
        .unwrap_or(false)
}

/// A desktop notification (org.freedesktop.Notifications), sent from a
/// worker thread; failures are ignored.
fn notify(summary: &str, body: &str) {
    #[cfg(target_os = "linux")]
    {
        let (summary, body) = (summary.to_owned(), body.to_owned());
        std::thread::spawn(move || {
            let _ = send_notification(&summary, &body);
        });
    }
    #[cfg(not(target_os = "linux"))]
    let _ = (summary, body);
}

#[cfg(target_os = "linux")]
fn send_notification(summary: &str, body: &str) -> zbus::Result<()> {
    use std::collections::HashMap;
    let conn = zbus::blocking::Connection::session()?;
    let hints: HashMap<&str, zbus::zvariant::Value> = HashMap::new();
    conn.call_method(
        Some("org.freedesktop.Notifications"),
        "/org/freedesktop/Notifications",
        Some("org.freedesktop.Notifications"),
        "Notify",
        &(
            "Crate",
            0u32,
            "io.github.mlbrainlab.crate",
            summary,
            body,
            Vec::<&str>::new(),
            hints,
            -1i32,
        ),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_control_and_history() {
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: this test alone changes these variables in this binary.
        unsafe {
            std::env::set_var("CRATE_OPS_DIR", dir.path().join("ops"));
            std::env::set_var("XDG_STATE_HOME", dir.path().join("state"));
        }
        let cancel = CancelToken::new();
        let op = Op::start(
            Kind::Extract,
            "Extracting a.zip".into(),
            "→ ~/x/".into(),
            cancel.clone(),
        );
        let p = Progress {
            bytes_done: 50,
            bytes_total: 100,
            files_done: 1,
            files_total: 2,
            current: "a/b.txt".into(),
        };
        *op.last_write.lock().unwrap() = None; // (writes are throttled)
        op.progress(&p);
        let all = running();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].title, "Extracting a.zip");
        assert_eq!(all[0].fraction(), 0.5);
        // Pause from the panel: the next progress call blocks until Resume.
        request(op.id(), "pause");
        *op.last_write.lock().unwrap() = None;
        let id = op.id().to_owned();
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(400));
            request(&id, "resume");
        });
        let before = Instant::now();
        op.progress(&p);
        assert!(
            before.elapsed() >= Duration::from_millis(300),
            "waited while paused"
        );
        t.join().unwrap();
        // Cancel from the panel reaches the job's token.
        request(op.id(), "cancel");
        *op.last_write.lock().unwrap() = None;
        op.progress(&p);
        assert!(cancel.is_cancelled());
        op.finish(Outcome::Cancelled, "Extraction cancelled", None);
        assert!(running().is_empty());
        let h = history();
        assert_eq!(h.len(), 1);
        assert_eq!(
            (h[0].outcome, h[0].text.as_str()),
            (Outcome::Cancelled, "Extraction cancelled")
        );
        clear_history();
        assert!(history().is_empty());
    }
}
