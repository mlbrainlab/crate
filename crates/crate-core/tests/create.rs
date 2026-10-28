//! Creating archives: every format round-trips byte for byte through Crate's
//! own reader, with exclusions, permissions, links, passwords and volumes.

mod support;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate_core::create::{self, CreateOptions, OutFormat};
use crate_core::{Archive, CancelToken, EntryKind, Error, ExtractOptions, OpenOptions, Selection};

/// A small project folder: nested files, an empty file, an executable, a
/// symlink, and things to exclude.
fn project(root: &Path) -> std::path::PathBuf {
    let p = root.join("project");
    fs::create_dir_all(p.join("src/deep")).unwrap();
    fs::create_dir_all(p.join(".git/objects")).unwrap();
    fs::create_dir_all(p.join("empty")).unwrap();
    fs::write(p.join("README.md"), "# Project\n").unwrap();
    fs::write(p.join("src/main.rs"), "fn main() {}\n".repeat(500)).unwrap();
    fs::write(
        p.join("src/deep/data.bin"),
        (0..200_000u32)
            .map(|i| (i * 31 % 251) as u8)
            .collect::<Vec<_>>(),
    )
    .unwrap();
    fs::write(p.join("src/deep/تقرير.txt"), "مرحبا\n").unwrap();
    fs::write(p.join("empty.txt"), "").unwrap();
    fs::write(p.join("run.sh"), "#!/bin/sh\necho hi\n").unwrap();
    fs::write(p.join("scratch.tmp"), "junk").unwrap();
    fs::write(p.join(".git/objects/abc"), "object").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(p.join("run.sh"), fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink("README.md", p.join("readme-link")).unwrap();
    }
    p
}

