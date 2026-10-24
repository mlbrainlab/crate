//! Test archives, generated rather than downloaded (M1). Used by the
//! integration tests and by `cargo run -p crate-core --example gen_fixtures`,
//! which writes them into `tests/fixtures/` for trying the CLI by hand.
#![allow(dead_code)]

use std::fs;
use std::io::Write;
use std::path::Path;

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
