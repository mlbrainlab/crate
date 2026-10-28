//! Names a Windows disk won't take, and names that differ only by case
//! (`FileIssues.dc.html`). The Windows rules are passed in, so this runs on
//! every system.

mod support;

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use support::Case;

use crate_core::names::{NameIssue, NameRules};
use crate_core::{
    Archive, CancelToken, CaseChoice, ExtractOptions, ExtractReport, Selection, SkipReason,
};

/// A deep path: 6 folders of 50 characters, about 320 characters in all.
fn deep() -> String {
    let folder = "d".repeat(50);
    format!("{}/deep.txt", vec![folder; 6].join("/"))
}

/// `CON.txt`, `report: v2?.pdf`, `notes.`, `aux/inner.txt`, a deep path,
/// `README.md` (older) and `docs/readme.md`, `readme.md` (newer).
fn awkward(path: &Path) {
    let mut b = tar::Builder::new(Vec::new());
    let mut add = |name: &str, data: &str, mtime: u64| {
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(0o644);
        h.set_mtime(mtime);
        h.set_entry_type(tar::EntryType::Regular);
        b.append_data(&mut h, name, data.as_bytes()).unwrap();
    };
    add("CON.txt", "con", 1_000_000);
    add("report: v2?.pdf", "pdf", 1_000_000);
    add("notes.", "notes", 1_000_000);
    add("aux/inner.txt", "inner", 1_000_000);
    add(&deep(), "deep", 1_000_000);
    add("README.md", "old readme", 1_000_000);
    add("readme.md", "new readme", 2_000_000);
    fs::write(path, b.into_inner().unwrap()).unwrap();
}

fn windows(case_clash: CaseChoice) -> ExtractOptions {
    ExtractOptions {
        names: NameRules::windows(),
        case_clash,
        ..ExtractOptions::default()
    }
}

fn extract(case: &Case, archive: &Path, options: &ExtractOptions) -> ExtractReport {
    Archive::open(archive)
        .unwrap()
        .extract(
            &Selection::All,
            &case.out(),
            options,
            &mut |_| {},
            &CancelToken::new(),
        )
        .unwrap()
}

#[test]
fn check_lists_what_will_change() {
    let (case, tar) = Case::new("awkward.tar", awkward);
    let archive = Archive::open(&tar).unwrap();
    let check = archive.check_names(&Selection::All, &case.out(), &windows(CaseChoice::KeepBoth));
    let renamed: Vec<(&str, &str)> = check
        .renamed
        .iter()
        .map(|r| (r.path.as_str(), r.new_name.as_str()))
        .collect();
    assert_eq!(
        renamed,
        [
            ("CON.txt", "CON_.txt"),
            ("report: v2?.pdf", "report_ v2_.pdf"),
            ("notes.", "notes"),
            ("aux", "aux_"),
        ]
    );
    assert_eq!(
        check.renamed[1].issue,
        NameIssue::Characters(vec![':', '?'])
    );
    assert_eq!(check.long_paths.len(), 1);
    assert_eq!(check.long_paths[0].0, deep());
    assert!(check.long_paths[0].1 > 260);
    assert_eq!(check.case_clashes.len(), 1);
    let clash = &check.case_clashes[0];
    assert_eq!(
        (clash.path.as_str(), clash.other.as_str()),
        ("readme.md", "README.md")
    );
    assert_eq!(clash.both_name, "readme (2).md");
    assert!(clash.modified > clash.other_modified);

    // This system's rules: nothing to say on Linux.
    if cfg!(target_os = "linux") {
        let native = archive.check_names(&Selection::All, &case.out(), &ExtractOptions::default());
        assert!(native.is_empty(), "{native:?}");
    }
}

#[test]
fn renames_and_keeps_both() {
    let (case, tar) = Case::new("awkward.tar", awkward);
    let report = extract(&case, &tar, &windows(CaseChoice::KeepBoth));
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    let out = case.out();
    assert_eq!(fs::read_to_string(out.join("CON_.txt")).unwrap(), "con");
    assert_eq!(
        fs::read_to_string(out.join("report_ v2_.pdf")).unwrap(),
        "pdf"
    );
    assert_eq!(fs::read_to_string(out.join("notes")).unwrap(), "notes");
    assert_eq!(
        fs::read_to_string(out.join("aux_/inner.txt")).unwrap(),
        "inner"
    );
    assert_eq!(fs::read_to_string(out.join(deep())).unwrap(), "deep");
    assert_eq!(
        fs::read_to_string(out.join("README.md")).unwrap(),
        "old readme"
    );
    assert_eq!(
        fs::read_to_string(out.join("readme (2).md")).unwrap(),
        "new readme"
    );
}

#[test]
fn keep_newer_skip_and_edited_names() {
    let (case, tar) = Case::new("awkward.tar", awkward);
    let report = extract(&case, &tar, &windows(CaseChoice::KeepNewer));
    let out = case.out();
    let readmes: Vec<_> = fs::read_dir(&out)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_lowercase())
        .filter(|n| n.starts_with("readme"))
        .collect();
    assert_eq!(readmes, ["readme.md"], "one file, the newer");
    // Whichever spelling the disk keeps, the contents are the newer file's.
    let kept = if out.join("README.md").exists() {
        "README.md"
    } else {
        "readme.md"
    };
    assert_eq!(fs::read_to_string(out.join(kept)).unwrap(), "new readme");
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].path, "README.md");
    assert_eq!(report.skipped[0].reason, SkipReason::Conflict);

    // Skip the clash, skip the bad names, and a name the user typed.
    let (case, tar) = Case::new("awkward.tar", awkward);
    let options = ExtractOptions {
        skip_bad_names: true,
        ..windows(CaseChoice::Skip)
    };
    let report = extract(&case, &tar, &options);
    let mut skipped: Vec<_> = report.skipped.iter().map(|s| s.path.as_str()).collect();
    skipped.sort();
    assert_eq!(
        skipped,
        [
            "CON.txt",
            "aux/inner.txt",
            "notes.",
            "readme.md",
            "report: v2?.pdf"
        ]
    );

    let (case, tar) = Case::new("awkward.tar", awkward);
    let options = ExtractOptions {
        edited_names: HashMap::from([
            ("CON.txt".to_owned(), "console.txt".to_owned()),
            ("aux".to_owned(), "../escape".to_owned()),
        ]),
        case_choices: HashMap::from([("readme.md".to_owned(), CaseChoice::Skip)]),
        ..windows(CaseChoice::KeepBoth)
    };
    extract(&case, &tar, &options);
    let out = case.out();
    assert_eq!(fs::read_to_string(out.join("console.txt")).unwrap(), "con");
    assert!(
        out.join("aux_/inner.txt").exists(),
        "unsafe typed name ignored"
    );
    assert!(!case.dir.path().join("escape").exists());
    assert!(!out.join("readme (2).md").exists());
}
