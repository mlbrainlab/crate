//! Creating archives from files and folders: ZIP, 7z and tar (plain or
//! gz / xz / zst / bz2), each optionally password-protected (ZIP and 7z
//! with AES-256, tar as a whole with age). The archive is written to a
//! temporary file next to the target, flushed to disk, then renamed into
//! place, so a crash never leaves half an archive under the real name.

use std::cell::RefCell;
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::error::{Error, Result};
use crate::pattern;
use crate::progress::{CancelToken, Progress};

/// Formats Crate can write.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OutFormat {
    Zip,
    SevenZ,
    Tar,
    TarGz,
    TarXz,
    TarZst,
    TarBz2,
}

impl OutFormat {
    pub const ALL: [OutFormat; 7] = [
        OutFormat::Zip,
        OutFormat::SevenZ,
        OutFormat::TarZst,
        OutFormat::TarGz,
        OutFormat::TarXz,
        OutFormat::TarBz2,
        OutFormat::Tar,
    ];

    /// `.tar.zst`; with a password, tar formats end in `.age` too.
    pub fn extension(self, encrypted: bool) -> &'static str {
        match (self, encrypted && self.is_tar()) {
            (OutFormat::Zip, _) => ".zip",
            (OutFormat::SevenZ, _) => ".7z",
            (OutFormat::Tar, false) => ".tar",
            (OutFormat::Tar, true) => ".tar.age",
            (OutFormat::TarGz, false) => ".tar.gz",
            (OutFormat::TarGz, true) => ".tar.gz.age",
            (OutFormat::TarXz, false) => ".tar.xz",
            (OutFormat::TarXz, true) => ".tar.xz.age",
            (OutFormat::TarZst, false) => ".tar.zst",
            (OutFormat::TarZst, true) => ".tar.zst.age",
            (OutFormat::TarBz2, false) => ".tar.bz2",
            (OutFormat::TarBz2, true) => ".tar.bz2.age",
        }
    }

    /// The format a file name asks for (`backup.tar.zst`, `x.tgz`), and
    /// whether it ends in `.age` (tar encrypted with a password).
    pub fn from_file_name(name: &str) -> Option<(OutFormat, bool)> {
        let lower = name.to_ascii_lowercase();
        let (base, age) = match lower.strip_suffix(".age") {
            Some(b) => (b, true),
            None => (lower.as_str(), false),
        };
        let format = [
            (".zip", OutFormat::Zip),
            (".7z", OutFormat::SevenZ),
            (".tar.gz", OutFormat::TarGz),
            (".tgz", OutFormat::TarGz),
            (".tar.xz", OutFormat::TarXz),
            (".txz", OutFormat::TarXz),
            (".tar.zst", OutFormat::TarZst),
            (".tzst", OutFormat::TarZst),
            (".tar.bz2", OutFormat::TarBz2),
            (".tbz2", OutFormat::TarBz2),
            (".tar", OutFormat::Tar),
        ]
        .into_iter()
        .find(|(ext, _)| base.ends_with(ext))
        .map(|(_, f)| f)?;
        (!age || format.is_tar()).then_some((format, age))
    }

    pub fn name(self) -> &'static str {
        match self {
            OutFormat::Zip => "ZIP",
            OutFormat::SevenZ => "7Z",
            OutFormat::Tar => "TAR",
            OutFormat::TarGz => "TAR.GZ",
            OutFormat::TarXz => "TAR.XZ",
            OutFormat::TarZst => "TAR.ZST",
            OutFormat::TarBz2 => "TAR.BZ2",
        }
    }

    pub fn is_tar(self) -> bool {
        !matches!(self, OutFormat::Zip | OutFormat::SevenZ)
    }

    /// Lowest, highest and default compression level, on the format's own
    /// scale (zstd 1–19, xz 0–9…). Plain tar has no levels.
    pub fn levels(self) -> (u32, u32, u32) {
        match self {
            OutFormat::Zip => (0, 9, 6),
            OutFormat::SevenZ => (0, 9, 5),
            OutFormat::Tar => (0, 0, 0),
            OutFormat::TarGz => (1, 9, 6),
            OutFormat::TarXz => (0, 9, 6),
            OutFormat::TarZst => (1, 19, 3),
            OutFormat::TarBz2 => (1, 9, 9),
        }
    }

    /// Whether volumes (`name.7z.001`, `.002`…) make sense: 7-Zip opens
    /// split ZIP and 7z archives; tar is usually split with other tools.
    pub fn supports_volumes(self) -> bool {
        matches!(self, OutFormat::Zip | OutFormat::SevenZ)
    }
}

