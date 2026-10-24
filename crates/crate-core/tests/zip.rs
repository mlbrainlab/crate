mod support;

use std::fs;
use std::path::Path;

use support::{Case, extract, extract_to, reasons};

use crate_core::{
    Archive, CancelToken, EntryKind, Error, ExtractOptions, IfExists, NameCodec, OpenOptions,
    Selection, SkipReason,
};

#[test]
fn lists_a_normal_zip() {
    let (_case, zip) = Case::new("test.zip", support::sample_zip);
    let archive = Archive::open(&zip).unwrap();
    assert_eq!(archive.format(), crate_core::Format::Zip);
    assert_eq!(archive.name_encoding(), None);
    let paths: Vec<_> = archive
        .entries()
        .iter()
        .map(|e| (e.path.as_str(), e.kind.clone()))
        .collect();
    assert_eq!(
        paths,
        [
            ("project/", EntryKind::Dir),
            ("project/docs/", EntryKind::Dir),
            ("project/README.md", EntryKind::File),
            ("project/docs/guide.txt", EntryKind::File),
            ("project/docs/تقرير.txt", EntryKind::File),
            ("project/run.sh", EntryKind::File),
        ]
    );
    let guide = &archive.entries()[3];
    assert_eq!(guide.size, 11_000);
    assert!(guide.packed_size.unwrap() < guide.size);
    assert!(guide.modified.is_some());
}

