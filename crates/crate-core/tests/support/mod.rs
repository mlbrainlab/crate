//! Test archives, generated rather than downloaded (M1). Used by the
//! integration tests and by `cargo run -p crate-core --example gen_fixtures`,
//! which writes them into `tests/fixtures/` for trying the CLI by hand.
#![allow(dead_code)]

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate_core::{Archive, CancelToken, ExtractOptions, ExtractReport, Selection, SkipReason};

/// A minimal ZIP writer (stored entries only) that writes names and headers
/// exactly as given, so tests can build archives a normal writer refuses:
/// `../` names, absolute paths, legacy-encoded names, wrong CRCs, lying sizes.
#[derive(Default)]
pub struct RawZip {
    entries: Vec<RawZipEntry>,
}

struct RawZipEntry {
    name: Vec<u8>,
    data: Vec<u8>,
    unix_mode: Option<u32>,
    utf8: bool,
    crc: Option<u32>,
    declared_size: Option<u32>,
}

impl RawZip {
    pub fn new() -> Self {
        Self::default()
    }

    fn push(&mut self, name: &[u8], data: &[u8], unix_mode: Option<u32>) -> &mut Self {
        let utf8 = std::str::from_utf8(name).is_ok_and(|s| !s.is_ascii());
        self.entries.push(RawZipEntry {
            name: name.to_vec(),
            data: data.to_vec(),
            unix_mode,
            utf8,
            crc: None,
            declared_size: None,
        });
        self
    }

    pub fn file(&mut self, name: impl AsRef<[u8]>, data: impl AsRef<[u8]>) -> &mut Self {
        self.push(name.as_ref(), data.as_ref(), Some(0o100644))
    }

    pub fn dir(&mut self, name: &str) -> &mut Self {
        self.push(name.as_bytes(), b"", Some(0o040755))
    }

    pub fn symlink(&mut self, name: &str, target: &str) -> &mut Self {
        self.push(name.as_bytes(), target.as_bytes(), Some(0o120777))
    }

    /// The last entry gets this CRC instead of the real one.
    pub fn wrong_crc(&mut self) -> &mut Self {
        let e = self.entries.last_mut().unwrap();
        e.crc = Some(crc32fast::hash(&e.data) ^ 0xDEAD_BEEF);
        self
    }

    /// The last entry claims to unpack to `size` bytes.
    pub fn declared_size(&mut self, size: u32) -> &mut Self {
        self.entries.last_mut().unwrap().declared_size = Some(size);
        self
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for e in &self.entries {
            let offset = out.len() as u32;
            let crc = e.crc.unwrap_or_else(|| crc32fast::hash(&e.data));
            let csize = e.data.len() as u32;
            let usize = e.declared_size.unwrap_or(csize);
            let flags: u16 = if e.utf8 { 1 << 11 } else { 0 };
            let name_len = e.name.len() as u16;
            // Local file header: stored, 1980-01-01 00:00.
            out.write_all(&0x0403_4b50u32.to_le_bytes()).unwrap();
            for v in [20u16, flags, 0, 0, 0x21] {
                out.write_all(&v.to_le_bytes()).unwrap();
            }
            for v in [crc, csize, usize] {
                out.write_all(&v.to_le_bytes()).unwrap();
            }
            for v in [name_len, 0] {
                out.write_all(&v.to_le_bytes()).unwrap();
            }
            out.write_all(&e.name).unwrap();
            out.write_all(&e.data).unwrap();
            // Central directory record.
            let made_by: u16 = if e.unix_mode.is_some() {
                (3 << 8) | 20
            } else {
                20
            };
            let is_dir = e.unix_mode.is_some_and(|m| m & 0o170000 == 0o040000);
            let external = (e.unix_mode.unwrap_or(0) << 16) | if is_dir { 0x10 } else { 0 };
            central.write_all(&0x0201_4b50u32.to_le_bytes()).unwrap();
            for v in [made_by, 20, flags, 0, 0, 0x21] {
                central.write_all(&v.to_le_bytes()).unwrap();
            }
            for v in [crc, csize, usize] {
                central.write_all(&v.to_le_bytes()).unwrap();
            }
            for v in [name_len, 0, 0, 0, 0] {
                central.write_all(&v.to_le_bytes()).unwrap();
            }
            for v in [external, offset] {
                central.write_all(&v.to_le_bytes()).unwrap();
            }
            central.write_all(&e.name).unwrap();
        }
        let cd_offset = out.len() as u32;
        out.extend_from_slice(&central);
        let n = self.entries.len() as u16;
        out.write_all(&0x0605_4b50u32.to_le_bytes()).unwrap();
        for v in [0u16, 0, n, n] {
            out.write_all(&v.to_le_bytes()).unwrap();
        }
        for v in [central.len() as u32, cd_offset] {
            out.write_all(&v.to_le_bytes()).unwrap();
        }
        out.write_all(&0u16.to_le_bytes()).unwrap();
        out
    }

