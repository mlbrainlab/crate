//! Changing the archive from the full view: add files and folders, new
//! folder, rename, delete. Each change rewrites the archive safely in
//! `crate_core::edit` (temp file, then rename), then the window re-reads it.

use std::path::PathBuf;

use crate_core::CancelToken;
use crate_core::create::{CreateOptions, OutFormat};
use crate_core::edit::{self, Change, EditOptions};

use super::{PROGRESS_INTERVAL, PasswordFor, View, post};
use crate::model;

/// What the name prompt is for.
#[derive(Clone, Debug)]
pub(super) enum Naming {
    /// Rename the item at this path.
    Rename {
        path: String,
        parent: String,
    },
    NewFolder {
        parent: String,
    },
}

fn join(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_owned()
    } else {
        format!("{parent}/{name}")
    }
}

/// `photos/` for labels; "the top of the archive" for the root.
fn shown_folder(path: &str) -> String {
    if path.is_empty() {
        "the top of the archive".to_owned()
    } else {
        format!("{path}/")
    }
}

impl View {
    /// The folder the full view shows (`""`: the root).
    fn current_folder(&self) -> String {
        let s = self.state.borrow();
        match &s.tree {
            Some(tree) if self.g(|g| g.get_full()) => tree.path(s.folder),
            _ => String::new(),
        }
    }

    /// Names already used directly inside `folder`.
    fn names_in(&self, folder: &str) -> Vec<String> {
        let s = self.state.borrow();
        let Some(tree) = &s.tree else {
            return Vec::new();
        };
        let Some(node) = tree.find(folder) else {
            return Vec::new();
        };
        tree.node(node)
            .children
            .iter()
            .map(|&c| tree.node(c).name.clone())
            .collect()
    }

    // ----- Add -------------------------------------------------------------

    /// Add files / Add folder: the desktop's file chooser, then a summary
    /// to confirm (Slint can't receive drops from other apps yet).
    pub(super) fn pick_to_add(&self, folders: bool) {
        let into = self.current_folder();
        let start = self.path.parent().map(|p| p.to_owned());
        std::thread::spawn(move || {
            let mut dialog =
                rfd::FileDialog::new().set_title(if folders { "Add folders" } else { "Add files" });
            if let Some(dir) = start {
                dialog = dialog.set_directory(dir);
            }
            let picked = if folders {
                dialog.pick_folders()
            } else {
                dialog.pick_files()
            };
            if let Some(paths) = picked {
                post(move |v| v.confirm_add(paths, into));
            }
        });
    }

    /// Shows what will be added where, and what it replaces.
    pub fn confirm_add(&self, paths: Vec<PathBuf>, into: String) {
        if paths.is_empty() {
            return;
        }
        let existing = self.names_in(&into);
        let names: Vec<String> = paths
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        let clashes = names.iter().filter(|n| existing.contains(n)).count() as u64;
        let listed = match names.as_slice() {
            [one] => one.clone(),
            [a, b] => format!("{a} and {b}"),
            [a, b, rest @ ..] => format!("{a}, {b} and {} more", rest.len()),
            [] => String::new(),
        };
        let archive = self.g(|g| g.get_archive_name());
        let title = format!(
            "Add {} to {}",
            model::count(names.len() as u64, "item"),
            shown_folder(&into)
        );
        let note = match clashes {
            0 => String::new(),
            n => format!(
                "{} with the same name will be replaced",
                model::count(n, "item")
            ),
        };
        self.state.borrow_mut().pending_add = Some((paths, into));
        self.g(|g| {
            g.set_add_title(title.into());
            g.set_add_text(format!("{listed} will be compressed into {archive}.").into());
            g.set_add_note(note.into());
            g.set_add_show(true);
        });
    }

    pub(super) fn add_confirm(&self) {
        self.g(|g| g.set_add_show(false));
        let Some((sources, into)) = self.state.borrow_mut().pending_add.take() else {
            return;
        };
        self.edit(vec![Change::Add { sources, into }], false);
    }

