//! Converting between formats, straight from reader to writer.

mod support;

use std::fs;
use std::path::Path;

use crate_core::convert;
use crate_core::create::{CreateOptions, OutFormat};
use crate_core::{Archive, CancelToken, OpenOptions, Selection};

fn files(archive: &Path, password: Option<&str>) -> Vec<(String, Vec<u8>)> {
    let mut a = Archive::open_with(
        archive,
        &OpenOptions {
            password: password.map(str::to_owned),
            ..OpenOptions::default()
        },
    )
    .unwrap();
    let mut out = Vec::new();
    a.for_each_entry(
        &mut |e, data| {
            let mut buf = Vec::new();
            data.read_to_end(&mut buf)?;
            if e.kind == crate_core::EntryKind::File {
                out.push((e.path.trim_start_matches("./").to_owned(), buf));
            }
            Ok(())
        },
        &CancelToken::new(),
    )
    .unwrap();
    out.sort();
    out
}

fn run(
    src: &Path,
    src_pw: Option<&str>,
    out: &Path,
    format: OutFormat,
    pw: Option<&str>,
) -> convert::ConvertReport {
    let options = CreateOptions {
        format,
        level: format.levels().2,
        password: pw.map(str::to_owned),
        ..CreateOptions::default()
    };
    convert::convert(src, src_pw, out, &options, &mut |_| {}, &CancelToken::new()).unwrap()
}

#[test]
fn rar_zip_tar_7z() {
    let dir = tempfile::tempdir().unwrap();
    // RAR → ZIP: how read-only archives are saved elsewhere.
    let rar = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rar/solid.rar");
    let zip = dir.path().join("from-rar.zip");
    run(&rar, None, &zip, OutFormat::Zip, None);
    assert_eq!(files(&zip, None), files(&rar, None));

    // ZIP → age-encrypted tar.zst.
    let sample = dir.path().join("sample.zip");
    support::sample_zip(&sample);
    let age = dir.path().join("sample.tar.zst.age");
    let report = run(&sample, None, &age, OutFormat::TarZst, Some("pw"));
    assert_eq!(report.created.files, 4);
    assert_eq!(files(&age, Some("pw")), files(&sample, None));

    // tar.gz → 7z, encrypted source entries need the password.
    let tgz = dir.path().join("sample.tar.gz");
    support::sample_tar(&tgz, support::Comp::Gz);
    let sz = dir.path().join("sample.7z");
    run(&tgz, None, &sz, OutFormat::SevenZ, None);
    assert_eq!(files(&sz, None), files(&tgz, None));
    // Permissions survive (run.sh is executable in the sample).
    let a = Archive::open(&sz).unwrap();
    let run_sh = a
        .entries()
        .iter()
        .find(|e| e.path.ends_with("run.sh"))
        .unwrap();
    assert_eq!(run_sh.mode.map(|m| m & 0o111), Some(0o111));
}

#[test]
fn unsafe_names_are_left_out_and_encrypted_sources_need_a_password() {
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("bad.zip");
    support::traversal_zip(&bad);
    let out = dir.path().join("clean.tar");
    let report = run(&bad, None, &out, OutFormat::Tar, None);
    assert!(!report.skipped.is_empty());
    let a = Archive::open(&out).unwrap();
    assert!(
        a.entries()
            .iter()
            .all(|e| !e.path.contains("..") && !e.path.starts_with('/'))
    );

    let enc = dir.path().join("enc.zip");
    support::encrypted_zip(&enc);
    let options = CreateOptions::default();
    let r = convert::convert(
        &enc,
        None,
        &dir.path().join("x.zip"),
        &options,
        &mut |_| {},
        &CancelToken::new(),
    );
    assert!(matches!(r, Err(crate_core::Error::PasswordRequired)));
    assert!(!dir.path().join("x.zip").exists());
    let _ = (Selection::All, fs::metadata(&enc));
}