    pub fn write(&self, path: &Path) {
        fs::write(path, self.to_bytes()).unwrap();
    }
}

/// A normal archive: nested folders, a Unicode name, deflate-compressed.
pub fn sample_zip(path: &Path) {
    use zip::write::SimpleFileOptions;
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let opts = SimpleFileOptions::default().unix_permissions(0o644);
    zip.add_directory("project/", opts).unwrap();
    zip.add_directory("project/docs/", opts).unwrap();
    zip.start_file("project/README.md", opts).unwrap();
    zip.write_all(b"# Sample\n").unwrap();
    zip.start_file("project/docs/guide.txt", opts).unwrap();
    zip.write_all(&b"guide line\n".repeat(1000)).unwrap();
    zip.start_file("project/docs/تقرير.txt", opts).unwrap();
    zip.write_all("مرحبا\n".as_bytes()).unwrap();
    zip.start_file("project/run.sh", opts.unix_permissions(0o4755))
        .unwrap();
    zip.write_all(b"#!/bin/sh\necho hi\n").unwrap();
    zip.finish().unwrap();
}

/// Names that try to leave the destination.
pub fn traversal_zip(path: &Path) {
    RawZip::new()
        .file("ok.txt", "fine")
        .file("../evil.txt", "escaped")
        .file("a/../../evil2.txt", "escaped")
        .file("..\\evil3.txt", "escaped")
        .file("/tmp/crate-absolute.txt", "absolute")
        .file("C:\\crate-drive.txt", "drive")
        .write(path);
}

/// Links that try to leave the destination, or to be written through.
pub fn symlink_zip(path: &Path) {
    RawZip::new()
        .file("docs/readme.txt", "inside")
        .symlink("good-link", "docs/readme.txt")
        .symlink("escape", "../../outside")
        .symlink("absolute", "/etc/passwd")
        .symlink("sneaky", "docs/../..")
        .file("escape/pwned.txt", "through an escaping link")
        .symlink("dirlink", "docs")
        .file("dirlink/pwned2.txt", "through an inside link")
        .write(path);
}

/// Arabic names stored in Windows-1256 without the UTF-8 flag.
pub fn arabic_cp1256_zip(path: &Path) {
    let name = |s: &str| encoding_rs::WINDOWS_1256.encode(s).0.into_owned();
    RawZip::new()
        .file(name("التقرير السنوي.txt"), "report")
        .file(name("الصور/رحلة الصيف.jpg"), "photo")
        .file("readme.txt", "plain")
        .write(path);
}

/// A small file that expands about 1000× (100 MiB of zeros).
pub fn bomb_zip(path: &Path) {
    use zip::write::SimpleFileOptions;
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let opts = SimpleFileOptions::default().compression_level(Some(9));
    zip.start_file("zeros.bin", opts).unwrap();
    let chunk = vec![0u8; 1 << 20];
    for _ in 0..100 {
        zip.write_all(&chunk).unwrap();
    }
    zip.finish().unwrap();
}

/// An entry with more data than its header declares.
pub fn lying_size_zip(path: &Path) {
    RawZip::new()
        .file("liar.bin", vec![1u8; 64 * 1024])
        .declared_size(10)
        .write(path);
}

/// An entry whose CRC doesn't match its data.
pub fn bad_crc_zip(path: &Path) {
    RawZip::new()
        .file("good.txt", "good")
        .file("bad.txt", "corrupted")
        .wrong_crc()
        .write(path);
}

