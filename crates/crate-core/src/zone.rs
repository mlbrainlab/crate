//! The "downloaded from the internet" mark (Mark-of-the-Web). On Windows a
//! browser saves it in a file's `Zone.Identifier` stream, and SmartScreen and
//! Office's Protected View check it before running or opening the file.
//! Extracting copies the archive's mark onto every file it writes, as
//! Explorer does, so a program inside a downloaded archive is still known to
//! come from the internet. Other systems have no such mark: nothing to do.

use std::io;
use std::path::Path;

/// The archive's mark, if it has one.
#[cfg(windows)]
pub fn read(archive: &Path) -> Option<Vec<u8>> {
    std::fs::read(stream(archive))
        .ok()
        .filter(|m| !m.is_empty())
}

/// Puts `mark` on `file`. Fails where the file system has no alternate
/// streams (FAT32, some network shares); callers carry on without it.
#[cfg(windows)]
pub fn write(file: &Path, mark: &[u8]) -> io::Result<()> {
    std::fs::write(stream(file), mark)
}

#[cfg(windows)]
fn stream(path: &Path) -> std::ffi::OsString {
    let mut s = path.as_os_str().to_owned();
    s.push(":Zone.Identifier");
    s
}

#[cfg(not(windows))]
pub fn read(_archive: &Path) -> Option<Vec<u8>> {
    None
}

#[cfg(not(windows))]
pub fn write(_file: &Path, _mark: &[u8]) -> io::Result<()> {
    Ok(())
}
