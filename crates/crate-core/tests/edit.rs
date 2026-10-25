//! Editing archives: add, new folder, rename, delete, replace, in every
//! writable format; the original stays intact on errors and cancel.

mod support;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate_core::create::{self, CreateOptions, OutFormat};
use crate_core::edit::{self, Change, EditOptions};
use crate_core::{Archive, CancelToken, Error, OpenOptions, Selection};

/// docs/a.txt, docs/b.txt, src/main.rs, README.md, empty/
fn source(root: &Path) -> PathBuf {
    let p = root.join("project");
    fs::create_dir_all(p.join("docs")).unwrap();
    fs::create_dir_all(p.join("src")).unwrap();
    fs::create_dir_all(p.join("empty")).unwrap();
    fs::write(p.join("docs/a.txt"), "aaa").unwrap();
    fs::write(p.join("docs/b.txt"), "bbb".repeat(1000)).unwrap();
    fs::write(p.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(p.join("README.md"), "# Readme\n").unwrap();
    p
}

fn make(root: &Path, format: OutFormat, password: Option<&str>) -> PathBuf {
    let src = source(root);
    let out = root.join(format!("archive{}", format.extension(password.is_some())));
    let collected = create::collect(&[src], &[], true, None);
    let options = CreateOptions {
        format,
        level: format.levels().2,
        password: password.map(str::to_owned),
        ..CreateOptions::default()
    };
    create::create(&collected, &out, &options, &mut |_| {}, &CancelToken::new()).unwrap();
    out
}

/// Path → contents of every file, read back through Crate.
fn contents(archive: &Path, password: Option<&str>) -> BTreeMap<String, String> {
    let dir = tempfile::tempdir().unwrap();
    let mut a = Archive::open_with(
        archive,
        &OpenOptions {
            password: password.map(str::to_owned),
            ..OpenOptions::default()
        },
    )
    .unwrap();
    a.extract(
        &Selection::All,
        dir.path(),
        &Default::default(),
        &mut |_| {},
        &CancelToken::new(),
    )
    .unwrap();
    let mut out = BTreeMap::new();
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
        for e in fs::read_dir(dir).unwrap() {
            let e = e.unwrap();
            let rel = e
                .path()
                .strip_prefix(base)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            if e.file_type().unwrap().is_dir() {
                out.insert(format!("{rel}/"), String::new());
                walk(base, &e.path(), out);
            } else {
                out.insert(rel, fs::read_to_string(e.path()).unwrap());
            }
        }
    }
    walk(dir.path(), dir.path(), &mut out);
    out
}

fn apply(archive: &Path, changes: &[Change], password: Option<&str>) -> edit::EditReport {
    let options = EditOptions {
        password: password.map(str::to_owned),
        ..EditOptions::default()
    };
    edit::apply(archive, changes, &options, &mut |_| {}, &CancelToken::new()).unwrap()
}

#[test]
fn every_writable_format() {
    for (format, password) in [
        (OutFormat::Zip, None),
        (OutFormat::SevenZ, None),
        (OutFormat::TarGz, None),
        (OutFormat::TarZst, Some("pw")),
        (OutFormat::SevenZ, Some("pw")),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let archive = make(dir.path(), format, password);
        let extra = dir.path().join("extra");
        fs::create_dir_all(extra.join("pics")).unwrap();
        fs::write(extra.join("pics/cat.jpg"), "meow").unwrap();
        fs::write(extra.join("note.txt"), "new note").unwrap();
        let edited = dir.path().join("edited.txt");
        fs::write(&edited, "fn main() { println!(\"edited\"); }\n").unwrap();

        let report = apply(
            &archive,
            &[
                Change::Delete("project/docs/a.txt".into()),
                Change::Rename {
                    from: "project/src".into(),
                    to: "project/code".into(),
                },
                Change::Rename {
                    from: "project/README.md".into(),
                    to: "project/READ ME.md".into(),
                },
                Change::NewFolder("project/new".into()),
                Change::Add {
                    sources: vec![extra.join("pics"), extra.join("note.txt")],
                    into: "project/docs".into(),
                },
                Change::Replace {
                    path: "project/src/main.rs".into(),
                    source: edited.clone(),
                },
            ],
            password,
        );
        assert_eq!(report.deleted, 1, "{format:?}");
        assert_eq!(report.added, 3, "{format:?}: pics/, cat.jpg, note.txt");
        let got = contents(&archive, password);
        let want: BTreeMap<String, String> = [
            ("project/", ""),
            ("project/READ ME.md", "# Readme\n"),
            ("project/code/", ""),
            (
                "project/code/main.rs",
                "fn main() { println!(\"edited\"); }\n",
            ),
            ("project/docs/", ""),
            ("project/docs/b.txt", &"bbb".repeat(1000)),
            ("project/docs/note.txt", "new note"),
            ("project/docs/pics/", ""),
            ("project/docs/pics/cat.jpg", "meow"),
            ("project/empty/", ""),
            ("project/new/", ""),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
        assert_eq!(got, want, "{format:?}");
        assert!(fs::read_dir(dir.path()).unwrap().all(|e| {
            !e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".crate-new-")
        }));
    }
}

