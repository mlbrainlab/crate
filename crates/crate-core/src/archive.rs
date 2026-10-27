//! The public `Archive` type: open, list, extract, test. Extraction is the
//! same for every format: plan with `safety`, check limits, write into a
//! staging folder next to the destination, then move the result into place.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::encoding::{NameCodec, NameEncoding, decode_names};
use crate::entry::{Entry, EntryKind, Format};
use crate::error::{Error, Result};
use crate::formats::{self, FormatReader, RawEntry, RawKind};
use crate::progress::{CancelToken, Progress};
use crate::safety::{self, ConflictResolver, IfExists, Limits, Resolution, SkipReason, Skipped};

/// Options for opening an archive.
#[derive(Clone, Debug, Default)]
pub struct OpenOptions {
    /// Decode non-UTF-8 entry names with this encoding instead of guessing.
    pub encoding: Option<NameCodec>,
    /// Needed to open archives whose file list is encrypted (7z, RAR); used
    /// for encrypted entries afterwards too. See [`Archive::set_password`].
    pub password: Option<String>,
    /// Parts of a multi-volume set that the user located in another folder
    /// (see [`crate::volumes::VolumeSet::find_with`]).
    pub volumes: Vec<PathBuf>,
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
    /// True when the entry at `path` is part of this selection.
    pub fn includes(&self, path: &str) -> bool {
        match self {
            Selection::All => true,
            Selection::Paths(paths) => {
                let path = path.trim_start_matches("./").trim_end_matches('/');
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

#[derive(Clone, Debug)]
pub struct ExtractOptions {
    pub if_exists: IfExists,
    pub limits: Limits,
    /// False: every file goes straight into the destination, without its
    /// folders; clashing names get `name (2)`.
    pub keep_structure: bool,
}

impl Default for ExtractOptions {
    fn default() -> Self {
        Self {
            if_exists: IfExists::default(),
            limits: Limits::default(),
            keep_structure: true,
        }
    }
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

/// How far a slow listing (tar) has got.
#[derive(Clone, Copy, Debug, Default)]
pub struct ListProgress {
    pub entries: usize,
    /// Bytes of the archive file read so far, of `bytes_total`.
    pub bytes_read: u64,
    pub bytes_total: u64,
}

/// An open archive. Listing happens on open; the archive file stays open
/// for later extraction. `Send`, so it can move to a worker thread.
pub struct Archive {
    path: PathBuf,
    file_size: u64,
    reader: Box<dyn FormatReader>,
    raw: Arc<Vec<RawEntry>>,
    entries: Arc<Vec<Entry>>,
    names: Option<NameEncoding>,
    has_password: bool,
    complete: bool,
}

impl Archive {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with(path, &OpenOptions::default())
    }

    pub fn open_with(path: impl AsRef<Path>, options: &OpenOptions) -> Result<Self> {
        Self::open_streaming(path, options, &mut |_, _| {}, &CancelToken::new())
    }

    /// Opens and lists, handing out entries as they are read for formats
    /// without an index (tar), so a UI can show a huge listing while it
    /// loads. Batch entries have provisional names (UTF-8); the finished
    /// archive's `entries()` are authoritative. Cancelling stops the listing
    /// and returns the archive with the entries read so far
    /// ([`Archive::is_complete`] is then false); they can still be extracted.
    pub fn open_streaming(
        path: impl AsRef<Path>,
        options: &OpenOptions,
        on_batch: &mut dyn FnMut(&[Entry], ListProgress),
        cancel: &CancelToken,
    ) -> Result<Self> {
        let path = path.as_ref();
        let password = options.password.as_deref();
        let file_size = match crate::volumes::VolumeSet::find_with(path, &options.volumes) {
            Some(set) => set.size(),
            None => fs::metadata(path)?.len(),
        };
        let mut reader = formats::open(path, password, &options.volumes)?;
        reader.set_password(password);
        let mut partial: Vec<RawEntry> = Vec::new();
        let listed = reader.list(&mut |batch, bytes_read| {
            let start = partial.len();
            partial.extend_from_slice(batch);
            let entries: Vec<Entry> = batch
                .iter()
                .enumerate()
                .map(|(i, raw)| {
                    make_entry(
                        start + i,
                        raw,
                        String::from_utf8_lossy(&raw.name).into_owned(),
                        NameCodec::Utf8,
                    )
                })
                .collect();
            on_batch(
                &entries,
                ListProgress {
                    entries: partial.len(),
                    bytes_read,
                    bytes_total: file_size,
                },
            );
            if cancel.is_cancelled() {
                Err(Error::Cancelled)
            } else {
                Ok(())
            }
        });
        let (raw, complete) = match listed {
            Ok(raw) => (raw, true),
            Err(Error::Cancelled) => (partial, false),
            Err(e) => return Err(e),
        };
        let mut archive = Self {
            path: path.to_owned(),
            file_size,
            reader,
            raw: Arc::new(raw),
            entries: Arc::default(),
            names: None,
            has_password: password.is_some(),
            complete,
        };
        archive.decode_names(options.encoding);
        Ok(archive)
    }

    /// False when the listing was stopped early: `entries()` holds only the
    /// first part of the archive.
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// Another handle to the same archive, sharing the listing: reads on the
    /// copy (preview, thumbnails) don't wait for the original.
    pub fn try_clone(&self) -> Result<Self> {
        Ok(Self {
            path: self.path.clone(),
            file_size: self.file_size,
            reader: self.reader.reopen()?,
            raw: self.raw.clone(),
            entries: self.entries.clone(),
            names: self.names,
            has_password: self.has_password,
            complete: self.complete,
        })
    }

    /// Reads up to `limit` bytes of each wanted file entry into memory, in
    /// archive order, without writing anything to disk. Entries that aren't
    /// files are skipped. Stops reading the archive once all are done.
    pub fn read_entries(
        &mut self,
        wanted: &[usize],
        limit: u64,
        each: &mut dyn FnMut(usize, Vec<u8>),
        cancel: &CancelToken,
    ) -> Result<()> {
        let mut left: HashSet<usize> = wanted
            .iter()
            .copied()
            .filter(|&i| {
                self.entries
                    .get(i)
                    .is_some_and(|e| e.kind == EntryKind::File)
            })
            .collect();
        if left.is_empty() {
            return Ok(());
        }
        if !self.has_password && left.iter().any(|&i| self.entries[i].encrypted) {
            return Err(Error::PasswordRequired);
        }
        let entries = self.entries.clone();
        let wanted_set = left.clone();
        let result = self
            .reader
            .read(&|i| wanted_set.contains(&i), &mut |i, data| {
                if cancel.is_cancelled() {
                    return Err(Error::Cancelled);
                }
                let entry = &entries[i];
                let mut buf = Vec::new();
                data.take(limit)
                    .read_to_end(&mut buf)
                    .map_err(|e| password_error(e.into(), entry.encrypted))?;
                each(i, buf);
                left.remove(&i);
                if left.is_empty() {
                    // Everything is in: stop reading (a sentinel, undone below).
                    return Err(Error::Cancelled);
                }
                Ok(())
            });
        match result {
            Err(Error::Cancelled) if !cancel.is_cancelled() => Ok(()),
            other => other,
        }
    }

    /// Calls `each` for every entry in archive order, with its data
    /// (folders and links get an empty reader). Encrypted entries need the
    /// password. For converting to another format.
    pub fn for_each_entry(
        &mut self,
        each: &mut dyn FnMut(&Entry, &mut dyn Read) -> Result<()>,
        cancel: &CancelToken,
    ) -> Result<()> {
        if !self.has_password && self.entries.iter().any(|e| e.encrypted && !e.is_dir()) {
            return Err(Error::PasswordRequired);
        }
        let entries = self.entries.clone();
        self.reader.read(&|_| true, &mut |i, data| {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let entry = &entries[i];
            each(entry, data).map_err(|e| password_error(e, entry.encrypted))
        })
    }

    /// Reads up to `limit` bytes of one file entry into memory.
    pub fn read_entry(
        &mut self,
        index: usize,
        limit: u64,
        cancel: &CancelToken,
    ) -> Result<Vec<u8>> {
        let mut out = None;
        self.read_entries(&[index], limit, &mut |_, data| out = Some(data), cancel)?;
        out.ok_or_else(|| Error::Corrupt("entry not found".into()))
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
        self.entries = Arc::new(
            self.raw
                .iter()
                .zip(names)
                .enumerate()
                .map(|(index, (raw, path))| make_entry(index, raw, path, codec))
                .collect(),
        );
    }

    /// Sets the password for encrypted entries. Without one, extraction and
    /// testing skip encrypted entries and report them.
    pub fn set_password(&mut self, password: Option<&str>) {
        self.reader.set_password(password);
        self.has_password = password.is_some();
    }

    pub fn has_password(&self) -> bool {
        self.has_password
    }

    /// True when any file entry is encrypted.
    pub fn has_encrypted_entries(&self) -> bool {
        self.entries.iter().any(|e| e.encrypted && !e.is_dir())
    }

    /// Tries `password` on the smallest encrypted file (the whole file, so
    /// the checksum is verified) and keeps it when it works. True also when
    /// nothing is encrypted.
    pub fn check_password(&mut self, password: &str, cancel: &CancelToken) -> Result<bool> {
        let Some(probe) = self
            .entries
            .iter()
            .filter(|e| e.encrypted && e.kind == EntryKind::File)
            .min_by_key(|e| e.size)
            .map(|e| e.index)
        else {
            return Ok(true);
        };
        let previous = self.has_password;
        self.reader.set_password(Some(password));
        let Archive {
            reader, entries, ..
        } = self;
        let result = reader.read(&|i| i == probe, &mut |i, data| {
            let entry = &entries[i];
            let n =
                safety::copy_limited(data, &mut io::sink(), entry.size, &entry.path, &mut |_| {
                    if cancel.is_cancelled() {
                        Err(Error::Cancelled)
                    } else {
                        Ok(())
                    }
                })
                .map_err(|e| password_error(e, true))?;
            if n == entry.size {
                Ok(())
            } else {
                Err(Error::WrongPassword)
            }
        });
        match result {
            Ok(()) => {
                self.has_password = true;
                Ok(true)
            }
            Err(e) => {
                if !previous {
                    self.reader.set_password(None);
                }
                match e {
                    Error::WrongPassword => Ok(false),
                    other => Err(other),
                }
            }
        }
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

    /// The entries, shared rather than copied (for a UI that keeps them).
    pub fn shared_entries(&self) -> Arc<Vec<Entry>> {
        self.entries.clone()
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
        self.extract_with(
            selection,
            dest,
            options,
            progress,
            &mut |_| Resolution::Skip,
            cancel,
        )
    }

    /// Like [`Archive::extract`], with `resolve` deciding each name clash in
    /// the destination when `options.if_exists` is [`IfExists::Ask`]. It is
    /// called at the end, after everything is extracted into staging.
    pub fn extract_with(
        &mut self,
        selection: &Selection,
        dest: &Path,
        options: &ExtractOptions,
        progress: &mut dyn FnMut(&Progress),
        resolve: &mut ConflictResolver<'_>,
        cancel: &CancelToken,
    ) -> Result<ExtractReport> {
        let mut report = ExtractReport::default();
        let plan = Plan::new(
            &self.entries,
            selection,
            self.has_password,
            options.keep_structure,
            &mut report.skipped,
        );
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
            let encrypted = entry.encrypted;
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
                })
                .map_err(|e| password_error(e, encrypted))?;
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
        staging.commit(dest, options.if_exists, resolve, &mut report.skipped)?;
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
        for e in self.entries.iter() {
            if e.kind != EntryKind::File {
                continue;
            }
            if e.encrypted && !self.has_password {
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
                Err(e) => report.failed.push((
                    entry.path.clone(),
                    password_error(e, entry.encrypted).to_string(),
                )),
            }
            prog.files_done += 1;
            Ok(())
        })?;
        progress(&prog);
        Ok(report)
    }
}

/// `a.txt`, then `a (2).txt`, `a (3).txt`… (case-insensitively unique, so
/// flattened files don't clash on case-insensitive disks either).
fn unique_name(used: &mut HashSet<String>, name: &str) -> String {
    let (stem, ext) = safety::split_extension(name);
    let mut candidate = name.to_owned();
    let mut n = 2;
    while !used.insert(candidate.to_lowercase()) {
        candidate = format!("{stem} ({n}){ext}");
        n += 1;
    }
    candidate
}

fn make_entry(index: usize, raw: &RawEntry, path: String, codec: NameCodec) -> Entry {
    Entry {
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
        method: raw.method.clone(),
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
    fn new(
        entries: &[Entry],
        selection: &Selection,
        has_password: bool,
        keep_structure: bool,
        skipped: &mut Vec<Skipped>,
    ) -> Self {
        let mut flat_names: HashSet<String> = HashSet::new();
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
            // Without folders: just the file name, made unique.
            let rel = if keep_structure {
                rel
            } else if e.kind == EntryKind::Dir || rel.file_name().is_none() {
                continue;
            } else {
                let name = rel
                    .file_name()
                    .expect("checked")
                    .to_string_lossy()
                    .into_owned();
                PathBuf::from(unique_name(&mut flat_names, &name))
            };
            if safety::passes_through_link(&rel, &link_paths) {
                skipped.push(skip(e, SkipReason::ThroughLink));
                continue;
            }
            if e.encrypted && !has_password && e.kind != EntryKind::Dir {
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

    fn commit(
        self,
        dest: &Path,
        if_exists: IfExists,
        resolve: &mut ConflictResolver<'_>,
        skipped: &mut Vec<Skipped>,
    ) -> Result<()> {
        if self.dest_exists {
            // Leftovers (items skipped as existing) go when `self.dir` drops.
            safety::commit(self.dir.path(), dest, if_exists, resolve, skipped)
        } else {
            let path = self.dir.keep();
            fs::rename(&path, dest).inspect_err(|_| {
                let _ = fs::remove_dir_all(&path);
            })?;
            Ok(())
        }
    }
}

/// Bad data in an encrypted entry almost always means a wrong password
/// (ZipCrypto's check byte lets 1 in 256 wrong passwords through, and the
/// checksum catches them later).
fn password_error(e: Error, encrypted: bool) -> Error {
    match e {
        Error::Corrupt(_) if encrypted => Error::WrongPassword,
        Error::Io(e) if encrypted && e.kind() == io::ErrorKind::InvalidData => Error::WrongPassword,
        other => other,
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
