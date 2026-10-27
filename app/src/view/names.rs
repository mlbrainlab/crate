//! "Fix names": the filename encoding picker with a live preview.

use crate_core::{Entry, NameCodec};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::View;

impl View {
    pub(super) fn open_picker(&self) {
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
        let labels: Vec<SharedString> = codecs.iter().map(|c| c.label().into()).collect();
        let suggested = guessed
            .and_then(|c| codecs.iter().position(|&x| x == c))
            .map_or(-1, |i| i as i32);
        let remembered = super::listing::remembered_codec(&self.path).is_some();
        self.g(|g| {
            g.set_codec_labels(ModelRc::new(VecModel::from(labels)));
            g.set_codec_suggested(suggested);
            g.set_codec_remember(remembered);
        });
        self.state.borrow_mut().codecs = codecs;
        self.pick_codec(index);
        self.g(|g| g.set_show_picker(true));
    }

    /// Decodes the names with the picked encoding to preview them.
    pub(super) fn pick_codec(&self, index: usize) {
        let preview = {
            let mut s = self.state.borrow_mut();
            let Some(&codec) = s.codecs.get(index) else {
                return;
            };
            s.codec_index = index;
            let Some(archive) = s.archive.as_mut() else {
                return;
            };
            archive.set_name_encoding(Some(codec));
            preview_names(archive.entries())
        };
        self.g(|g| {
            g.set_codec_index(index as i32);
            g.set_codec_preview(ModelRc::new(VecModel::from(preview)));
        });
    }

    pub(super) fn apply_codec(&self) {
        let codec = {
            let mut s = self.state.borrow_mut();
            s.forced_codec = s.codecs.get(s.codec_index).copied();
            s.forced_codec
        };
        // Remembered (or forgotten) for the next time this archive opens.
        let remember = self.ui.upgrade().is_some_and(|ui| {
            ui.global::<crate::ArchiveUi>().get_codec_remember()
        });
        let key = crate::settings::archive_key(&self.path, "encoding");
        let mut settings = crate::settings::Settings::load();
        let saved = settings.get::<String>(&key).filter(|v| !v.is_empty());
        let wanted = codec.filter(|_| remember).map(|c| c.name().to_owned());
        if saved != wanted {
            settings.set(&key, wanted.unwrap_or_default());
            settings.save();
        }
        self.g(|g| g.set_show_picker(false));
        self.refresh_listing();
    }

    pub(super) fn cancel_picker(&self) {
        {
            let mut s = self.state.borrow_mut();
            let forced = s.forced_codec;
            if let Some(archive) = s.archive.as_mut() {
                archive.set_name_encoding(forced);
            }
        }
        self.g(|g| g.set_show_picker(false));
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