    // ----- New folder and rename ---------------------------------------------

    pub(super) fn new_folder(&self) {
        let parent = self.current_folder();
        let existing = self.names_in(&parent);
        let name = (1..)
            .map(|n| {
                if n == 1 {
                    "New folder".to_owned()
                } else {
                    format!("New folder ({n})")
                }
            })
            .find(|n| !existing.contains(n))
            .expect("some name is free");
        self.state.borrow_mut().naming = Some(Naming::NewFolder {
            parent: parent.clone(),
        });
        self.show_name_dialog(
            "New folder",
            &name,
            &format!("Inside {}", shown_folder(&parent)),
            "Create",
        );
    }

    pub(super) fn rename_current(&self) {
        let target = {
            let s = self.state.borrow();
            let (Some(c), Some(tree)) = (s.selection.cursor, &s.tree) else {
                return;
            };
            let Some(&node) = s.full_rows.get(c) else {
                return;
            };
            let path = tree.path(node);
            let parent = tree.path(tree.node(node).parent);
            Some((path, parent, tree.node(node).name.clone()))
        };
        let Some((path, parent, name)) = target else {
            return;
        };
        self.state.borrow_mut().naming = Some(Naming::Rename {
            path,
            parent: parent.clone(),
        });
        self.show_name_dialog(
            "Rename",
            &name,
            &format!("Path inside archive: {}", shown_folder(&parent)),
            "Rename",
        );
    }

    fn show_name_dialog(&self, title: &str, value: &str, note: &str, button: &str) {
        self.g(|g| {
            g.set_name_title(title.into());
            g.set_name_value(value.into());
            g.set_name_note(note.into());
            g.set_name_button(button.into());
            g.set_name_error("".into());
            g.set_name_show(true);
        });
    }

    /// Why a name can't be used, if it can't.
    fn name_problem(&self, name: &str) -> Option<String> {
        let naming = self.state.borrow().naming.clone()?;
        let name = name.trim();
        if name.is_empty() {
            return Some(String::new());
        }
        if name.contains('/') || name == "." || name == ".." || name.contains('\0') {
            return Some("A name can’t contain “/” or be “.” or “..”".into());
        }
        let (parent, current) = match &naming {
            Naming::Rename { path, parent } => {
                (parent.clone(), path.rsplit('/').next().map(str::to_owned))
            }
            Naming::NewFolder { parent } => (parent.clone(), None),
        };
        if current.as_deref() != Some(name) && self.names_in(&parent).iter().any(|n| n == name) {
            return Some(format!("“{name}” is already in {}", shown_folder(&parent)));
        }
        None
    }

    pub(super) fn name_edited(&self, name: &str) {
        let problem = self.name_problem(name).unwrap_or_default();
        self.g(|g| g.set_name_error(problem.into()));
    }

    pub(super) fn name_confirm(&self, name: &str) {
        if let Some(problem) = self.name_problem(name) {
            self.g(|g| g.set_name_error(problem.into()));
            return;
        }
        let name = name.trim().to_owned();
        self.g(|g| g.set_name_show(false));
        let Some(naming) = self.state.borrow_mut().naming.take() else {
            return;
        };
        let change = match naming {
            Naming::Rename { path, parent } => {
                let to = join(&parent, &name);
                if to == path {
                    return;
                }
                Change::Rename { from: path, to }
            }
            Naming::NewFolder { parent } => Change::NewFolder(join(&parent, &name)),
        };
        self.edit(vec![change], false);
    }

    // ----- Delete ------------------------------------------------------------

