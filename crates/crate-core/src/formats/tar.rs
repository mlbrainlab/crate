use std::fs::File;
use std::io::{BufReader, Read};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

use super::{EntrySink, FormatReader, ListSink, RawEntry, RawKind};
use crate::entry::Format;
use crate::error::{Error, Result};

/// Plain and compressed tar. Tar can only be read front to back, so every
/// pass (list, extract, test) reopens the file.
pub(crate) struct TarReader {
    path: PathBuf,
    format: Format,
    /// For age-encrypted tar: the passphrase.
    passphrase: Option<String>,
}

impl TarReader {
    pub fn new(path: PathBuf, format: Format) -> Self {
        Self {
            path,
            format,
            passphrase: None,
        }
    }

    /// An age-encrypted tar. Checks the passphrase right away, so a wrong
    /// one is reported on open.
    pub fn age(path: PathBuf, passphrase: &str) -> Result<Self> {
        let reader = Self {
            path,
            format: Format::TarAge,
            passphrase: Some(passphrase.to_owned()),
        };
        reader.open()?;
        Ok(reader)
    }

    fn open(&self) -> Result<tar::Archive<Box<dyn Read + Send>>> {
        self.open_counted(Arc::default())
    }

    /// Opens the stream; `read` counts the archive file's bytes consumed.
    fn open_counted(&self, read: Arc<AtomicU64>) -> Result<tar::Archive<Box<dyn Read + Send>>> {
        let file = Counted {
            inner: File::open(&self.path)?,
            read,
        };
        Ok(open_stream(file, self.format, self.passphrase.as_deref())?.0)
    }
}

/// The tar inside `path` (`format`: its compression, or `TarAge` with a
/// passphrase), and the compression found inside an age file.
pub(crate) fn open_tar(
    path: &std::path::Path,
    format: Format,
    passphrase: Option<&str>,
) -> Result<(tar::Archive<Box<dyn Read + Send>>, Format)> {
    open_stream(File::open(path)?, format, passphrase)
}

fn open_stream<R: Read + Send + 'static>(
    file: R,
    format: Format,
    passphrase: Option<&str>,
) -> Result<(tar::Archive<Box<dyn Read + Send>>, Format)> {
    let file = BufReader::new(file);
    let (file, format): (Box<dyn Read + Send>, Format) = match passphrase {
        Some(pw) if format == Format::TarAge => decrypt_age(file, pw)?,
        _ => (Box::new(file), format),
    };
    let stream: Box<dyn Read + Send> = match format {
        Format::Tar => Box::new(file),
        Format::TarGz => Box::new(flate2::read::MultiGzDecoder::new(file)),
        Format::TarXz => Box::new(liblzma::read::XzDecoder::new_multi_decoder(file)),
        Format::TarZst => Box::new(zstd::stream::read::Decoder::new(file)?),
        Format::TarBz2 => Box::new(bzip2::read::MultiBzDecoder::new(file)),
        other => return Err(Error::Unsupported(other.name())),
    };
    Ok((tar::Archive::new(stream), format))
}

/// Decrypts an age stream and works out which compression is inside.
fn decrypt_age<R: Read + Send + 'static>(
    file: R,
    passphrase: &str,
) -> Result<(Box<dyn Read + Send>, Format)> {
    use std::io::BufRead;
    let decryptor = age::Decryptor::new(file).map_err(age_error)?;
    if !decryptor.is_scrypt() {
        return Err(Error::Unsupported("age files for a key (not a passphrase)"));
    }
    let identity =
        age::scrypt::Identity::new(age::secrecy::SecretString::from(passphrase.to_owned()));
    let plain = decryptor
        .decrypt(std::iter::once(&identity as &dyn age::Identity))
        .map_err(age_error)?;
    let mut plain = BufReader::with_capacity(64 * 1024, plain);
    let head = plain.fill_buf()?;
    let inner = if head.starts_with(&[0x1F, 0x8B]) {
        Format::TarGz
    } else if head.starts_with(&[0xFD, b'7', b'z', b'X', b'Z', 0x00]) {
        Format::TarXz
    } else if head.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
        Format::TarZst
    } else if head.starts_with(b"BZh") {
        Format::TarBz2
    } else {
        Format::Tar
    };
    Ok((Box::new(plain), inner))
}

