use std::fs::File;
use std::io::{BufReader, Read};

use super::{EntrySink, FormatReader, RawEntry, RawKind, civil_to_system_time};
use crate::entry::Format;
use crate::error::{Error, Result};

/// ZIP and ZIP64, through the `zip` crate.
pub(crate) struct ZipReader {
    zip: zip::ZipArchive<BufReader<File>>,
    encrypted: Vec<bool>,
    password: Option<String>,
}

impl ZipReader {
    pub fn new(file: File) -> Result<Self> {
        Ok(Self {
            zip: open_zip(file)?,
            encrypted: Vec::new(),
            password: None,
        })
    }
}

/// Most ZIPs start at byte 0, and then the central directory is found from
/// the end record alone. The crate's default instead scans the whole file for
/// signatures (a second for a 1 GB file), which only archives with data in
/// front (self-extracting .exe stubs) need; those take the slow path.
fn open_zip(mut file: File) -> Result<zip::ZipArchive<BufReader<File>>> {
    use std::io::{Seek, SeekFrom};
    use zip::read::{ArchiveOffset, Config};
    let config = Config {
        archive_offset: ArchiveOffset::Known(0),
    };
    if let Ok(zip) = zip::ZipArchive::with_config(config, BufReader::new(file.try_clone()?)) {
        return Ok(zip);
    }
    file.seek(SeekFrom::Start(0))?;
    zip::ZipArchive::new(BufReader::new(file)).map_err(zip_error)
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

    fn list(&mut self) -> Result<Vec<RawEntry>> {
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