#[derive(Clone, Debug)]
pub struct CreateOptions {
    pub format: OutFormat,
    /// On the format's scale; see [`OutFormat::levels`].
    pub level: u32,
    /// ZIP and 7z: AES-256 (7z also hides the file names). Tar: the whole
    /// archive is encrypted with age.
    pub password: Option<String>,
    /// Patterns to leave out (see [`crate::pattern`]): `*.tmp`, `.git/`.
    pub exclude: Vec<String>,
    /// Keep Unix permissions and store symlinks as links. Off: plain modes,
    /// and links to files are stored as the files they point to.
    pub keep_permissions: bool,
    /// Split the result into volumes of this many bytes.
    pub volume_size: Option<u64>,
}

impl Default for CreateOptions {
    fn default() -> Self {
        Self {
            format: OutFormat::Zip,
            level: OutFormat::Zip.levels().2,
            password: None,
            exclude: Vec::new(),
            keep_permissions: true,
            volume_size: None,
        }
    }
}

/// One thing to put into the archive.
#[derive(Clone, Debug)]
pub struct Item {
    pub source: PathBuf,
    /// Path inside the archive, `/`-separated.
    pub name: String,
    pub kind: ItemKind,
    pub size: u64,
    pub mode: u32,
    pub modified: Option<SystemTime>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemKind {
    File,
    Dir,
    Symlink(PathBuf),
}

/// What [`collect`] found.
#[derive(Clone, Debug, Default)]
pub struct Collected {
    pub items: Vec<Item>,
    /// Items left out by the exclude patterns, and their total size.
    pub excluded: u64,
    pub excluded_bytes: u64,
    /// Things that couldn't be read, with why.
    pub unreadable: Vec<(PathBuf, String)>,
}

impl Collected {
    pub fn bytes(&self) -> u64 {
        self.items.iter().map(|i| i.size).sum()
    }
}

#[derive(Clone, Debug, Default)]
pub struct CreateReport {
    pub files: u64,
    pub dirs: u64,
    pub links: u64,
    /// Unpacked bytes put in.
    pub bytes: u64,
    /// Size of what was written (all volumes).
    pub written: u64,
    pub excluded: u64,
    pub unreadable: Vec<(PathBuf, String)>,
    /// The archive, or its volumes in order.
    pub outputs: Vec<PathBuf>,
}

/// Walks the sources (files and folders). Each source becomes a top-level
/// item named after it; two sources with the same name get `name (2)`.
/// `skip` is never included (the archive being written, when it sits inside
/// a source folder).
pub fn collect(
    sources: &[PathBuf],
    exclude: &[String],
    keep_links: bool,
    skip: Option<&Path>,
) -> Collected {
    let mut out = Collected::default();
    let skip = skip.and_then(|p| p.canonicalize().ok());
    let mut tops: Vec<String> = Vec::new();
    for source in sources {
        let base = source
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "root".into());
        let mut name = base.clone();
        let mut n = 2;
        while tops.iter().any(|t| t.eq_ignore_ascii_case(&name)) {
            let (stem, ext) = crate::safety::split_extension(&base);
            name = format!("{stem} ({n}){ext}");
            n += 1;
        }
        tops.push(name.clone());
        walk(
            source,
            &name,
            exclude,
            keep_links,
            skip.as_deref(),
            &mut out,
        );
    }
    out
}

fn walk(
    path: &Path,
    name: &str,
    exclude: &[String],
    keep_links: bool,
    skip: Option<&Path>,
    out: &mut Collected,
) {
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) => return out.unreadable.push((path.to_owned(), e.to_string())),
    };
    let is_link = meta.file_type().is_symlink();
    // Links are followed when they aren't kept, but only to files: a link
    // to a folder could loop.
    let meta = if is_link && !keep_links {
        match fs::metadata(path) {
            Ok(m) if m.is_file() => m,
            Ok(_) => return,
            Err(e) => return out.unreadable.push((path.to_owned(), e.to_string())),
        }
    } else {
        meta
    };
    let is_dir = meta.is_dir() && !is_link;
    if exclude.iter().any(|p| pattern::matches(p, name, is_dir)) {
        out.excluded += 1;
        out.excluded_bytes += if is_dir { dir_size(path) } else { meta.len() };
        return;
    }
    if skip.is_some_and(|s| path.canonicalize().is_ok_and(|p| p == s)) {
        return;
    }
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    if file_name.starts_with(".crate-new-") {
        return; // another archive being written right here
    }
    let item = |kind, size| Item {
        source: path.to_owned(),
        name: name.to_owned(),
        kind,
        size,
        mode: mode_of(&meta),
        modified: meta.modified().ok(),
    };
    if is_link && keep_links {
        match fs::read_link(path) {
            Ok(target) => out.items.push(item(ItemKind::Symlink(target), 0)),
            Err(e) => out.unreadable.push((path.to_owned(), e.to_string())),
        }
    } else if is_dir {
        out.items.push(item(ItemKind::Dir, 0));
        let mut children: Vec<_> = match fs::read_dir(path) {
            Ok(rd) => rd.filter_map(|e| e.ok()).map(|e| e.file_name()).collect(),
            Err(e) => return out.unreadable.push((path.to_owned(), e.to_string())),
        };
        children.sort();
        for child in children {
            let child_name = format!("{name}/{}", child.to_string_lossy());
            walk(
                &path.join(&child),
                &child_name,
                exclude,
                keep_links,
                skip,
                out,
            );
        }
    } else if meta.is_file() {
        out.items.push(item(ItemKind::File, meta.len()));
    }
    // Devices, sockets and FIFOs are never archived.
}

