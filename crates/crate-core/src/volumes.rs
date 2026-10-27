//! Archives split into volumes: 7-Zip style raw splits (`name.7z.001`,
//! `.002`…, any format inside, which is what Crate's own "Split into
//! volumes" writes), RAR sets (`name.part1.rar`, `.part2.rar`…, and the old
//! `name.rar`, `.r00`, `.r01`… naming). A split set reads as one file through
//! [`Input`]; UnRAR reads RAR sets itself.
//!
//! How many parts a set has isn't written anywhere Crate can see up front,
//! so the count is the highest part found next to the first one. A missing
//! last part shows up when reading: a full-size last part means more follow.

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::{Error, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VolumeKind {
    /// `name.ext.001`: the file cut into pieces.
    Split,
    /// `name.part1.rar`, `name.part2.rar`…
    RarParts,
    /// `name.rar`, then `name.r00`, `name.r01`…
    RarOld,
}

/// The parts of a multi-volume set, in order, whether or not each exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VolumeSet {
    pub kind: VolumeKind,
    pub parts: Vec<PathBuf>,
    /// What the part after the last one found would be called.
    pub next: PathBuf,
}

impl VolumeSet {
    /// The set `path` belongs to, if its name is a volume name and (for the
    /// old RAR naming) a second part exists. Any part of the set works.
    pub fn find(path: &Path) -> Option<Self> {
        Self::find_with(path, &[])
    }

    /// [`Self::find`], with parts the user located elsewhere: a located file
    /// stands in for the part with the same name (including the ones after
    /// the last part found).
    pub fn find_with(path: &Path, located: &[PathBuf]) -> Option<Self> {
        let mut set = Self::find_named(path)?;
        let named = |want: &Path| {
            located
                .iter()
                .find(|l| l.file_name() == want.file_name() && l.exists())
                .cloned()
        };
        for part in &mut set.parts {
            if !part.exists()
                && let Some(l) = named(part)
            {
                *part = l;
            }
        }
        while let Some(l) = named(&set.next) {
            set.parts.push(l);
            set.next = set.successor(&set.next);
        }
        Some(set)
    }

    /// The name after `part` in this set's numbering.
    fn successor(&self, part: &Path) -> PathBuf {
        let dir = part.parent().unwrap_or(Path::new(""));
        let name = part
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let bump = |s: &str| -> Option<String> {
            let digits = s.bytes().rev().take_while(u8::is_ascii_digit).count();
            let (head, num) = s.split_at(s.len() - digits);
            let n: usize = num.parse().ok()?;
            Some(format!("{head}{:0width$}", n + 1, width = digits))
        };
        let next = match self.kind {
            VolumeKind::RarParts => name
                .strip_suffix(".rar")
                .or_else(|| name.strip_suffix(".RAR"))
                .and_then(bump)
                .map(|s| format!("{s}{}", &name[name.len() - 4..])),
            _ => bump(name),
        };
        dir.join(next.unwrap_or_default())
    }

    fn find_named(path: &Path) -> Option<Self> {
        let dir = path.parent().unwrap_or(Path::new(""));
        let name = path.file_name()?.to_str()?;
        let lower = name.to_ascii_lowercase();

        // name.ext.001
        // (7-Zip numbers parts with 3 digits; other numbers count only when
        // a part 1 is there, so `photo.2024` stays a plain file.)
        if let Some((base, digits)) = split_number(name, ".") {
            let width = digits.len();
            let part = |n: usize| dir.join(format!("{base}.{n:0width$}"));
            if width == 3 || part(1).exists() {
                return Some(Self::collect(VolumeKind::Split, part, 1));
            }
        }
        // name.part1.rar
        if lower.ends_with(".rar") {
            let stem = &name[..name.len() - 4];
            if let Some((base, digits)) = split_number(stem, ".part") {
                let width = digits.len();
                let ext = &name[name.len() - 4..];
                let part = |n: usize| dir.join(format!("{base}.part{n:0width$}{ext}"));
                return Some(Self::collect(VolumeKind::RarParts, part, 1));
            }
            // name.rar + name.r00
            if dir.join(format!("{stem}.r00")).exists() {
                return Some(Self::collect_old(dir, stem));
            }
            return None;
        }
        // name.r00 (old RAR, not the first part)
        if lower.len() > 4 && lower[lower.len() - 4..].starts_with(".r") {
            let tail = &name[name.len() - 2..];
            if tail.chars().all(|c| c.is_ascii_digit()) {
                let stem = &name[..name.len() - 4];
                if dir.join(format!("{stem}.rar")).exists() {
                    return Some(Self::collect_old(dir, stem));
                }
            }
        }
        None
    }

