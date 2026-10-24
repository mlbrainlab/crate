//! RAR can't be generated here (there is no free RAR writer), so these tests
//! use the small sample archives from the `unrar` crate (MIT / Apache-2.0),
//! copied into tests/fixtures/rar/, plus damaged copies made on the fly. The
//! path and link checks are format-independent and covered by the zip, tar
//! and 7z tests.
#![cfg(feature = "rar")]

mod support;

use std::fs;
use std::path::{Path, PathBuf};

use support::{Case, extract, reasons};

use crate_core::{Archive, CancelToken, EntryKind, Error, Format, SkipReason};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/rar")
        .join(name)
}

fn copy_of(name: &'static str) -> impl FnOnce(&Path) {
    move |p| {
        fs::copy(fixture(name), p).unwrap();
    }
}

#[test]
fn lists_extracts_and_tests_rar() {
    for name in ["solid.rar", "comment.rar"] {
        let (case, path) = Case::new(name, copy_of(name));
        let mut archive = Archive::open(&path).unwrap();
        assert_eq!(archive.format(), Format::Rar);
        let e = &archive.entries()[0];
        assert_eq!(
            (e.path.as_str(), &e.kind, e.size),
            (".gitignore", &EntryKind::File, 18)
        );
        assert!(e.modified.is_some());
        assert!(
            archive
                .test(&mut |_| {}, &CancelToken::new())
                .unwrap()
                .is_ok()
        );

        let report = extract(&path, &case.out()).unwrap();
        assert_eq!(report.files, 1);
        assert_eq!(
            fs::read_to_string(case.out().join(".gitignore")).unwrap(),
            "target\nCargo.lock\n"
        );
    }
}

#[test]
fn unicode_names() {
    let (case, path) = Case::new("unicode.rar", copy_of("unicode.rar"));
    let archive = Archive::open(&path).unwrap();
    assert_eq!(archive.entries()[0].path, "te…―st✌");
    assert_eq!(archive.name_encoding(), None);
    extract(&path, &case.out()).unwrap();
    assert_eq!(
        fs::read_to_string(case.out().join("te…―st✌")).unwrap(),
        "🆃🄴🆂🅃\n"
    );
}

#[test]
fn encrypted_entries_are_skipped() {
    let (case, path) = Case::new("crypted.rar", copy_of("crypted.rar"));
    assert!(Archive::open(&path).unwrap().entries()[0].encrypted);
    let report = extract(&path, &case.out()).unwrap();
    assert_eq!(
        reasons(&report),
        [(".gitignore".into(), SkipReason::Encrypted)]
    );
    assert!(!case.out().join(".gitignore").exists());
}

#[test]
fn encrypted_entries_with_a_password() {
    let cancel = CancelToken::new();
    let (case, path) = Case::new("crypted.rar", copy_of("crypted.rar"));
    let mut archive = Archive::open(&path).unwrap();
    assert!(!archive.check_password("wrong", &cancel).unwrap());
    archive.set_password(Some("wrong"));
    assert!(matches!(
        support::extract_archive(&mut archive, &case.out()),
        Err(Error::WrongPassword)
    ));
    assert!(case.written().is_empty());
    // Password from the unrar crate's tests.
    assert!(archive.check_password("unrar", &cancel).unwrap());
    support::extract_archive(&mut archive, &case.out()).unwrap();
    assert_eq!(
        fs::read_to_string(case.out().join(".gitignore")).unwrap(),
        "target\nCargo.lock\n"
    );
}

#[test]
fn damaged_rar_fails_cleanly() {
    // Flip a byte in the packed data (just before the end-of-archive block).
    let (case, path) = Case::new("flipped.rar", |p| {
        let mut data = fs::read(fixture("comment.rar")).unwrap();
        let at = data.len() - 30;
        data[at] ^= 0xFF;
        fs::write(p, data).unwrap();
    });
    if let Ok(mut archive) = Archive::open(&path) {
        let tested = archive.test(&mut |_| {}, &CancelToken::new());
        assert!(
            tested.is_err() || !tested.unwrap().is_ok(),
            "damage noticed by test"
        );
        assert!(extract(&path, &case.out()).is_err());
    }
    assert!(case.written().is_empty(), "{:?}", case.written());

    // Cut inside the file header (50) and inside the packed data (70).
    for cut in [50, 70] {
        let (case, path) = Case::new("cut.rar", |p| {
            let data = fs::read(fixture("solid.rar")).unwrap();
            fs::write(p, &data[..cut]).unwrap();
        });
        let failed = match Archive::open(&path) {
            Err(_) => true,
            Ok(mut archive) => {
                let tested = archive.test(&mut |_| {}, &CancelToken::new());
                !tested.is_ok_and(|r| r.is_ok()) && extract(&path, &case.out()).is_err()
            }
        };
        assert!(failed, "cut at {cut} not noticed");
        assert!(case.written().is_empty());
    }
}
