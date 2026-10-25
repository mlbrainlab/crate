#[path = "../../crate-core/tests/support/mod.rs"]
mod support;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn crate_cmd(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_crate"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

#[test]
fn list_extract_and_test_a_zip() {
    let dir = tempfile::tempdir().unwrap();
    support::sample_zip(&dir.path().join("sample.zip"));

    let out = crate_cmd(dir.path(), &["l", "sample.zip"]);
    assert!(out.status.success());
    let listing = text(&out.stdout);
    assert!(listing.contains("project/docs/تقرير.txt"), "{listing}");
    assert!(listing.contains("4 files, 11038 bytes (ZIP)"), "{listing}");

    let out = crate_cmd(dir.path(), &["x", "sample.zip"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        dir.path().join("sample/project/README.md").is_file(),
        "default folder is named after the archive"
    );

    let out = crate_cmd(dir.path(), &["x", "sample.zip", "-o", "elsewhere"]);
    assert!(out.status.success());
    assert!(dir.path().join("elsewhere/project/run.sh").is_file());

    let out = crate_cmd(dir.path(), &["t", "sample.zip"]);
    assert!(out.status.success());
    assert!(text(&out.stdout).starts_with("OK: 4 files"));
}

#[test]
fn unsafe_entries_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    support::traversal_zip(&dir.path().join("t.zip"));
    let out = crate_cmd(dir.path(), &["x", "t.zip"]);
    assert!(out.status.success());
    let err = text(&out.stderr);
    assert!(err.contains("5 items were skipped"), "{err}");
    assert!(
        err.contains("../evil.txt: path leaves the destination folder"),
        "{err}"
    );
    assert!(!dir.path().join("evil.txt").exists());
}

#[test]
fn bomb_needs_explicit_permission() {
    let dir = tempfile::tempdir().unwrap();
    support::bomb_zip(&dir.path().join("b.zip"));
    let out = crate_cmd(dir.path(), &["x", "b.zip"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("decompression bomb"));
    assert!(!dir.path().join("b").exists());
    let out = crate_cmd(dir.path(), &["x", "b.zip", "--allow-high-ratio"]);
    assert!(out.status.success());
}

#[test]
fn damaged_archive_fails_the_test() {
    let dir = tempfile::tempdir().unwrap();
    support::bad_crc_zip(&dir.path().join("d.zip"));
    let out = crate_cmd(dir.path(), &["t", "d.zip"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("bad.txt"));
}

#[test]
fn arabic_names_with_encoding_option() {
    let dir = tempfile::tempdir().unwrap();
    support::arabic_cp1256_zip(&dir.path().join("arabic-cp1256.zip"));
    let out = crate_cmd(
        dir.path(),
        &["x", "arabic-cp1256.zip", "--encoding", "cp1256"],
    );
    assert!(out.status.success());
    assert!(
        dir.path()
            .join("arabic-cp1256/الصور/رحلة الصيف.jpg")
            .is_file()
    );

    let out = crate_cmd(
        dir.path(),
        &["l", "arabic-cp1256.zip", "--encoding", "klingon"],
    );
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn password_option() {
    let dir = tempfile::tempdir().unwrap();
    support::encrypted_zip(&dir.path().join("e.zip"));
    let out = crate_cmd(dir.path(), &["x", "e.zip", "-p", "nope"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("wrong password"),
        "{}",
        text(&out.stderr)
    );
    assert!(!dir.path().join("e").exists());
    let out = crate_cmd(dir.path(), &["x", "e.zip", "--password", "secret"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        fs::read_to_string(dir.path().join("e/secret.txt")).unwrap(),
        "secret"
    );
}

#[test]
fn usage_errors() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        crate_cmd(dir.path(), &["q", "a.zip"]).status.code(),
        Some(2)
    );
    assert_eq!(crate_cmd(dir.path(), &["x"]).status.code(), Some(2));
    let out = crate_cmd(dir.path(), &["l", "missing.zip"]);
    assert_eq!(out.status.code(), Some(1));
}

/// M1 acceptance: Ctrl+C during a large extract leaves no partial files.
#[cfg(unix)]
#[test]
fn ctrl_c_leaves_nothing_behind() {
    use std::io::Write as _;
    use std::time::{Duration, Instant};

    let dir = tempfile::tempdir().unwrap();
    // 40 × 16 MiB of zeros, stored: quick to build, slow enough to interrupt.
    {
        use zip::write::SimpleFileOptions;
        let mut zip = zip::ZipWriter::new(fs::File::create(dir.path().join("big.zip")).unwrap());
        let stored =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let chunk = vec![0u8; 1 << 20];
        for i in 0..40 {
            zip.start_file(format!("part{i:02}.bin"), stored).unwrap();
            for _ in 0..16 {
                zip.write_all(&chunk).unwrap();
            }
        }
        zip.finish().unwrap();
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_crate"))
        .current_dir(dir.path())
        .args(["x", "big.zip"])
        .spawn()
        .unwrap();
    // Wait until extraction has started writing into its staging folder.
    let start = Instant::now();
    let staging_exists = || {
        fs::read_dir(dir.path()).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".crate-extract-")
        })
    };
    while !staging_exists() {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "extraction never started"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    std::thread::sleep(Duration::from_millis(50));
    Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(130), "cancelled, not finished");
    let left: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(left, ["big.zip"], "no partial output or staging folder");
}

/// Every regular file under `dir` with its SHA-256 (via `crate sum`'s engine).
fn hashes(dir: &Path) -> Vec<(String, String)> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
        for e in fs::read_dir(dir).unwrap() {
            let e = e.unwrap();
            if e.file_type().unwrap().is_dir() {
                walk(base, &e.path(), out);
            } else if e.file_type().unwrap().is_file() {
                let sum = crate_core::checksum::compute(
                    &e.path(),
                    &mut |_| {},
                    &crate_core::CancelToken::new(),
                )
                .unwrap()
                .sha256;
                out.push((
                    e.path().strip_prefix(base).unwrap().display().to_string(),
                    sum,
                ));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

/// M4 acceptance: a folder compresses to tar.zst and extracts back
/// byte-for-byte identical.
#[test]
fn folder_to_tar_zst_and_back_is_identical() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("photos");
    fs::create_dir_all(src.join("2026/raw")).unwrap();
    for i in 0..30u32 {
        let data: Vec<u8> = (0..(i * 7919 % 100_000))
            .map(|j| (j * i % 251) as u8)
            .collect();
        fs::write(src.join(format!("2026/raw/IMG_{i:04}.cr3")), data).unwrap();
    }
    fs::write(src.join("notes.txt"), "trip notes\n").unwrap();
    let out = crate_cmd(
        dir.path(),
        &["c", "photos.tar.zst", "photos", "--level", "9"],
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stdout).starts_with("Created photos.tar.zst"));
    let out = crate_cmd(dir.path(), &["x", "photos.tar.zst", "-o", "back"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(hashes(&dir.path().join("back/photos")), hashes(&src));
    assert_eq!(hashes(&src).len(), 31);
}

#[test]
fn add_delete_rename_sum_convert() {
    let dir = tempfile::tempdir().unwrap();
    support::sample_zip(&dir.path().join("s.zip"));
    fs::write(dir.path().join("new.txt"), "new").unwrap();
    let ok = |args: &[&str]| {
        let out = crate_cmd(dir.path(), args);
        assert!(out.status.success(), "{args:?}: {}", text(&out.stderr));
        text(&out.stdout)
    };
    ok(&["a", "s.zip", "new.txt", "--into", "project/docs"]);
    ok(&["d", "s.zip", "project/run.sh", "--backup"]);
    assert!(dir.path().join("s.zip.bak").is_file());
    ok(&["rn", "s.zip", "project/docs", "project/manual"]);
    let listing = ok(&["l", "s.zip"]);
    assert!(listing.contains("project/manual/new.txt"), "{listing}");
    assert!(!listing.contains("run.sh"));
    let sums = ok(&["sum", "s.zip"]);
    assert!(
        sums.starts_with("SHA-256  ") && sums.contains("BLAKE3   ") && sums.contains("MD5      ")
    );
    ok(&["convert", "s.zip", "s.tar.zst.age", "-p", "pw"]);
    let listing = ok(&["l", "s.tar.zst.age", "-p", "pw"]);
    assert!(listing.contains("project/manual/new.txt"), "{listing}");
    // Mistakes: clear message, nothing changed.
    let out = crate_cmd(dir.path(), &["rn", "s.zip", "nope", "x"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("not in the archive"));
    let out = crate_cmd(dir.path(), &["c", "x.rar", "new.txt"]);
    assert_eq!(out.status.code(), Some(2));
}

/// M4 acceptance: deleting a file inside a zip and killing the process
/// mid-rewrite leaves the original zip intact.
#[cfg(unix)]
#[test]
fn killed_mid_rewrite_leaves_the_original_intact() {
    use std::io::Write as _;
    use std::time::{Duration, Instant};

    let dir = tempfile::tempdir().unwrap();
    let zip_path = dir.path().join("big.zip");
    {
        use zip::write::SimpleFileOptions;
        let mut zip = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
        let stored =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let chunk = vec![3u8; 1 << 20];
        for i in 0..40 {
            zip.start_file(format!("part{i:02}.bin"), stored).unwrap();
            for _ in 0..16 {
                zip.write_all(&chunk).unwrap();
            }
        }
        zip.finish().unwrap();
    }
    let before =
        crate_core::checksum::compute(&zip_path, &mut |_| {}, &crate_core::CancelToken::new())
            .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_crate"))
        .current_dir(dir.path())
        .args(["d", "big.zip", "part00.bin"])
        .spawn()
        .unwrap();
    // Wait until the new archive is well under way, then pull the plug.
    let start = Instant::now();
    let temp_size = || {
        fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with(".crate-new-"))
            .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
            .max()
    };
    while temp_size().unwrap_or(0) < 32 << 20 {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "rewrite never got going"
        );
        assert!(
            child.try_wait().unwrap().is_none(),
            "finished before it could be killed"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    child.kill().unwrap(); // SIGKILL: no cleanup code runs
    child.wait().unwrap();
    let after =
        crate_core::checksum::compute(&zip_path, &mut |_| {}, &crate_core::CancelToken::new())
            .unwrap();
    assert_eq!(before, after, "the original is untouched");
    // It still opens and still has the file that was being deleted.
    let a = crate_core::Archive::open(&zip_path).unwrap();
    assert_eq!(a.entries().len(), 40);
}
