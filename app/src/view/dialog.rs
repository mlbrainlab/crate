//! The Extract dialog (Extract.dc.html): what to extract, where, and how.

use crate::i18n::{tr, trn};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate_core::destination::{self, Destination};
use crate_core::{ExtractOptions, IfExists, Selection};

use super::extract::{Request, selection_of};
use super::{PasswordFor, View, post};
use crate::model;

/// What the dialog shows, kept between edits.
pub(super) struct Dialog {
    what: i32,
    pattern: String,
    dest: String,
    create_folder: bool,
    keep_structure: bool,
    open_after: bool,
    trash_after: bool,
    /// Also unpack archives found among the extracted files.
    nested: bool,
    if_exists: i32,
}

impl View {
    pub(super) fn open_dialog(&self) {
        let parent = self.path.parent().map(Path::to_owned).unwrap_or_default();
        self.open_dialog_at(&parent);
    }

    /// "Choose another folder…" after a failed extraction.
    pub(super) fn open_dialog_for_last(&self) {
        let last = self.state.borrow().last_request.clone();
        let parent = last
            .and_then(|r| r.dest.folder.parent().map(Path::to_owned))
            .or_else(|| self.path.parent().map(Path::to_owned))
            .unwrap_or_default();
        self.open_dialog_at(&parent);
    }

    fn open_dialog_at(&self, parent: &Path) {
        let selected = !self.state.borrow().selection.selected.is_empty();
        self.state.borrow_mut().dialog = Some(Dialog {
            what: if selected { 1 } else { 0 },
            pattern: String::new(),
            dest: shown_path(parent),
            create_folder: true,
            keep_structure: true,
            open_after: true,
            trash_after: false,
            nested: false,
            if_exists: 0,
        });
        let pattern_and_dest = {
            let s = self.state.borrow();
            let d = s.dialog.as_ref().expect("just set");
            (d.pattern.clone(), d.dest.clone())
        };
        self.g(|g| {
            g.set_ex_pattern(pattern_and_dest.0.into());
            g.set_ex_dest(pattern_and_dest.1.into());
            g.set_ex_show(true);
        });
        self.update_dialog();
    }

    pub(super) fn close_dialog(&self) {
        self.state.borrow_mut().dialog = None;
        self.g(|g| g.set_ex_show(false));
    }

    fn edit_dialog(&self, f: impl FnOnce(&mut Dialog)) {
        if let Some(d) = self.state.borrow_mut().dialog.as_mut() {
            f(d);
        }
        self.update_dialog();
    }

    pub(super) fn dialog_set_what(&self, what: i32) {
        self.edit_dialog(|d| d.what = what);
    }

    pub(super) fn dialog_pattern(&self, text: &str) {
        self.edit_dialog(|d| d.pattern = text.to_owned());
    }

    pub(super) fn dialog_dest(&self, text: &str) {
        self.edit_dialog(|d| d.dest = text.to_owned());
    }

    pub(super) fn dialog_quick_dest(&self, which: i32) {
        let dir = match which {
            1 => crate::settings::downloads_dir(),
            2 => crate::settings::desktop_dir(),
            _ => self.path.parent().map(Path::to_owned).unwrap_or_default(),
        };
        self.set_dialog_dest(&dir);
    }

    fn set_dialog_dest(&self, dir: &Path) {
        let shown = shown_path(dir);
        self.g(|g| g.set_ex_dest(shown.clone().into()));
        self.edit_dialog(|d| d.dest = shown);
    }

    /// The desktop's own folder chooser (through the XDG portal on Linux).
    pub(super) fn dialog_browse(&self) {
        let start = self
            .state
            .borrow()
            .dialog
            .as_ref()
            .map(|d| expand(&d.dest))
            .filter(|p| p.is_dir());
        std::thread::spawn(move || {
            let mut picker = rfd::FileDialog::new().set_title(tr("Extract to"));
            if let Some(dir) = start {
                picker = picker.set_directory(dir);
            }
            if let Some(dir) = picker.pick_folder() {
                post(move |v| v.set_dialog_dest(&dir));
            }
        });
    }

    pub(super) fn dialog_toggle(&self, which: i32) {
        self.edit_dialog(|d| match which {
            0 => d.create_folder = !d.create_folder,
            1 => d.keep_structure = !d.keep_structure,
            2 => d.open_after = !d.open_after,
            4 => d.nested = !d.nested,
            _ => d.trash_after = !d.trash_after,
        });
    }

    pub(super) fn dialog_if_exists(&self, which: i32) {
        self.edit_dialog(|d| d.if_exists = which);
    }

    /// The paths the dialog would extract (`None`: everything).
    fn dialog_paths(&self, d: &Dialog) -> Option<HashSet<String>> {
        let s = self.state.borrow();
        match d.what {
            1 => Some(s.selection.selected.clone()),
            2 => Some(
                s.entries
                    .iter()
                    .filter(|e| !e.is_dir() && model::matches_pattern(&d.pattern, &e.path))
                    .map(|e| e.path.trim_start_matches("./").to_owned())
                    .collect(),
            ),
            _ => None,
        }
    }

