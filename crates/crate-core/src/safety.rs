//! Rules for writing archive contents to disk. Every extraction goes through
//! this module (CLAUDE.md): no path may escape the destination, links may not
//! point outside it or be written through, and archives that expand far beyond
//! their size are refused unless the caller raises the limit.

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use crate::error::{Error, Result};

/// Why an entry was not extracted. Shown to the user afterwards, e.g. "2 items
/// were skipped for your safety" (`EdgeCases.dc.html`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkipReason {
    /// The name climbs out of the destination with `..`.
    PathTraversal,
    /// The name is absolute (`/etc/…`, `C:\…`, `\\server\…`).
    AbsolutePath,
    /// Empty, NUL bytes, or otherwise unusable.
    InvalidName,
    /// A symlink or hard link whose target lies outside the destination.
    LinkEscape,
    /// The entry would be written through a symlink from the same archive.
    ThroughLink,
    /// Device, FIFO or another type that is never extracted.
    UnsupportedType,
    /// Needs a password (handled from M2 on).
    Encrypted,
    /// Something with the same name already exists and the policy is to skip.
    Exists,
    /// Clashes with another entry, e.g. a file and a folder with the same name.
    Conflict,
    /// Symlinks can't be created on this platform.
    NotSupportedOnPlatform,
}

impl fmt::Display for SkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            SkipReason::PathTraversal => "path leaves the destination folder",
            SkipReason::AbsolutePath => "absolute path",
            SkipReason::InvalidName => "invalid name",
            SkipReason::LinkEscape => "link points outside the destination folder",
            SkipReason::ThroughLink => "would be written through a link",
            SkipReason::UnsupportedType => "unsupported entry type",
            SkipReason::Encrypted => "encrypted",
            SkipReason::Exists => "already exists",
            SkipReason::Conflict => "conflicts with another item",
            SkipReason::NotSupportedOnPlatform => "not supported on this system",
        })
    }
}

/// An entry that was left out, with the reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
    pub path: String,
    pub reason: SkipReason,
}

/// Turns an archive entry name into a relative path that stays inside the
/// destination, or says why it can't. `/` and `\` both separate components
/// (Windows tools write `\`). `..` is refused outright, even when it would
/// land back inside: no legitimate archive needs it.
pub fn safe_relative_path(name: &str) -> std::result::Result<PathBuf, SkipReason> {
    if name.contains('\0') {
        return Err(SkipReason::InvalidName);
    }
    if name.starts_with('/') || name.starts_with('\\') || has_drive_prefix(name) {
        return Err(SkipReason::AbsolutePath);
    }
    let mut path = PathBuf::new();
    for part in name.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => return Err(SkipReason::PathTraversal),
            _ => path.push(part),
        }
    }
    // Belt and braces: the result must be plain components only.
    if path.as_os_str().is_empty() || !path.components().all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(SkipReason::InvalidName);
    }
    Ok(path)
}

fn has_drive_prefix(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// Whether a symlink at `link` (a path from [`safe_relative_path`]) pointing
/// to `target` resolves inside the destination. Only leading `..` components
/// are allowed, and never more of them than `link` has parent folders. A `..`
/// after a normal component is refused, because that component could itself
/// be a link (`a -> .`, `b -> a/..` would escape).
pub fn link_target_is_inside(link: &Path, target: &str) -> bool {
    if target.is_empty()
        || target.contains('\0')
        || target.starts_with('/')
        || target.starts_with('\\')
        || has_drive_prefix(target)
    {
        return false;
    }
    let depth = link.components().count().saturating_sub(1);
    let mut ups = 0;
    let mut descended = false;
    for part in target.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." if descended => return false,
            ".." => ups += 1,
            _ => descended = true,
        }
    }
    ups <= depth
}

/// The hard-link counterpart: the target is another entry name, resolved from
/// the destination root.
pub fn hardlink_target(target: &str) -> std::result::Result<PathBuf, SkipReason> {
    safe_relative_path(target).map_err(|_| SkipReason::LinkEscape)
}

/// True when any parent of `path` is one of `links` (entries that are
/// symlinks in the same extraction).
pub fn passes_through_link(path: &Path, links: &HashSet<PathBuf>) -> bool {
    path.ancestors().skip(1).any(|a| links.contains(a))
}