/// An AES-256 encrypted entry next to a plain one (password "secret").
pub fn encrypted_zip(path: &Path) {
    use zip::write::SimpleFileOptions;
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    zip.start_file("plain.txt", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"plain").unwrap();
    let opts = SimpleFileOptions::default().with_aes_encryption(zip::AesMode::Aes256, "secret");
    zip.start_file("secret.txt", opts).unwrap();
    zip.write_all(b"secret").unwrap();
    zip.finish().unwrap();
}

/// Compression around a tar stream.
#[derive(Clone, Copy, Debug)]
pub enum Comp {
    None,
    Gz,
    Xz,
    Zst,
    Bz2,
}

impl Comp {
    pub const ALL: [Comp; 5] = [Comp::None, Comp::Gz, Comp::Xz, Comp::Zst, Comp::Bz2];

    pub fn ext(self) -> &'static str {
        match self {
            Comp::None => "tar",
            Comp::Gz => "tar.gz",
            Comp::Xz => "tar.xz",
            Comp::Zst => "tar.zst",
            Comp::Bz2 => "tar.bz2",
        }
    }

    pub fn compress(self, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            Comp::None => out.extend_from_slice(data),
            Comp::Gz => {
                let mut w = flate2::write::GzEncoder::new(&mut out, flate2::Compression::default());
                w.write_all(data).unwrap();
                w.finish().unwrap();
            }
            Comp::Xz => {
                let mut w = liblzma::write::XzEncoder::new(&mut out, 6);
                w.write_all(data).unwrap();
                w.finish().unwrap();
            }
            Comp::Zst => out = zstd::encode_all(data, 3).unwrap(),
            Comp::Bz2 => {
                let mut w = bzip2::write::BzEncoder::new(&mut out, bzip2::Compression::default());
                w.write_all(data).unwrap();
                w.finish().unwrap();
            }
        }
        out
    }
}

/// A tar writer that puts names and link targets into headers exactly as
/// given (the `tar` crate's own setters refuse `..`).
pub struct RawTar {
    builder: tar::Builder<Vec<u8>>,
}

impl RawTar {
    pub fn new() -> Self {
        Self {
            builder: tar::Builder::new(Vec::new()),
        }
    }

    fn entry(
        &mut self,
        name: &[u8],
        kind: tar::EntryType,
        data: &[u8],
        link: &[u8],
        mode: u32,
    ) -> &mut Self {
        let mut h = tar::Header::new_gnu();
        {
            let gnu = h.as_gnu_mut().unwrap();
            gnu.name[..name.len()].copy_from_slice(name);
            gnu.linkname[..link.len()].copy_from_slice(link);
        }
        h.set_entry_type(kind);
        h.set_size(data.len() as u64);
        h.set_mode(mode);
        h.set_mtime(1_792_845_015);
        h.set_cksum();
        self.builder.append(&h, data).unwrap();
        self
    }

    pub fn file(&mut self, name: impl AsRef<[u8]>, data: impl AsRef<[u8]>) -> &mut Self {
        self.entry(
            name.as_ref(),
            tar::EntryType::Regular,
            data.as_ref(),
            b"",
            0o644,
        )
    }

    pub fn file_mode(&mut self, name: &str, data: &[u8], mode: u32) -> &mut Self {
        self.entry(name.as_bytes(), tar::EntryType::Regular, data, b"", mode)
    }

    pub fn dir(&mut self, name: &str) -> &mut Self {
        self.entry(name.as_bytes(), tar::EntryType::Directory, b"", b"", 0o755)
    }

    pub fn symlink(&mut self, name: &str, target: &str) -> &mut Self {
        self.entry(
            name.as_bytes(),
            tar::EntryType::Symlink,
            b"",
            target.as_bytes(),
            0o777,
        )
    }

    pub fn hardlink(&mut self, name: &str, target: &str) -> &mut Self {
        self.entry(
            name.as_bytes(),
            tar::EntryType::Link,
            b"",
            target.as_bytes(),
            0o644,
        )
    }

    pub fn device(&mut self, name: &str) -> &mut Self {
        self.entry(name.as_bytes(), tar::EntryType::Char, b"", b"", 0o600)
    }

