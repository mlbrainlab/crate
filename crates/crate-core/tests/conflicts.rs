//! What happens when the destination already has files with the same names,
//! and extracting without folders.

mod support;

use std::fs;
use std::time::{Duration, SystemTime};

use support::{Case, RawZip};

use crate_core::{Archive, CancelToken, ExtractOptions, IfExists, Resolution, Selection};

/// An archive with `a.txt`, `b.txt` and `sub/c.txt`, all dated 1980.
fn three_files(path: &std::path::Path) {
    RawZip::new()
        .file("a.txt", "new a")
        .file("b.txt", "new b")
        .file("sub/c.txt", "new c")
        .write(path);
}

fn extract(
    case: &Case,
    archive: &std::path::Path,
    options: ExtractOptions,
    resolve: &mut crate_core::ConflictResolver<'_>,
) -> crate_core::ExtractReport {
    Archive::open(archive)
        .unwrap()
        .extract_with(
            &Selection::All,
            &case.out(),
            &options,
            &mut |_| {},
            resolve,
            &CancelToken::new(),
        )
        .unwrap()
}

fn existing(case: &Case) {
    fs::create_dir_all(case.out().join("sub")).unwrap();
    fs::write(case.out().join("a.txt"), "old a").unwrap();
    fs::write(case.out().join("sub/c.txt"), "old c").unwrap();
}

fn read(case: &Case, name: &str) -> String {
    fs::read_to_string(case.out().join(name)).unwrap()
}

#[test]
fn keep_both_renames_the_new_one() {
    let (case, zip) = Case::new("t.zip", three_files);
    existing(&case);
    let options = ExtractOptions {
        if_exists: IfExists::KeepBoth,
        ..Default::default()
    };
    let report = extract(&case, &zip, options, &mut |_| unreachable!());
    assert!(report.skipped.is_empty());
    assert_eq!(read(&case, "a.txt"), "old a");
    assert_eq!(read(&case, "a (2).txt"), "new a");
    assert_eq!(read(&case, "sub/c (2).txt"), "new c");
    assert_eq!(read(&case, "b.txt"), "new b");
}

#[test]
fn keep_newer_compares_dates() {
    let (case, zip) = Case::new("t.zip", three_files);
    existing(&case);
    // a.txt on disk is older than the archive's 1980 date; c.txt is newer.
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(3600);
    fs::File::options()
        .write(true)
        .open(case.out().join("a.txt"))
        .unwrap()
        .set_modified(old)
        .unwrap();
    let options = ExtractOptions {
        if_exists: IfExists::KeepNewer,
        ..Default::default()
    };
    let report = extract(&case, &zip, options, &mut |_| unreachable!());
    assert_eq!(read(&case, "a.txt"), "new a");
    assert_eq!(read(&case, "sub/c.txt"), "old c");
    assert_eq!(report.skipped.len(), 1);
}

#[test]
fn ask_calls_the_resolver_for_each_clash() {
    let (case, zip) = Case::new("t.zip", three_files);
    existing(&case);
    let mut asked = Vec::new();
    let options = ExtractOptions {
        if_exists: IfExists::Ask,
        ..Default::default()
    };
    extract(&case, &zip, options, &mut |c| {
        asked.push((c.path.clone(), c.existing_size, c.incoming_size));
        if c.path == "a.txt" {
            Resolution::Replace
        } else {
            Resolution::KeepBoth
        }
    });
    asked.sort();
    assert_eq!(asked, [("a.txt".into(), 5, 5), ("sub/c.txt".into(), 5, 5)]);
    assert_eq!(read(&case, "a.txt"), "new a");
    assert_eq!(read(&case, "sub/c.txt"), "old c");
    assert_eq!(read(&case, "sub/c (2).txt"), "new c");
}

#[test]
fn ask_without_a_resolver_skips() {
    let (case, zip) = Case::new("t.zip", three_files);
    existing(&case);
    let report = Archive::open(&zip)
        .unwrap()
        .extract(
            &Selection::All,
            &case.out(),
            &ExtractOptions {
                if_exists: IfExists::Ask,
                ..Default::default()
            },
            &mut |_| {},
            &CancelToken::new(),
        )
        .unwrap();
    assert_eq!(report.skipped.len(), 2);
    assert_eq!(read(&case, "a.txt"), "old a");
}

#[test]
fn without_folders() {
    let (case, zip) = Case::new("t.zip", |p| {
        RawZip::new()
            .dir("x/")
            .file("x/readme.txt", "1")
            .file("y/README.txt", "2")
            .file("y/notes.tar.gz", "3")
            .file("notes.tar.gz", "4")
            .file("a/تقرير.txt", "5")
            .file("b/تقرير.txt", "6")
            .write(p)
    });
    let options = ExtractOptions {
        keep_structure: false,
        ..Default::default()
    };
    extract(&case, &zip, options, &mut |_| unreachable!());
    let mut names: Vec<String> = fs::read_dir(case.out())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "README (2).txt",
            "notes (2).tar.gz",
            "notes.tar.gz",
            "readme.txt",
            "تقرير (2).txt",
            "تقرير.txt"
        ],
        "no folders; clashes (ignoring case) get a number"
    );
}