fn dir_size(path: &Path) -> u64 {
    fs::read_dir(path)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| match e.file_type() {
                    Ok(t) if t.is_dir() => dir_size(&e.path()),
                    Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
                    _ => 0,
                })
                .sum()
        })
        .unwrap_or(0)
}

#[cfg(unix)]
fn mode_of(meta: &fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o7777
}

#[cfg(not(unix))]
fn mode_of(meta: &fs::Metadata) -> u32 {
    if meta.is_dir() { 0o755 } else { 0o644 }
}

/// Modes when permissions aren't kept: 755 for folders and executables,
/// 644 for the rest.
fn plain_mode(item: &Item) -> u32 {
    if item.kind == ItemKind::Dir || item.mode & 0o111 != 0 {
        0o755
    } else {
        0o644
    }
}

/// A guess at the archive size and the time to make it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Estimate {
    pub bytes: u64,
    pub seconds: f64,
}

/// Estimates by compressing a sample (up to `budget` bytes, a slice from
/// each of many files) at the chosen level and scaling it up. Photos and
/// other compressed files sample as incompressible, as they should.
pub fn estimate(collected: &Collected, format: OutFormat, level: u32, budget: u64) -> Estimate {
    let files: Vec<&Item> = collected
        .items
        .iter()
        .filter(|i| i.kind == ItemKind::File && i.size > 0)
        .collect();
    let total = collected.bytes();
    if files.is_empty() || total == 0 {
        return Estimate {
            bytes: 0,
            seconds: 0.0,
        };
    }
    // A slice from evenly spread files, so one big video doesn't decide.
    let per_file = (budget / files.len() as u64).clamp(16 << 10, 1 << 20);
    let step = (files.len() as u64 * per_file / budget.max(1)).max(1) as usize;
    let mut sample = Vec::new();
    for item in files.iter().step_by(step) {
        if sample.len() as u64 >= budget {
            break;
        }
        if let Ok(f) = File::open(&item.source) {
            let _ = f.take(per_file).read_to_end(&mut sample);
        }
    }
    if sample.is_empty() {
        return Estimate {
            bytes: total,
            seconds: 0.0,
        };
    }
    let started = std::time::Instant::now();
    let packed = compressed_len(&sample, format, level).unwrap_or(sample.len() as u64);
    let elapsed = started.elapsed().as_secs_f64().max(1e-4);
    let scale = total as f64 / sample.len() as f64;
    Estimate {
        bytes: (packed as f64 * scale) as u64,
        seconds: elapsed * scale,
    }
}

/// Size of `data` compressed like `format` would (7z's LZMA2 measured with
/// the xz encoder, which is the same algorithm).
fn compressed_len(data: &[u8], format: OutFormat, level: u32) -> io::Result<u64> {
    let mut counter = CountingSink(0);
    match format {
        OutFormat::Tar => return Ok(data.len() as u64),
        OutFormat::Zip | OutFormat::TarGz => {
            if level == 0 {
                return Ok(data.len() as u64);
            }
            let mut e =
                flate2::write::DeflateEncoder::new(&mut counter, flate2::Compression::new(level));
            e.write_all(data)?;
            e.finish()?;
        }
        OutFormat::SevenZ | OutFormat::TarXz => {
            if level == 0 && format == OutFormat::SevenZ {
                return Ok(data.len() as u64);
            }
            let mut e = liblzma::write::XzEncoder::new(&mut counter, level);
            e.write_all(data)?;
            e.finish()?;
        }
        OutFormat::TarZst => {
            let mut e = zstd::Encoder::new(&mut counter, level as i32)?;
            e.write_all(data)?;
            e.finish()?;
        }
        OutFormat::TarBz2 => {
            let mut e =
                bzip2::write::BzEncoder::new(&mut counter, bzip2::Compression::new(level.max(1)));
            e.write_all(data)?;
            e.finish()?;
        }
    }
    Ok(counter.0)
}

