//! Changing an existing archive: add files, new folders, rename, delete,
//! replace a file with an edited copy. The original is never modified: the
//! new archive is written next to it, flushed to disk and renamed over it
//! (optionally keeping the old one as `name.bak`). A crash or a cancel at
//! any point leaves the original as it was.
//!
//! What can be copied as is, is: ZIP entries are copied raw (no
//! recompression, no password needed), 7z blocks without changes too. Tar
//! has to be rewritten as a whole, through the same compression (and age).

use std::cell::RefCell;
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::archive::{Archive, OpenOptions};
use crate::create::{self, AtomicFile, CreateOptions, Item, ItemKind, OutFormat, Tracker};
use crate::entry::{Entry, EntryKind, Format};
use crate::error::{Error, Result};
use crate::progress::{CancelToken, Progress};
use crate::safety;

/// One change to make. Paths are inside the archive, `/`-separated.
#[derive(Clone, Debug)]
pub enum Change {
    /// Files and folders from disk, into a folder of the archive (`""`: the
    /// top). An item with the same path as an existing file replaces it.
    Add {
        sources: Vec<PathBuf>,
        into: String,
    },
    NewFolder(String),
    /// A file or a folder (with everything inside).
    Rename {
        from: String,
        to: String,
    },
    /// A file or a folder (with everything inside).
    Delete(String),
    /// Put the file on disk in place of an entry (open → edit → update).
    Replace {
        path: String,
        source: PathBuf,
    },
}

#[derive(Clone, Debug)]
pub struct EditOptions {
    /// The archive's password: needed to open archives with hidden names
    /// (7z, age), to rewrite encrypted 7z blocks and tar, and to encrypt
    /// files added to an encrypted archive.
    pub password: Option<String>,
    /// Keep the previous version as `name.bak` next to it.
    pub backup: bool,
    pub keep_permissions: bool,
    /// Patterns left out of added folders.
    pub exclude: Vec<String>,
}

