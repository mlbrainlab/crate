//! Checksums of an archive file (Properties › Checksums): SHA-256, BLAKE3
//! and MD5 in one pass, and checking a pasted checksum against them.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use sha2::Digest;

use crate::error::{Error, Result};
use crate::progress::{CancelToken, Progress};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checksums {
    pub sha256: String,
    pub blake3: String,
    pub md5: String,
}

impl Checksums {
    /// Which algorithm a pasted checksum matches, if any (case and
    /// surrounding text such as `sha256:` or a file name are ignored).
    pub fn verify(&self, pasted: &str) -> Option<&'static str> {
        let hex: String = pasted
            .split(|c: char| !c.is_ascii_hexdigit())
            .find(|w| w.len() >= 32)?
            .to_ascii_lowercase();
        [
            ("SHA-256", &self.sha256),
            ("BLAKE3", &self.blake3),
            ("MD5", &self.md5),
        ]
        .into_iter()
        .find(|(_, v)| **v == hex)
        .map(|(name, _)| name)
    }
}

/// Reads `path` once and computes all three.
pub fn compute(
    path: &Path,
    progress: &mut dyn FnMut(&Progress),
    cancel: &CancelToken,
) -> Result<Checksums> {
    let mut file = File::open(path)?;
    let mut prog = Progress {
        bytes_total: file.metadata()?.len(),
        ..Progress::default()
    };
    let mut sha = sha2::Sha256::new();
    let mut blake = blake3::Hasher::new();
    let mut md5 = md5::Md5::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        sha.update(&buf[..n]);
        blake.update(&buf[..n]);
        md5.update(&buf[..n]);
        prog.bytes_done += n as u64;
        progress(&prog);
    }
    let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    Ok(Checksums {
        sha256: hex(&sha.finalize()),
        blake3: blake.finalize().to_hex().to_string(),
        md5: hex(&md5.finalize()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values_and_verify() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("abc");
        std::fs::write(&path, b"abc").unwrap();
        let c = compute(&path, &mut |_| {}, &CancelToken::new()).unwrap();
        assert_eq!(
            c.sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(c.md5, "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            c.blake3,
            "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85"
        );
        assert_eq!(
            c.verify("BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD  abc"),
            Some("SHA-256")
        );
        assert_eq!(
            c.verify("md5: 900150983cd24fb0d6963f7d28e17f72"),
            Some("MD5")
        );
        assert_eq!(c.verify("0000"), None);
        assert_eq!(c.verify("ffffffffffffffffffffffffffffffff"), None);
    }
}