struct CountingSink(u64);

impl Write for CountingSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0 += buf.len() as u64;
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Writes `items` into a new archive at `output` (replacing it atomically if
/// it exists). Cancelling, or any error, leaves nothing behind.
pub fn create(
    collected: &Collected,
    output: &Path,
    options: &CreateOptions,
    progress: &mut dyn FnMut(&Progress),
    cancel: &CancelToken,
) -> Result<CreateReport> {
    let items = &collected.items;
    let tracker = RefCell::new(Tracker::new(
        Progress {
            bytes_total: collected.bytes(),
            files_total: items.iter().filter(|i| i.kind == ItemKind::File).count() as u64,
            ..Progress::default()
        },
        progress,
        cancel,
    ));
    let mut out = AtomicFile::new(output)?;
    let result = match options.format {
        OutFormat::Zip => write_zip(out.file(), items, options, &tracker),
        OutFormat::SevenZ => write_7z(out.file(), items, options, &tracker),
        _ => write_tar_family(out.file(), items, options, &tracker),
    };
    result.map_err(|e| tracker.borrow().cancelled_or(e))?;
    tracker.borrow_mut().report();

    let mut report = CreateReport {
        bytes: collected.bytes(),
        excluded: collected.excluded,
        unreadable: collected.unreadable.clone(),
        ..CreateReport::default()
    };
    for item in items {
        match item.kind {
            ItemKind::File => report.files += 1,
            ItemKind::Dir => report.dirs += 1,
            ItemKind::Symlink(_) => report.links += 1,
        }
    }
    report.written = out.len()?;
    report.outputs = match options.volume_size {
        Some(size) if size > 0 && report.written > size => out.commit_split(size)?,
        _ => vec![out.commit()?],
    };
    Ok(report)
}

/// Progress and cancellation, shared by the readers that feed the writers.
pub(crate) struct Tracker<'a> {
    prog: Progress,
    callback: &'a mut dyn FnMut(&Progress),
    cancel: &'a CancelToken,
    since: u64,
}

impl<'a> Tracker<'a> {
    pub(crate) fn new(
        prog: Progress,
        callback: &'a mut dyn FnMut(&Progress),
        cancel: &'a CancelToken,
    ) -> Self {
        Self {
            prog,
            callback,
            cancel,
            since: 0,
        }
    }

    pub(crate) fn start(&mut self, name: &str) -> io::Result<()> {
        self.check()?;
        self.prog.current.clear();
        self.prog.current.push_str(name);
        (self.callback)(&self.prog);
        Ok(())
    }

    pub(crate) fn advance(&mut self, n: u64) -> io::Result<()> {
        self.prog.bytes_done += n;
        self.since += n;
        if self.since >= 1 << 20 {
            self.since = 0;
            (self.callback)(&self.prog);
        }
        self.check()
    }

    pub(crate) fn file_done(&mut self) {
        self.prog.files_done += 1;
    }

    pub(crate) fn report(&mut self) {
        (self.callback)(&self.prog);
    }

    pub(crate) fn check(&self) -> io::Result<()> {
        if self.cancel.is_cancelled() {
            // Not `Interrupted`: io::copy retries those forever.
            Err(io::Error::other("cancelled"))
        } else {
            Ok(())
        }
    }

    pub(crate) fn cancelled_or(&self, e: Error) -> Error {
        if self.cancel.is_cancelled() {
            Error::Cancelled
        } else {
            e
        }
    }
}

/// Reads a source file, counting bytes for the progress bar.
struct Tracked<'a, 'b> {
    file: File,
    tracker: &'a RefCell<Tracker<'b>>,
}

impl Read for Tracked<'_, '_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.file.read(buf)?;
        self.tracker.borrow_mut().advance(n as u64)?;
        Ok(n)
    }
}

fn open_tracked<'a, 'b>(
    item: &Item,
    tracker: &'a RefCell<Tracker<'b>>,
) -> io::Result<Tracked<'a, 'b>> {
    tracker.borrow_mut().start(&item.name)?;
    Ok(Tracked {
        file: File::open(&item.source)?,
        tracker,
    })
}