impl Default for EditOptions {
    fn default() -> Self {
        Self {
            password: None,
            backup: false,
            keep_permissions: true,
            exclude: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct EditReport {
    pub kept: u64,
    pub renamed: u64,
    pub deleted: u64,
    pub replaced: u64,
    pub added: u64,
    pub backup: Option<PathBuf>,
}

/// What happens to an existing entry.
#[derive(Clone, Debug)]
enum Fate {
    Keep,
    Rename(String),
    Drop,
    Replace(Item),
}

/// Whether Crate can change archives of this format (RAR is read-only).
pub fn can_edit(format: Format) -> bool {
    !matches!(format, Format::Rar)
}

/// Applies `changes` to the archive at `path`.
pub fn apply(
    path: &Path,
    changes: &[Change],
    options: &EditOptions,
    progress: &mut dyn FnMut(&Progress),
    cancel: &CancelToken,
) -> Result<EditReport> {
    let archive = Archive::open_with(
        path,
        &OpenOptions {
            password: options.password.clone(),
            ..OpenOptions::default()
        },
    )?;
    let format = archive.format();
    if !can_edit(format) {
        return Err(Error::ReadOnly(format.name()));
    }
    // Rewriting would have to re-split it; save a copy instead.
    if crate::volumes::VolumeSet::find(path).is_some() {
        return Err(Error::ReadOnly("Multi-volume"));
    }
    let entries = archive.entries().to_vec();
    let encrypted = entries.iter().any(|e| e.encrypted);
    drop(archive);

    let plan = Plan::new(path, &entries, changes, options)?;
    let mut report = EditReport::default();
    for fate in &plan.fates {
        match fate {
            Fate::Keep => report.kept += 1,
            Fate::Rename(_) => report.renamed += 1,
            Fate::Drop => report.deleted += 1,
            Fate::Replace(_) => report.replaced += 1,
        }
    }
    // Entries dropped because an added file takes their place count as replaced.
    report.deleted -= plan.overwritten;
    report.replaced += plan.overwritten;
    report.added = plan.added.len() as u64 - plan.overwritten;

    let bytes_total = entries
        .iter()
        .zip(&plan.fates)
        .filter(|(_, f)| !matches!(f, Fate::Drop))
        .map(|(e, _)| e.size)
        .sum::<u64>()
        + plan.added.iter().map(|i| i.size).sum::<u64>();
    let tracker = RefCell::new(Tracker::new(
        Progress {
            bytes_total,
            ..Progress::default()
        },
        progress,
        cancel,
    ));
    let mut out = AtomicFile::new(path)?;
    let written = match format {
        Format::Zip => write_zip(
            path,
            out.file(),
            &entries,
            &plan,
            options,
            encrypted,
            &tracker,
        ),
        Format::SevenZ => write_7z(path, out.file(), &plan, options, &tracker),
        _ => write_tar(path, format, out.file(), &entries, &plan, options, &tracker),
    };
    written.map_err(|e| tracker.borrow().cancelled_or(e))?;
    tracker.borrow().check()?;

    if options.backup {
        report.backup = Some(backup(path)?);
    }
    out.commit()?;
    Ok(report)
}

/// Keeps the current version as `name.bak` (replacing an older backup).
fn backup(path: &Path) -> Result<PathBuf> {
    let mut name = path.file_name().unwrap_or_default().to_owned();
    name.push(".bak");
    let bak = path.with_file_name(name);
    let _ = fs::remove_file(&bak);
    // A hard link costs nothing; filesystems without them get a copy.
    if fs::hard_link(path, &bak).is_err() {
        fs::copy(path, &bak)?;
    }
    Ok(bak)
}

/// Every existing entry's fate, and what to add, decided before writing.
struct Plan {
    fates: Vec<Fate>,
    new_folders: Vec<String>,
    added: Vec<Item>,
    /// Existing files that added files replace (also counted in `added`).
    overwritten: u64,
}

fn clean(path: &str) -> String {
    path.trim_start_matches("./").trim_matches('/').to_owned()
}

fn is_under(path: &str, folder: &str) -> bool {
    path == folder
        || path
            .strip_prefix(folder)
            .is_some_and(|r| r.starts_with('/'))
}

/// A name that is safe to write into an archive, or an error.
fn checked(name: &str) -> Result<String> {
    let name = clean(name);
    if name.is_empty() || safety::safe_relative_path(&name).is_err() {
        return Err(Error::BadName(name));
    }
    Ok(name)
}

impl Plan {
    fn new(
        archive: &Path,
        entries: &[Entry],
        changes: &[Change],
        options: &EditOptions,
    ) -> Result<Self> {
        let paths: Vec<String> = entries.iter().map(|e| clean(&e.path)).collect();
        let exists = |p: &str| paths.iter().any(|q| is_under(q, p));
        let mut fates = vec![Fate::Keep; entries.len()];
        let mut names = paths.clone();
        let mut plan = Plan {
            fates: Vec::new(),
            new_folders: Vec::new(),
            added: Vec::new(),
            overwritten: 0,
        };
        for change in changes {
            match change {
                Change::Delete(p) => {
                    let p = clean(p);
                    if !exists(&p) {
                        return Err(Error::NotInArchive(p));
                    }
                    for (i, q) in paths.iter().enumerate() {
                        if is_under(q, &p) {
                            fates[i] = Fate::Drop;
                        }
                    }
                }
                Change::Rename { from, to } => {
                    let (from, to) = (clean(from), checked(to)?);
                    if !exists(&from) {
                        return Err(Error::NotInArchive(from));
                    }
                    let taken = names.iter().enumerate().any(|(i, n)| {
                        is_under(n, &to)
                            && !matches!(fates[i], Fate::Drop)
                            && !is_under(&paths[i], &from)
                    });
                    if taken {
                        return Err(Error::NameTaken(to));
                    }
                    for (i, q) in paths.iter().enumerate() {
                        if is_under(q, &from) && !matches!(fates[i], Fate::Drop) {
                            let new = format!("{to}{}", &q[from.len()..]);
                            names[i] = new.clone();
                            fates[i] = Fate::Rename(new);
                        }
                    }
                }
                Change::Replace { path, source } => {
                    let p = clean(path);
                    let i = paths
                        .iter()
                        .position(|q| *q == p)
                        .filter(|&i| entries[i].kind == EntryKind::File)
                        .ok_or_else(|| Error::NotInArchive(p.clone()))?;
                    let meta = fs::metadata(source)?;
                    fates[i] = Fate::Replace(Item {
                        source: source.clone(),
                        name: names[i].clone(),
                        kind: ItemKind::File,
                        size: meta.len(),
                        mode: entries[i].mode.unwrap_or(0o644),
                        modified: meta.modified().ok(),
                    });
                }
                Change::NewFolder(p) => {
                    let p = checked(p)?;
                    if !exists(&p) && !plan.new_folders.contains(&p) {
                        plan.new_folders.push(p);
                    }
                }
                Change::Add { sources, into } => {
                    let into = if clean(into).is_empty() {
                        String::new()
                    } else {
                        checked(into)?
                    };
                    let collected = create::collect(
                        sources,
                        &options.exclude,
                        options.keep_permissions,
                        Some(archive),
                    );
                    if let Some((p, why)) = collected.unreadable.first() {
                        return Err(Error::Io(io::Error::other(format!(
                            "{}: {why}",
                            p.display()
                        ))));
                    }
                    for mut item in collected.items {
                        if !into.is_empty() {
                            item.name = format!("{into}/{}", item.name);
                        }
                        // Same path as an existing file: the new one replaces it.
                        if let Some(i) = names.iter().position(|n| *n == item.name) {
                            if item.kind == ItemKind::Dir && entries[i].kind == EntryKind::Dir {
                                continue; // the folder is there already
                            }
                            if !matches!(fates[i], Fate::Drop) {
                                fates[i] = Fate::Drop;
                                plan.overwritten += 1;
                            }
                        }
                        plan.added.push(item);
                    }
                }
            }
        }
        plan.fates = fates;
        Ok(plan)
    }
}

// ----- ZIP: entries are copied raw ---------------------------------------------

fn write_zip(
    path: &Path,
    out: &mut File,
    entries: &[Entry],
    plan: &Plan,
    options: &EditOptions,
    encrypted: bool,
    tracker: &RefCell<Tracker>,
) -> Result<()> {
    let mut zin = crate::formats::open_zip(File::open(path)?)?;
    let mut zout = zip::ZipWriter::new(BufWriter::new(out));
    let add_options = CreateOptions {
        format: OutFormat::Zip,
        level: OutFormat::Zip.levels().2,
        // New files in an encrypted archive are encrypted too.
        password: options.password.clone().filter(|_| encrypted),
        keep_permissions: options.keep_permissions,
        ..CreateOptions::default()
    };
    let zerr = create::zip_error;
    for (i, fate) in plan.fates.iter().enumerate() {
        tracker.borrow_mut().start(&entries[i].path)?;
        match fate {
            Fate::Keep => zout
                .raw_copy_file(zin.by_index_raw(i).map_err(zerr)?)
                .map_err(zerr)?,
            Fate::Rename(name) => {
                let name = if entries[i].is_dir() {
                    format!("{name}/")
                } else {
                    name.clone()
                };
                zout.raw_copy_file_rename(zin.by_index_raw(i).map_err(zerr)?, name)
                    .map_err(zerr)?;
            }
            Fate::Drop => continue,
            Fate::Replace(item) => {
                let options = CreateOptions {
                    password: options.password.clone().filter(|_| entries[i].encrypted),
                    ..add_options.clone()
                };
                create::zip_append(&mut zout, item, &options, tracker)?;
                continue;
            }
        }
        tracker.borrow_mut().advance(entries[i].size)?;
    }
    for folder in &plan.new_folders {
        zout.add_directory(
            format!("{folder}/"),
            zip::write::SimpleFileOptions::default(),
        )
        .map_err(zerr)?;
    }
    for item in &plan.added {
        create::zip_append(&mut zout, item, &add_options, tracker)?;
    }
    if !zin.comment().is_empty() {
        zout.set_raw_comment(zin.comment().into()).map_err(zerr)?;
    }
    zout.finish().map_err(zerr)?.flush()?;
    Ok(())
}

// ----- 7z: unchanged blocks are copied raw -------------------------------------

fn write_7z(
    path: &Path,
    out: &mut File,
    plan: &Plan,
    options: &EditOptions,
    tracker: &RefCell<Tracker>,
) -> Result<()> {
    use sevenz_rust2::encoder_options::{AesEncoderOptions, Lzma2Options};
    use sevenz_rust2::{ArchiveEntry, ArchiveWriter, BlockDecoder, Password};

    let password = options
        .password
        .as_deref()
        .map_or_else(Password::empty, Password::from);
    let mut source = io::BufReader::new(File::open(path)?);
    let archive =
        sevenz_rust2::Archive::read(&mut source, &password).map_err(create::sevenz_error)?;
    let mut w = ArchiveWriter::new(BufWriter::new(out)).map_err(create::sevenz_error)?;
    let mut methods = Vec::new();
    if let Some(pw) = &options.password {
        // Re-encoded and added data is encrypted like the rest, names too.
        methods.push(AesEncoderOptions::new(pw.as_str().into()).into());
        w.set_encrypt_header(true);
    }
    methods.push(Lzma2Options::from_level(OutFormat::SevenZ.levels().2).into());
    w.set_content_methods(methods);

    let files = &archive.files;
    let block_of = &archive.stream_map.file_block_index;
    let renamed = |i: usize, e: &ArchiveEntry| -> ArchiveEntry {
        let mut e = e.clone();
        if let Fate::Rename(name) = &plan.fates[i] {
            e.name = name.clone();
        }
        e
    };
    let (base, size) = (files.as_ptr() as usize, std::mem::size_of::<ArchiveEntry>());
    let index_of = |e: &ArchiveEntry| (e as *const ArchiveEntry as usize - base) / size;

    for block in 0..archive.blocks.len() {
        let members: Vec<usize> = (0..files.len())
            .filter(|&i| block_of[i] == Some(block))
            .collect();
        if members.iter().all(|&i| matches!(plan.fates[i], Fate::Drop)) {
            continue;
        }
        let untouched = members
            .iter()
            .all(|&i| matches!(plan.fates[i], Fate::Keep | Fate::Rename(_)));
        if untouched {
            w.push_raw_block(&archive, block, &mut source, |e| {
                renamed(index_of(e), e).name
            })
            .map_err(create::sevenz_error)?;
            let bytes: u64 = members.iter().map(|&i| files[i].size).sum();
            tracker.borrow_mut().advance(bytes)?;
            continue;
        }
        // Something in this block changes: re-encode what stays.
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get() as u32);
        let decoder = BlockDecoder::new(threads, block, &archive, &password, &mut source);
        let failure = RefCell::new(None);
        decoder
            .for_each_entries(&mut |e, data| {
                let i = index_of(e);
                let result = (|| -> Result<()> {
                    tracker.borrow_mut().start(e.name())?;
                    match &plan.fates[i] {
                        Fate::Drop => {
                            io::copy(data, &mut io::sink())?;
                        }
                        Fate::Replace(item) => {
                            io::copy(data, &mut io::sink())?;
                            push_file(&mut w, renamed(i, e), item, tracker)?;
                        }
                        Fate::Keep | Fate::Rename(_) => {
                            let mut counted = Counted {
                                inner: data,
                                tracker,
                            };
                            w.push_archive_entry(renamed(i, e), Some(&mut counted))
                                .map_err(create::sevenz_error)?;
                        }
                    }
                    Ok(())
                })();
                match result {
                    Ok(()) => Ok(true),
                    Err(err) => {
                        *failure.borrow_mut() = Some(err);
                        Ok(false)
                    }
                }
            })
            .map_err(create::sevenz_error)?;
        if let Some(e) = failure.into_inner() {
            return Err(e);
        }
    }
    // Folders and empty files have no data.
    for i in (0..files.len()).filter(|&i| block_of[i].is_none()) {
        match &plan.fates[i] {
            Fate::Drop => {}
            Fate::Replace(item) => push_file(&mut w, renamed(i, &files[i]), item, tracker)?,
            _ => {
                w.push_archive_entry::<&[u8]>(renamed(i, &files[i]), None)
                    .map_err(create::sevenz_error)?;
            }
        }
    }
    for folder in &plan.new_folders {
        w.push_archive_entry::<&[u8]>(ArchiveEntry::new_directory(folder), None)
            .map_err(create::sevenz_error)?;
    }
    for item in &plan.added {
        let entry = create::sevenz_entry(item, options.keep_permissions);
        match &item.kind {
            ItemKind::File if item.size > 0 => push_file(&mut w, entry, item, tracker)?,
            ItemKind::Symlink(target) => {
                let data = target.to_string_lossy().into_owned().into_bytes();
                w.push_archive_entry(entry, Some(&data[..]))
                    .map_err(create::sevenz_error)?;
            }
            _ => {
                w.push_archive_entry::<&[u8]>(entry, None)
                    .map_err(create::sevenz_error)?;
            }
        }
    }
    w.finish()?.flush()?;
    Ok(())
}

/// A file from disk as a 7z entry (its own block).
fn push_file<W: Write + Seek>(
    w: &mut sevenz_rust2::ArchiveWriter<W>,
    mut entry: sevenz_rust2::ArchiveEntry,
    item: &Item,
    tracker: &RefCell<Tracker>,
) -> Result<()> {
    tracker.borrow_mut().start(&item.name)?;
    // The planned name (a renamed folder moves replaced files too).
    entry.name = item.name.clone();
    if let Some(t) = item
        .modified
        .and_then(|t| sevenz_rust2::NtTime::try_from(t).ok())
    {
        entry.last_modified_date = t;
        entry.has_last_modified_date = true;
    }
    let file = File::open(&item.source)?;
    let mut counted = Counted {
        inner: file,
        tracker,
    };
    if item.size == 0 {
        w.push_archive_entry::<&[u8]>(entry, None)
            .map_err(create::sevenz_error)?;
    } else {
        w.push_archive_entry(entry, Some(&mut counted))
            .map_err(create::sevenz_error)?;
    }
    tracker.borrow_mut().file_done();
    Ok(())
}

/// Counts bytes passing through for the progress bar (and checks cancel).
struct Counted<'a, 'b, R> {
    inner: R,
    tracker: &'a RefCell<Tracker<'b>>,
}

impl<R: Read> Read for Counted<'_, '_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.tracker.borrow_mut().advance(n as u64)?;
        Ok(n)
    }
}