    pub fn finish(&mut self) -> Vec<u8> {
        self.builder.finish().unwrap();
        self.builder.get_ref().clone()
    }

    pub fn write(&mut self, path: &Path, comp: Comp) {
        fs::write(path, comp.compress(&self.finish())).unwrap();
    }
}

impl Default for RawTar {
    fn default() -> Self {
        Self::new()
    }
}

/// The tar counterpart of `sample_zip`, plus a hard link.
pub fn sample_tar(path: &Path, comp: Comp) {
    RawTar::new()
        .dir("project/")
        .dir("project/docs/")
        .file("project/README.md", "# Sample\n")
        .file("project/docs/guide.txt", b"guide line\n".repeat(1000))
        .file("project/docs/تقرير.txt", "مرحبا\n")
        .file_mode("project/run.sh", b"#!/bin/sh\necho hi\n", 0o4755)
        .hardlink("project/docs/guide-copy.txt", "project/docs/guide.txt")
        .write(path, comp);
}

/// Every way a tar entry can try to escape, plus a device node.
pub fn malicious_tar(path: &Path, comp: Comp) {
    RawTar::new()
        .file("ok.txt", "fine")
        .file("../evil.txt", "escaped")
        .file("/tmp/crate-tar-absolute.txt", "absolute")
        .symlink("escape", "../../outside")
        .file("escape/pwned.txt", "through a link")
        .hardlink("hard-escape", "../../etc/passwd")
        .hardlink("hard-absolute", "/etc/passwd")
        .device("dev-null")
        .write(path, comp);
}

/// 512 MiB of zeros in a few hundred bytes of zstd.
pub fn bomb_tar_zst(path: &Path) {
    let file = fs::File::create(path).unwrap();
    let mut builder = tar::Builder::new(zstd::Encoder::new(file, 3).unwrap().auto_finish());
    let mut h = tar::Header::new_gnu();
    h.set_path("zeros.bin").unwrap();
    h.set_size(512 << 20);
    h.set_mode(0o644);
    h.set_cksum();
    builder
        .append(&h, std::io::repeat(0).take(512 << 20))
        .unwrap();
    builder.finish().unwrap();
}

/// A tar.gz cut off halfway.
pub fn truncated_tar_gz(path: &Path) {
    sample_tar(path, Comp::Gz);
    let data = fs::read(path).unwrap();
    fs::write(path, &data[..data.len() / 2]).unwrap();
}

// --- Test harness ---

/// A temp folder with the archive in `in/` and extraction going to `out/`.
pub struct Case {
    pub dir: tempfile::TempDir,
}

impl Case {
    /// `name` is the archive's file name, e.g. `test.tar.gz`.
    pub fn new(name: &str, make: impl FnOnce(&Path)) -> (Self, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("in")).unwrap();
        let archive = dir.path().join("in").join(name);
        make(&archive);
        (Self { dir }, archive)
    }

    pub fn out(&self) -> PathBuf {
        self.dir.path().join("out")
    }

    /// Everything in the temp folder except the archive's own folder.
    pub fn written(&self) -> Vec<String> {
        let mut found = Vec::new();
        walk(self.dir.path(), self.dir.path(), &mut found);
        found.retain(|p| !p.starts_with("in"));
        found.sort();
        found
    }
}

fn walk(root: &Path, dir: &Path, found: &mut Vec<String>) {
    for e in fs::read_dir(dir).unwrap() {
        let path = e.unwrap().path();
        found.push(
            path.strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        );
        if fs::symlink_metadata(&path).unwrap().is_dir() {
            walk(root, &path, found);
        }
    }
}

pub fn extract_to(
    archive: &Path,
    dest: &Path,
    options: &ExtractOptions,
) -> crate_core::Result<ExtractReport> {
    Archive::open(archive)?.extract(
        &Selection::All,
        dest,
        options,
        &mut |_| {},
        &CancelToken::new(),
    )
}

pub fn extract(archive: &Path, dest: &Path) -> crate_core::Result<ExtractReport> {
    extract_to(archive, dest, &ExtractOptions::default())
}

pub fn reasons(report: &ExtractReport) -> Vec<(String, SkipReason)> {
    let mut r: Vec<_> = report
        .skipped
        .iter()
        .map(|s| (s.path.clone(), s.reason))
        .collect();
    r.sort_by(|a, b| a.0.cmp(&b.0));
    r
}