// ----- ZIP -------------------------------------------------------------------

fn write_zip(
    out: &mut File,
    items: &[Item],
    options: &CreateOptions,
    tracker: &RefCell<Tracker>,
) -> Result<()> {
    let mut zip = zip::ZipWriter::new(BufWriter::new(out));
    for item in items {
        zip_append(&mut zip, item, options, tracker)?;
    }
    zip.finish().map_err(zip_error)?.flush()?;
    Ok(())
}

/// Adds one file, folder or link from disk to a ZIP, with `options`'
/// level, permissions and password.
pub(crate) fn zip_append<W: Write + Seek>(
    zip: &mut zip::ZipWriter<W>,
    item: &Item,
    options: &CreateOptions,
    tracker: &RefCell<Tracker>,
) -> Result<()> {
    use zip::write::SimpleFileOptions;
    let mut opts = SimpleFileOptions::default()
        .compression_method(if options.level == 0 {
            zip::CompressionMethod::Stored
        } else {
            zip::CompressionMethod::Deflated
        })
        .compression_level((options.level > 0).then_some(options.level as i64))
        .unix_permissions(if options.keep_permissions {
            item.mode
        } else {
            plain_mode(item)
        })
        .large_file(item.size >= 0xFFFF_0000);
    if let Some(t) = item.modified.and_then(zip_time) {
        opts = opts.last_modified_time(t);
    }
    if let Some(pw) = &options.password {
        opts = opts.with_aes_encryption(zip::AesMode::Aes256, pw);
    }
    match &item.kind {
        ItemKind::Dir => {
            zip.add_directory(format!("{}/", item.name), opts)
                .map_err(zip_error)?;
        }
        ItemKind::Symlink(target) => {
            zip.add_symlink(&item.name, target.to_string_lossy(), opts)
                .map_err(zip_error)?;
        }
        ItemKind::File => {
            let mut src = open_tracked(item, tracker)?;
            zip.start_file(&item.name, opts).map_err(zip_error)?;
            io::copy(&mut src, zip)?;
            tracker.borrow_mut().file_done();
        }
    }
    Ok(())
}

pub(crate) fn zip_error(e: zip::result::ZipError) -> Error {
    match e {
        zip::result::ZipError::Io(e) => Error::Io(e),
        other => Error::Io(io::Error::other(other.to_string())),
    }
}

pub(crate) fn zip_time(t: SystemTime) -> Option<zip::DateTime> {
    let (y, mo, d, h, mi, s) = civil(t)?;
    zip::DateTime::from_date_and_time(u16::try_from(y).ok()?, mo, d, h, mi, s).ok()
}

/// UTC date and time of `t`.
fn civil(t: SystemTime) -> Option<(i64, u8, u8, u8, u8, u8)> {
    let secs = t.duration_since(SystemTime::UNIX_EPOCH).ok()?.as_secs() as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    let y = yoe + era * 400 + i64::from(m <= 2);
    Some((
        y,
        m,
        d,
        (rem / 3600) as u8,
        (rem / 60 % 60) as u8,
        (rem % 60) as u8,
    ))
}

// ----- 7z --------------------------------------------------------------------

/// Files go into solid blocks of about this much data (better compression
/// than one block per file, without one huge block for everything).
const SOLID_BLOCK: u64 = 64 << 20;
const SOLID_FILES: usize = 1000;
/// Unix permissions live in the high 16 bits of the Windows attributes.
pub(crate) const UNIX_EXTENSION: u32 = 0x8000;
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;

