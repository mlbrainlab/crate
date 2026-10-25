//! Converting an archive to another format (Dialogs.dc.html › Convert):
//! entries stream from the reader straight into the writer, nothing is
//! extracted to disk. Works from every readable format, RAR included, which
//! is also how a read-only archive gets "saved as a copy elsewhere".

use std::cell::RefCell;
use std::io::{self, BufWriter, Read, Write};
use std::path::Path;
use std::time::SystemTime;

use crate::archive::{Archive, OpenOptions};
use crate::create::{self, AtomicFile, CreateOptions, CreateReport, OutFormat, Tracker};
use crate::entry::{Entry, EntryKind};
use crate::error::Result;
use crate::progress::{CancelToken, Progress};
use crate::safety::{self, SkipReason, Skipped};

#[derive(Clone, Debug, Default)]
pub struct ConvertReport {
    pub created: CreateReport,
    /// Entries left out (unsafe names, devices…).
    pub skipped: Vec<Skipped>,
}

/// Writes every entry of the archive at `source` into a new archive at
/// `output` in `options.format`. `source_password` opens the source.
pub fn convert(
    source: &Path,
    source_password: Option<&str>,
    output: &Path,
    options: &CreateOptions,
    progress: &mut dyn FnMut(&Progress),
    cancel: &CancelToken,
) -> Result<ConvertReport> {
    let mut archive = Archive::open_with(
        source,
        &OpenOptions {
            password: source_password.map(str::to_owned),
            ..OpenOptions::default()
        },
    )?;
    let entries = archive.entries().to_vec();
    let mut report = ConvertReport::default();
    // Names are checked like extraction does: nothing unsafe is carried over.
    let mut keep = vec![false; entries.len()];
    for e in &entries {
        let reason = match (&e.kind, safety::safe_relative_path(&e.path)) {
            (_, Err(reason)) => Some(reason),
            (EntryKind::Other, _) => Some(SkipReason::UnsupportedType),
            (EntryKind::Hardlink { .. }, _) => Some(SkipReason::UnsupportedType),
            _ => None,
        };
        match reason {
            Some(reason) => report.skipped.push(Skipped {
                path: e.path.clone(),
                reason,
            }),
            None => keep[e.index] = true,
        }
    }
    let tracker = RefCell::new(Tracker::new(
        Progress {
            bytes_total: entries.iter().map(|e| e.size).sum(),
            files_total: entries.iter().filter(|e| e.kind == EntryKind::File).count() as u64,
            ..Progress::default()
        },
        progress,
        cancel,
    ));
    let mut out = AtomicFile::new(output)?;
    let result = match options.format {
        OutFormat::Zip => to_zip(&mut archive, &keep, out.file(), options, &tracker, cancel),
        OutFormat::SevenZ => to_7z(&mut archive, &keep, out.file(), options, &tracker, cancel),
        _ => create::write_tar_stream(
            out.file(),
            options.format,
            options.level,
            options.password.as_deref(),
            |tar| {
                archive.for_each_entry(
                    &mut |e, data| {
                        if !keep[e.index] {
                            return Ok(());
                        }
                        tracker.borrow_mut().start(&e.path)?;
                        let mut h = tar::Header::new_gnu();
                        h.set_mode(mode(e));
                        h.set_mtime(mtime(e));
                        let name = clean(&e.path);
                        match &e.kind {
                            EntryKind::Dir => {
                                h.set_entry_type(tar::EntryType::Directory);
                                h.set_size(0);
                                tar.append_data(&mut h, format!("{name}/"), io::empty())?;
                            }
                            EntryKind::Symlink { target } => {
                                h.set_entry_type(tar::EntryType::Symlink);
                                h.set_size(0);
                                tar.append_link(&mut h, &name, target)?;
                            }
                            _ => {
                                h.set_entry_type(tar::EntryType::Regular);
                                h.set_size(e.size);
                                let mut counted = Counted {
                                    inner: data,
                                    tracker: &tracker,
                                };
                                tar.append_data(
                                    &mut h,
                                    &name,
                                    ExactSize {
                                        inner: &mut counted,
                                        left: e.size,
                                    },
                                )?;
                                tracker.borrow_mut().file_done();
                            }
                        }
                        Ok(())
                    },
                    cancel,
                )
            },
        ),
    };
    result.map_err(|e| tracker.borrow().cancelled_or(e))?;
    for (e, k) in entries.iter().zip(&keep) {
        if !*k {
            continue;
        }
        match e.kind {
            EntryKind::File => report.created.files += 1,
            EntryKind::Dir => report.created.dirs += 1,
            _ => report.created.links += 1,
        }
        report.created.bytes += e.size;
    }
    report.created.outputs = vec![out.commit()?];
    report.created.written = std::fs::metadata(output)?.len();
    Ok(report)
}

fn clean(path: &str) -> String {
    path.trim_start_matches("./").trim_matches('/').to_owned()
}

