//! Properties (Properties.dc.html): overview with a type breakdown,
//! integrity, checksums with verify-by-paste; and Convert format.

use std::path::PathBuf;
use std::time::Instant;

use crate_core::checksum::{self, Checksums};
use crate_core::create::{CreateOptions, OutFormat};
use crate_core::{CancelToken, EntryKind};

use super::{View, open_with_default_app, post};
use crate::model;

/// The Convert dialog's formats, in button order.
const CONVERT_FORMATS: [OutFormat; 5] = [
    OutFormat::Zip,
    OutFormat::SevenZ,
    OutFormat::TarZst,
    OutFormat::TarGz,
    OutFormat::TarXz,
];

/// Buckets for "What's inside".
fn category(name: &str) -> &'static str {
    let (ext, tile) = model::file_kind(name);
    if matches!(ext.as_str(), "MD" | "TXT" | "RTF" | "LOG") {
        return "Documents";
    }
    match tile {
        model::Tile::Image => "Images",
        model::Tile::Media => "Audio & video",
        model::Tile::Pdf | model::Tile::Document | model::Tile::Sheet => "Documents",
        model::Tile::Code => "Code",
        model::Tile::Archive => "Archives",
        _ => "Other",
    }
}

impl View {
    pub(super) fn open_properties(&self) {
        {
            let s = self.state.borrow();
            if s.archive.is_none() {
                return;
            }
        }
        self.refresh_properties();
        self.g(|g| {
            g.set_pr_verify("".into());
            g.set_pr_verify_state(0);
            g.set_pr_show(true);
        });
        self.compute_checksums();
    }