fn write_7z(
    out: &mut File,
    items: &[Item],
    options: &CreateOptions,
    tracker: &RefCell<Tracker>,
) -> Result<()> {
    use sevenz_rust2::encoder_options::{AesEncoderOptions, Lzma2Options};
    use sevenz_rust2::{ArchiveWriter, EncoderMethod, SourceReader};

    let mut w = ArchiveWriter::new(BufWriter::new(out)).map_err(sevenz_error)?;
    let mut methods = Vec::new();
    if let Some(pw) = &options.password {
        methods.push(AesEncoderOptions::new(pw.as_str().into()).into());
        w.set_encrypt_header(true);
    }
    methods.push(if options.level == 0 {
        EncoderMethod::COPY.into()
    } else {
        Lzma2Options::from_level(options.level).into()
    });
    w.set_content_methods(methods);

    let entry = |item: &Item| sevenz_entry(item, options.keep_permissions);

    // Folders and empty files have no data.
    for item in items
        .iter()
        .filter(|i| i.kind == ItemKind::Dir || (i.kind == ItemKind::File && i.size == 0))
    {
        let mut e = entry(item);
        e.has_stream = false;
        w.push_archive_entry::<&[u8]>(e, None)
            .map_err(sevenz_error)?;
        if item.kind == ItemKind::File {
            tracker.borrow_mut().file_done();
        }
    }
    // Symlinks: the target is the data (as p7zip stores them).
    for item in items {
        if let ItemKind::Symlink(target) = &item.kind {
            let data = target.to_string_lossy().into_owned().into_bytes();
            w.push_archive_entry(entry(item), Some(&data[..]))
                .map_err(sevenz_error)?;
        }
    }
    // Files with data, in solid blocks.
    let files: Vec<&Item> = items
        .iter()
        .filter(|i| i.kind == ItemKind::File && i.size > 0)
        .collect();
    let mut start = 0;
    while start < files.len() {
        let mut end = start;
        let mut bytes = 0;
        while end < files.len()
            && (end == start || (bytes < SOLID_BLOCK && end - start < SOLID_FILES))
        {
            bytes += files[end].size;
            end += 1;
        }
        let block = &files[start..end];
        let readers: Vec<SourceReader<LazyFile>> = block
            .iter()
            .map(|item| SourceReader::new(LazyFile::new(item, tracker)))
            .collect();
        w.push_archive_entries(block.iter().map(|i| entry(i)).collect(), readers)
            .map_err(|e| tracker.borrow().cancelled_or(sevenz_error(e)))?;
        start = end;
    }
    w.finish()?.flush()?;
    Ok(())
}

/// A 7z entry for a file, folder or link from disk.
pub(crate) fn sevenz_entry(item: &Item, keep_permissions: bool) -> sevenz_rust2::ArchiveEntry {
    let mut e = sevenz_rust2::ArchiveEntry::from_path(&item.source, item.name.clone());
    let file_type = match item.kind {
        ItemKind::Dir => 0o040000,
        ItemKind::Symlink(_) => 0o120000,
        ItemKind::File => 0o100000,
    };
    let mode = if keep_permissions {
        item.mode
    } else {
        plain_mode(item)
    };
    e.has_windows_attributes = true;
    e.windows_attributes = UNIX_EXTENSION
        | ((file_type | mode) << 16)
        | if item.kind == ItemKind::Dir {
            FILE_ATTRIBUTE_DIRECTORY
        } else {
            0
        };
    e.is_directory = item.kind == ItemKind::Dir;
    e.has_stream =
        item.kind != ItemKind::Dir && (item.size > 0 || matches!(item.kind, ItemKind::Symlink(_)));
    e
}

/// Opens its file only when the solid block reaches it, so a block of a
/// thousand files doesn't hold a thousand file handles.
struct LazyFile<'a, 'b> {
    item: Item,
    tracker: &'a RefCell<Tracker<'b>>,
    file: Option<Tracked<'a, 'b>>,
    done: bool,
}

impl<'a, 'b> LazyFile<'a, 'b> {
    fn new(item: &Item, tracker: &'a RefCell<Tracker<'b>>) -> Self {
        Self {
            item: item.clone(),
            tracker,
            file: None,
            done: false,
        }
    }
}

impl Read for LazyFile<'_, '_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.done {
            return Ok(0);
        }
        if self.file.is_none() {
            self.file = Some(open_tracked(&self.item, self.tracker)?);
        }
        let n = self.file.as_mut().expect("opened").read(buf)?;
        if n == 0 {
            self.done = true;
            self.file = None;
            self.tracker.borrow_mut().file_done();
        }
        Ok(n)
    }
}

pub(crate) fn sevenz_error(e: sevenz_rust2::Error) -> Error {
    match e {
        sevenz_rust2::Error::Io(e, _) => Error::Io(e),
        other => Error::Io(io::Error::other(other.to_string())),
    }
}

// ----- tar -------------------------------------------------------------------

/// A tar stream being written, whatever compression is under it.
pub(crate) type TarOut<'w> = tar::Builder<&'w mut dyn Write>;

fn write_tar_family(
    out: &mut File,
    items: &[Item],
    options: &CreateOptions,
    tracker: &RefCell<Tracker>,
) -> Result<()> {
    write_tar_stream(
        out,
        options.format,
        options.level,
        options.password.as_deref(),
        |tar| {
            for item in items {
                tar_append(tar, item, options.keep_permissions, tracker)?;
            }
            Ok(())
        },
    )
}