    pub(super) fn delete_selected(&self) {
        let mut paths: Vec<String> = self
            .state
            .borrow()
            .selection
            .selected
            .iter()
            .cloned()
            .collect();
        if paths.is_empty() {
            return;
        }
        paths.sort();
        let names: Vec<String> = paths
            .iter()
            .map(|p| p.rsplit('/').next().unwrap_or(p).to_owned())
            .collect();
        let title = match names.as_slice() {
            [one] => format!("Delete “{one}” from the archive?"),
            _ => format!(
                "Delete {} from the archive?",
                model::count(names.len() as u64, "item")
            ),
        };
        let listed = match names.as_slice() {
            [one] => one.clone(),
            [a, b] => format!("{a} and {b}"),
            [a, b, c] => format!("{a}, {b} and {c}"),
            [a, b, rest @ ..] => format!("{a}, {b} and {} more", rest.len()),
            [] => String::new(),
        };
        let archive = self.g(|g| g.get_archive_name());
        self.state.borrow_mut().deleting = paths;
        self.g(|g| {
            g.set_delete_title(title.into());
            g.set_delete_text(
                format!("{listed} will be removed from {archive}. The archive is rewritten; this can’t be undone.").into(),
            );
            g.set_delete_show(true);
        });
    }

    pub(super) fn delete_confirm(&self, backup: bool) {
        self.g(|g| g.set_delete_show(false));
        let paths = std::mem::take(&mut self.state.borrow_mut().deleting);
        if !paths.is_empty() {
            self.edit(paths.into_iter().map(Change::Delete).collect(), backup);
        }
    }

    // ----- Running an edit -------------------------------------------------

    /// Asks for the password first when the change needs it (re-encrypting
    /// 7z or age, encrypting files added to an encrypted ZIP).
    fn edit(&self, changes: Vec<Change>, backup: bool) {
        let needs_password = {
            let s = self.state.borrow();
            let Some(archive) = &s.archive else { return };
            let encrypted = s.entries.iter().any(|e| e.encrypted);
            let only_moves = changes.iter().all(|c| {
                matches!(
                    c,
                    Change::Delete(_) | Change::Rename { .. } | Change::NewFolder(_)
                )
            });
            // ZIP entries are copied raw: moving them around needs no password.
            let zip_ok = archive.format() == crate_core::Format::Zip && only_moves;
            encrypted && s.password.is_none() && !zip_ok
        };
        if needs_password {
            self.state.borrow_mut().pending_edit = Some((changes, backup));
            self.ask_password(PasswordFor::Extract, "");
        } else {
            self.run_edit(changes, backup);
        }
    }

    pub(super) fn run_edit(&self, changes: Vec<Change>, backup: bool) {
        let (archive, password) = {
            let mut s = self.state.borrow_mut();
            let Some(archive) = s.archive.take() else {
                return;
            };
            (archive, s.password.clone())
        };
        let cancel = CancelToken::new();
        self.state.borrow_mut().cancel = Some(cancel.clone());
        self.g(|g| {
            g.set_busy(true);
            g.set_progress(0.0);
            g.set_busy_text("Updating the archive…".into());
        });
        let path = self.path.clone();
        std::thread::spawn(move || {
            let options = EditOptions {
                password,
                backup,
                ..EditOptions::default()
            };
            let mut last = std::time::Instant::now() - PROGRESS_INTERVAL;
            let result = edit::apply(
                &path,
                &changes,
                &options,
                &mut |p| {
                    if last.elapsed() < PROGRESS_INTERVAL {
                        return;
                    }
                    last = std::time::Instant::now();
                    let f = p.bytes_done as f32 / p.bytes_total.max(1) as f32;
                    post(move |v| {
                        v.g(|g| {
                            g.set_progress(f);
                            g.set_busy_text(
                                format!("Updating the archive… {}%", (f * 100.0).floor()).into(),
                            );
                        })
                    });
                },
                &cancel,
            );
            post(move |v| v.edited(archive, result));
        });
    }

