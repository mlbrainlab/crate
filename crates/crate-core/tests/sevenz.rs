mod support;

use std::fs;

use support::{Case, extract, reasons};

use crate_core::{Archive, CancelToken, EntryKind, Error, Format, OpenOptions, SkipReason};

#[test]
fn lists_and_extracts_7z() {
    for solid in [false, true] {
        let (case, path) = Case::new("test.7z", |p| support::sample_7z(p, solid));
        let archive = Archive::open(&path).unwrap();
        assert_eq!(archive.format(), Format::SevenZ);
        let mut kinds: Vec<_> = archive
            .entries()
            .iter()
            .map(|e| (e.path.clone(), e.kind.clone()))
            .collect();
        kinds.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            kinds,
            [
                ("project".into(), EntryKind::Dir),
                ("project/README.md".into(), EntryKind::File),
                ("project/docs".into(), EntryKind::Dir),
                ("project/docs/guide.txt".into(), EntryKind::File),
                ("project/docs/تقرير.txt".into(), EntryKind::File),
                (
                    "project/latest".into(),
                    EntryKind::Symlink {
                        target: "docs/guide.txt".into()
                    }
                ),
                ("project/run.sh".into(), EntryKind::File),
            ],
            "solid: {solid}"
        );

        let report = extract(&path, &case.out()).unwrap();
        assert!(report.skipped.is_empty(), "{:?}", report.skipped);
        assert_eq!((report.files, report.dirs), (4, 2));
        let out = case.out();
        assert_eq!(
            fs::read(out.join("project/docs/guide.txt")).unwrap().len(),
            11_000
        );
        assert_eq!(
            fs::read_to_string(out.join("project/docs/تقرير.txt")).unwrap(),
            "مرحبا\n"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::read_link(out.join("project/latest")).unwrap().to_str(),
                Some("docs/guide.txt")
            );
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
fn malicious_7z_entries_are_blocked() {
    let (case, path) = Case::new("bad.7z", support::malicious_7z);
    let report = extract(&path, &case.out()).unwrap();
    assert_eq!(
        reasons(&report),
        [
            ("../evil.txt".into(), SkipReason::PathTraversal),
            (
                "/tmp/crate-7z-absolute.txt".into(),
                SkipReason::AbsolutePath
            ),
            ("escape".into(), SkipReason::LinkEscape),
            ("escape/pwned.txt".into(), SkipReason::ThroughLink),
        ]
    );
    assert_eq!(case.written(), ["out", "out/ok.txt"]);
}

#[test]
fn encrypted_7z_is_skipped_or_needs_a_password() {
    let (case, path) = Case::new("enc.7z", |p| support::encrypted_7z(p, false));
    let archive = Archive::open(&path).unwrap();
    assert!(archive.entries()[0].encrypted);
    let report = extract(&path, &case.out()).unwrap();
    assert_eq!(
        reasons(&report),
        [("secret.txt".into(), SkipReason::Encrypted)]
    );

    let (_case, path) = Case::new("enc-names.7z", |p| support::encrypted_7z(p, true));
    assert!(matches!(Archive::open(&path), Err(Error::PasswordRequired)));
}

#[test]
fn encrypted_7z_with_a_password() {
    let cancel = CancelToken::new();
    let (case, path) = Case::new("enc.7z", |p| support::encrypted_7z(p, false));
    let mut archive = Archive::open(&path).unwrap();
    assert!(!archive.check_password("wrong", &cancel).unwrap());
    archive.set_password(Some("wrong"));
    assert!(matches!(
        support::extract_archive(&mut archive, &case.out()),
        Err(Error::WrongPassword)
    ));
    assert!(case.written().is_empty());
    assert!(archive.check_password("secret", &cancel).unwrap());
    support::extract_archive(&mut archive, &case.out()).unwrap();
    assert_eq!(
        fs::read_to_string(case.out().join("secret.txt")).unwrap(),
        "secret"
    );

    // An encrypted file list needs the password to open.
    let (case, path) = Case::new("enc-names.7z", |p| support::encrypted_7z(p, true));
    let with = |pw: &str| {
        Archive::open_with(
            &path,
            &OpenOptions {
                password: Some(pw.into()),
                ..OpenOptions::default()
            },
        )
    };
    assert!(matches!(with("wrong"), Err(Error::WrongPassword)));
    let mut archive = with("secret").unwrap();
    assert_eq!(archive.entries()[0].path, "secret.txt");
    support::extract_archive(&mut archive, &case.out()).unwrap();
    assert_eq!(
        fs::read_to_string(case.out().join("secret.txt")).unwrap(),
        "secret"
    );
}

#[test]
fn damaged_7z_fails_cleanly() {
    let (case, path) = Case::new("cut.7z", |p| {
        support::sample_7z(p, true);
        let data = fs::read(p).unwrap();
        fs::write(p, &data[..data.len() - 40]).unwrap();
    });
    assert!(Archive::open(&path).is_err() || extract(&path, &case.out()).is_err());
    assert!(case.written().is_empty());
}
