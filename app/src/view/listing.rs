//! Opening and listing the archive. Formats without an index (tar) stream
//! their listing in; the list fills as batches arrive.

use crate::i18n::{tr, trf};
use std::sync::Arc;

use crate_core::{Archive, CancelToken, Entry, Error, ListProgress, OpenOptions};

use super::{PasswordFor, View, post, sentence};
use crate::model;

impl View {
    pub(super) fn open(&self, password: Option<String>) {
        let path = self.path.clone();
        let cancel = CancelToken::new();
        // An encoding remembered for this archive beats the preference.
        let remembered = remembered_codec(&self.path);
        let encoding = remembered.or(self.state.borrow().prefs.encoding);
        let volumes = self.state.borrow().volumes.clone();
        if remembered.is_some() {
            self.state.borrow_mut().forced_codec = remembered;
        }
        {
            let mut s = self.state.borrow_mut();
            s.streaming = Some(cancel.clone());
            // Remembered once it works: edits re-encrypt with it.
            s.trying_password = password.clone();
        }
        std::thread::spawn(move || {
            let mut on_batch = |batch: &[Entry], progress: ListProgress| {
                let batch = batch.to_vec();
                post(move |v| v.listed_batch(batch, progress));
            };
            let result = Archive::open_streaming(
                &path,
                &OpenOptions {
                    password,
                    encoding,
                    volumes,
                },
                &mut on_batch,
                &cancel,
            );
            post(move |v| v.opened(result));
        });
    }

    /// Part of a slow listing arrived: show it while the rest loads.
    fn listed_batch(&self, batch: Vec<Entry>, progress: ListProgress) {
        let first = {
            let mut s = self.state.borrow_mut();
            if s.streaming.is_none() {
                return; // already finished
            }
            let first = s.entries.is_empty();
            let entries = Arc::make_mut(&mut s.entries);
            entries.extend(batch.iter().cloned());
            if let Some(tree) = s.tree.as_mut() {
                for e in &batch {
                    tree.insert(e);
                }
            }
            first
        };
        let fraction = if progress.bytes_total > 0 {
            progress.bytes_read as f32 / progress.bytes_total as f32
        } else {
            0.0
        };
        self.g(|g| {
            if first {
                g.set_status("ready".into());
            }
            g.set_streaming(true);
            g.set_stream_progress(fraction.clamp(0.0, 1.0));
            g.set_stream_text(
                trf(
                    "Reading contents… {items} items · {pct}%",
                    &[
                        ("items", &model::thousands(progress.entries as u64)),
                        ("pct", &(fraction * 100.0).floor().to_string()),
                    ],
                )
                .into(),
            );
            g.set_item_count(progress.entries as i32);
        });
        self.refresh_rows();
    }

    pub(super) fn stop_reading(&self) {
        if let Some(t) = &self.state.borrow().streaming {
            t.cancel();
        }
    }

    fn opened(&self, result: crate_core::Result<Archive>) {
        self.state.borrow_mut().streaming = None;
        self.g(|g| {
            g.set_password_checking(false);
            g.set_streaming(false);
        });
        match result {
            Ok(archive) => {
                {
                    let mut s = self.state.borrow_mut();
                    if let Some(pw) = s.trying_password.take() {
                        s.password = Some(pw);
                    }
                }
                let complete = archive.is_complete();
                let editable = crate_core::edit::can_edit(archive.format());
                self.g(|g| {
                    g.set_show_password(false);
                    g.set_can_edit(editable && complete);
                    g.set_format_name(archive.format().name().into());
                    g.set_status("ready".into());
                });
                self.state.borrow_mut().archive = Some(archive);
                self.refresh_listing();
                if !complete {
                    self.g(|g| {
                        let summary = g.get_summary();
                        g.set_summary(
                            trf("{summary} · stopped reading", &[("summary", &summary)]).into(),
                        );
                    });
                }
                let auto = std::mem::take(&mut self.state.borrow_mut().auto_extract);
                if auto {
                    self.extract();
                }
            }
            Err(Error::PasswordRequired) => {
                self.g(|g| {
                    g.set_status("locked".into());
                    g.set_status_text(tr("Enter the password to see what’s inside.").into());
                });
                self.ask_password(PasswordFor::Open, "");
            }
            Err(Error::WrongPassword) => {
                self.g(|g| g.set_status("locked".into()));
                self.wrong_password(PasswordFor::Open);
            }
            Err(Error::MissingVolume { name, part, total }) => {
                self.missing_volume(&name, part, total);
            }
            Err(e) => self.g(|g| {
                g.set_status("failed".into());
                g.set_status_text(sentence(&e.to_string()).into());
            }),
        }
        if let Some(listed) = self.on_listed.borrow_mut().take() {
            listed();
        }
    }

    /// Takes the entries from the archive (after opening or "Fix names") and
    /// redraws everything that depends on them.
    pub(super) fn refresh_listing(&self) {
        {
            let mut s = self.state.borrow_mut();
            let Some(archive) = &s.archive else { return };
            let entries = archive.shared_entries();
            let guessed = archive.name_encoding().filter(|e| e.guessed);
            let summary = model::summary(&entries, archive.file_size());
            let size = archive.file_size();
            let forced = s.forced_codec;
            let method = model::main_method(&entries);
            let saved = model::space_saved(&entries, size);
            self.g(|g| {
                g.set_summary(summary.into());
                g.set_item_count(entries.len() as i32);
                g.set_encrypted_count(model::encrypted_count(&entries) as i32);
                g.set_show_banner(guessed.is_some());
                if let Some(e) = guessed {
                    g.set_banner_text(
                        trf(
                            "Names were read as {encoding}.",
                            &[("encoding", &e.codec.label())],
                        )
                        .into(),
                    );
                }
                g.set_encoding_chip(forced.map(|c| c.label()).unwrap_or_default().into());
                g.set_method_name(method.unwrap_or_default().into());
                g.set_space_saved(saved.unwrap_or(-1.0));
            });
            s.entries = entries;
            s.selection.clear();
            s.selection.rows_changed();
            if let Some(old) = &s.tree {
                // After an edit the folder on screen stays, if it still exists.
                let path = old.path(s.folder);
                let tree = crate::tree::Tree::from_entries(&s.entries);
                s.folder = tree.find(&path).unwrap_or(crate::tree::ROOT);
                s.tree = Some(tree);
            }
        }
        self.refresh_destination();
        self.refresh_rows();
    }

    pub(super) fn refresh_destination(&self) {
        let mut s = self.state.borrow_mut();
        let parent = match self.path.parent() {
            Some(p) if !p.as_os_str().is_empty() => p.to_owned(),
            _ => std::path::PathBuf::from("."),
        };
        let dest = self.suggest_in(&parent, &s);
        self.g(|g| {
            g.set_dest_name(dest.name.clone().into());
            g.set_dest_label(model::display_folder(&dest.folder).into());
        });
        s.dest = Some(dest);
    }
}

/// The filename encoding "Remember for this archive" saved for `path`.
pub(super) fn remembered_codec(path: &std::path::Path) -> Option<crate_core::NameCodec> {
    crate::settings::Settings::load()
        .get::<String>(&crate::settings::archive_key(path, "encoding"))
        .and_then(|label| crate_core::NameCodec::from_label(&label))
}