// ----- tar: rewritten through the same compression ----------------------------

fn write_tar(
    path: &Path,
    format: Format,
    out: &mut File,
    entries: &[Entry],
    plan: &Plan,
    options: &EditOptions,
    tracker: &RefCell<Tracker>,
) -> Result<()> {
    let passphrase = options
        .password
        .as_deref()
        .filter(|_| format == Format::TarAge);
    let (mut tin, inner) = crate::formats::tar::open_tar(path, format, passphrase)?;
    let out_format = match inner {
        Format::TarGz => OutFormat::TarGz,
        Format::TarXz => OutFormat::TarXz,
        Format::TarZst => OutFormat::TarZst,
        Format::TarBz2 => OutFormat::TarBz2,
        _ => OutFormat::Tar,
    };
    let level = out_format.levels().2;
    // Errors inside the tar stream are I/O errors; damaged input is Corrupt.
    let read_err = |e: io::Error| Error::Corrupt(e.to_string());
    create::write_tar_stream(out, out_format, level, passphrase, |tar| {
        for (i, entry) in tin.entries().map_err(read_err)?.enumerate() {
            let mut entry = entry.map_err(read_err)?;
            let Some(fate) = plan.fates.get(i) else { break };
            tracker
                .borrow_mut()
                .start(entries.get(i).map_or("", |e| e.path.as_str()))?;
            let name = match fate {
                Fate::Drop => continue,
                Fate::Replace(item) => {
                    create::tar_append(tar, item, true, tracker)?;
                    continue;
                }
                Fate::Rename(name) => name.clone(),
                Fate::Keep => entries[i].path.clone(),
            };
            let mut header = entry.header().clone();
            let kind = header.entry_type();
            if kind.is_symlink() || kind.is_hard_link() {
                let target = entry
                    .link_name()
                    .map_err(read_err)?
                    .unwrap_or_default()
                    .into_owned();
                tar.append_link(&mut header, &name, target)?;
            } else if kind.is_dir() {
                tar.append_data(
                    &mut header,
                    format!("{}/", name.trim_end_matches('/')),
                    io::empty(),
                )?;
            } else {
                let mut counted = Counted {
                    inner: &mut entry,
                    tracker,
                };
                tar.append_data(&mut header, &name, &mut counted)?;
            }
        }
        for folder in &plan.new_folders {
            let item = Item {
                source: PathBuf::new(),
                name: folder.clone(),
                kind: ItemKind::Dir,
                size: 0,
                mode: 0o755,
                modified: Some(SystemTime::now()),
            };
            create::tar_append(tar, &item, true, tracker)?;
        }
        for item in &plan.added {
            create::tar_append(tar, item, options.keep_permissions, tracker)?;
        }
        Ok(())
    })
}
