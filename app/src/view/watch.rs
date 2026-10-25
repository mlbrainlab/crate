//! Open → edit → update (EditUpdate.dc.html): files opened from the archive
//! are watched; after a save, Crate offers to put the new version back.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate_core::edit::Change;

use super::{View, post};
use crate::model;

/// How often opened files are checked for saves.
const POLL: Duration = Duration::from_secs(1);

/// A file opened from the archive for editing.
#[derive(Clone, Debug)]
pub(super) struct OpenEdit {
    /// Path inside the archive.
    pub inside: String,
    /// The temporary copy the app is editing.
    pub file: PathBuf,
    /// What it looked like when opened or last put back.
    baseline: Stamp,
    /// What the last poll saw: a save counts once it stops changing.
    seen: Stamp,
    /// Being put back right now: the baseline it gets if that works.
    updating: Option<Stamp>,
    /// The prompt was shown for this change.
    asked: bool,
}

type Stamp = Option<(SystemTime, u64)>;

fn stamp(path: &Path) -> Stamp {
    let m = std::fs::metadata(path).ok()?;
    Some((m.modified().ok()?, m.len()))
}

impl OpenEdit {
    fn changed(&self) -> bool {
        self.seen.is_some() && self.seen != self.baseline && self.updating.is_none()
    }
}

impl View {
    /// An item was opened with its app: start watching it.
    pub(super) fn watch(&self, inside: String, file: PathBuf) {
        if !file.is_file() {
            return; // folders open in the file manager: nothing to put back
        }
        let now = stamp(&file);
        {
            let mut s = self.state.borrow_mut();
            s.edits.retain(|e| e.inside != inside);
            s.edits.push(OpenEdit {
                inside,
                file,
                baseline: now,
                seen: now,
                updating: None,
                asked: false,
            });
        }
        if !self.edit_timer.running() {
            self.edit_timer.start(slint::TimerMode::Repeated, POLL, || {
                super::now(View::poll_edits);
            });
        }
        self.show_edits();
    }

    fn poll_edits(&self) {
        let mut prompt = None;
        let mut auto = Vec::new();
        {
            let mut s = self.state.borrow_mut();
            let always = s.always_update;
            for (i, e) in s.edits.iter_mut().enumerate() {
                let now = stamp(&e.file);
                let stable = now == e.seen;
                e.seen = now;
                if !stable || !e.changed() || e.asked {
                    continue;
                }
                e.asked = true;
                if always {
                    auto.push(i);
                } else if prompt.is_none() {
                    prompt = Some(i);
                }
            }
        }
        self.show_edits();
        if !auto.is_empty() {
            self.update_edits(&auto);
        }
        if let Some(i) = prompt
            && !self.g(|g| g.get_upd_show() || g.get_busy())
        {
            self.prompt_update(i);
        }
    }

    /// The tray: how many are open, how many changed.
    fn show_edits(&self) {
        let s = self.state.borrow();
        let open = s.edits.len() as i32;
        let changed = s.edits.iter().filter(|e| e.changed()).count() as i32;
        let text = match (open, changed) {
            (_, 0) => format!(
                "{} open for editing · saves come back here",
                model::count(open as u64, "file")
            ),
            (_, c) => format!(
                "{} open for editing · {c} changed",
                model::count(open as u64, "file")
            ),
        };
        let read_only = !self.g(|g| g.get_can_edit());
        drop(s);
        self.g(|g| {
            g.set_edits_open(open);
            g.set_edits_changed(changed);
            g.set_edits_text(text.into());
            g.set_upd_read_only(read_only);
        });
    }

    fn prompt_update(&self, i: usize) {
        let (name, before, after) = {
            let s = self.state.borrow();
            let Some(e) = s.edits.get(i) else { return };
            let size = |st: Stamp| st.map(|(_, n)| model::size_text(n)).unwrap_or_default();
            (
                e.inside.rsplit('/').next().unwrap_or(&e.inside).to_owned(),
                size(e.baseline),
                size(e.seen),
            )
        };
        let archive = self.g(|g| g.get_archive_name());
        let read_only = !self.g(|g| g.get_can_edit());
        let text = if read_only {
            format!(
                "{name} was changed ({before} → {after}). {archive} can’t be changed; save the new version as a separate file?"
            )
        } else {
            format!(
                "{name} was changed ({before} → {after}). Save the new version back into {archive}?"
            )
        };
        self.state.borrow_mut().prompting = Some(i);
        self.g(|g| {
            g.set_upd_title(
                if read_only {
                    "Save your changes?"
                } else {
                    "Update the archive?"
                }
                .into(),
            );
            g.set_upd_text(text.into());
            g.set_upd_always(false);
            g.set_upd_show(true);
        });
    }

