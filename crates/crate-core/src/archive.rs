//! The public `Archive` type: open, list, extract, test. Extraction is the
//! same for every format: plan with `safety`, check limits, write into a
//! staging folder next to the destination, then move the result into place.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use crate::encoding::{NameCodec, NameEncoding, decode_names};
use crate::entry::{Entry, EntryKind, Format};
use crate::error::{Error, Result};
use crate::formats::{self, FormatReader, RawEntry, RawKind};
use crate::progress::{CancelToken, Progress};
use crate::safety::{self, IfExists, Limits, SkipReason, Skipped};

/// Options for opening an archive.
#[derive(Clone, Debug, Default)]
pub struct OpenOptions {
    /// Decode non-UTF-8 entry names with this encoding instead of guessing.
    pub encoding: Option<NameCodec>,
}

/// Which entries to extract.
#[derive(Clone, Debug, Default)]
pub enum Selection {
    #[default]
    All,
    /// Entry paths as listed; a folder includes everything inside it.
    Paths(Vec<String>),
}

impl Selection {
    fn includes(&self, path: &str) -> bool {
        match self {
            Selection::All => true,
            Selection::Paths(paths) => {
                let path = path.trim_end_matches('/');
                paths.iter().any(|p| {
                    let p = p.trim_end_matches('/');
                    path == p
                        || path
                            .strip_prefix(p)
                            .is_some_and(|rest| rest.starts_with('/'))
                })
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ExtractOptions {
    pub if_exists: IfExists,
    pub limits: Limits,
}

/// What an extraction did.
#[derive(Clone, Debug, Default)]
pub struct ExtractReport {
    pub files: u64,
    pub dirs: u64,
    pub links: u64,
    pub bytes: u64,
    /// Entries left out, and why ("2 items were skipped for your safety").
    pub skipped: Vec<Skipped>,
}

/// Result of an integrity test.
#[derive(Clone, Debug, Default)]
pub struct TestReport {
    pub files: u64,
    pub bytes: u64,
    /// Entries that failed, with the error.
    pub failed: Vec<(String, String)>,
    /// Entries that couldn't be tested (e.g. encrypted).
    pub skipped: Vec<Skipped>,
}

impl TestReport {
    pub fn is_ok(&self) -> bool {
        self.failed.is_empty()
    }
}

/// An open archive. Listing happens on open; the archive file stays open
/// for later extraction. `Send`, so it can move to a worker thread.
pub struct Archive {
    path: PathBuf,
    file_size: u64,
    reader: Box<dyn FormatReader>,
    raw: Vec<RawEntry>,
    entries: Vec<Entry>,
    names: Option<NameEncoding>,
}

impl Archive {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with(path, &OpenOptions::default())
    }

    pub fn open_with(path: impl AsRef<Path>, options: &OpenOptions) -> Result<Self> {
        let path = path.as_ref();
        let mut reader = formats::open(path)?;
        let raw = reader.list()?;
        let file_size = fs::metadata(path)?.len();
        let mut archive = Self {
            path: path.to_owned(),
            file_size,
            reader,
            raw,
            entries: Vec::new(),
            names: None,
        };
        archive.decode_names(options.encoding);
        Ok(archive)
    }

    /// Re-decodes entry names with another encoding (`None`: guess again).
    pub fn set_name_encoding(&mut self, codec: Option<NameCodec>) {
        self.decode_names(codec);
    }

    fn decode_names(&mut self, forced: Option<NameCodec>) {
        let raw_names: Vec<&[u8]> = self.raw.iter().map(|e| e.name.as_slice()).collect();
        let (names, encoding) = decode_names(&raw_names, forced);
        let codec = encoding.map_or(NameCodec::Utf8, |e| e.codec);
        self.names = encoding;
        self.entries = self
            .raw
            .iter()
            .zip(names)
            .enumerate()
            .map(|(index, (raw, path))| Entry {
                index,
                path,
                kind: match &raw.kind {
                    RawKind::File => EntryKind::File,
                    RawKind::Dir => EntryKind::Dir,
                    RawKind::Symlink(t) => EntryKind::Symlink {
                        target: codec.decode(t),
                    },
                    RawKind::Hardlink(t) => EntryKind::Hardlink {
                        target: codec.decode(t),
                    },
                    RawKind::Other => EntryKind::Other,
                },
                size: raw.size,
                packed_size: raw.packed_size,
                modified: raw.modified,
                mode: raw.mode,
                crc32: raw.crc32,
                encrypted: raw.encrypted,
            })
            .collect();
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn format(&self) -> Format {
        self.reader.format()
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// How non-UTF-8 names were decoded; `None` when all names were UTF-8.
    pub fn name_encoding(&self) -> Option<NameEncoding> {
        self.names
    }

    /// Size of the archive file in bytes.
    pub fn file_size(&self) -> u64 {
        self.file_size
    }

    /// Extracts the selected entries into `dest` (created if needed). Unsafe
    /// entries are skipped and listed in the report; limits, I/O errors and
    /// cancellation stop the whole operation and leave `dest` untouched.
    pub fn extract(
        &mut self,
        selection: &Selection,
        dest: &Path,
        options: &ExtractOptions,
        progress: &mut dyn FnMut(&Progress),
        cancel: &CancelToken,
    ) -> Result<ExtractReport> {
        let mut report = ExtractReport::default();
        let plan = Plan::new(&self.entries, selection, &mut report.skipped);
        safety::check_expansion(plan.bytes, self.file_size, dest, &options.limits)?;
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }

        let staging = Staging::new(dest)?;
        let root = staging.path();

        for (_, rel) in &plan.dirs {
            match fs::create_dir_all(root.join(rel)) {
                Ok(()) => report.dirs += 1,
                Err(e) if is_conflict(&e) => {
                    report.skipped.push(skip_path(rel, SkipReason::Conflict))
                }
                Err(e) => return Err(e.into()),
            }
        }

        let mut prog = Progress {
            bytes_total: plan.bytes,
            files_total: plan.files.len() as u64,
            ..Progress::default()
        };
        let Archive {
            reader, entries, ..
        } = self;
        let files = &plan.files;
        reader.read(&|i| files.contains_key(&i), &mut |i, data| {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let entry = &entries[i];
            let target = root.join(&files[&i]);
            prog.current.clone_from(&entry.path);
            progress(&prog);
            if let Some(parent) = target.parent()
                && let Err(e) = fs::create_dir_all(parent)
            {
                if is_conflict(&e) {
                    report.skipped.push(skip(entry, SkipReason::Conflict));
                    return Ok(());
                }
                return Err(e.into());
            }
            if target.is_dir() {
                report.skipped.push(skip(entry, SkipReason::Conflict));
                return Ok(());
            }
            let mut file = File::create(&target)?;
            let mut since_report = 0u64;
            let written =
                safety::copy_limited(data, &mut file, entry.size, &entry.path, &mut |n| {
                    prog.bytes_done += n;
                    since_report += n;
                    if cancel.is_cancelled() {
                        return Err(Error::Cancelled);
                    }
                    if since_report >= 1 << 20 {
                        since_report = 0;
                        progress(&prog);
                    }
                    Ok(())
                })?;
            if let Some(t) = entry.modified {
                file.set_modified(t)?;
            }
            set_mode(&target, safety::file_mode(entry.mode))?;
            report.files += 1;
            report.bytes += written;
            prog.files_done += 1;
            Ok(())
        })?;
        progress(&prog);

        for (i, rel, target) in &plan.symlinks {
            match make_symlink(root, rel, target) {
                Ok(()) => report.links += 1,
                Err(reason) => report.skipped.push(skip(&entries[*i], reason)),
            }
        }
        for (i, rel, target) in &plan.hardlinks {
            let source = root.join(target);
            let ok = fs::symlink_metadata(&source).is_ok_and(|m| m.is_file())
                && fs::symlink_metadata(root.join(rel)).is_err()
                && rel
                    .parent()
                    .is_none_or(|p| fs::create_dir_all(root.join(p)).is_ok())
                && fs::copy(&source, root.join(rel)).is_ok();
            if ok {
                report.links += 1;
            } else {
                report
                    .skipped
                    .push(skip(&entries[*i], SkipReason::LinkEscape));
            }
        }
        // Folder permissions last, so a read-only folder can still be filled.
        for (i, rel) in &plan.dirs {
            if let Some(mode) = entries[*i].mode {
                let _ = set_mode(&root.join(rel), safety::dir_mode(Some(mode)));
            }
        }

        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        staging.commit(dest, options.if_exists, &mut report.skipped)?;
        Ok(report)
    }

    /// Reads every file and checks its integrity (CRC where the format has
    /// one). Nothing is written.
    pub fn test(
        &mut self,
        progress: &mut dyn FnMut(&Progress),
        cancel: &CancelToken,
    ) -> Result<TestReport> {
        let mut report = TestReport::default();
        let mut wanted = HashSet::new();
        let mut prog = Progress::default();
        for e in &self.entries {
            if e.kind != EntryKind::File {
                continue;
            }
            if e.encrypted {
                report.skipped.push(skip(e, SkipReason::Encrypted));
                continue;
            }
            wanted.insert(e.index);
            prog.bytes_total += e.size;
            prog.files_total += 1;
        }
        let Archive {
            reader, entries, ..
        } = self;
        reader.read(&|i| wanted.contains(&i), &mut |i, data| {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let entry = &entries[i];
            prog.current.clone_from(&entry.path);
            progress(&prog);
            let result =
                safety::copy_limited(data, &mut io::sink(), entry.size, &entry.path, &mut |n| {
                    prog.bytes_done += n;
                    if cancel.is_cancelled() {
                        Err(Error::Cancelled)
                    } else {
                        Ok(())
                    }
                });
            match result {
                Ok(n) if n == entry.size => {
                    report.files += 1;
                    report.bytes += n;
                }
                Ok(n) => report
                    .failed
                    .push((entry.path.clone(), format!("{n} of {} bytes", entry.size))),
                Err(Error::Cancelled) => return Err(Error::Cancelled),
                Err(e) => report.failed.push((entry.path.clone(), e.to_string())),
            }
            prog.files_done += 1;
            Ok(())
        })?;
        progress(&prog);
        Ok(report)
    }
}

/// Where each selected entry goes, decided before anything is written.
struct Plan {
    files: HashMap<usize, PathBuf>,
    dirs: Vec<(usize, PathBuf)>,
    symlinks: Vec<(usize, PathBuf, String)>,
    hardlinks: Vec<(usize, PathBuf, PathBuf)>,
    bytes: u64,
}

impl Plan {
    fn new(entries: &[Entry], selection: &Selection, skipped: &mut Vec<Skipped>) -> Self {
        let selected: Vec<&Entry> = entries
            .iter()
            .filter(|e| selection.includes(&e.path))
            .collect();
        // Every symlink path in the selection, valid target or not: nothing
        // is ever written underneath one.
        let link_paths: HashSet<PathBuf> = selected
            .iter()
            .filter(|e| matches!(e.kind, EntryKind::Symlink { .. }))
            .filter_map(|e| safety::safe_relative_path(&e.path).ok())
            .collect();
        let mut plan = Plan {
            files: HashMap::new(),
            dirs: Vec::new(),
            symlinks: Vec::new(),
            hardlinks: Vec::new(),
            bytes: 0,
        };
        for e in selected {
            let rel = match safety::safe_relative_path(&e.path) {
                Ok(rel) => rel,
                Err(reason) => {
                    skipped.push(skip(e, reason));
                    continue;
                }
            };
            if safety::passes_through_link(&rel, &link_paths) {
                skipped.push(skip(e, SkipReason::ThroughLink));
                continue;
            }
            if e.encrypted && e.kind != EntryKind::Dir {
                skipped.push(skip(e, SkipReason::Encrypted));
                continue;
            }
            match &e.kind {
                EntryKind::File => {
                    plan.bytes = plan.bytes.saturating_add(e.size);
                    plan.files.insert(e.index, rel);
                }
                EntryKind::Dir => plan.dirs.push((e.index, rel)),
                EntryKind::Symlink { target } if safety::link_target_is_inside(&rel, target) => {
                    plan.symlinks
                        .push((e.index, rel, target.replace('\\', "/")))
                }
                EntryKind::Symlink { .. } => skipped.push(skip(e, SkipReason::LinkEscape)),
                EntryKind::Hardlink { target } => match safety::hardlink_target(target) {
                    Ok(t)
                        if !safety::passes_through_link(&t, &link_paths)
                            && !link_paths.contains(&t) =>
                    {
                        plan.hardlinks.push((e.index, rel, t))
                    }
                    _ => skipped.push(skip(e, SkipReason::LinkEscape)),
                },
                EntryKind::Other => skipped.push(skip(e, SkipReason::UnsupportedType)),
            }
        }
        plan
    }
}

/// A hidden temporary folder on the same filesystem as the destination,
/// removed automatically unless the extraction commits.
struct Staging {
    dir: tempfile::TempDir,
    dest_exists: bool,
}

impl Staging {
    fn new(dest: &Path) -> Result<Self> {
        let dest_exists = dest.is_dir();
        let parent = if dest_exists {
            dest.to_owned()
        } else {
            let parent = match dest.parent() {
                Some(p) if !p.as_os_str().is_empty() => p.to_owned(),
                _ => PathBuf::from("."),
            };
            fs::create_dir_all(&parent)?;
            parent
        };
        let dir = tempfile::Builder::new()
            .prefix(".crate-extract-")
            .tempdir_in(parent)?;
        Ok(Self { dir, dest_exists })
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn commit(self, dest: &Path, if_exists: IfExists, skipped: &mut Vec<Skipped>) -> Result<()> {
        if self.dest_exists {
            // Leftovers (items skipped as existing) go when `self.dir` drops.
            safety::commit(self.dir.path(), dest, if_exists, skipped)
        } else {
            let path = self.dir.keep();
            fs::rename(&path, dest).inspect_err(|_| {
                let _ = fs::remove_dir_all(&path);
            })?;
            Ok(())
        }
    }
}

fn is_conflict(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::AlreadyExists | io::ErrorKind::NotADirectory | io::ErrorKind::IsADirectory
    )
}

fn skip(entry: &Entry, reason: SkipReason) -> Skipped {
    Skipped {
        path: entry.path.clone(),
        reason,
    }
}

fn skip_path(rel: &Path, reason: SkipReason) -> Skipped {
    Skipped {
        path: rel.to_string_lossy().into_owned(),
        reason,
    }
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn make_symlink(root: &Path, rel: &Path, target: &str) -> std::result::Result<(), SkipReason> {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| SkipReason::Conflict)?;
    }
    if fs::symlink_metadata(&path).is_ok() {
        return Err(SkipReason::Conflict);
    }
    std::os::unix::fs::symlink(target, &path).map_err(|_| SkipReason::Conflict)
}

#[cfg(not(unix))]
fn make_symlink(_root: &Path, _rel: &Path, _target: &str) -> std::result::Result<(), SkipReason> {
    Err(SkipReason::NotSupportedOnPlatform)
}
