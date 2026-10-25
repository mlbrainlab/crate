//! What the full view needs from the engine: streamed listings, parallel
//! readers, reading entries into memory, method names.

mod support;

use std::fs;
use std::path::Path;

use support::{Case, Comp, RawTar};

use crate_core::{Archive, CancelToken, Error, OpenOptions, Selection};

/// A tar.zst with `n` small files in a few folders.
fn many_files_tar(path: &Path, n: usize) {
    let mut tar = RawTar::new();
    for i in 0..n {
        tar.file(
            format!("dir{}/file{i:05}.txt", i % 7),
            format!("file {i}\n"),
        );
    }
    let data = tar.finish();
    fs::write(path, zstd::encode_all(&data[..], 1).unwrap()).unwrap();
}

#[test]
fn tar_listing_streams_in_batches() {
    let (_case, path) = Case::new("many.tar.zst", |p| many_files_tar(p, 20_000));
    let mut batches = 0;
    let mut seen = 0;
    let mut last = Default::default();
    let archive = Archive::open_streaming(
        &path,
        &OpenOptions::default(),
        &mut |batch, progress| {
            batches += 1;
            assert_eq!(
                batch.first().map(|e| e.index),
                (!batch.is_empty()).then_some(seen)
            );
            seen += batch.len();
            last = progress;
        },
        &CancelToken::new(),
    )
    .unwrap();
    assert!(batches > 1, "a big tar arrives in several batches");
    assert_eq!(seen, 20_000);
    assert_eq!(last.entries, 20_000);
    assert!(last.bytes_read > 0 && last.bytes_read <= last.bytes_total);
    assert!(archive.is_complete());
    assert_eq!(archive.entries()[123].path, "dir4/file00123.txt");
    assert_eq!(archive.entries()[0].method.as_deref(), Some("Zstandard"));
}

#[test]
fn stopping_a_listing_keeps_what_was_read() {
    let (case, path) = Case::new("many.tar.zst", |p| many_files_tar(p, 20_000));
    let cancel = CancelToken::new();
    let mut archive = Archive::open_streaming(
        &path,
        &OpenOptions::default(),
        &mut |_, progress| {
            if progress.entries >= 4096 {
                cancel.cancel();
            }
        },
        &cancel,
    )
    .unwrap();
    assert!(!archive.is_complete());
    let n = archive.entries().len();
    assert!((4096..20_000).contains(&n), "{n}");
    // What was listed can still be extracted.
    let report = archive
        .extract(
            &Selection::All,
            &case.out(),
            &Default::default(),
            &mut |_| {},
            &CancelToken::new(),
        )
        .unwrap();
    assert_eq!(report.files as usize, n);
}

#[test]
fn clones_read_independently() {
    let (_case, path) = Case::new("sample.zip", support::sample_zip);
    let mut a = Archive::open(&path).unwrap();
    let mut b = a.try_clone().unwrap();
    let readme = a
        .entries()
        .iter()
        .position(|e| e.path == "project/README.md")
        .unwrap();
    let cancel = CancelToken::new();
    // Interleave reads on both handles.
    let one = a.read_entry(readme, 1 << 20, &cancel).unwrap();
    let two = std::thread::spawn(move || b.read_entry(readme, 1 << 20, &CancelToken::new()))
        .join()
        .unwrap()
        .unwrap();
    assert_eq!(one, b"# Sample\n");
    assert_eq!(one, two);
    assert_eq!(a.entries()[readme].method.as_deref(), Some("Deflate"));
}

#[test]
fn read_entries_into_memory() {
    for (name, make) in [
        ("s.zip", support::sample_zip as support::Maker),
        ("s.7z", |p: &Path| support::sample_7z(p, true)),
        ("s.tar.gz", |p: &Path| support::sample_tar(p, Comp::Gz)),
    ] {
        let (_case, path) = Case::new(name, make);
        let mut archive = Archive::open(&path).unwrap();
        let find = |p: &str| {
            archive
                .entries()
                .iter()
                .position(|e| e.path.trim_start_matches("./") == p)
                .unwrap()
        };
        let (readme, guide) = (find("project/README.md"), find("project/docs/guide.txt"));
        let mut got = Vec::new();
        archive
            .read_entries(
                &[guide, readme],
                16,
                &mut |i, data| got.push((i, data)),
                &CancelToken::new(),
            )
            .unwrap();
        got.sort();
        let mut want = vec![
            (readme, b"# Sample\n".to_vec()),
            (guide, b"guide line\nguide".to_vec()),
        ];
        want.sort();
        assert_eq!(got, want, "{name}");
        assert!(archive.entries()[guide].method.is_some(), "{name}");
    }
}

#[test]
fn encrypted_entries_need_a_password_to_read() {
    let (_case, path) = Case::new("e.zip", support::encrypted_zip);
    let mut archive = Archive::open(&path).unwrap();
    let secret = archive.entries().iter().position(|e| e.encrypted).unwrap();
    let cancel = CancelToken::new();
    assert!(matches!(
        archive.read_entry(secret, 100, &cancel),
        Err(Error::PasswordRequired)
    ));
    archive.set_password(Some("secret"));
    assert_eq!(archive.read_entry(secret, 100, &cancel).unwrap(), b"secret");
}
