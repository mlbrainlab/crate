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
