//! Quick preview: reads the entry under the cursor into memory on a copy of
//! the archive handle, so the window stays responsive and nothing is
//! written to disk.

use crate::i18n::tr;
use slint::{Image, ModelRc, SharedString, VecModel};

use crate_core::{CancelToken, Error};

use super::{View, post, sentence};
use crate::preview::{self, Content};
use crate::tree::NodeId;

impl View {
    /// Space / Preview: the file under the cursor.
    pub(super) fn preview_current(&self) {
        let cursor = self.state.borrow().selection.cursor;
        let Some(row) = cursor else { return };
        if self.preview_node(row).is_some() {
            if !self.state.borrow().mono_checked {
                self.state.borrow_mut().mono_checked = true;
                if let Some(family) = crate::platform::monospace_family() {
                    self.g(|g| g.set_mono_font(family.into()));
                }
            }
            self.state.borrow_mut().preview = Some(row);
            self.g(|g| g.set_pv_show(true));
            self.load_preview();
        }
    }

    /// The node behind a list row, if it is a previewable file.
    fn preview_node(&self, row: usize) -> Option<(NodeId, usize)> {
        let s = self.state.borrow();
        let node = *s.full_rows.get(row)?;
        let tree = s.tree.as_ref()?;
        let n = tree.node(node);
        (!n.is_dir).then_some(())?;
        Some((node, n.entry?))
    }

    /// ← / →: the previous or next file in the list (folders are skipped).
    pub(super) fn preview_step(&self, delta: i32) {
        let Some(at) = self.state.borrow().preview else {
            return;
        };
        let len = self.state.borrow().full_rows.len() as i64;
        let mut i = at as i64 + delta as i64;
        while (0..len).contains(&i) {
            if self.preview_node(i as usize).is_some() {
                let row = i as usize;
                self.state.borrow_mut().preview = Some(row);
                // The list follows, so Enter or Extract act on this file.
                self.select(|p, rows| p.click(rows, row, false, false));
                self.load_preview();
                return;
            }
            i += delta.signum() as i64;
        }
    }

    pub(super) fn preview_close(&self) {
        let mut s = self.state.borrow_mut();
        s.preview = None;
        s.preview_serial += 1;
        drop(s);
        self.g(|g| {
            g.set_pv_show(false);
            g.set_pv_lines(ModelRc::default());
            g.set_pv_image(Image::default());
        });
    }

    fn can_step(&self, row: usize, delta: i64) -> bool {
        let len = self.state.borrow().full_rows.len() as i64;
        let mut i = row as i64 + delta;
        while (0..len).contains(&i) {
            if self.preview_node(i as usize).is_some() {
                return true;
            }
            i += delta;
        }
        false
    }

    fn load_preview(&self) {
        let Some(row) = self.state.borrow().preview else {
            return;
        };
        let Some((_, entry)) = self.preview_node(row) else {
            return;
        };
        let (name, size, copy, serial) = {
            let mut s = self.state.borrow_mut();
            s.preview_serial += 1;
            let e = &s.entries[entry];
            let name = e.path.rsplit('/').next().unwrap_or(&e.path).to_owned();
            let copy = s.archive.as_ref().map(|a| a.try_clone());
            (name, e.size, copy, s.preview_serial)
        };
        let (can_prev, can_next) = (self.can_step(row, -1), self.can_step(row, 1));
        self.g(|g| {
            g.set_pv_name(name.clone().into());
            g.set_pv_info(SharedString::new());
            g.set_pv_language(SharedString::new());
            g.set_pv_kind(0);
            g.set_pv_can_prev(can_prev);
            g.set_pv_can_next(can_next);
        });
        let mut archive = match copy {
            Some(Ok(a)) => a,
            Some(Err(e)) => return self.preview_message(&sentence(&e.to_string())),
            None => {
                return self.preview_message(&tr("The archive is busy. Try again in a moment."));
            }
        };
        std::thread::spawn(move || {
            let limit = preview::limit_for(&name);
            let result = archive
                .read_entry(entry, limit, &CancelToken::new())
                .map(|data| preview::prepare(&name, &data, size));
            post(move |v| v.preview_ready(serial, result));
        });
    }

    fn preview_message(&self, text: &str) {
        self.g(|g| {
            g.set_pv_kind(4);
            g.set_pv_message(text.into());
        });
    }

    fn preview_ready(&self, serial: u64, result: crate_core::Result<Content>) {
        if self.state.borrow().preview_serial != serial {
            return; // the user moved on
        }
        let lines = |lines: Vec<String>| {
            let lines: Vec<SharedString> = lines.into_iter().map(Into::into).collect();
            ModelRc::new(VecModel::from(lines))
        };
        match result {
            Ok(Content::Image { pixels, info }) => self.g(|g| {
                g.set_pv_image(Image::from_rgba8(pixels));
                g.set_pv_info(info.into());
                g.set_pv_language(tr("Image").into());
                g.set_pv_kind(1);
            }),
            Ok(Content::Svg(data)) => match Image::load_from_svg_data(&data) {
                Ok(image) => self.g(|g| {
                    g.set_pv_image(image);
                    g.set_pv_language("SVG".into());
                    g.set_pv_kind(1);
                }),
                Err(_) => self.preview_message(&tr("This SVG can’t be shown.")),
            },
            Ok(Content::Text {
                lines: text,
                language,
                info,
            }) => {
                self.state.borrow_mut().preview_text = text.join("\n");
                self.g(|g| {
                    g.set_pv_lines(lines(text));
                    g.set_pv_info(info.into());
                    g.set_pv_language(format!("{language} · UTF-8").into());
                    g.set_pv_kind(2);
                });
            }
            Ok(Content::Hex { lines: hex, info }) => self.g(|g| {
                g.set_pv_lines(lines(hex));
                g.set_pv_info(info.into());
                g.set_pv_language(tr("Hex").into());
                g.set_pv_kind(3);
            }),
            Err(Error::PasswordRequired) => self.preview_message(&tr(
                "This file is encrypted. Extract or open it to enter the password.",
            )),
            Err(Error::WrongPassword) => self.preview_message(&tr("Wrong password for this file.")),
            Err(e) => self.preview_message(&sentence(&e.to_string())),
        }
    }

    pub(super) fn preview_copy(&self) {
        let text = self.state.borrow().preview_text.clone();
        self.copy_to_clipboard(&text, &tr("Copied the text"));
    }

    pub(super) fn preview_open(&self) {
        if let Some(row) = self.state.borrow().preview {
            self.preview_close();
            self.open_row(row);
        }
    }

    pub(super) fn preview_extract(&self) {
        if let Some(row) = self.state.borrow().preview {
            self.preview_close();
            self.select(|p, rows| p.click(rows, row, false, false));
            self.open_dialog();
        }
    }
}