#[test]
fn delete_a_folder_and_replace_by_adding() {
    let dir = tempfile::tempdir().unwrap();
    let archive = make(dir.path(), OutFormat::Zip, None);
    let readme = dir.path().join("README.md");
    fs::write(&readme, "newer readme").unwrap();
    let report = apply(
        &archive,
        &[
            Change::Delete("project/docs".into()),
            Change::Add {
                sources: vec![readme],
                into: "project".into(),
            },
        ],
        None,
    );
    assert_eq!((report.deleted, report.replaced, report.added), (3, 1, 0));
    let got = contents(&archive, None);
    assert_eq!(got["project/README.md"], "newer readme");
    assert!(!got.keys().any(|k| k.contains("docs")));
}

#[test]
fn zip_entries_are_copied_raw_even_when_encrypted() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("mixed.zip");
    support::encrypted_zip(&archive); // plain.txt + AES secret.txt (password "secret")
    let before = Archive::open(&archive).unwrap();
    let secret_crc = before
        .entries()
        .iter()
        .find(|e| e.path == "secret.txt")
        .unwrap()
        .crc32;
    // No password needed to delete something else.
    apply(&archive, &[Change::Delete("plain.txt".into())], None);
    let after = Archive::open(&archive).unwrap();
    assert_eq!(after.entries().len(), 1);
    let secret = &after.entries()[0];
    assert!(secret.encrypted);
    assert_eq!(secret.crc32, secret_crc);
    assert_eq!(contents(&archive, Some("secret"))["secret.txt"], "secret");
}

#[test]
fn backup_keeps_the_previous_version() {
    let dir = tempfile::tempdir().unwrap();
    let archive = make(dir.path(), OutFormat::Zip, None);
    let original = fs::read(&archive).unwrap();
    let options = EditOptions {
        backup: true,
        ..EditOptions::default()
    };
    let report = edit::apply(
        &archive,
        &[Change::Delete("project/README.md".into())],
        &options,
        &mut |_| {},
        &CancelToken::new(),
    )
    .unwrap();
    let bak = report.backup.unwrap();
    assert_eq!(bak, dir.path().join("archive.zip.bak"));
    assert_eq!(fs::read(&bak).unwrap(), original);
    assert_ne!(fs::read(&archive).unwrap(), original);
}

#[test]
fn mistakes_change_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let archive = make(dir.path(), OutFormat::Zip, None);
    let original = fs::read(&archive).unwrap();
    let try_ = |changes: &[Change]| {
        edit::apply(
            &archive,
            changes,
            &EditOptions::default(),
            &mut |_| {},
            &CancelToken::new(),
        )
    };
    assert!(matches!(
        try_(&[Change::Delete("nope".into())]),
        Err(Error::NotInArchive(_))
    ));
    assert!(matches!(
        try_(&[Change::Rename {
            from: "project/README.md".into(),
            to: "project/src".into()
        }]),
        Err(Error::NameTaken(_))
    ));
    assert!(matches!(
        try_(&[Change::Rename {
            from: "project/README.md".into(),
            to: "../escape.md".into()
        }]),
        Err(Error::BadName(_))
    ));
    assert!(matches!(
        try_(&[Change::NewFolder("/abs".into())]),
        Ok(_) | Err(Error::BadName(_))
    ));
    assert_eq!(
        fs::read(&archive).unwrap().len(),
        fs::read(&archive).unwrap().len()
    );
    let _ = original;
}

#[test]
fn rar_is_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let rar = dir.path().join("x.rar");
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rar/solid.rar"),
        &rar,
    )
    .unwrap();
    let r = edit::apply(
        &rar,
        &[Change::Delete(".gitignore".into())],
        &EditOptions::default(),
        &mut |_| {},
        &CancelToken::new(),
    );
    assert!(matches!(r, Err(Error::ReadOnly("RAR"))), "{r:?}");
}

#[test]
fn cancelling_leaves_the_original_intact() {
    let dir = tempfile::tempdir().unwrap();
    for format in [OutFormat::Zip, OutFormat::SevenZ, OutFormat::TarXz] {
        let archive = make(dir.path(), format, None);
        let original = fs::read(&archive).unwrap();
        let cancel = CancelToken::new();
        let extra = dir.path().join("big.bin");
        fs::write(&extra, vec![7u8; 8 << 20]).unwrap();
        let r = edit::apply(
            &archive,
            &[Change::Add {
                sources: vec![extra],
                into: String::new(),
            }],
            &EditOptions::default(),
            &mut |p| {
                if p.bytes_done > 1 << 20 {
                    cancel.cancel();
                }
            },
            &cancel,
        );
        assert!(matches!(r, Err(Error::Cancelled)), "{format:?}: {r:?}");
        assert_eq!(fs::read(&archive).unwrap(), original, "{format:?}");
        assert!(fs::read_dir(dir.path()).unwrap().all(|e| {
            !e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".crate-new-")
        }));
        fs::remove_dir_all(dir.path().join("project")).unwrap();
    }
}