fn age_error(e: age::DecryptError) -> Error {
    match e {
        age::DecryptError::Io(e) => Error::Io(e),
        age::DecryptError::DecryptionFailed | age::DecryptError::NoMatchingKeys => {
            Error::WrongPassword
        }
        age::DecryptError::ExcessiveWork { .. } => {
            Error::Corrupt("the passphrase protection is too costly to check".into())
        }
        other => Error::Corrupt(other.to_string()),
    }
}

/// How often a listing reports progress.
const LIST_BATCH_TIME: Duration = Duration::from_millis(100);

/// Counts the bytes read from the archive file (for "Reading… 40%").
struct Counted {
    inner: File,
    read: Arc<AtomicU64>,
}

impl Read for Counted {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}

/// tar reports bad headers and broken compression as plain I/O errors, many of
/// them as `Other`; real I/O failures have specific kinds.
fn tar_error(e: std::io::Error) -> Error {
    match e.kind() {
        std::io::ErrorKind::InvalidData
        | std::io::ErrorKind::InvalidInput
        | std::io::ErrorKind::UnexpectedEof
        | std::io::ErrorKind::Other => Error::Corrupt(e.to_string()),
        _ => Error::Io(e),
    }
}

impl FormatReader for TarReader {
    fn format(&self) -> Format {
        self.format
    }

    fn list(&mut self, on_batch: &mut ListSink<'_>) -> Result<Vec<RawEntry>> {
        let read = Arc::new(AtomicU64::new(0));
        let mut archive = self.open_counted(read.clone())?;
        let mut out = Vec::new();
        let mut reported = 0;
        let mut last = Instant::now();
        let method = match self.format {
            Format::TarAge => Some("age"),
            Format::TarGz => Some("Gzip"),
            Format::TarXz => Some("XZ"),
            Format::TarZst => Some("Zstandard"),
            Format::TarBz2 => Some("BZip2"),
            _ => None,
        };
        for entry in archive.entries().map_err(tar_error)? {
            // Tar has no index: hand out what we have every so often.
            if out.len() - reported >= 4096 || last.elapsed() >= LIST_BATCH_TIME {
                on_batch(&out[reported..], read.load(Ordering::Relaxed))?;
                reported = out.len();
                last = Instant::now();
            }
            let entry = entry.map_err(tar_error)?;
            let header = entry.header();
            let link = || {
                entry
                    .link_name_bytes()
                    .map(|l| l.into_owned())
                    .unwrap_or_default()
            };
            let kind = match header.entry_type() {
                tar::EntryType::Regular
                | tar::EntryType::Continuous
                | tar::EntryType::GNUSparse => RawKind::File,
                tar::EntryType::Directory => RawKind::Dir,
                tar::EntryType::Symlink => RawKind::Symlink(link()),
                tar::EntryType::Link => RawKind::Hardlink(link()),
                _ => RawKind::Other,
            };
            let is_file = kind == RawKind::File;
            let size = if kind == RawKind::File {
                entry.size()
            } else {
                0
            };
            out.push(RawEntry {
                name: entry.path_bytes().into_owned(),
                kind,
                size,
                packed_size: None,
                modified: header
                    .mtime()
                    .ok()
                    .map(|s| SystemTime::UNIX_EPOCH + Duration::from_secs(s)),
                mode: header.mode().ok().map(|m| m & 0o7777),
                crc32: None,
                encrypted: false,
                method: method.filter(|_| is_file).map(str::to_owned),
            });
        }
        on_batch(&out[reported..], read.load(Ordering::Relaxed))?;
        if out.is_empty() && self.format != Format::Tar {
            // A compressed file that holds a single file, not a tar archive.
            return Err(Error::Corrupt(
                "no tar archive inside the compressed file".into(),
            ));
        }
        Ok(out)
    }

    fn reopen(&self) -> Result<Box<dyn FormatReader>> {
        Ok(Box::new(TarReader {
            path: self.path.clone(),
            format: self.format,
            passphrase: self.passphrase.clone(),
        }))
    }

    fn read(&mut self, wanted: &dyn Fn(usize) -> bool, sink: &mut EntrySink<'_>) -> Result<()> {
        let mut archive = self.open()?;
        for (i, entry) in archive.entries().map_err(tar_error)?.enumerate() {
            let mut entry = entry.map_err(tar_error)?;
            if wanted(i) {
                sink(i, &mut entry).map_err(|e| match e {
                    Error::Io(io) => tar_error(io),
                    other => other,
                })?;
            }
        }
        Ok(())
    }
}