/// Every regular file under `dir`, relative path → bytes.
fn files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for e in fs::read_dir(dir).unwrap() {
            let e = e.unwrap();
            let t = e.file_type().unwrap();
            let rel = e
                .path()
                .strip_prefix(base)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            if t.is_dir() {
                walk(base, &e.path(), out);
            } else if t.is_file() {
                out.insert(rel, fs::read(e.path()).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

fn make(src: &Path, out: &Path, options: &CreateOptions) -> create::CreateReport {
    let exclude = options.exclude.clone();
    let collected = create::collect(
        &[src.to_owned()],
        &exclude,
        options.keep_permissions,
        Some(out),
    );
    create::create(&collected, out, options, &mut |_| {}, &CancelToken::new()).unwrap()
}

fn extract(archive: &Path, password: Option<&str>, to: &Path) {
    let mut a = Archive::open_with(
        archive,
        &OpenOptions {
            password: password.map(str::to_owned),
            ..OpenOptions::default()
        },
    )
    .unwrap();
    a.extract(
        &crate_core::Selection::All,
        to,
        &Default::default(),
        &mut |_| {},
        &CancelToken::new(),
    )
    .unwrap();
}

#[test]
fn every_format_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let src = project(dir.path());
    let mut want = files(&src);
    want.retain(|k, _| !k.starts_with(".git") && !k.ends_with(".tmp"));
    for format in OutFormat::ALL {
        let out = dir.path().join(format!("out{}", format.extension(false)));
        let options = CreateOptions {
            format,
            level: format.levels().2,
            exclude: vec![".git/".into(), "*.tmp".into()],
            ..CreateOptions::default()
        };
        let report = make(&src, &out, &options);
        assert_eq!(report.excluded, 2, "{format:?}");
        assert_eq!(report.outputs, std::slice::from_ref(&out));
        let back = dir.path().join(format!("back-{}", format.name()));
        extract(&out, None, &back);
        let mut got = files(&back.join("project"));
        // The symlink comes back as a link (not a regular file).
        got.remove("readme-link");
        assert_eq!(
            got.keys().collect::<Vec<_>>(),
            want.keys().collect::<Vec<_>>(),
            "{format:?}"
        );
        assert_eq!(got, want, "{format:?}: contents");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(back.join("project/run.sh"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o755, "{format:?}: executable bit kept");
            let link = fs::read_link(back.join("project/readme-link")).unwrap();
            assert_eq!(link, Path::new("README.md"), "{format:?}: symlink kept");
            assert!(
                back.join("project/empty").is_dir(),
                "{format:?}: empty folder kept"
            );
        }
        // Nothing left over next to the archive.
        assert!(fs::read_dir(dir.path()).unwrap().all(|e| {
            !e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".crate-new-")
        }));
    }
}

#[test]
fn passwords_for_zip_and_7z() {
    let dir = tempfile::tempdir().unwrap();
    let src = project(dir.path());
    for format in [OutFormat::Zip, OutFormat::SevenZ] {
        let out = dir.path().join(format!("secret{}", format.extension(true)));
        let options = CreateOptions {
            format,
            password: Some("hunter2".into()),
            ..CreateOptions::default()
        };
        make(&src, &out, &options);
        let back = dir.path().join(format!("back-{}", format.name()));
        if format == OutFormat::SevenZ {
            assert!(Archive::open(&out).is_err(), "7z hides the names too");
        } else {
            let a = Archive::open(&out).unwrap();
            assert!(a.entries().iter().any(|e| e.encrypted));
        }
        extract(&out, Some("hunter2"), &back);
        assert_eq!(
            fs::read(back.join("project/README.md")).unwrap(),
            b"# Project\n"
        );
    }
}

// The project has a symlink only where one can be made without privileges.
#[cfg(unix)]
#[test]
fn without_permissions_links_become_files() {
    let dir = tempfile::tempdir().unwrap();
    let src = project(dir.path());
    let out = dir.path().join("plain.tar.gz");
    let options = CreateOptions {
        format: OutFormat::TarGz,
        level: 6,
        keep_permissions: false,
        ..CreateOptions::default()
    };
    make(&src, &out, &options);
    let a = Archive::open(&out).unwrap();
    let link = a
        .entries()
        .iter()
        .find(|e| e.path.ends_with("readme-link"))
        .unwrap();
    assert_eq!(link.kind, EntryKind::File);
    let readme = a
        .entries()
        .iter()
        .find(|e| e.path.ends_with("README.md"))
        .unwrap();
    assert_eq!(readme.mode, Some(0o644));
}

#[test]
fn volumes() {
    let dir = tempfile::tempdir().unwrap();
    let src = project(dir.path());
    let out = dir.path().join("split.7z");
    let options = CreateOptions {
        format: OutFormat::SevenZ,
        level: 0,
        volume_size: Some(64 * 1024),
        ..CreateOptions::default()
    };
    let report = make(&src, &out, &options);
    assert!(report.outputs.len() >= 3, "{:?}", report.outputs);
    assert_eq!(report.outputs[0], dir.path().join("split.7z.001"));
    assert!(!out.exists(), "only the volumes");
    // Joined back together they are a normal archive.
    let joined = dir.path().join("joined.7z");
    let mut data = Vec::new();
    for part in &report.outputs {
        let bytes = fs::read(part).unwrap();
        assert!(bytes.len() as u64 <= 64 * 1024);
        data.extend(bytes);
    }
    fs::write(&joined, data).unwrap();
    let back = dir.path().join("back");
    extract(&joined, None, &back);
    assert_eq!(
        fs::read(back.join("project/README.md")).unwrap(),
        b"# Project\n"
    );
}

/// Split sets open from any part, as one archive; a missing part, in the
/// middle or at the end, is named; a part found in another folder fills in.
#[test]
fn split_sets_open_from_any_part() {
    use crate_core::volumes::VolumeSet;
    // (Crate splits 7z and ZIP; other formats are read the same way.)
    for format in [OutFormat::SevenZ, OutFormat::Zip] {
        let dir = tempfile::tempdir().unwrap();
        let src = project(dir.path());
        let out = dir.path().join(format!("split{}", format.extension(false)));
        let options = CreateOptions {
            format,
            level: 0,
            volume_size: Some(16 * 1024),
            ..CreateOptions::default()
        };
        let report = make(&src, &out, &options);
        let parts = report.outputs;
        assert!(parts.len() >= 3, "{format:?}: {parts:?}");
        let back = dir.path().join("back");
        extract(&parts[1], None, &back);
        assert_eq!(
            fs::read(back.join("project/README.md")).unwrap(),
            b"# Project\n",
            "{format:?}"
        );
        let size = Archive::open(&parts[0]).unwrap().file_size();
        assert_eq!(
            size,
            parts
                .iter()
                .map(|p| fs::metadata(p).unwrap().len())
                .sum::<u64>()
        );

        // Part 2 missing: named, with the count.
        let away = dir.path().join("away");
        fs::create_dir(&away).unwrap();
        let moved = away.join(parts[1].file_name().unwrap());
        fs::rename(&parts[1], &moved).unwrap();
        match Archive::open(&parts[0]) {
            Err(Error::MissingVolume { part, total, name }) => {
                assert_eq!((part, total), (2, Some(parts.len())), "{format:?}");
                assert_eq!(name, moved.file_name().unwrap().to_string_lossy());
            }
            Err(e) => panic!("{format:?}: {e}"),
            Ok(_) => panic!("{format:?}: opened without part 2"),
        }
        // Located elsewhere: fine again.
        let options = OpenOptions {
            volumes: vec![moved.clone()],
            ..OpenOptions::default()
        };
        let mut a = Archive::open_with(&parts[0], &options).unwrap();
        let again = dir.path().join("again");
        a.extract(
            &Selection::All,
            &again,
            &ExtractOptions::default(),
            &mut |_| {},
            &CancelToken::new(),
        )
        .unwrap();
        assert!(again.join("project/README.md").is_file());
        fs::rename(&moved, &parts[1]).unwrap();

        // The last part missing: 7z and ZIP notice on open (their ends say
        // so).
        let last = parts.last().unwrap();
        fs::remove_file(last).unwrap();
        match Archive::open(&parts[0]) {
            Err(Error::MissingVolume { part, total, .. }) => {
                assert_eq!((part, total), (parts.len(), None), "{format:?}");
            }
            Err(e) => panic!("{format:?}: {e}"),
            Ok(_) => panic!("{format:?}: opened without the last part"),
        }
        assert_eq!(
            VolumeSet::find(&parts[0]).unwrap().parts.len(),
            parts.len() - 1
        );
    }
}

#[test]
fn cancelling_leaves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let src = project(dir.path());
    let out = dir.path().join("never.zip");
    let cancel = CancelToken::new();
    let collected = create::collect(&[src], &[], true, None);
    let result = create::create(
        &collected,
        &out,
        &CreateOptions::default(),
        &mut |p| {
            if p.files_done >= 1 {
                cancel.cancel();
            }
        },
        &cancel,
    );
    assert!(
        matches!(result, Err(crate_core::Error::Cancelled)),
        "{result:?}"
    );
    assert!(!out.exists());
    assert!(fs::read_dir(dir.path()).unwrap().all(|e| {
        !e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".crate-new-")
    }));
}