/// Limits that protect against decompression bombs and full disks.
#[derive(Clone, Debug)]
pub struct Limits {
    /// Refuse when the unpacked total is more than this many times the size
    /// of the archive file (and more than `ratio_floor` bytes). `None` turns
    /// the check off.
    pub max_ratio: Option<f64>,
    /// Below this unpacked total the ratio isn't checked: a few MB of zeros
    /// compressing 1000× is harmless.
    pub ratio_floor: u64,
    /// Check free disk space before writing anything.
    pub check_free_space: bool,
}

impl Default for Limits {
    fn default() -> Self {
        // Deflate tops out near 1032×, so honest archives of very redundant
        // data can come close; nested and overlapping bombs go far beyond.
        Self {
            max_ratio: Some(1000.0),
            ratio_floor: 64 * 1024 * 1024,
            check_free_space: true,
        }
    }
}

/// Free-space headroom on top of the unpacked size.
const SPACE_MARGIN: u64 = 16 * 1024 * 1024;

/// Checks the declared unpacked size against the limits before anything is
/// written. Declared sizes can lie; [`copy_limited`] enforces them while
/// writing.
pub fn check_expansion(
    unpacked: u64,
    archive_size: u64,
    dest: &Path,
    limits: &Limits,
) -> Result<()> {
    if let Some(limit) = limits.max_ratio
        && unpacked > limits.ratio_floor
        && archive_size > 0
    {
        let ratio = unpacked as f64 / archive_size as f64;
        if ratio > limit {
            return Err(Error::SuspiciousRatio { ratio, limit });
        }
    }
    if limits.check_free_space {
        let available = fs4::available_space(existing_ancestor(dest))?;
        let needed = unpacked.saturating_add(SPACE_MARGIN);
        if needed > available {
            return Err(Error::NotEnoughSpace { needed, available });
        }
    }
    Ok(())
}

fn existing_ancestor(path: &Path) -> &Path {
    path.ancestors()
        .find(|a| !a.as_os_str().is_empty() && a.exists())
        .unwrap_or(Path::new("."))
}

/// Copies one entry's data, refusing to write more than it declared.
/// `on_chunk` gets the byte count of each chunk and may cancel by returning an
/// error.
pub fn copy_limited(
    reader: &mut dyn io::Read,
    writer: &mut dyn io::Write,
    declared: u64,
    path: &str,
    on_chunk: &mut dyn FnMut(u64) -> Result<()>,
) -> Result<u64> {
    let mut buf = vec![0u8; 256 * 1024];
    let mut written = 0u64;
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        };
        written += n as u64;
        if written > declared {
            return Err(Error::SizeExceeded {
                path: path.to_owned(),
            });
        }
        writer.write_all(&buf[..n])?;
        on_chunk(n as u64)?;
    }
    Ok(written)
}

/// Permission bits Crate applies to extracted files: the stored rwx bits only
/// (never setuid/setgid/sticky), and always readable and writable by the user.
pub fn file_mode(stored: Option<u32>) -> u32 {
    (stored.unwrap_or(0o644) & 0o777) | 0o600
}

/// Same for folders, which must stay enterable by the user.
pub fn dir_mode(stored: Option<u32>) -> u32 {
    (stored.unwrap_or(0o755) & 0o777) | 0o700
}

/// What to do when the destination already has an item with the same name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IfExists {
    /// Keep the existing item and report the entry as skipped.
    #[default]
    Skip,
    /// Replace existing files. Existing folders are merged, never deleted.
    Overwrite,
    /// Keep the existing item; the new one gets a free name: `a (2).txt`.
    KeepBoth,
    /// Replace only when the incoming file is newer.
    KeepNewer,
    /// Ask a [`ConflictResolver`] for each clash (without one: skip).
    Ask,
}

/// An extracted item whose name is already taken in the destination.
#[derive(Clone, Debug)]
pub struct Conflict {
    /// Path relative to the destination.
    pub path: String,
    pub existing_size: u64,
    pub existing_modified: Option<SystemTime>,
    pub existing_is_dir: bool,
    pub incoming_size: u64,
    pub incoming_modified: Option<SystemTime>,
}

/// What to do with one [`Conflict`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolution {
    Skip,
    Replace,
    KeepBoth,
}

