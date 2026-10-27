//! "When extracting, also unpack archives found inside": after an
//! extraction, every archive among the extracted files is extracted next to
//! itself (one folder per archive, or as it is when it holds a single
//! folder) and the extracted copy is removed. Goes a few levels deep. Every
//! level is an ordinary [`Archive::extract`], so all the safety checks and
//! limits apply each time.

use std::path::{Path, PathBuf};

use crate::archive::{Archive, ExtractOptions, Selection};
use crate::destination;
use crate::error::{Error, Result};
use crate::progress::CancelToken;

/// How deep "archives inside archives inside archives" goes.
pub const MAX_DEPTH: u32 = 3;

/// True for names Crate opens as archives (by extension; split parts and
/// RAR volumes after the first don't count).
pub fn is_archive_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [
        ".zip", ".7z", ".rar", ".tar", ".tgz", ".tar.gz", ".tar.xz", ".txz", ".tar.zst", ".tzst",
        ".tar.bz2", ".tbz2", ".tbz", ".tar.age",
    ]
    .iter()
    .any(|ext| name.ends_with(ext))
}

#[derive(Debug, Default)]
pub struct NestedReport {
    /// Inner archives unpacked (and removed).
    pub unpacked: Vec<PathBuf>,
    /// Inner archives left as they are, with why (encrypted, damaged…).
    pub kept: Vec<(PathBuf, String)>,
}

/// Unpacks the archives among `files` (paths just extracted), then the
/// archives found in what they held, up to [`MAX_DEPTH`] levels.
pub fn unpack(files: &[PathBuf], cancel: &CancelToken) -> Result<NestedReport> {
    let mut report = NestedReport::default();
    let mut level: Vec<PathBuf> = files
        .iter()
        .filter(|p| p.is_file() && is_archive_name(&p.to_string_lossy()))
        .cloned()
        .collect();
    for _ in 0..MAX_DEPTH {
        let mut next = Vec::new();
        for file in level {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            match unpack_one(&file, cancel) {
                Ok(folder) => {
                    report.unpacked.push(file);
                    next.extend(archives_in(&folder));
                }
                Err(Error::Cancelled) => return Err(Error::Cancelled),
                Err(e) => report.kept.push((file, e.to_string())),
            }
        }
        if next.is_empty() {
            break;
        }
        level = next;
    }
    Ok(report)
}

/// Extracts `file` next to itself and removes it; returns the folder.
fn unpack_one(file: &Path, cancel: &CancelToken) -> Result<PathBuf> {
    let mut archive = Archive::open(file)?;
    if archive.entries().iter().any(|e| e.encrypted) {
        return Err(Error::PasswordRequired);
    }
    let dest = destination::suggest(file, archive.entries());
    archive.extract(
        &Selection::All,
        &dest.dest,
        &ExtractOptions::default(),
        &mut |_| {},
        cancel,
    )?;
    drop(archive);
    std::fs::remove_file(file)?;
    Ok(dest.folder)
}

/// Archive files anywhere under `dir`.
fn archives_in(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let Ok(kind) = e.file_type() else { continue };
            let path = e.path();
            if kind.is_dir() {
                stack.push(path);
            } else if kind.is_file() && is_archive_name(&e.file_name().to_string_lossy()) {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}