    fn edited(&self, archive: crate_core::Archive, result: crate_core::Result<edit::EditReport>) {
        {
            let mut s = self.state.borrow_mut();
            s.archive = Some(archive);
            s.cancel = None;
        }
        self.g(|g| {
            g.set_busy(false);
            g.set_busy_text("".into());
        });
        if self.state.borrow().quit_when_idle {
            let _ = slint::quit_event_loop();
            return;
        }
        match result {
            Ok(report) => {
                let mut parts = Vec::new();
                if report.added > 0 {
                    parts.push(format!("added {}", model::count(report.added, "item")));
                }
                if report.replaced > 0 {
                    parts.push(format!(
                        "replaced {}",
                        model::count(report.replaced, "item")
                    ));
                }
                if report.renamed > 0 {
                    parts.push("renamed".to_owned());
                }
                if report.deleted > 0 {
                    parts.push(format!("deleted {}", model::count(report.deleted, "item")));
                }
                let mut text = if parts.is_empty() {
                    "Archive updated".to_owned()
                } else {
                    sentence_case(&parts.join(", "))
                };
                if let Some(bak) = report
                    .backup
                    .and_then(|b| b.file_name().map(|n| n.to_string_lossy().into_owned()))
                {
                    text.push_str(&format!(" · previous version kept as {bak}"));
                }
                self.show_toast(&text);
                // Read it again: the list, sizes and offsets all changed.
                let password = self.state.borrow().password.clone();
                self.open(password);
            }
            Err(crate_core::Error::Cancelled) => {
                self.show_toast("Cancelled; the archive wasn’t changed")
            }
            Err(e) => self.show_error(
                "Couldn’t update the archive",
                &format!("{e}. The archive wasn’t changed"),
            ),
        }
    }

    // ----- Read-only formats ----------------------------------------------

    /// RAR can't be changed: offer a ZIP copy (any name ending picks the format).
    pub(super) fn save_copy(&self) {
        let dir = self.path.parent().map(|p| p.to_owned());
        let stem = crate_core::destination::folder_name(&self.path);
        let source = self.path.clone();
        let password = self.state.borrow().password.clone();
        std::thread::spawn(move || {
            let mut dialog = rfd::FileDialog::new()
                .set_title("Save a copy as")
                .set_file_name(format!("{stem}.zip"));
            if let Some(dir) = dir {
                dialog = dialog.set_directory(dir);
            }
            let Some(out) = dialog.save_file() else {
                return;
            };
            post(move |v| v.convert_copy(source, password, out));
        });
    }

    pub fn convert_copy(&self, source: PathBuf, password: Option<String>, out: PathBuf) {
        let format = OutFormat::from_file_name(&out.to_string_lossy())
            .filter(|(_, age)| !age)
            .map_or(OutFormat::Zip, |(f, _)| f);
        let out = if OutFormat::from_file_name(&out.to_string_lossy()).is_some() {
            out
        } else {
            out.with_extension("zip")
        };
        let cancel = CancelToken::new();
        self.state.borrow_mut().cancel = Some(cancel.clone());
        self.g(|g| {
            g.set_busy(true);
            g.set_progress(0.0);
            g.set_busy_text("Saving a copy…".into());
        });
        std::thread::spawn(move || {
            let options = CreateOptions {
                format,
                level: format.levels().2,
                ..CreateOptions::default()
            };
            let result = crate_core::convert::convert(
                &source,
                password.as_deref(),
                &out,
                &options,
                &mut |_| {},
                &cancel,
            );
            post(move |v| {
                v.state.borrow_mut().cancel = None;
                v.g(|g| {
                    g.set_busy(false);
                    g.set_busy_text("".into());
                });
                match result {
                    Ok(_) => v.show_toast(&format!(
                        "Saved a copy as {}",
                        out.file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    )),
                    Err(e) => v.show_error("Couldn’t save a copy", &e.to_string()),
                }
            });
        });
    }
}

fn sentence_case(text: &str) -> String {
    let mut c = text.chars();
    match c.next() {
        Some(first) => first.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}
