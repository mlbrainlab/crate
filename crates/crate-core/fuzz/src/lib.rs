//! What every fuzz target does with its input: open it as an archive, list
//! it, test it, extract it, and check crate-core's promise that nothing
//! lands outside the destination and no link points out of it.

use std::fs;
use std::path::Path;

use crate_core::{Archive, CancelToken, ExtractOptions, Limits, Selection};

/// Small limits, so a short input can't unpack into gigabytes.
fn options() -> ExtractOptions {
    ExtractOptions {
        limits: Limits {
            max_ratio: Some(100.0),
            ratio_floor: 1024 * 1024,
            check_free_space: false,
        },
        ..ExtractOptions::default()
    }
}

/// `data` as an archive file named `name` (the name matters only for split
/// sets; formats are detected from the bytes).
pub fn exercise(name: &str, data: &[u8]) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(name);
    fs::write(&file, data).unwrap();
    let Ok(mut archive) = Archive::open(&file) else {
        return;
    };
    let _ = archive.entries().len();
    let cancel = CancelToken::new();
    let _ = archive.test(&mut |_| {}, &cancel);
    let dest = dir.path().join("out");
    let _ = archive.extract(&Selection::All, &dest, &options(), &mut |_| {}, &cancel);
    check_contained(dir.path(), &dest);
}

/// Everything under `root` is either the input or inside `dest`, and every
/// link inside `dest` stays inside it.
fn check_contained(root: &Path, dest: &Path) {
    let dest = dest.canonicalize().unwrap_or_else(|_| dest.to_owned());
    for entry in fs::read_dir(root).unwrap().flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        assert!(
            path == dest || name.ends_with(".input") || name.starts_with(".crate-"),
            "something was written outside the destination: {path:?}"
        );
    }
    let mut stack = vec![dest.clone()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            let meta = fs::symlink_metadata(&path).unwrap();
            if meta.file_type().is_symlink() {
                let target = fs::read_link(&path).unwrap();
                let resolved = path.parent().unwrap().join(&target);
                let normal = normalize(&resolved);
                assert!(
                    normal.starts_with(&dest),
                    "link {path:?} -> {target:?} points outside the destination"
                );
            } else if meta.is_dir() {
                stack.push(path);
            }
        }
    }
}

/// `a/b/../c` → `a/c`, without touching the disk.
fn normalize(path: &Path) -> std::path::PathBuf {
    let mut out = std::path::PathBuf::new();
    for c in path.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}