#[test]
fn replacing_an_existing_file_is_atomic() {
    let dir = tempfile::tempdir().unwrap();
    let src = project(dir.path());
    let out = dir.path().join("x.zip");
    fs::write(&out, "old").unwrap();
    make(&src, &out, &CreateOptions::default());
    assert!(Archive::open(&out).is_ok());
}

#[test]
fn the_archive_never_contains_itself() {
    let dir = tempfile::tempdir().unwrap();
    let src = project(dir.path());
    let out = src.join("inside.zip");
    fs::write(&out, "previous version").unwrap();
    make(&src, &out, &CreateOptions::default());
    let a = Archive::open(&out).unwrap();
    assert!(a.entries().iter().all(|e| !e.path.ends_with("inside.zip")));
}

#[test]
fn same_names_get_numbers() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("a/notes")).unwrap();
    fs::create_dir_all(dir.path().join("b/notes")).unwrap();
    let c = create::collect(
        &[dir.path().join("a/notes"), dir.path().join("b/notes")],
        &[],
        true,
        None,
    );
    let names: Vec<&str> = c.items.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["notes", "notes (2)"]);
    let _ = support::Comp::None;
}

#[test]
fn tar_with_a_password_is_age_encrypted() {
    let dir = tempfile::tempdir().unwrap();
    let src = project(dir.path());
    // Every unlock costs about a second (age tunes scrypt that way): two formats.
    for format in [OutFormat::TarZst, OutFormat::Tar] {
        let out = dir.path().join(format!("backup{}", format.extension(true)));
        assert!(out.to_string_lossy().ends_with(".age"));
        let options = CreateOptions {
            format,
            level: format.levels().2,
            password: Some("correct horse".into()),
            ..CreateOptions::default()
        };
        make(&src, &out, &options);
        assert!(
            fs::read(&out)
                .unwrap()
                .starts_with(b"age-encryption.org/v1\n")
        );
        assert!(
            matches!(
                Archive::open(&out),
                Err(crate_core::Error::PasswordRequired)
            ),
            "{format:?}"
        );
        let wrong = Archive::open_with(
            &out,
            &OpenOptions {
                password: Some("nope".into()),
                ..OpenOptions::default()
            },
        );
        assert!(
            matches!(wrong, Err(crate_core::Error::WrongPassword)),
            "{format:?}"
        );
        let a = Archive::open_with(
            &out,
            &OpenOptions {
                password: Some("correct horse".into()),
                ..OpenOptions::default()
            },
        )
        .unwrap();
        assert_eq!(a.format(), crate_core::Format::TarAge);
        let back = dir.path().join(format!("back-{}", format.name()));
        extract(&out, Some("correct horse"), &back);
        assert_eq!(
            fs::read(back.join("project/src/main.rs")).unwrap(),
            "fn main() {}\n".repeat(500).as_bytes(),
            "{format:?}"
        );
    }
}
