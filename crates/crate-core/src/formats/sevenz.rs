use std::fs::File;
use std::io::{BufReader, Read};
use std::time::{Duration, SystemTime};

use sevenz_rust2::{Archive, ArchiveEntry, BlockDecoder, Password};

use super::{EntrySink, FormatReader, RawEntry, RawKind};
use crate::entry::Format;
use crate::error::{Error, Result};

/// 7z coder id for AES-256 + SHA-256.
const AES256_SHA256: &[u8] = &[0x06, 0xF1, 0x07, 0x01];
/// Set in a 7z entry's Windows attributes when the high 16 bits hold a Unix
/// mode (p7zip and 7-Zip on Linux write this).
const UNIX_EXTENSION: u32 = 0x8000;
const S_IFMT: u32 = 0o170000;
const S_IFLNK: u32 = 0o120000;

/// 7z through `sevenz-rust2`. Solid blocks can only be decoded front to back,
/// so data is streamed block by block, and blocks with nothing wanted (e.g.
/// only encrypted entries) are never decoded.
pub(crate) struct SevenZReader {
    archive: Archive,
    source: BufReader<File>,
    password: Password,
}

impl SevenZReader {
    pub fn new(file: File, password: Option<&str>) -> Result<Self> {
        let mut source = BufReader::new(file);
        let password = password.map_or_else(Password::empty, Password::from);
        let archive = Archive::read(&mut source, &password).map_err(|e| {
            // Only an encrypted file list needs the password this early.
            match sevenz_error(e) {
                Error::PasswordRequired | Error::Corrupt(_) if !password.is_empty() => {
                    Error::WrongPassword
                }
                other => other,
            }
        })?;
        Ok(Self {
            archive,
            source,
            password,
        })
    }

    /// Calls `each` with the index and data of every entry `wanted` selects,
    /// in decoding order (block by block, then entries without data).
    fn for_each(
        &mut self,
        wanted: &dyn Fn(usize) -> bool,
        mut each: impl FnMut(usize, &mut dyn Read) -> Result<()>,
    ) -> Result<()> {
        let files = &self.archive.files;
        let block_of = &self.archive.stream_map.file_block_index;
        let (base, len) = (files.as_ptr() as usize, files.len());
        let size = std::mem::size_of::<ArchiveEntry>();
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get() as u32);
        let failure = std::cell::RefCell::new(None);
        let mut callback = |entry: &ArchiveEntry, data: &mut dyn Read| {
            // The decoder hands out references into `archive.files`; the
            // position there is the entry index.
            let index = (entry as *const ArchiveEntry as usize).wrapping_sub(base) / size;
            if index >= len {
                *failure.borrow_mut() =
                    Some(Error::Corrupt("7z entry outside the file list".into()));
                return Ok(false);
            }
            if !wanted(index) {
                return Ok(true);
            }
            match each(index, data) {
                Ok(()) => Ok(true),
                Err(e) => {
                    *failure.borrow_mut() = Some(e);
                    Ok(false)
                }
            }
        };
        for block in 0..self.archive.blocks.len() {
            if !(0..len).any(|i| block_of[i] == Some(block) && wanted(i)) {
                continue;
            }
            // With the wrong key, AES output is noise and the decoder fails.
            let wrong_password = |e: Error| match e {
                Error::PasswordRequired if !self.password.is_empty() => Error::WrongPassword,
                Error::Corrupt(_) if is_encrypted(&self.archive, block) => Error::WrongPassword,
                Error::Io(e) if is_bad_data(&e) && is_encrypted(&self.archive, block) => {
                    Error::WrongPassword
                }
                other => other,
            };
            let decoder = BlockDecoder::new(
                threads,
                block,
                &self.archive,
                &self.password,
                &mut self.source,
            );
            let more = decoder.for_each_entries(&mut callback);
            if let Some(e) = failure.borrow_mut().take() {
                return Err(wrong_password(e));
            }
            if !more.map_err(|e| wrong_password(sevenz_error(e)))? {
                return Ok(());
            }
        }
        // Folders and empty files have no data in any block.
        for i in (0..len).filter(|&i| block_of[i].is_none()) {
            let mut empty: &[u8] = &[];
            callback(&files[i], &mut empty).map_err(sevenz_error)?;
            if let Some(e) = failure.borrow_mut().take() {
                return Err(e);
            }
        }
        Ok(())
    }
}