/// Writes a tar into `out` through `format`'s compression (and age, with a
/// passphrase); `body` adds the entries.
pub(crate) fn write_tar_stream(
    out: &mut File,
    format: OutFormat,
    level: u32,
    passphrase: Option<&str>,
    body: impl FnOnce(&mut TarOut<'_>) -> Result<()>,
) -> Result<()> {
    let file = BufWriter::new(out);
    match passphrase {
        Some(pw) => {
            let passphrase = age::secrecy::SecretString::from(pw.to_owned());
            let encrypted = age::Encryptor::with_user_passphrase(passphrase).wrap_output(file)?;
            compress(encrypted, format, level, body)?
                .finish()?
                .flush()?;
        }
        None => compress(file, format, level, body)?.flush()?,
    }
    Ok(())
}

/// The tar from `body`, through the format's compressor, into `w`.
fn compress<W: Write>(
    w: W,
    format: OutFormat,
    level: u32,
    body: impl FnOnce(&mut TarOut<'_>) -> Result<()>,
) -> Result<W> {
    fn tar_into(mut w: impl Write, body: impl FnOnce(&mut TarOut<'_>) -> Result<()>) -> Result<()> {
        let mut tar = tar::Builder::new(&mut w as &mut dyn Write);
        body(&mut tar)?;
        tar.finish()?;
        Ok(())
    }
    Ok(match format {
        OutFormat::TarGz => {
            let mut e = flate2::write::GzEncoder::new(w, flate2::Compression::new(level));
            tar_into(&mut e, body)?;
            e.finish()?
        }
        OutFormat::TarXz => {
            let mut e = liblzma::write::XzEncoder::new(w, level);
            tar_into(&mut e, body)?;
            e.finish()?
        }
        OutFormat::TarZst => {
            let mut e = zstd::Encoder::new(w, level as i32)?;
            let _ = e.include_checksum(true);
            tar_into(&mut e, body)?;
            e.finish()?
        }
        OutFormat::TarBz2 => {
            let mut e = bzip2::write::BzEncoder::new(w, bzip2::Compression::new(level.max(1)));
            tar_into(&mut e, body)?;
            e.finish()?
        }
        _ => {
            let mut w = w;
            tar_into(&mut w, body)?;
            w
        }
    })
}

/// Adds one file, folder or link from disk to a tar.
pub(crate) fn tar_append(
    tar: &mut TarOut<'_>,
    item: &Item,
    keep_permissions: bool,
    tracker: &RefCell<Tracker>,
) -> Result<()> {
    let mut h = tar::Header::new_gnu();
    h.set_mode(if keep_permissions {
        item.mode
    } else {
        plain_mode(item)
    });
    h.set_mtime(
        item.modified
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs()),
    );
    match &item.kind {
        ItemKind::Dir => {
            h.set_entry_type(tar::EntryType::Directory);
            h.set_size(0);
            tar.append_data(&mut h, format!("{}/", item.name), io::empty())?;
        }
        ItemKind::Symlink(target) => {
            h.set_entry_type(tar::EntryType::Symlink);
            h.set_size(0);
            tar.append_link(&mut h, &item.name, target)?;
        }
        ItemKind::File => {
            h.set_entry_type(tar::EntryType::Regular);
            h.set_size(item.size);
            // The file may change size while we read: never write more or
            // less than the header says.
            let src = open_tracked(item, tracker)?;
            tar.append_data(&mut h, &item.name, ExactSize::new(src, item.size))?;
            tracker.borrow_mut().file_done();
        }
    }
    Ok(())
}

/// Exactly `size` bytes: cut off if the file grew, zero-padded if it shrank.
struct ExactSize<R> {
    inner: R,
    left: u64,
}

impl<R: Read> ExactSize<R> {
    fn new(inner: R, size: u64) -> Self {
        Self { inner, left: size }
    }
}

impl<R: Read> Read for ExactSize<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.left == 0 {
            return Ok(0);
        }
        let want = buf.len().min(self.left as usize);
        let n = match self.inner.read(&mut buf[..want])? {
            0 => {
                buf[..want].fill(0);
                want
            }
            n => n,
        };
        self.left -= n as u64;
        Ok(n)
    }
}

// ----- Writing safely ---------------------------------------------------------

/// A new file next to `target`: written under a hidden temporary name, then
/// flushed to disk and renamed over `target`. Dropped without `commit`, it
/// is deleted.
pub struct AtomicFile {
    target: PathBuf,
    temp: tempfile::NamedTempFile,
}