    /// Where the files go: the chosen folder, or a new folder in it.
    fn dialog_destination(&self, d: &Dialog) -> Destination {
        let base = expand(&d.dest);
        if !d.create_folder {
            return Destination {
                dest: base.clone(),
                name: base
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                folder: base,
            };
        }
        let s = self.state.borrow();
        let smart = self.suggest_in(&base, &s);
        if d.keep_structure && d.what == 0 {
            smart
        } else {
            // A partial or flattened extraction always gets its own folder.
            destination::fresh_folder(&base, &destination::folder_name(&self.path))
        }
    }

    /// Recomputes everything the dialog shows from its choices.
    fn update_dialog(&self) {
        let Some(d) = self.state.borrow_mut().dialog.take() else {
            return;
        };
        let paths = self.dialog_paths(&d);
        let selection = paths.as_ref().map_or(Selection::All, selection_of);
        let (needed, encrypted, files, all, selected) = {
            let s = self.state.borrow();
            let has_password = s.archive.as_ref().is_some_and(|a| a.has_password());
            let mut needed = 0u64;
            let mut encrypted = 0;
            let mut files = 0;
            for e in s.entries.iter().filter(|e| {
                !e.is_dir() && paths.as_ref().is_none_or(|_| selection.includes(&e.path))
            }) {
                needed += e.size;
                files += 1;
                if e.encrypted && !has_password {
                    encrypted += 1;
                }
            }
            (
                needed,
                encrypted,
                files,
                s.entries.len(),
                s.selection.selected.len(),
            )
        };
        let dest = self.dialog_destination(&d);
        let base = expand(&d.dest);
        let free = free_space(&base);
        let ready = !d.dest.trim().is_empty()
            && base.is_absolute()
            && (d.what != 2 || files > 0)
            && (d.what != 1 || selected > 0);
        let button = match d.what {
            0 => tr("Extract everything"),
            _ => trn("Extract {n} file", "Extract {n} files", files),
        };
        let pattern_note = if d.pattern.trim().is_empty() {
            String::new()
        } else {
            trn("{n} file matches", "{n} files match", files)
        };
        self.g(|g| {
            g.set_ex_what(d.what);
            g.set_ex_all_count(all as i32);
            g.set_ex_selected_count(selected as i32);
            g.set_ex_pattern_note(pattern_note.into());
            g.set_ex_folder_name(dest.name.clone().into());
            g.set_ex_smart(d.create_folder && dest.dest == base && d.what == 0);
            g.set_ex_create_folder(d.create_folder);
            g.set_ex_keep_structure(d.keep_structure);
            g.set_ex_open_after(d.open_after);
            g.set_ex_trash_after(d.trash_after);
            g.set_ex_nested(d.nested);
            g.set_ex_if_exists(d.if_exists);
            g.set_ex_encrypted(encrypted);
            g.set_ex_needed(model::size_text(needed).into());
            g.set_ex_free(
                free.map(model::size_text)
                    .unwrap_or_else(|| "—".into())
                    .into(),
            );
            g.set_ex_space(free.map_or(0.0, |f| needed as f32 / f.max(1) as f32));
            g.set_ex_button(button.into());
            g.set_ex_ready(ready);
        });
        self.state.borrow_mut().dialog = Some(d);
    }

    pub(super) fn dialog_confirm(&self, password: String) {
        let Some(d) = self.state.borrow_mut().dialog.take() else {
            return;
        };
        let paths = self.dialog_paths(&d);
        let picked = paths.as_ref().map_or(0, HashSet::len);
        let request = Request {
            selection: paths.as_ref().map_or(Selection::All, selection_of),
            picked,
            dest: self.dialog_destination(&d),
            options: ExtractOptions {
                if_exists: match d.if_exists {
                    1 => IfExists::Skip,
                    2 => IfExists::Overwrite,
                    3 => IfExists::KeepBoth,
                    4 => IfExists::KeepNewer,
                    _ => IfExists::Ask,
                },
                keep_structure: d.keep_structure,
                ..ExtractOptions::default()
            },
            open_after: d.open_after,
            trash_after: d.trash_after,
            nested: d.nested,
            names_checked: false,
        };
        self.g(|g| g.set_ex_show(false));
        if self.needs_password(&request.selection) {
            self.state.borrow_mut().pending_request = Some(request);
            if password.is_empty() {
                self.ask_password(PasswordFor::Extract, "");
            } else {
                // Check it like the prompt would; a wrong one opens the prompt.
                self.state.borrow_mut().password_for = Some(PasswordFor::Extract);
                self.unlock(password);
            }
        } else {
            self.run(request);
        }
    }
}

/// `/home/me/Downloads` → `~/Downloads`.
fn shown_path(dir: &Path) -> String {
    model::display_folder(dir).trim_end_matches('/').to_owned()
}

/// `~/Downloads` → `/home/me/Downloads`.
fn expand(text: &str) -> PathBuf {
    let text = text.trim();
    match text.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default();
            home.join(rest.trim_start_matches('/'))
        }
        _ => PathBuf::from(text),
    }
}

/// Free space where `dir` is, or would be created.
fn free_space(dir: &Path) -> Option<u64> {
    let existing = dir.ancestors().find(|p| p.is_dir())?;
    fs4::available_space(existing).ok()
}