// --- 7z ---

fn sevenz_entry(name: &str, mode: Option<u32>) -> sevenz_rust2::ArchiveEntry {
    let mut e = sevenz_rust2::ArchiveEntry::new_file(name);
    if let Some(mode) = mode {
        // p7zip's way of storing a Unix mode: high 16 bits + 0x8000.
        e.has_windows_attributes = true;
        e.windows_attributes = (mode << 16) | 0x8000 | 0x20;
    }
    e
}

/// The 7z counterpart of `sample_zip`. `solid` packs every file into one
/// block, as 7-Zip does by default.
pub fn sample_7z(path: &Path, solid: bool) {
    use sevenz_rust2::{ArchiveEntry, ArchiveWriter, SourceReader};
    let mut w = ArchiveWriter::create(path).unwrap();
    w.push_archive_entry::<&[u8]>(ArchiveEntry::new_directory("project"), None)
        .unwrap();
    w.push_archive_entry::<&[u8]>(ArchiveEntry::new_directory("project/docs"), None)
        .unwrap();
    let guide = b"guide line\n".repeat(1000);
    let files: Vec<(&str, &[u8], u32)> = vec![
        ("project/README.md", b"# Sample\n", 0o100644),
        ("project/docs/guide.txt", &guide, 0o100644),
        ("project/docs/تقرير.txt", "مرحبا\n".as_bytes(), 0o100644),
        ("project/run.sh", b"#!/bin/sh\necho hi\n", 0o104755),
    ];
    if solid {
        let entries = files
            .iter()
            .map(|(n, _, m)| sevenz_entry(n, Some(*m)))
            .collect();
        let readers = files
            .iter()
            .map(|(_, d, _)| SourceReader::new(*d))
            .collect();
        w.push_archive_entries(entries, readers).unwrap();
    } else {
        for (name, data, mode) in &files {
            w.push_archive_entry(sevenz_entry(name, Some(*mode)), Some(*data))
                .unwrap();
        }
    }
    w.push_archive_entry(
        sevenz_entry("project/latest", Some(0o120777)),
        Some(&b"docs/guide.txt"[..]),
    )
    .unwrap();
    w.finish().unwrap();
}

/// Escaping names and links in 7z.
pub fn malicious_7z(path: &Path) {
    let mut w = sevenz_rust2::ArchiveWriter::create(path).unwrap();
    for (name, data, mode) in [
        ("ok.txt", &b"fine"[..], None),
        ("../evil.txt", b"escaped", None),
        ("/tmp/crate-7z-absolute.txt", b"absolute", None),
        ("escape", b"../../outside", Some(0o120777)),
        ("escape/pwned.txt", b"through a link", None),
    ] {
        w.push_archive_entry(sevenz_entry(name, mode), Some(data))
            .unwrap();
    }
    w.finish().unwrap();
}

/// AES-256 encrypted 7z (password "secret"). With `encrypted_names` even the
/// file list needs the password.
pub fn encrypted_7z(path: &Path, encrypted_names: bool) {
    use sevenz_rust2::encoder_options::AesEncoderOptions;
    use sevenz_rust2::{ArchiveWriter, EncoderMethod};
    let mut w = ArchiveWriter::create(path).unwrap();
    w.set_content_methods(vec![
        AesEncoderOptions::new("secret".into()).into(),
        EncoderMethod::LZMA2.into(),
    ]);
    w.set_encrypt_header(encrypted_names);
    w.push_archive_entry(sevenz_entry("secret.txt", None), Some(&b"secret"[..]))
        .unwrap();
    w.finish().unwrap();
}

/// ZIP64 headers on every entry (as written for files over 4 GiB).
pub fn zip64_zip(path: &Path) {
    use zip::write::SimpleFileOptions;
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let opts = SimpleFileOptions::default().large_file(true);
    zip.start_file("big/one.bin", opts).unwrap();
    zip.write_all(&[1u8; 5000]).unwrap();
    zip.start_file("big/two.txt", opts).unwrap();
    zip.write_all(b"zip64").unwrap();
    zip.finish().unwrap();
}
