//! Windows: files extracted from a downloaded archive keep the "downloaded
//! from the internet" mark (the `Zone.Identifier` stream).
#![cfg(windows)]

mod support;

use std::fs;
use std::path::Path;

use support::Case;

use crate_core::{Archive, CancelToken, ExtractOptions, Selection};

const MARK: &str = "[ZoneTransfer]\r\nZoneId=3\r\nHostUrl=https://example.com/project.zip\r\n";

fn stream(path: &Path) -> std::path::PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(":Zone.Identifier");
    s.into()
}

fn extract(archive: &Path, out: &Path) {
    Archive::open(archive)
        .unwrap()
        .extract(
            &Selection::All,
            out,
            &ExtractOptions::default(),
            &mut |_| {},
            &CancelToken::new(),
        )
        .unwrap();
}

#[test]
fn extracted_files_keep_the_mark() {
    let (case, zip) = Case::new("project.zip", support::sample_zip);
    fs::write(stream(&zip), MARK).unwrap();
    extract(&zip, &case.out());
    for file in [
        "project/docs/guide.txt",
        "project/docs/تقرير.txt",
        "project/run.sh",
    ] {
        let path = case.out().join(file);
        assert!(path.is_file(), "{file}");
        assert_eq!(fs::read_to_string(stream(&path)).unwrap(), MARK, "{file}");
    }
}

#[test]
fn no_mark_without_one_on_the_archive() {
    let (case, zip) = Case::new("project.zip", support::sample_zip);
    extract(&zip, &case.out());
    let path = case.out().join("project/docs/guide.txt");
    assert!(path.is_file());
    assert!(fs::read(stream(&path)).is_err());
}
