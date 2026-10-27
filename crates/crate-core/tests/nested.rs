//! "Also unpack archives found inside": three levels of zips.

mod support;

use std::fs;
use std::io::Write;
use std::path::Path;

use crate_core::nested;
use crate_core::{Archive, CancelToken, ExtractOptions, Selection};

fn zip_with(path: &Path, files: &[(&str, Vec<u8>)]) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, data) in files {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap();
}

#[test]
fn unpacks_archives_inside_archives() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    support::sample_zip(&d.join("inner.zip"));
    zip_with(
        &d.join("middle.zip"),
        &[
            ("libs/inner.zip", fs::read(d.join("inner.zip")).unwrap()),
            ("note.txt", b"hi".to_vec()),
        ],
    );
    zip_with(
        &d.join("outer.zip"),
        &[("vendor/middle.zip", fs::read(d.join("middle.zip")).unwrap())],
    );
    support::encrypted_zip(&d.join("locked.zip"));

    let out = d.join("out");
    let mut a = Archive::open(d.join("outer.zip")).unwrap();
    a.extract(
        &Selection::All,
        &out,
        &ExtractOptions::default(),
        &mut |_| {},
        &CancelToken::new(),
    )
    .unwrap();
    fs::copy(d.join("locked.zip"), out.join("locked.zip")).unwrap();
    let files = [out.join("vendor/middle.zip"), out.join("locked.zip")];
    let report = nested::unpack(&files, &CancelToken::new()).unwrap();

    assert_eq!(report.unpacked.len(), 2, "{report:?}");
    assert!(out.join("vendor/middle/note.txt").is_file());
    // inner.zip holds one folder, extracted as it is.
    assert!(out.join("vendor/middle/libs/project/README.md").is_file());
    assert!(
        !out.join("vendor/middle.zip").exists()
            && !out.join("vendor/middle/libs/inner.zip").exists()
    );
    // The encrypted one stays, with the reason.
    assert_eq!(report.kept.len(), 1);
    assert!(out.join("locked.zip").is_file());
    assert!(nested::is_archive_name("a.TAR.GZ") && !nested::is_archive_name("a.txt"));
}
