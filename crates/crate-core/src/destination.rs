//! Where "Extract all" puts things by default: next to the archive, in a
//! folder named after it, unless the archive already holds a single folder
//! (then that folder is extracted as it is, with no extra wrapper).

use std::path::{Path, PathBuf};

use crate::entry::Entry;

/// A suggested extraction target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Destination {
    /// Pass this to [`crate::Archive::extract`].
    pub dest: PathBuf,
    /// The folder the files end up in: what to open afterwards.
    pub folder: PathBuf,
    /// `folder`'s name, for labels like Extract all to “name/”.
    pub name: String,
}

/// Suggests where to extract the whole archive at `archive`. Never picks a
/// folder that already exists: `photos (2)` follows `photos`.
pub fn suggest(archive: &Path, entries: &[Entry]) -> Destination {
    let parent = match archive.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_owned(),
        _ => PathBuf::from("."),
    };
    if let Some(top) = single_top_folder(entries) {
        let folder = parent.join(&top);
        if std::fs::symlink_metadata(&folder).is_err() {
            return Destination {
                dest: parent,
                folder,
                name: top,
            };
        }
    }
    let stem = folder_name(archive);
    let name = (1..)
        .map(|n| {
            if n == 1 {
                stem.clone()
            } else {
                format!("{stem} ({n})")
            }
        })
        .find(|name| std::fs::symlink_metadata(parent.join(name)).is_err())
        .expect("some name is free");
    let folder = parent.join(&name);
    Destination {
        dest: folder.clone(),
        folder,
        name,
    }
}

/// `photos.tar.gz` → `photos`. Falls back to `extracted` for names like `.zip`
/// that are nothing but an extension.
pub fn folder_name(archive: &Path) -> String {
    let name = archive
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let lower = name.to_ascii_lowercase();
    let suffixes = [
        ".tar.gz", ".tar.xz", ".tar.zst", ".tar.bz2", ".tgz", ".txz", ".tzst", ".tbz2", ".tar",
        ".zip", ".7z", ".rar",
    ];
    let stem = suffixes
        .iter()
        .find(|s| lower.ends_with(*s) && lower.len() > s.len())
        .map_or(name.as_str(), |s| &name[..name.len() - s.len()]);
    if stem.is_empty() {
        "extracted".into()
    } else {
        stem.into()
    }
}

/// The name of the one folder everything sits in, if there is exactly one
/// and nothing else at the top level. Only names that are safe as a single
/// path component qualify.
fn single_top_folder(entries: &[Entry]) -> Option<String> {
    let mut top: Option<&str> = None;
    let mut is_folder = false;
    for e in entries {
        let path = e.path.trim_start_matches("./");
        let (first, rest) = match path.split_once('/') {
            Some((first, rest)) => (first, rest),
            None => (path, ""),
        };
        if first.is_empty() {
            continue;
        }
        match top {
            None => top = Some(first),
            Some(t) if t == first => {}
            Some(_) => return None,
        }
        is_folder |= e.is_dir() || !rest.is_empty();
        if rest.is_empty() && !e.is_dir() {
            return None; // a file with the folder's name
        }
    }
    let top = top?;
    let safe = is_folder
        && !matches!(top, "." | "..")
        && !top.contains(['\\', ':', '\0'])
        && crate::safety::safe_relative_path(top).is_ok();
    safe.then(|| top.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::EntryKind;

    fn entries(paths: &[&str]) -> Vec<Entry> {
        paths
            .iter()
            .enumerate()
            .map(|(index, p)| Entry {
                index,
                path: (*p).into(),
                kind: if p.ends_with('/') {
                    EntryKind::Dir
                } else {
                    EntryKind::File
                },
                size: 1,
                packed_size: None,
                modified: None,
                mode: None,
                crc32: None,
                encrypted: false,
                method: None,
            })
            .collect()
    }

    #[test]
    fn folder_names() {
        let name = |p: &str| folder_name(Path::new(p));
        assert_eq!(name("dl/photos.tar.gz"), "photos");
        assert_eq!(name("Report.ZIP"), "Report");
        assert_eq!(name("a.b.7z"), "a.b");
        assert_eq!(name("noext"), "noext");
        assert_eq!(name(".zip"), ".zip");
    }

    #[test]
    fn single_folder_is_not_wrapped() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("project-assets.zip");
        let d = suggest(
            &archive,
            &entries(&["project/", "project/a.txt", "project/docs/b.md"]),
        );
        assert_eq!(d.dest, dir.path());
        assert_eq!(d.folder, dir.path().join("project"));
        assert_eq!(d.name, "project");

        // Without folder entries, and with ./ prefixes.
        let d = suggest(&archive, &entries(&["./project/a.txt", "project/b.txt"]));
        assert_eq!(d.name, "project");
    }

    #[test]
    fn several_top_level_items_get_a_folder() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("project-assets.zip");
        for list in [
            &["a.txt", "b.txt"][..],
            &["docs/", "docs/a.md", "README.md"],
            &["single.txt"],
            &["../escape/a.txt"],
        ] {
            let d = suggest(&archive, &entries(list));
            assert_eq!(d.dest, dir.path().join("project-assets"), "{list:?}");
            assert_eq!(d.dest, d.folder);
            assert_eq!(d.name, "project-assets");
        }
    }

    #[test]
    fn existing_folders_are_never_reused() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("photos.zip");
        std::fs::create_dir(dir.path().join("photos")).unwrap();
        std::fs::create_dir(dir.path().join("photos (2)")).unwrap();
        let d = suggest(&archive, &entries(&["a.jpg", "b.jpg"]));
        assert_eq!(d.name, "photos (3)");

        // The single folder exists already: wrap it instead of merging.
        std::fs::create_dir(dir.path().join("trip")).unwrap();
        let d = suggest(&archive, &entries(&["trip/a.jpg"]));
        assert_eq!(d.name, "photos (3)");
        assert_eq!(d.dest, dir.path().join("photos (3)"));
    }
}
