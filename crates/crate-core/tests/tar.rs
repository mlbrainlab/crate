mod support;

use std::fs;
use std::path::Path;

use support::{Case, Comp, extract, reasons};

use crate_core::{Archive, CancelToken, EntryKind, Error, Format, SkipReason};

fn format_of(comp: Comp) -> Format {
    match comp {
        Comp::None => Format::Tar,
        Comp::Gz => Format::TarGz,
        Comp::Xz => Format::TarXz,
        Comp::Zst => Format::TarZst,
        Comp::Bz2 => Format::TarBz2,
    }
}

#[test]
fn lists_every_tar_variant() {
    for comp in Comp::ALL {
        let (_case, path) = Case::new(&format!("test.{}", comp.ext()), |p| {
            support::sample_tar(p, comp)
        });
        let archive = Archive::open(&path).unwrap();
        assert_eq!(archive.format(), format_of(comp));
        let kinds: Vec<_> = archive
            .entries()
            .iter()
            .map(|e| (e.path.as_str(), e.kind.clone()))
            .collect();
        assert_eq!(
            kinds,
            [
                ("project/", EntryKind::Dir),
                ("project/docs/", EntryKind::Dir),
                ("project/README.md", EntryKind::File),
                ("project/docs/guide.txt", EntryKind::File),
                ("project/docs/تقرير.txt", EntryKind::File),
                ("project/run.sh", EntryKind::File),
                (
                    "project/docs/guide-copy.txt",
                    EntryKind::Hardlink {
                        target: "project/docs/guide.txt".into()
                    }
                ),
            ],
            "{comp:?}"
        );
        assert_eq!(archive.entries()[3].size, 11_000);
        assert!(archive.entries()[3].modified.is_some());
    }
}

#[test]
fn extracts_every_tar_variant() {
    for comp in Comp::ALL {
        let (case, path) = Case::new(&format!("test.{}", comp.ext()), |p| {
            support::sample_tar(p, comp)
        });
        let report = extract(&path, &case.out()).unwrap();
        assert!(report.skipped.is_empty(), "{comp:?}: {:?}", report.skipped);
        assert_eq!(
            (report.files, report.dirs, report.links),
            (4, 2, 1),
            "{comp:?}"
        );
        let out = case.out();
        assert_eq!(
            fs::read_to_string(out.join("project/docs/تقرير.txt")).unwrap(),
            "مرحبا\n"
        );
        assert_eq!(
            fs::read(out.join("project/docs/guide-copy.txt"))
                .unwrap()
                .len(),
            11_000,
            "hard link copied"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(out.join("project/run.sh"))
                .unwrap()
                .permissions()
                .mode()
                & 0o7777;
            assert_eq!(mode, 0o755, "setuid bit dropped");
        }
        assert!(
            Archive::open(&path)
                .unwrap()
                .test(&mut |_| {}, &CancelToken::new())
                .unwrap()
                .is_ok()
        );
    }
}

#[test]
fn malicious_tar_entries_are_blocked() {
    for comp in [Comp::None, Comp::Gz] {
        let (case, path) = Case::new(&format!("bad.{}", comp.ext()), |p| {
            support::malicious_tar(p, comp)
        });
        let report = extract(&path, &case.out()).unwrap();
        assert_eq!(
            reasons(&report),
            [
                ("../evil.txt".into(), SkipReason::PathTraversal),
                (
                    "/tmp/crate-tar-absolute.txt".into(),
                    SkipReason::AbsolutePath
                ),
                ("dev-null".into(), SkipReason::UnsupportedType),
                ("escape".into(), SkipReason::LinkEscape),
                ("escape/pwned.txt".into(), SkipReason::ThroughLink),
                ("hard-absolute".into(), SkipReason::LinkEscape),
                ("hard-escape".into(), SkipReason::LinkEscape),
            ]
        );
        assert_eq!(case.written(), ["out", "out/ok.txt"]);
        assert!(!Path::new("/tmp/crate-tar-absolute.txt").exists());
    }
}

#[test]
fn tar_zst_bomb_is_refused() {
    let (case, path) = Case::new("bomb.tar.zst", support::bomb_tar_zst);
    assert!(fs::metadata(&path).unwrap().len() < 64 * 1024);
    let err = extract(&path, &case.out()).unwrap_err();
    assert!(matches!(err, Error::SuspiciousRatio { .. }), "{err}");
    assert!(case.written().is_empty());
}

#[test]
fn truncated_tar_gz_fails_cleanly() {
    let (case, path) = Case::new("cut.tar.gz", support::truncated_tar_gz);
    // Listing reads the whole stream, so the damage shows up on open.
    match Archive::open(&path) {
        Err(Error::Corrupt(_)) => {}
        Err(e) => panic!("expected a corrupt-archive error, got {e:?}"),
        Ok(mut archive) => {
            assert!(
                archive
                    .test(&mut |_| {}, &CancelToken::new())
                    .map_or(true, |r| !r.is_ok())
            );
            assert!(extract(&path, &case.out()).is_err());
        }
    }
    assert!(case.written().is_empty());
}

#[test]
fn gzip_of_a_single_file_is_not_a_tar() {
    let (_case, path) = Case::new("notes.txt.gz", |p| {
        fs::write(p, Comp::Gz.compress(b"just some text\n")).unwrap()
    });
    let result = Archive::open(&path).map(|a| a.entries().len());
    assert!(matches!(result, Err(Error::Corrupt(_))), "{result:?}");
}
