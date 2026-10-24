//! One reader per format. Readers only list entries and hand out their data;
//! everything about writing to disk lives in `crate::archive` and
//! `crate::safety`, so every format gets the same protections.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::entry::Format;
use crate::error::{Error, Result};

mod sevenz;
mod tar;
mod zip;

/// Entry type with names still as raw bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RawKind {
    File,
    Dir,
    Symlink(Vec<u8>),
    Hardlink(Vec<u8>),
    Other,
}

/// An entry as the reader sees it; names are decoded later, for the whole
/// archive at once.
#[derive(Clone, Debug)]
pub(crate) struct RawEntry {
    pub name: Vec<u8>,
    pub kind: RawKind,
    pub size: u64,
    pub packed_size: Option<u64>,
    pub modified: Option<SystemTime>,
    pub mode: Option<u32>,
    pub crc32: Option<u32>,
    pub encrypted: bool,
}

/// Receives each wanted entry's index and data, in archive order.
pub(crate) type EntrySink<'a> = dyn FnMut(usize, &mut dyn Read) -> Result<()> + 'a;

pub(crate) trait FormatReader: Send {
    fn format(&self) -> Format;
    /// All entries, in archive order; the position is the entry index.
    fn list(&mut self) -> Result<Vec<RawEntry>>;
    /// Streams the data of every entry for which `wanted` is true, in archive
    /// order. Directories and links get an empty reader.
    fn read(&mut self, wanted: &dyn Fn(usize) -> bool, sink: &mut EntrySink<'_>) -> Result<()>;
}

/// Opens the right reader by looking at the file's first bytes.
pub(crate) fn open(path: &Path) -> Result<Box<dyn FormatReader>> {
    let mut file = File::open(path)?;
    let mut magic = [0u8; 264];
    let n = read_up_to(&mut file, &mut magic)?;
    file.seek(SeekFrom::Start(0))?;
    let magic = &magic[..n];
    match detect(magic) {
        Some(Format::Zip) => Ok(Box::new(zip::ZipReader::new(file)?)),
        Some(
            f @ (Format::Tar | Format::TarGz | Format::TarXz | Format::TarZst | Format::TarBz2),
        ) => Ok(Box::new(tar::TarReader::new(path.to_owned(), f))),
        Some(Format::SevenZ) => Ok(Box::new(sevenz::SevenZReader::new(file)?)),
        Some(other) => Err(Error::Unsupported(other.name())),
        None => Err(Error::UnknownFormat(path.to_owned())),
    }
}

fn detect(m: &[u8]) -> Option<Format> {
    if m.starts_with(b"PK\x03\x04") || m.starts_with(b"PK\x05\x06") || m.starts_with(b"PK\x07\x08")
    {
        Some(Format::Zip)
    } else if m.starts_with(b"7z\xBC\xAF\x27\x1C") {
        Some(Format::SevenZ)
    } else if m.starts_with(b"Rar!\x1A\x07") {
        Some(Format::Rar)
    } else if m.starts_with(&[0x1F, 0x8B]) {
        Some(Format::TarGz)
    } else if m.starts_with(&[0xFD, b'7', b'z', b'X', b'Z', 0x00]) {
        Some(Format::TarXz)
    } else if m.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
        Some(Format::TarZst)
    } else if m.starts_with(b"BZh") {
        Some(Format::TarBz2)
    } else if m.len() >= 262 && &m[257..262] == b"ustar" {
        Some(Format::Tar)
    } else {
        None
    }
}

fn read_up_to(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

/// Seconds since 1970 for a civil date and time, read as UTC (ZIP and DOS
/// times carry no zone).
pub(crate) fn civil_to_system_time(
    y: i64,
    mo: u32,
    d: u32,
    h: u32,
    mi: u32,
    s: u32,
) -> Option<SystemTime> {
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }
    // Howard Hinnant's days_from_civil.
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (mo as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let secs = days * 86_400 + (h * 3600 + mi * 60 + s) as i64;
    u64::try_from(secs)
        .ok()
        .map(|s| SystemTime::UNIX_EPOCH + Duration::from_secs(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_time() {
        let t = civil_to_system_time(2026, 10, 24, 12, 30, 15).unwrap();
        assert_eq!(
            t.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs(),
            1_792_845_015
        );
        assert_eq!(
            civil_to_system_time(1970, 1, 1, 0, 0, 0),
            Some(SystemTime::UNIX_EPOCH)
        );
        assert_eq!(civil_to_system_time(2026, 13, 1, 0, 0, 0), None);
    }
}