    /// Parts `first..`, up to the highest one present (looking a little past
    /// gaps, so `001, 003` still finds 003).
    fn collect(kind: VolumeKind, part: impl Fn(usize) -> PathBuf, first: usize) -> Self {
        let mut last = first;
        let mut n = first;
        let mut misses = 0;
        while misses < 20 {
            if part(n).exists() {
                last = n;
                misses = 0;
            } else {
                misses += 1;
            }
            n += 1;
        }
        Self {
            kind,
            parts: (first..=last).map(&part).collect(),
            next: part(last + 1),
        }
    }

    fn collect_old(dir: &Path, stem: &str) -> Self {
        let rest = Self::collect(
            VolumeKind::RarOld,
            |n| dir.join(format!("{stem}.r{n:02}")),
            0,
        );
        let mut parts = vec![dir.join(format!("{stem}.rar"))];
        parts.extend(rest.parts);
        Self {
            kind: VolumeKind::RarOld,
            parts,
            next: rest.next,
        }
    }

    /// Positions (0-based) of parts that aren't there.
    pub fn missing(&self) -> Vec<usize> {
        (0..self.parts.len())
            .filter(|&i| !self.parts[i].exists())
            .collect()
    }

    /// The error for the first missing part, if any.
    pub fn check(&self) -> Result<()> {
        match self.missing().first() {
            Some(&i) => Err(self.missing_error(i)),
            None => Ok(()),
        }
    }

    /// `MissingVolume` for part `i` (0-based); `i` may be past the end.
    pub fn missing_error(&self, i: usize) -> Error {
        let name = self
            .parts
            .get(i)
            .unwrap_or(&self.next)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Error::MissingVolume {
            name,
            part: i + 1,
            total: (i < self.parts.len()).then_some(self.parts.len()),
        }
    }

    /// Total size of the parts that exist.
    pub fn size(&self) -> u64 {
        self.parts
            .iter()
            .filter_map(|p| fs::metadata(p).ok())
            .map(|m| m.len())
            .sum()
    }
}

/// `name` = `base` + `sep` + digits → (`base`, digits).
fn split_number<'a>(name: &'a str, sep: &str) -> Option<(&'a str, &'a str)> {
    let digits_len = name.bytes().rev().take_while(u8::is_ascii_digit).count();
    if digits_len == 0 {
        return None;
    }
    let (head, digits) = name.split_at(name.len() - digits_len);
    let base = head.strip_suffix(sep)?;
    (!base.is_empty()).then_some((base, digits))
}

/// The archive file as one byte range: a single file, or the parts of a
/// split set one after another. Read with positioned reads, so clones and
/// threads share it.
pub(crate) struct Input {
    parts: Vec<(File, u64)>, // file, offset of its first byte
    len: u64,
}

impl Input {
    /// Opens `path`, or every part of the split set it belongs to (from
    /// any part; `located`: parts found elsewhere). A missing part is a
    /// [`Error::MissingVolume`].
    pub fn open(path: &Path, located: &[PathBuf]) -> Result<Self> {
        match VolumeSet::find_with(path, located) {
            Some(set) if set.kind == VolumeKind::Split => {
                set.check()?;
                let mut parts = Vec::with_capacity(set.parts.len());
                let mut offset = 0;
                for p in &set.parts {
                    let file = File::open(p)?;
                    let len = file.metadata()?.len();
                    parts.push((file, offset));
                    offset += len;
                }
                Ok(Self { parts, len: offset })
            }
            _ => Ok(Self::file(File::open(path)?)),
        }
    }

