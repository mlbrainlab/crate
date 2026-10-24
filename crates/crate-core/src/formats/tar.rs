use std::fs::File;
use std::io::{BufReader, Read};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use super::{EntrySink, FormatReader, RawEntry, RawKind};
use crate::entry::Format;
use crate::error::{Error, Result};

/// Plain and compressed tar. Tar can only be read front to back, so every
/// pass (list, extract, test) reopens the file.
pub(crate) struct TarReader {
    path: PathBuf,
    format: Format,
}

impl TarReader {
    pub fn new(path: PathBuf, format: Format) -> Self {
        Self { path, format }
    }

    fn open(&self) -> Result<tar::Archive<Box<dyn Read + Send>>> {
        let file = BufReader::new(File::open(&self.path)?);
        let stream: Box<dyn Read + Send> = match self.format {
            Format::Tar => Box::new(file),
            Format::TarGz => Box::new(flate2::read::MultiGzDecoder::new(file)),
            Format::TarXz => Box::new(liblzma::read::XzDecoder::new_multi_decoder(file)),
            Format::TarZst => Box::new(zstd::stream::read::Decoder::with_buffer(file)?),
            Format::TarBz2 => Box::new(bzip2::read::MultiBzDecoder::new(file)),
            other => unreachable!("not a tar format: {other:?}"),
        };
        Ok(tar::Archive::new(stream))
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

    fn list(&mut self) -> Result<Vec<RawEntry>> {
        let mut archive = self.open()?;
        let mut out = Vec::new();
        for entry in archive.entries().map_err(tar_error)? {
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
            });
        }
        if out.is_empty() && self.format != Format::Tar {
            // A compressed file that holds a single file, not a tar archive.
            return Err(Error::Corrupt(
                "no tar archive inside the compressed file".into(),
            ));
        }
        Ok(out)
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
