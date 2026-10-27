use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;

use super::{EntrySink, FormatReader, ListSink, RawEntry, RawKind, civil_to_system_time};
use crate::entry::Format;
use crate::error::{Error, Result};
use crate::volumes::Input;

/// ZIP and ZIP64, through the `zip` crate.
pub(crate) struct ZipReader {
    zip: zip::ZipArchive<SharedFile>,
    encrypted: Vec<bool>,
    password: Option<String>,
}

impl ZipReader {
    pub fn new(input: Arc<Input>) -> Result<Self> {
        Ok(Self {
            zip: open_zip_input(input)?,
            encrypted: Vec::new(),
            password: None,
        })
    }
}

/// Most ZIPs start at byte 0, and then the central directory is found from
/// the end record alone. The crate's default instead scans the whole file for
/// signatures (a second for a 1 GB file), which only archives with data in
/// front (self-extracting .exe stubs) need; those take the slow path.
pub(crate) fn open_zip(file: File) -> Result<zip::ZipArchive<SharedFile>> {
    open_zip_input(Arc::new(Input::file(file)))
}

/// [`open_zip`] for any [`Input`] (a split set reads as one file).
pub(crate) fn open_zip_input(input: Arc<Input>) -> Result<zip::ZipArchive<SharedFile>> {
    use zip::read::{ArchiveOffset, Config};
    let file = SharedFile::new(input);
    let config = Config {
        archive_offset: ArchiveOffset::Known(0),
    };
    if let Ok(zip) = zip::ZipArchive::with_config(config, file.clone()) {
        return Ok(zip);
    }
    zip::ZipArchive::new(file).map_err(zip_error)
}

/// A buffered file reader that can be cloned: every clone has its own
/// position and buffer and reads with positioned reads, so a cloned
/// `ZipArchive` (which shares the parsed central directory) reads in
/// parallel without reopening or re-parsing anything.
pub(crate) struct SharedFile {
    file: Arc<Input>,
    pos: u64,
    len: u64,
    buf: Vec<u8>,
    /// File offset of `buf[0]`.
    buf_start: u64,
}

impl SharedFile {
    const BUF: usize = 64 * 1024;

    fn new(file: Arc<Input>) -> Self {
        let len = file.len();
        Self {
            file,
            pos: 0,
            len,
            buf: Vec::new(),
            buf_start: 0,
        }
    }
}

impl Clone for SharedFile {
    fn clone(&self) -> Self {
        Self {
            file: self.file.clone(),
            pos: self.pos,
            len: self.len,
            buf: Vec::new(),
            buf_start: 0,
        }
    }
}

impl Read for SharedFile {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let in_buf =
            self.pos >= self.buf_start && self.pos < self.buf_start + self.buf.len() as u64;
        if !in_buf {
            if out.len() >= Self::BUF {
                // Big reads skip the buffer.
                let n = self.file.read_at(out, self.pos)?;
                self.pos += n as u64;
                return Ok(n);
            }
            self.buf.resize(Self::BUF, 0);
            let n = self.file.read_at(&mut self.buf, self.pos)?;
            self.buf.truncate(n);
            self.buf_start = self.pos;
            if n == 0 {
                return Ok(0);
            }
        }
        let at = (self.pos - self.buf_start) as usize;
        let n = out.len().min(self.buf.len() - at);
        out[..n].copy_from_slice(&self.buf[at..at + n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for SharedFile {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        let pos = match to {
            SeekFrom::Start(p) => Some(p),
            SeekFrom::End(d) => self.len.checked_add_signed(d),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
        };
        self.pos = pos.ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "seek before start")
        })?;
        Ok(self.pos)
    }
}

fn method_name(m: zip::CompressionMethod) -> String {
    use zip::CompressionMethod as M;
    match m {
        M::Stored => "Stored".into(),
        M::Deflated => "Deflate".into(),
        M::Deflate64 => "Deflate64".into(),
        M::Bzip2 => "BZip2".into(),
        M::Zstd => "Zstandard".into(),
        M::Lzma => "LZMA".into(),
        M::Xz => "XZ".into(),
        M::Aes => "AES".into(),
        other => other.to_string(),
    }
}

fn zip_error(e: zip::result::ZipError) -> Error {
    match e {
        zip::result::ZipError::Io(e) => Error::Io(e),
        zip::result::ZipError::InvalidPassword => Error::WrongPassword,
        other => Error::Corrupt(other.to_string()),
    }
}

impl FormatReader for ZipReader {
    fn format(&self) -> Format {
        Format::Zip
    }

    fn list(&mut self, _on_batch: &mut ListSink<'_>) -> Result<Vec<RawEntry>> {
        let mut out = Vec::with_capacity(self.zip.len());
        for i in 0..self.zip.len() {
            let f = self.zip.by_index_raw(i).map_err(zip_error)?;
            let kind = if f.is_dir() {
                RawKind::Dir
            } else if f.is_symlink() {
                RawKind::Symlink(Vec::new()) // target filled in below
            } else if f.is_file() {
                RawKind::File
            } else {
                RawKind::Other
            };
            let modified = f.last_modified().and_then(|t| {
                civil_to_system_time(
                    t.year() as i64,
                    t.month() as u32,
                    t.day() as u32,
                    t.hour() as u32,
                    t.minute() as u32,
                    t.second() as u32,
                )
            });
            let entry = RawEntry {
                name: f.name_raw().to_vec(),
                kind,
                size: f.size(),
                packed_size: Some(f.compressed_size()),
                modified,
                mode: f.unix_mode().map(|m| m & 0o7777),
                crc32: Some(f.crc32()),
                encrypted: f.encrypted(),
                method: (!f.is_dir()).then(|| method_name(f.compression())),
            };
            out.push(entry);
        }
        // A ZIP symlink stores its target as the entry's data.
        for (i, e) in out.iter_mut().enumerate() {
            if matches!(e.kind, RawKind::Symlink(_)) {
                if e.encrypted || e.size > 4096 {
                    e.kind = RawKind::Other;
                    continue;
                }
                let mut target = Vec::new();
                self.zip
                    .by_index(i)
                    .map_err(zip_error)?
                    .read_to_end(&mut target)?;
                e.kind = RawKind::Symlink(target);
            }
        }
        self.encrypted = out.iter().map(|e| e.encrypted).collect();
        Ok(out)
    }

    fn set_password(&mut self, password: Option<&str>) {
        self.password = password.map(str::to_owned);
    }

    fn reopen(&self) -> Result<Box<dyn FormatReader>> {
        Ok(Box::new(ZipReader {
            zip: self.zip.clone(),
            encrypted: self.encrypted.clone(),
            password: self.password.clone(),
        }))
    }

    fn read(&mut self, wanted: &dyn Fn(usize) -> bool, sink: &mut EntrySink<'_>) -> Result<()> {
        for i in 0..self.zip.len() {
            if !wanted(i) {
                continue;
            }
            // The sink reads to the end, which makes the zip crate check the CRC.
            let mut f = match &self.password {
                Some(pw) if self.encrypted.get(i) == Some(&true) => {
                    self.zip.by_index_decrypt(i, pw.as_bytes())
                }
                _ => self.zip.by_index(i),
            }
            .map_err(zip_error)?;
            sink(i, &mut f)?;
        }
        Ok(())
    }
}