/// Decides conflicts for [`IfExists::Ask`]; may block on a UI.
pub type ConflictResolver<'a> = dyn FnMut(&Conflict) -> Resolution + 'a;

/// `notes.tar.gz` → (`notes`, `.tar.gz`); a leading dot is part of the name
/// (`.bashrc`).
pub fn split_extension(name: &str) -> (&str, &str) {
    let first = name.chars().next().map_or(0, char::len_utf8);
    match name[first..].find('.') {
        Some(i) => name.split_at(first + i),
        None => (name, ""),
    }
}

/// `report.pdf` → `report (2).pdf` (or `(3)`…), whichever is free in `dir`.
pub fn free_name(dir: &Path, name: &std::ffi::OsStr) -> std::path::PathBuf {
    let name = name.to_string_lossy();
    // Keep ".tar.gz" together; split at the first dot that isn't leading.
    let (stem, ext) = split_extension(&name);
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| fs::symlink_metadata(p).is_err())
        .expect("some name is free")
}

/// Moves a finished staging folder into `dest`. Folders are merged; an
/// existing item that is a symlink is never descended into, so extraction
/// can't be redirected through links already in the destination.
pub fn commit(
    staging: &Path,
    dest: &Path,
    if_exists: IfExists,
    resolve: &mut ConflictResolver<'_>,
    skipped: &mut Vec<Skipped>,
) -> Result<()> {
    merge(staging, dest, Path::new(""), if_exists, resolve, skipped)
}