    fn refresh_properties(&self) {
        let s = self.state.borrow();
        let Some(archive) = &s.archive else { return };
        let entries = &s.entries;
        let (mut files, mut dirs, mut unpacked) = (0u64, 0u64, 0u64);
        // name → (bytes, packed bytes it has figures for, bytes with figures)
        let mut cats: std::collections::HashMap<&'static str, (u64, u64, u64)> = Default::default();
        let mut encrypted = 0u64;
        for e in entries.iter() {
            match &e.kind {
                EntryKind::Dir => dirs += 1,
                EntryKind::File => {
                    files += 1;
                    unpacked += e.size;
                    let c = cats.entry(category(&e.path)).or_default();
                    c.0 += e.size;
                    if let Some(p) = e.packed_size {
                        c.1 += p;
                        c.2 += e.size;
                    }
                    if e.encrypted {
                        encrypted += 1;
                    }
                }
                _ => {}
            }
        }
        let file_size = archive.file_size();
        let saved = model::space_saved(entries, file_size);
        let mut rows: Vec<(&'static str, (u64, u64, u64))> = cats.into_iter().collect();
        rows.sort_by_key(|(_, (b, _, _))| std::cmp::Reverse(*b));
        let categories: Vec<crate::Category> = rows
            .into_iter()
            .take(6)
            .map(|(name, (bytes, packed, with))| crate::Category {
                name: name.into(),
                size: model::size_text(bytes).into(),
                share: if unpacked > 0 {
                    bytes as f32 / unpacked as f32
                } else {
                    0.0
                },
                saved: if with > 0 {
                    (1.0 - packed as f64 / with.max(1) as f64).clamp(0.0, 1.0) as f32
                } else {
                    -1.0
                },
            })
            .collect();
        let location = self
            .path
            .parent()
            .map(|p| model::display_folder(p).trim_end_matches('/').to_owned())
            .unwrap_or_default();
        let format = archive.format();
        let method = model::main_method(entries).unwrap_or_else(|| "—".into());
        let encryption = match (encrypted, format) {
            (0, crate_core::Format::TarAge) => "age (whole archive)".to_owned(),
            (0, _) => "None".to_owned(),
            (n, crate_core::Format::Zip) => format!("AES-256 · {}", model::count(n, "file")),
            (n, _) => format!("{} encrypted", model::count(n, "file")),
        };
        // 7z with several files per block compresses them together.
        let solid = if format == crate_core::Format::SevenZ && files > 1 && method != "Stored" {
            "Yes (7z blocks)"
        } else {
            "No"
        };
        let format_name = format.name().to_owned();
        drop(s);
        self.g(|g| {
            g.set_pr_location(format!("{location} · {}", model::size_text(file_size)).into());
            g.set_pr_original(model::size_text(unpacked).into());
            g.set_pr_compressed(model::size_text(file_size).into());
            g.set_pr_saved(
                saved
                    .map(|f| format!("{}%", (f * 100.0).round()))
                    .unwrap_or_else(|| "—".into())
                    .into(),
            );
            g.set_pr_items(
                format!(
                    "{} files · {} folders",
                    model::thousands(files),
                    model::thousands(dirs)
                )
                .into(),
            );
            g.set_pr_categories(slint::ModelRc::new(slint::VecModel::from(categories)));
            g.set_pr_format(format_name.into());
            g.set_pr_method(method.into());
            g.set_pr_encryption(encryption.into());
            g.set_pr_solid(solid.into());
        });
    }

    /// Checksums of the archive file, on a worker; cached per file state.
    fn compute_checksums(&self) {
        let stamp = std::fs::metadata(&self.path)
            .ok()
            .and_then(|m| m.modified().ok().map(|t| (t, m.len())));
        {
            let s = self.state.borrow();
            if let Some((st, sums)) = &s.checksums
                && Some(*st) == stamp
            {
                let sums = sums.clone();
                drop(s);
                self.show_checksums(&sums);
                return;
            }
        }
        self.g(|g| {
            g.set_pr_sums_busy(true);
            g.set_pr_sha256("".into());
            g.set_pr_blake3("".into());
            g.set_pr_md5("".into());
        });
        let path = self.path.clone();
        std::thread::spawn(move || {
            let result = checksum::compute(&path, &mut |_| {}, &CancelToken::new());
            post(move |v| {
                v.g(|g| g.set_pr_sums_busy(false));
                match result {
                    Ok(sums) => {
                        let stamp = std::fs::metadata(&path)
                            .ok()
                            .and_then(|m| m.modified().ok().map(|t| (t, m.len())));
                        if let Some(st) = stamp {
                            v.state.borrow_mut().checksums = Some((st, sums.clone()));
                        }
                        v.show_checksums(&sums);
                        // Something may be pasted already.
                        let pasted = v.g(|g| g.get_pr_verify());
                        let _ = pasted;
                    }
                    Err(e) => v.show_error("Couldn’t compute the checksums", &e.to_string()),
                }
            });
        });
    }

    fn show_checksums(&self, sums: &Checksums) {
        self.g(|g| {
            g.set_pr_sha256(sums.sha256.clone().into());
            g.set_pr_blake3(sums.blake3.clone().into());
            g.set_pr_md5(sums.md5.clone().into());
        });
    }

    pub(super) fn copy_checksum_row(&self, which: i32) {
        let Some((_, sums)) = self.state.borrow().checksums.clone() else {
            return;
        };
        let (value, name) = match which {
            0 => (sums.sha256, "SHA-256"),
            1 => (sums.blake3, "BLAKE3"),
            _ => (sums.md5, "MD5"),
        };
        self.copy_to_clipboard(&value, &format!("Copied the {name} checksum"));
    }

    pub(super) fn verify_pasted(&self, pasted: &str) {
        let Some((_, sums)) = self.state.borrow().checksums.clone() else {
            return;
        };
        let (state, text) = if pasted.trim().is_empty() {
            (0, String::new())
        } else {
            match sums.verify(pasted) {
                Some(name) => (1, format!("Matches the {name} checksum")),
                None => (2, "Doesn’t match any checksum of this file".to_owned()),
            }
        };
        self.g(|g| {
            g.set_pr_verify_state(state);
            g.set_pr_verify(text.into());
        });
    }

    pub(super) fn tested_at(&self, seconds: f64) {
        let took = if seconds < 0.95 {
            "under a second".to_owned()
        } else {
            format!("{seconds:.1} s")
        };
        self.g(|g| g.set_pr_tested(format!("CRC verified · tested just now · {took}").into()));
    }

    // ----- Convert ---------------------------------------------------------

    pub(super) fn open_convert(&self) {
        let (format, size) = {
            let s = self.state.borrow();
            let Some(archive) = &s.archive else { return };
            (archive.format(), archive.file_size())
        };
        // Default: RAR and ZIP go to 7z (smaller), everything else to ZIP.
        let target = match format {
            crate_core::Format::Zip | crate_core::Format::Rar => 1,
            _ => 0,
        };
        self.g(|g| {
            g.set_pr_show(false);
            g.set_cv_from(format.name().into());
            g.set_cv_from_size(model::size_text(size).into());
            g.set_cv_keep(true);
            g.set_cv_show(true);
        });
        self.set_convert_format(target);
    }

    pub(super) fn set_convert_format(&self, which: i32) {
        let format = CONVERT_FORMATS[which.clamp(0, 4) as usize];
        let stem = crate_core::destination::folder_name(&self.path);
        self.g(|g| {
            g.set_cv_format(which);
            g.set_cv_target(format!("{stem}{}", format.extension(false)).into());
        });
    }

    pub(super) fn convert_confirm(&self, keep_original: bool) {
        let which = self.g(|g| {
            g.set_cv_show(false);
            g.get_cv_format()
        });
        let format = CONVERT_FORMATS[which.clamp(0, 4) as usize];
        let stem = crate_core::destination::folder_name(&self.path);
        let dir = self.path.parent().map(PathBuf::from).unwrap_or_default();
        let out = crate_core::destination::fresh_folder(
            &dir,
            &format!("{stem}{}", format.extension(false)),
        )
        .folder;
        let (source, password) = (self.path.clone(), self.state.borrow().password.clone());
        let cancel = CancelToken::new();
        self.state.borrow_mut().cancel = Some(cancel.clone());
        self.g(|g| {
            g.set_busy(true);
            g.set_progress(0.0);
            g.set_busy_text("Converting…".into());
        });
        std::thread::spawn(move || {
            let options = CreateOptions {
                format,
                level: format.levels().2,
                ..CreateOptions::default()
            };
            let started = Instant::now();
            let mut last = started;
            let result = crate_core::convert::convert(
                &source,
                password.as_deref(),
                &out,
                &options,
                &mut |p| {
                    if last.elapsed() < super::PROGRESS_INTERVAL {
                        return;
                    }
                    last = Instant::now();
                    let f = p.bytes_done as f32 / p.bytes_total.max(1) as f32;
                    post(move |v| {
                        v.g(|g| {
                            g.set_progress(f);
                            g.set_busy_text(format!("Converting… {}%", (f * 100.0).floor()).into());
                        })
                    });
                },
                &cancel,
            );
            // The original goes to the Trash only once the copy is safely there.
            let trashed = match (&result, keep_original) {
                (Ok(_), false) => trash::delete(&source).map_err(|e| e.to_string()),
                _ => Ok(()),
            };
            post(move |v| {
                v.state.borrow_mut().cancel = None;
                v.g(|g| {
                    g.set_busy(false);
                    g.set_busy_text("".into());
                });
                let name = out
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                match (result, trashed) {
                    (Ok(_), Ok(())) if keep_original => {
                        v.show_toast(&format!("Converted to {name}"))
                    }
                    (Ok(_), Ok(())) => {
                        // The original is gone: open the new archive instead.
                        v.show_toast(&format!(
                            "Converted to {name} · original moved to the Trash"
                        ));
                        let _ = open_with_default_app(&out);
                        v.close_window();
                    }
                    (Ok(_), Err(e)) => v.show_toast(&format!(
                        "Converted to {name} · couldn’t trash the original: {e}"
                    )),
                    (Err(crate_core::Error::Cancelled), _) => {
                        v.show_toast("Conversion cancelled; nothing was written")
                    }
                    (Err(e), _) => v.show_error("Couldn’t convert", &e.to_string()),
                }
            });
        });
    }
}
