//! One window per archive. The first Crate process that opens an archive
//! listens on a socket named after it; opening the same archive again asks
//! that process to raise its window, then exits.

use std::path::{Path, PathBuf};

/// What happened when claiming an archive.
pub enum Claim {
    /// This process owns the archive. Keep the guard alive; `raise` is called
    /// (on a background thread) whenever the archive is opened again.
    Owner(Guard),
    /// Another Crate window already shows this archive and was asked to come
    /// to the front.
    AlreadyOpen,
}

#[cfg(unix)]
pub use unix::{Guard, claim};

#[cfg(not(unix))]
pub use other::{Guard, claim};

/// Socket file for an archive: the same archive (after resolving links)
/// always gives the same name.
fn socket_path(archive: &Path) -> Option<PathBuf> {
    use std::hash::{Hash, Hasher};
    let canonical = archive.canonicalize().ok()?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    canonical.hash(&mut h);
    let dir = runtime_dir()?.join("crate");
    Some(dir.join(format!("{:016x}.sock", h.finish())))
}

fn runtime_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
}

#[cfg(unix)]
mod unix {
    use std::io::{Read, Write};
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::{Path, PathBuf};

    use super::{Claim, socket_path};

    const RAISE: &[u8] = b"raise\n";

    /// Removes the socket file when the owning window goes away.
    pub struct Guard(Option<PathBuf>);

    impl Drop for Guard {
        fn drop(&mut self) {
            if let Some(path) = &self.0 {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    pub fn claim(archive: &Path, raise: impl Fn() + Send + 'static) -> Claim {
        let Some(path) = socket_path(archive) else {
            // No runtime dir or unreadable path: just open another window.
            return Claim::Owner(Guard(None));
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir);
        }
        // Two tries: a socket left by a crashed process is removed and the
        // name claimed again.
        for _ in 0..2 {
            if let Ok(mut stream) = UnixStream::connect(&path) {
                let _ = stream.write_all(RAISE);
                return Claim::AlreadyOpen;
            }
            let _ = std::fs::remove_file(&path);
            if let Ok(listener) = UnixListener::bind(&path) {
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
                std::thread::spawn(move || {
                    for mut stream in listener.incoming().flatten() {
                        let mut buf = [0u8; RAISE.len()];
                        if stream.read_exact(&mut buf).is_ok() && buf == RAISE {
                            raise();
                        }
                    }
                });
                return Claim::Owner(Guard(Some(path)));
            }
        }
        Claim::Owner(Guard(None))
    }
}

#[cfg(not(unix))]
mod other {
    use std::path::Path;

    use super::Claim;

    // Windows and macOS get their own single-instance handling in M7/M8.
    pub struct Guard;

    pub fn claim(_archive: &Path, _raise: impl Fn() + Send + 'static) -> Claim {
        Claim::Owner(Guard)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn second_open_raises_the_first_window() {
        if runtime_dir().is_none() {
            return; // nothing to test without XDG_RUNTIME_DIR
        }
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("a.zip");
        std::fs::write(&archive, b"x").unwrap();
        let (tx, rx) = mpsc::channel();
        let Claim::Owner(guard) = claim(&archive, move || tx.send(()).unwrap()) else {
            panic!("first claim must own the archive");
        };
        // The same file through another path still counts as the same.
        let other_path = dir.path().join(".").join("a.zip");
        assert!(matches!(claim(&other_path, || {}), Claim::AlreadyOpen));
        rx.recv_timeout(Duration::from_secs(5)).unwrap();

        // Once the first window is gone, the archive can be claimed again.
        drop(guard);
        assert!(matches!(claim(&archive, || {}), Claim::Owner(_)));
    }
}