fn sevenz_error(e: sevenz_rust2::Error) -> Error {
    use sevenz_rust2::Error as E;
    match e {
        E::PasswordRequired | E::MaybeBadPassword(_) => Error::PasswordRequired,
        E::Io(e, _) | E::FileOpen(e, _) if e.kind() != std::io::ErrorKind::InvalidData => {
            Error::Io(e)
        }
        other => Error::Corrupt(other.to_string()),
    }
}

fn is_bad_data(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof
    )
}

fn is_encrypted(archive: &Archive, block: usize) -> bool {
    archive.blocks[block]
        .coders
        .iter()
        .any(|c| c.encoder_method_id() == AES256_SHA256)
}

fn unix_mode(e: &ArchiveEntry) -> Option<u32> {
    (e.has_windows_attributes && e.windows_attributes & UNIX_EXTENSION != 0)
        .then_some(e.windows_attributes >> 16)
}

fn system_time(nt: sevenz_rust2::NtTime) -> Option<SystemTime> {
    // 100 ns ticks since 1601-01-01.
    let secs = (u64::from(nt) / 10_000_000).checked_sub(11_644_473_600)?;
    Some(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
}

impl FormatReader for SevenZReader {
    fn format(&self) -> Format {
        Format::SevenZ
    }

    fn list(&mut self) -> Result<Vec<RawEntry>> {
        let archive = &self.archive;
        let encrypted_block = |file: usize| {
            archive.stream_map.file_block_index[file].is_some_and(|b| is_encrypted(archive, b))
        };
        let mut out: Vec<RawEntry> = archive
            .files
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let mode = unix_mode(e);
                let kind = if e.is_anti_item {
                    RawKind::Other
                } else if e.is_directory {
                    RawKind::Dir
                } else if mode.is_some_and(|m| m & S_IFMT == S_IFLNK) {
                    RawKind::Symlink(Vec::new()) // target filled in below
                } else {
                    RawKind::File
                };
                RawEntry {
                    name: e.name.as_bytes().to_vec(),
                    size: if kind == RawKind::File { e.size } else { 0 },
                    kind,
                    packed_size: None,
                    modified: e
                        .has_last_modified_date
                        .then(|| system_time(e.last_modified_date))
                        .flatten(),
                    mode: mode.map(|m| m & 0o7777),
                    crc32: e.has_crc.then_some(e.crc as u32),
                    encrypted: encrypted_block(i),
                }
            })
            .collect();
        // A symlink's target is its data; read it only if there are any.
        let links: Vec<usize> = (0..out.len())
            .filter(|&i| matches!(out[i].kind, RawKind::Symlink(_)) && !out[i].encrypted)
            .collect();
        if !links.is_empty() {
            let mut targets = Vec::new();
            self.for_each(&|i| links.contains(&i), |i, data| {
                let mut target = Vec::new();
                data.take(4096).read_to_end(&mut target)?;
                targets.push((i, target));
                Ok(())
            })?;
            for (i, target) in targets {
                out[i].kind = RawKind::Symlink(target);
            }
        }
        for e in &mut out {
            if e.encrypted && matches!(e.kind, RawKind::Symlink(_)) {
                e.kind = RawKind::Other;
            }
        }
        Ok(out)
    }

    fn read(&mut self, wanted: &dyn Fn(usize) -> bool, sink: &mut EntrySink<'_>) -> Result<()> {
        self.for_each(wanted, |i, data| sink(i, data))
    }

    fn set_password(&mut self, password: Option<&str>) {
        self.password = password.map_or_else(Password::empty, Password::from);
    }
}