    pub fn file(file: File) -> Self {
        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        Self {
            parts: vec![(file, 0)],
            len,
        }
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    /// Reads at `offset`, within one part (a short read at a part's end is
    /// fine; callers loop).
    pub fn read_at(&self, buf: &mut [u8], offset: u64) -> io::Result<usize> {
        if offset >= self.len || buf.is_empty() {
            return Ok(0);
        }
        let i = self
            .parts
            .partition_point(|(_, start)| *start <= offset)
            .saturating_sub(1);
        let (file, start) = &self.parts[i];
        let end = self.parts.get(i + 1).map_or(self.len, |(_, s)| *s);
        let n = buf.len().min((end - offset) as usize);
        read_at(file, &mut buf[..n], offset - start)
    }

    /// A reader with its own position.
    pub fn reader(self: &Arc<Self>) -> InputReader {
        InputReader {
            input: self.clone(),
            pos: 0,
        }
    }
}

#[cfg(unix)]
fn read_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    std::os::unix::fs::FileExt::read_at(file, buf, offset)
}

#[cfg(windows)]
fn read_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    std::os::windows::fs::FileExt::seek_read(file, buf, offset)
}

/// `Read` + `Seek` over an [`Input`].
pub(crate) struct InputReader {
    input: Arc<Input>,
    pos: u64,
}

impl Read for InputReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.input.read_at(buf, self.pos)?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for InputReader {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let pos = match to {
            SeekFrom::Start(p) => Some(p),
            SeekFrom::End(d) => self.input.len.checked_add_signed(d),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
        };
        self.pos = pos.ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        Ok(self.pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_sets_and_gaps() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        for n in [
            "a.7z.001",
            "a.7z.003",
            "p.part1.rar",
            "p.part2.rar",
            "o.rar",
            "o.r00",
            "solo.zip",
        ] {
            fs::write(d.join(n), b"x").unwrap();
        }
        let set = VolumeSet::find(&d.join("a.7z.003")).unwrap();
        assert_eq!(set.kind, VolumeKind::Split);
        assert_eq!(set.parts.len(), 3);
        assert_eq!(set.missing(), [1]);
        match set.check() {
            Err(Error::MissingVolume { name, part, total }) => {
                assert_eq!((name.as_str(), part, total), ("a.7z.002", 2, Some(3)));
            }
            other => panic!("{other:?}"),
        }
        let rar = VolumeSet::find(&d.join("p.part2.rar")).unwrap();
        assert_eq!(rar.parts[0], d.join("p.part1.rar"));
        assert!(rar.missing().is_empty());
        let old = VolumeSet::find(&d.join("o.r00")).unwrap();
        assert_eq!(old.parts, [d.join("o.rar"), d.join("o.r00")]);
        assert_eq!(VolumeSet::find(&d.join("solo.zip")), None);
        assert_eq!(
            VolumeSet::find(&d.join("o.rar")).unwrap().kind,
            VolumeKind::RarOld
        );
        // Part 2 found in another folder.
        let other = tempfile::tempdir().unwrap();
        fs::write(other.path().join("a.7z.002"), b"x").unwrap();
        let located =
            VolumeSet::find_with(&d.join("a.7z.001"), &[other.path().join("a.7z.002")]).unwrap();
        assert!(located.missing().is_empty());
        assert_eq!(located.parts[1], other.path().join("a.7z.002"));
        assert_eq!(located.next, d.join("a.7z.004"));
    }

    #[test]
    fn reads_across_parts() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        fs::write(d.join("s.bin.001"), b"hello ").unwrap();
        fs::write(d.join("s.bin.002"), b"split ").unwrap();
        fs::write(d.join("s.bin.003"), b"world").unwrap();
        let input = Arc::new(Input::open(&d.join("s.bin.002"), &[]).unwrap());
        assert_eq!(input.len(), 17);
        let mut all = String::new();
        input.reader().read_to_string(&mut all).unwrap();
        assert_eq!(all, "hello split world");
        let mut r = input.reader();
        r.seek(SeekFrom::End(-5)).unwrap();
        let mut tail = String::new();
        r.read_to_string(&mut tail).unwrap();
        assert_eq!(tail, "world");
    }
}