#[test]
fn extracts_a_normal_zip() {
    let (case, zip) = Case::new("test.zip", support::sample_zip);
    let report = extract(&zip, &case.out()).unwrap();
    assert_eq!(
        (report.files, report.dirs, report.bytes),
        (4, 2, 9 + 11_000 + 11 + 18)
    );
    assert!(report.skipped.is_empty());
    let out = case.out();
    assert_eq!(
        fs::read_to_string(out.join("project/README.md")).unwrap(),
        "# Sample\n"
    );
    assert_eq!(
        fs::read_to_string(out.join("project/docs/تقرير.txt")).unwrap(),
        "مرحبا\n"
    );
    assert_eq!(
        fs::read(out.join("project/docs/guide.txt")).unwrap().len(),
        11_000
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
    // Nothing left behind next to the destination.
    assert!(
        case.written()
            .iter()
            .all(|p| !p.contains(".crate-extract-"))
    );
}

#[test]
fn extracts_a_selection() {
    let (case, zip) = Case::new("test.zip", support::sample_zip);
    let mut archive = Archive::open(&zip).unwrap();
    let selection = Selection::Paths(vec!["project/docs".into()]);
    let report = archive
        .extract(
            &selection,
            &case.out(),
            &ExtractOptions::default(),
            &mut |_| {},
            &CancelToken::new(),
        )
        .unwrap();
    assert_eq!(report.files, 2);
    assert_eq!(
        case.written(),
        [
            "out",
            "out/project",
            "out/project/docs",
            "out/project/docs/guide.txt",
            "out/project/docs/تقرير.txt"
        ]
    );
}

#[test]
fn traversal_and_absolute_names_are_blocked() {
    let (case, zip) = Case::new("test.zip", support::traversal_zip);
    let report = extract(&zip, &case.out()).unwrap();
    assert_eq!(report.files, 1);
    assert_eq!(
        reasons(&report),
        [
            ("../evil.txt".into(), SkipReason::PathTraversal),
            ("..\\evil3.txt".into(), SkipReason::PathTraversal),
            ("/tmp/crate-absolute.txt".into(), SkipReason::AbsolutePath),
            ("C:\\crate-drive.txt".into(), SkipReason::AbsolutePath),
            ("a/../../evil2.txt".into(), SkipReason::PathTraversal),
        ]
    );
    assert_eq!(case.written(), ["out", "out/ok.txt"]);
    assert!(!Path::new("/tmp/crate-absolute.txt").exists());
}

#[test]
fn links_cannot_escape_or_be_written_through() {
    let (case, zip) = Case::new("test.zip", support::symlink_zip);
    let report = extract(&zip, &case.out()).unwrap();
    assert_eq!(
        reasons(&report),
        [
            ("absolute".into(), SkipReason::LinkEscape),
            ("dirlink/pwned2.txt".into(), SkipReason::ThroughLink),
            ("escape".into(), SkipReason::LinkEscape),
            ("escape/pwned.txt".into(), SkipReason::ThroughLink),
            ("sneaky".into(), SkipReason::LinkEscape),
        ]
    );
    let out = case.out();
    assert_eq!(
        fs::read_to_string(out.join("docs/readme.txt")).unwrap(),
        "inside"
    );
    #[cfg(unix)]
    {
        assert_eq!(
            fs::read_link(out.join("good-link")).unwrap(),
            Path::new("docs/readme.txt")
        );
        assert_eq!(
            fs::read_link(out.join("dirlink")).unwrap(),
            Path::new("docs")
        );
        assert_eq!(report.links, 2);
    }
    assert!(!out.join("docs/pwned2.txt").exists());
    assert!(!case.dir.path().join("outside").exists());
}

#[test]
fn existing_links_in_the_destination_are_not_followed() {
    let (case, zip) = Case::new("test.zip", support::sample_zip);
    let out = case.out();
    let elsewhere = case.dir.path().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    fs::create_dir_all(&out).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&elsewhere, out.join("project")).unwrap();
    let report = extract(&zip, &out).unwrap();
    #[cfg(unix)]
    assert_eq!(reasons(&report), [("project".into(), SkipReason::Exists)]);
    let _ = report;
    assert_eq!(
        fs::read_dir(&elsewhere).unwrap().count(),
        0,
        "nothing written through the link"
    );

    // Overwrite replaces the link itself with the real folder.
    extract_to(
        &zip,
        &out,
        &ExtractOptions {
            if_exists: IfExists::Overwrite,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(fs::symlink_metadata(out.join("project")).unwrap().is_dir());
    assert!(out.join("project/README.md").is_file());
    assert_eq!(
        fs::read_dir(&elsewhere).unwrap().count(),
        0,
        "nothing written through the link"
    );
}

#[test]
fn existing_files_are_kept_or_replaced() {
    let (case, zip) = Case::new("test.zip", support::sample_zip);
    let out = case.out();
    fs::create_dir_all(out.join("project")).unwrap();
    fs::write(out.join("project/README.md"), "mine").unwrap();
    fs::write(out.join("project/notes.txt"), "keep me").unwrap();

    let report = extract(&zip, &out).unwrap();
    assert_eq!(
        reasons(&report),
        [("project/README.md".into(), SkipReason::Exists)]
    );
    assert_eq!(
        fs::read_to_string(out.join("project/README.md")).unwrap(),
        "mine"
    );
    assert_eq!(
        fs::read_to_string(out.join("project/docs/guide.txt"))
            .unwrap()
            .len(),
        11_000,
        "merged"
    );

    let report = extract_to(
        &zip,
        &out,
        &ExtractOptions {
            if_exists: IfExists::Overwrite,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(report.skipped.is_empty());
    assert_eq!(
        fs::read_to_string(out.join("project/README.md")).unwrap(),
        "# Sample\n"
    );
    assert_eq!(
        fs::read_to_string(out.join("project/notes.txt")).unwrap(),
        "keep me"
    );
    assert!(fs::read_dir(&out).unwrap().all(|e| {
        !e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".crate")
    }));
}

#[test]
fn decompression_bomb_is_refused() {
    let (case, zip) = Case::new("test.zip", support::bomb_zip);
    let err = extract(&zip, &case.out()).unwrap_err();
    assert!(matches!(err, Error::SuspiciousRatio { .. }), "{err}");
    assert!(case.written().is_empty());

    // The limit is configurable.
    let mut options = ExtractOptions::default();
    options.limits.max_ratio = None;
    let report = extract_to(&zip, &case.out(), &options).unwrap();
    assert_eq!(report.bytes, 100 << 20);
}

#[test]
fn entry_with_more_data_than_declared_is_refused() {
    let (case, zip) = Case::new("test.zip", support::lying_size_zip);
    let err = extract(&zip, &case.out()).unwrap_err();
    // The zip crate notices first; `safety::copy_limited` is the backstop for
    // formats that don't (unit-tested in `safety`).
    let caught_by_zip = matches!(&err, Error::Io(e) if e.kind() == std::io::ErrorKind::InvalidData);
    assert!(
        matches!(err, Error::SizeExceeded { .. }) || caught_by_zip,
        "{err:?}"
    );
    assert!(
        case.written().is_empty(),
        "no partial output: {:?}",
        case.written()
    );
}

#[test]
fn cancel_leaves_nothing_behind() {
    let (case, zip) = Case::new("test.zip", support::sample_zip);
    let cancel = CancelToken::new();
    let mut archive = Archive::open(&zip).unwrap();
    let c = cancel.clone();
    let err = archive
        .extract(
            &Selection::All,
            &case.out(),
            &ExtractOptions::default(),
            &mut |p| {
                if p.files_done >= 1 {
                    c.cancel();
                }
            },
            &cancel,
        )
        .unwrap_err();
    assert!(matches!(err, Error::Cancelled));
    assert!(case.written().is_empty(), "{:?}", case.written());
}

#[test]
fn arabic_cp1256_names_are_guessed_or_forced() {
    let (case, zip) = Case::new("test.zip", support::arabic_cp1256_zip);
    let archive = Archive::open(&zip).unwrap();
    let enc = archive.name_encoding().unwrap();
    assert_eq!(enc.codec, NameCodec::Web(encoding_rs::WINDOWS_1256));
    assert!(enc.guessed);
    let names: Vec<_> = archive.entries().iter().map(|e| e.path.as_str()).collect();
    assert_eq!(
        names,
        ["التقرير السنوي.txt", "الصور/رحلة الصيف.jpg", "readme.txt"]
    );

    let forced = OpenOptions {
        encoding: NameCodec::from_label("cp1256"),
    };
    let mut archive = Archive::open_with(&zip, &forced).unwrap();
    assert!(!archive.name_encoding().unwrap().guessed);
    archive
        .extract(
            &Selection::All,
            &case.out(),
            &ExtractOptions::default(),
            &mut |_| {},
            &CancelToken::new(),
        )
        .unwrap();
    assert!(case.out().join("الصور/رحلة الصيف.jpg").is_file());
}

#[test]
fn test_reports_crc_errors() {
    let (_case, zip) = Case::new("test.zip", support::bad_crc_zip);
    let report = Archive::open(&zip)
        .unwrap()
        .test(&mut |_| {}, &CancelToken::new())
        .unwrap();
    assert_eq!(report.files, 1);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].0, "bad.txt");
    assert!(!report.is_ok());

    let (_case, zip) = Case::new("test.zip", support::sample_zip);
    assert!(
        Archive::open(&zip)
            .unwrap()
            .test(&mut |_| {}, &CancelToken::new())
            .unwrap()
            .is_ok()
    );
}

#[test]
fn extraction_with_a_bad_crc_leaves_nothing() {
    let (case, zip) = Case::new("test.zip", support::bad_crc_zip);
    assert!(extract(&zip, &case.out()).is_err());
    assert!(case.written().is_empty());
}

#[test]
fn encrypted_entries_are_skipped() {
    let (case, zip) = Case::new("test.zip", support::encrypted_zip);
    let report = extract(&zip, &case.out()).unwrap();
    assert_eq!(
        reasons(&report),
        [("secret.txt".into(), SkipReason::Encrypted)]
    );
    assert_eq!(
        fs::read_to_string(case.out().join("plain.txt")).unwrap(),
        "plain"
    );
}

#[test]
fn not_an_archive() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.txt");
    fs::write(&path, "just text").unwrap();
    assert!(matches!(Archive::open(&path), Err(Error::UnknownFormat(_))));
}

#[test]
fn zip64_entries() {
    let (case, zip) = Case::new("test.zip", support::zip64_zip);
    let report = extract(&zip, &case.out()).unwrap();
    assert_eq!((report.files, report.bytes), (2, 5005));
    assert_eq!(
        fs::read_to_string(case.out().join("big/two.txt")).unwrap(),
        "zip64"
    );
}

#[test]
fn progress_reports_bytes_files_and_names() {
    let (case, zip) = Case::new("test.zip", support::sample_zip);
    let mut seen = Vec::new();
    let mut archive = Archive::open(&zip).unwrap();
    archive
        .extract(
            &Selection::All,
            &case.out(),
            &ExtractOptions::default(),
            &mut |p| seen.push(p.clone()),
            &CancelToken::new(),
        )
        .unwrap();
    let last = seen.last().unwrap();
    assert_eq!((last.bytes_done, last.bytes_total), (11_038, 11_038));
    assert_eq!((last.files_done, last.files_total), (4, 4));
    assert!(seen.iter().any(|p| p.current == "project/docs/guide.txt"));
    assert!(
        seen.windows(2).all(|w| w[0].bytes_done <= w[1].bytes_done),
        "never goes backwards"
    );
}