fn mode(e: &Entry) -> u32 {
    e.mode.unwrap_or(if e.is_dir() { 0o755 } else { 0o644 })
}

fn mtime(e: &Entry) -> u64 {
    e.modified
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}

fn to_zip(
    archive: &mut Archive,
    keep: &[bool],
    out: &mut std::fs::File,
    options: &CreateOptions,
    tracker: &RefCell<Tracker>,
    cancel: &CancelToken,
) -> Result<()> {
    use zip::write::SimpleFileOptions;
    let mut zip = zip::ZipWriter::new(BufWriter::new(out));
    archive.for_each_entry(
        &mut |e, data| {
            if !keep[e.index] {
                return Ok(());
            }
            tracker.borrow_mut().start(&e.path)?;
            let mut opts = SimpleFileOptions::default()
                .compression_method(if options.level == 0 {
                    zip::CompressionMethod::Stored
                } else {
                    zip::CompressionMethod::Deflated
                })
                .compression_level((options.level > 0).then_some(options.level as i64))
                .unix_permissions(mode(e))
                .large_file(e.size >= 0xFFFF_0000);
            if let Some(t) = e.modified.and_then(create::zip_time) {
                opts = opts.last_modified_time(t);
            }
            if let Some(pw) = &options.password {
                opts = opts.with_aes_encryption(zip::AesMode::Aes256, pw);
            }
            let name = clean(&e.path);
            match &e.kind {
                EntryKind::Dir => {
                    zip.add_directory(format!("{name}/"), opts)
                        .map_err(create::zip_error)?;
                }
                EntryKind::Symlink { target } => {
                    zip.add_symlink(&name, target, opts)
                        .map_err(create::zip_error)?;
                }
                _ => {
                    zip.start_file(&name, opts).map_err(create::zip_error)?;
                    io::copy(
                        &mut Counted {
                            inner: data,
                            tracker,
                        },
                        &mut zip,
                    )?;
                    tracker.borrow_mut().file_done();
                }
            }
            Ok(())
        },
        cancel,
    )?;
    zip.finish().map_err(create::zip_error)?.flush()?;
    Ok(())
}

fn to_7z(
    archive: &mut Archive,
    keep: &[bool],
    out: &mut std::fs::File,
    options: &CreateOptions,
    tracker: &RefCell<Tracker>,
    cancel: &CancelToken,
) -> Result<()> {
    use sevenz_rust2::encoder_options::{AesEncoderOptions, Lzma2Options};
    use sevenz_rust2::{ArchiveEntry, ArchiveWriter, EncoderMethod};
    let mut w = ArchiveWriter::new(BufWriter::new(out)).map_err(create::sevenz_error)?;
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
    archive.for_each_entry(
        &mut |e, data| {
            if !keep[e.index] {
                return Ok(());
            }
            tracker.borrow_mut().start(&e.path)?;
            let name = clean(&e.path);
            let mut entry = if e.is_dir() {
                ArchiveEntry::new_directory(&name)
            } else {
                ArchiveEntry::new_file(&name)
            };
            let file_type = match e.kind {
                EntryKind::Dir => 0o040000,
                EntryKind::Symlink { .. } => 0o120000,
                _ => 0o100000,
            };
            entry.has_windows_attributes = true;
            entry.windows_attributes = create::UNIX_EXTENSION
                | ((file_type | mode(e)) << 16)
                | if e.is_dir() { 0x10 } else { 0 };
            if let Some(t) = e
                .modified
                .and_then(|t| sevenz_rust2::NtTime::try_from(t).ok())
            {
                entry.last_modified_date = t;
                entry.has_last_modified_date = true;
            }
            match &e.kind {
                EntryKind::Symlink { target } => {
                    w.push_archive_entry(entry, Some(target.as_bytes()))
                        .map_err(create::sevenz_error)?;
                }
                EntryKind::File if e.size > 0 => {
                    w.push_archive_entry(
                        entry,
                        Some(Counted {
                            inner: data,
                            tracker,
                        }),
                    )
                    .map_err(create::sevenz_error)?;
                    tracker.borrow_mut().file_done();
                }
                _ => {
                    w.push_archive_entry::<&[u8]>(entry, None)
                        .map_err(create::sevenz_error)?;
                }
            }
            Ok(())
        },
        cancel,
    )?;
    w.finish()?.flush()?;
    Ok(())
}

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

/// Exactly the declared size into tar: a lying entry can't break the stream.
struct ExactSize<R> {
    inner: R,
    left: u64,
}

impl<R: Read> Read for ExactSize<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.left == 0 {
            return Ok(0);
        }
        let want = buf.len().min(self.left as usize);
        let n = match self.inner.read(&mut buf[..want])? {
            0 => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "entry shorter than declared",
                ));
            }
            n => n,
        };
        self.left -= n as u64;
        Ok(n)
    }
}