impl AtomicFile {
    pub fn new(target: &Path) -> Result<Self> {
        let dir = match target.parent() {
            Some(p) if !p.as_os_str().is_empty() => p.to_owned(),
            _ => PathBuf::from("."),
        };
        let temp = tempfile::Builder::new()
            .prefix(".crate-new-")
            .tempfile_in(dir)?;
        Ok(Self {
            target: target.to_owned(),
            temp,
        })
    }

    pub fn file(&mut self) -> &mut File {
        self.temp.as_file_mut()
    }

    fn len(&mut self) -> io::Result<u64> {
        self.temp.as_file_mut().flush()?;
        Ok(self.temp.as_file().metadata()?.len())
    }

    /// Flush to disk, rename into place, and flush the folder entry.
    pub fn commit(self) -> Result<PathBuf> {
        self.temp.as_file().sync_all()?;
        self.temp
            .persist(&self.target)
            .map_err(|e| Error::Io(e.error))?;
        sync_dir(&self.target);
        Ok(self.target)
    }

    /// Like `commit`, but as `target.001`, `.002`… of at most `size` bytes.
    fn commit_split(self, size: u64) -> Result<Vec<PathBuf>> {
        let mut source = self.temp.reopen()?;
        source.rewind()?;
        let mut parts = Vec::new();
        let mut temps = Vec::new();
        loop {
            let mut part = AtomicFile::new(&self.target)?;
            let n = io::copy(&mut (&mut source).take(size), part.file())?;
            if n == 0 {
                break;
            }
            temps.push(part);
        }
        for (i, part) in temps.into_iter().enumerate() {
            let name = format!("{}.{:03}", self.target.display(), i + 1);
            let mut part = part;
            part.target = PathBuf::from(name);
            parts.push(part.commit()?);
        }
        // `self.temp`, the unsplit copy, is deleted when `self` drops.
        Ok(parts)
    }
}

#[cfg(unix)]
fn sync_dir(path: &Path) {
    if let Some(dir) = path.parent().filter(|p| !p.as_os_str().is_empty())
        && let Ok(d) = File::open(dir)
    {
        let _ = d.sync_all();
    }
}

#[cfg(not(unix))]
fn sync_dir(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_follow_compressibility() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("text.txt"),
            "the same line again\n".repeat(50_000),
        )
        .unwrap();
        let mut x = 0x9E37_79B9_7F4A_7C15u64;
        let noise: Vec<u8> = (0..1_000_000)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                x as u8
            })
            .collect();
        std::fs::write(dir.path().join("noise.bin"), &noise).unwrap();
        let text = collect(&[dir.path().join("text.txt")], &[], true, None);
        let noisy = collect(&[dir.path().join("noise.bin")], &[], true, None);
        let t = estimate(&text, OutFormat::TarZst, 3, 4 << 20);
        let n = estimate(&noisy, OutFormat::TarZst, 3, 4 << 20);
        assert!(t.bytes < text.bytes() / 20, "{t:?}");
        assert!(n.bytes > noisy.bytes() * 9 / 10, "{n:?}");
        assert_eq!(
            estimate(&text, OutFormat::Tar, 0, 4 << 20).bytes,
            text.bytes()
        );
    }

    #[test]
    fn civil_dates() {
        let t = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_792_845_015);
        assert_eq!(civil(t), Some((2026, 10, 24, 12, 30, 15)));
    }

    #[test]
    fn exact_size_pads_and_cuts() {
        let mut out = Vec::new();
        ExactSize::new(&b"abc"[..], 5)
            .read_to_end(&mut out)
            .unwrap();
        assert_eq!(out, b"abc\0\0");
        out.clear();
        ExactSize::new(&b"abcdef"[..], 2)
            .read_to_end(&mut out)
            .unwrap();
        assert_eq!(out, b"ab");
    }

    #[test]
    fn formats_from_names() {
        assert_eq!(
            OutFormat::from_file_name("a.TAR.ZST"),
            Some((OutFormat::TarZst, false))
        );
        assert_eq!(
            OutFormat::from_file_name("a.tgz"),
            Some((OutFormat::TarGz, false))
        );
        assert_eq!(
            OutFormat::from_file_name("a.tar.zst.age"),
            Some((OutFormat::TarZst, true))
        );
        assert_eq!(OutFormat::from_file_name("a.zip.age"), None);
        assert_eq!(OutFormat::from_file_name("a.rar"), None);
    }

    #[test]
    fn extensions() {
        assert_eq!(OutFormat::TarZst.extension(true), ".tar.zst.age");
        assert_eq!(OutFormat::Zip.extension(true), ".zip");
        assert_eq!(OutFormat::TarZst.levels(), (1, 19, 3));
    }
}