    /// Puts the changed files back (all of them, or `only` these).
    pub(super) fn update_edits(&self, only: &[usize]) {
        let changes: Vec<Change> = {
            let mut s = self.state.borrow_mut();
            let all = only.is_empty();
            let mut changes = Vec::new();
            for (i, e) in s.edits.iter_mut().enumerate() {
                if (all || only.contains(&i)) && e.seen != e.baseline && e.updating.is_none() {
                    e.updating = Some(e.seen);
                    changes.push(Change::Replace {
                        path: e.inside.clone(),
                        source: e.file.clone(),
                    });
                }
            }
            changes
        };
        if !changes.is_empty() {
            self.edit(changes, false);
        }
        self.show_edits();
    }

    /// After an edit ends: the files put back have a new baseline.
    pub(super) fn edits_finished(&self, ok: bool) {
        {
            let mut s = self.state.borrow_mut();
            for e in &mut s.edits {
                if let Some(st) = e.updating.take()
                    && ok
                {
                    e.baseline = st;
                    e.asked = false;
                }
            }
        }
        self.show_edits();
        if ok && self.state.borrow().close_after_update {
            self.state.borrow_mut().close_after_update = false;
            self.state.borrow_mut().edits.clear();
            self.close_window();
        }
    }

    pub(super) fn upd_update(&self) {
        let always = self.g(|g| g.get_upd_always());
        self.g(|g| g.set_upd_show(false));
        let i = self.state.borrow_mut().prompting.take();
        if always {
            self.state.borrow_mut().always_update = true;
        }
        if let Some(i) = i {
            self.update_edits(&[i]);
        }
    }

    pub(super) fn upd_discard(&self) {
        self.g(|g| g.set_upd_show(false));
        let mut s = self.state.borrow_mut();
        if let Some(i) = s.prompting.take()
            && let Some(e) = s.edits.get_mut(i)
        {
            // Not put back: the next save asks again.
            e.baseline = e.seen;
            e.asked = false;
        }
        drop(s);
        self.show_edits();
    }

    /// Save copy elsewhere…: the edited file, wherever the user wants it.
    pub(super) fn upd_copy(&self) {
        self.g(|g| g.set_upd_show(false));
        let Some(i) = self.state.borrow_mut().prompting.take() else {
            return;
        };
        let Some(e) = self.state.borrow().edits.get(i).cloned() else {
            return;
        };
        let name = e.inside.rsplit('/').next().unwrap_or(&e.inside).to_owned();
        let dir = self.path.parent().map(Path::to_owned);
        std::thread::spawn(move || {
            let mut dialog = rfd::FileDialog::new()
                .set_title("Save a copy as")
                .set_file_name(&name);
            if let Some(dir) = dir {
                dialog = dialog.set_directory(dir);
            }
            let Some(out) = dialog.save_file() else {
                return;
            };
            let copied = std::fs::copy(&e.file, &out);
            post(move |v| match copied {
                Ok(_) => {
                    if let Some(x) = v
                        .state
                        .borrow_mut()
                        .edits
                        .iter_mut()
                        .find(|x| x.file == e.file)
                    {
                        x.baseline = x.seen;
                    }
                    v.show_edits();
                    v.show_toast(&format!(
                        "Saved {}",
                        out.file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    ));
                }
                Err(err) => v.show_error("Couldn’t save the copy", &err.to_string()),
            });
        });
    }

    /// Closing: edited files not put back yet → ask first. True when the
    /// window may close now.
    pub(super) fn may_close(&self) -> bool {
        let changed: Vec<String> = self
            .state
            .borrow()
            .edits
            .iter()
            .filter(|e| e.changed())
            .map(|e| e.inside.rsplit('/').next().unwrap_or(&e.inside).to_owned())
            .collect();
        if changed.is_empty() {
            return true;
        }
        let title = match changed.len() {
            1 => "1 edited file isn’t in the archive yet".to_owned(),
            n => format!("{n} edited files aren’t in the archive yet"),
        };
        let names = changed.join(", ");
        self.g(|g| {
            g.set_close_title(title.into());
            g.set_close_text(
                format!("If you close now, changes to {names} are lost. Temp copies are deleted when Crate quits.").into(),
            );
            g.set_close_show(true);
        });
        false
    }

    pub(super) fn close_update(&self) {
        self.g(|g| g.set_close_show(false));
        self.state.borrow_mut().close_after_update = true;
        self.update_edits(&[]);
    }

    pub(super) fn close_discard(&self) {
        self.g(|g| g.set_close_show(false));
        self.state.borrow_mut().edits.clear();
        self.close_window();
    }
}