fn merge(
    src: &Path,
    dst: &Path,
    rel: &Path,
    if_exists: IfExists,
    resolve: &mut ConflictResolver<'_>,
    skipped: &mut Vec<Skipped>,
) -> Result<()> {
    for item in fs::read_dir(src)? {
        let item = item?;
        let name = item.file_name();
        let from = item.path();
        let to = dst.join(&name);
        let rel = rel.join(&name);
        let existing = match fs::symlink_metadata(&to) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                fs::rename(&from, &to)?;
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        let incoming = fs::symlink_metadata(&from)?;
        if incoming.is_dir() && existing.is_dir() {
            merge(&from, &to, &rel, if_exists, resolve, skipped)?;
            continue;
        }
        let skip = |skipped: &mut Vec<Skipped>| {
            skipped.push(Skipped {
                path: rel.to_string_lossy().replace('\\', "/"),
                reason: SkipReason::Exists,
            })
        };
        let resolution = match if_exists {
            IfExists::Skip => Resolution::Skip,
            IfExists::Overwrite => Resolution::Replace,
            IfExists::KeepBoth => Resolution::KeepBoth,
            IfExists::KeepNewer => {
                let newer = match (incoming.modified(), existing.modified()) {
                    (Ok(a), Ok(b)) => a > b,
                    _ => false,
                };
                if newer {
                    Resolution::Replace
                } else {
                    Resolution::Skip
                }
            }
            IfExists::Ask => resolve(&Conflict {
                path: rel.to_string_lossy().replace('\\', "/"),
                existing_size: existing.len(),
                existing_modified: existing.modified().ok(),
                existing_is_dir: existing.is_dir(),
                incoming_size: incoming.len(),
                incoming_modified: incoming.modified().ok(),
            }),
        };
        match resolution {
            Resolution::Skip => skip(skipped),
            // Never delete a folder (or follow a link to one) to make room.
            Resolution::Replace if existing.is_dir() => skip(skipped),
            Resolution::Replace => {
                // rename() replaces a file in one step, but not a file or
                // link with a folder: remove it first.
                if incoming.is_dir() {
                    fs::remove_file(&to)?;
                }
                fs::rename(&from, &to)?;
            }
            Resolution::KeepBoth => {
                fs::rename(&from, free_name(dst, &name))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(name: &str) -> String {
        safe_relative_path(name)
            .unwrap()
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn extensions() {
        assert_eq!(split_extension("notes.tar.gz"), ("notes", ".tar.gz"));
        assert_eq!(split_extension(".bashrc"), (".bashrc", ""));
        assert_eq!(split_extension("تقرير.txt"), ("تقرير", ".txt"));
        assert_eq!(split_extension("README"), ("README", ""));
        assert_eq!(split_extension(""), ("", ""));
    }

    #[test]
    fn plain_names_pass() {
        assert_eq!(ok("a.txt"), "a.txt");
        assert_eq!(ok("dir/sub/a.txt"), "dir/sub/a.txt");
        assert_eq!(ok("dir\\sub\\a.txt"), "dir/sub/a.txt");
        assert_eq!(ok("./dir//a.txt"), "dir/a.txt");
        assert_eq!(ok("dir/"), "dir");
        assert_eq!(ok("..."), "...");
        assert_eq!(ok("a..b"), "a..b");
    }

    #[test]
    fn traversal_is_refused() {
        for name in [
            "../evil",
            "a/../../evil",
            "a/..",
            "..\\evil",
            "a\\..\\..\\evil",
            "..",
        ] {
            assert_eq!(
                safe_relative_path(name),
                Err(SkipReason::PathTraversal),
                "{name}"
            );
        }
    }

    #[test]
    fn absolute_paths_are_refused() {
        for name in [
            "/etc/passwd",
            "\\windows\\x",
            "C:\\x",
            "c:/x",
            "C:x",
            "\\\\server\\share\\x",
            "//server/x",
        ] {
            assert_eq!(
                safe_relative_path(name),
                Err(SkipReason::AbsolutePath),
                "{name}"
            );
        }
    }

    #[test]
    fn empty_and_nul_names_are_refused() {
        for name in ["", ".", "./", "a\0b"] {
            assert_eq!(
                safe_relative_path(name),
                Err(SkipReason::InvalidName),
                "{name:?}"
            );
        }
    }

    #[test]
    fn link_targets() {
        let link = Path::new("dir/sub/link");
        assert!(link_target_is_inside(link, "file"));
        assert!(link_target_is_inside(link, "../x"));
        assert!(link_target_is_inside(link, "../../x"));
        assert!(link_target_is_inside(link, "./x/y"));
        assert!(!link_target_is_inside(link, "../../../x"));
        assert!(!link_target_is_inside(link, "/etc/passwd"));
        assert!(!link_target_is_inside(link, "C:\\x"));
        // `..` after a normal component could climb out through another link.
        assert!(!link_target_is_inside(link, "x/../y"));
        assert!(!link_target_is_inside(Path::new("top"), ".."));
        assert!(!link_target_is_inside(Path::new("top"), ""));
    }

    #[test]
    fn through_link() {
        let links: HashSet<PathBuf> = [PathBuf::from("a/link")].into();
        assert!(passes_through_link(Path::new("a/link/file"), &links));
        assert!(passes_through_link(Path::new("a/link/x/y"), &links));
        assert!(!passes_through_link(Path::new("a/link"), &links));
        assert!(!passes_through_link(Path::new("a/other"), &links));
    }

    #[test]
    fn copy_stops_at_declared_size() {
        let data = vec![7u8; 1000];
        let mut out = Vec::new();
        let err = copy_limited(&mut &data[..], &mut out, 999, "x", &mut |_| Ok(())).unwrap_err();
        assert!(matches!(err, Error::SizeExceeded { .. }));
        let mut out = Vec::new();
        assert_eq!(
            copy_limited(&mut &data[..], &mut out, 1000, "x", &mut |_| Ok(())).unwrap(),
            1000
        );
    }

    #[test]
    fn ratio_limit() {
        let limits = Limits {
            check_free_space: false,
            ..Limits::default()
        };
        let dir = Path::new(".");
        assert!(
            check_expansion(10 << 20, 1, dir, &limits).is_ok(),
            "small totals are not checked"
        );
        assert!(check_expansion(500 << 20, 1 << 20, dir, &limits).is_ok());
        assert!(matches!(
            check_expansion(1 << 40, 1 << 20, dir, &limits),
            Err(Error::SuspiciousRatio { .. })
        ));
        let off = Limits {
            max_ratio: None,
            ..limits
        };
        assert!(check_expansion(1 << 40, 1 << 20, dir, &off).is_ok());
    }

    #[test]
    fn modes_drop_special_bits() {
        assert_eq!(file_mode(Some(0o4755)), 0o755);
        assert_eq!(file_mode(Some(0o000)), 0o600);
        assert_eq!(dir_mode(Some(0o1777)), 0o777);
        assert_eq!(dir_mode(Some(0o500)), 0o700);
    }
}
