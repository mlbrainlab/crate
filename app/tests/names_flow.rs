//! Names a Windows disk won't take and names that differ only by case
//! (FileIssues.dc.html), asked before extracting. Runs with Windows' naming
//! rules on any system (`CRATE_NAME_RULES=windows`). Screenshots 13x-*.png.

mod common;

use std::fs;
use std::path::Path;

use common::{Harness, g};
use slint::Model;

/// `CON.txt`, `report: v2?.pdf`, `notes.`, a 320-character path, and
/// `README.md` / `readme.md` (the second one newer).
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
    add("CON.txt", "con", 1_700_000_000);
    add("report: v2?.pdf", "pdf", 1_700_000_000);
    add("notes.", "notes", 1_700_000_000);
    let deep = vec!["d".repeat(50); 6].join("/");
    add(&format!("{deep}/deep.txt"), "deep", 1_700_000_000);
    add("README.md", "old readme", 1_700_000_000);
    add("readme.md", "new readme", 1_760_000_000);
    fs::write(path, b.into_inner().unwrap()).unwrap();
}

#[test]
fn names_flow() {
    let h = Harness::new();
    // SAFETY: set before the window starts any threads.
    unsafe { std::env::set_var("CRATE_NAME_RULES", "windows") };
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("awkward.tar");
    awkward(&archive);

    let ui = h.open(&archive);
    h.until("listing", || {
        g(&ui).get_status() == "ready" && !g(&ui).get_streaming()
    });
    g(&ui).invoke_extract();
    h.until("names", || g(&ui).get_nf_show());
    assert_eq!(
        g(&ui).get_nf_title(),
        "3 names need to change to extract here"
    );
    let rows: Vec<_> = g(&ui).get_nf_rows().iter().collect();
    let pairs: Vec<(String, String)> = rows
        .iter()
        .map(|r| (r.old.to_string(), r.new.to_string()))
        .collect();
    assert_eq!(pairs[0], ("CON.txt".into(), "CON_.txt".into()));
    assert_eq!(
        pairs[1],
        ("report: v2?.pdf".into(), "report_ v2_.pdf".into())
    );
    assert_eq!(pairs[2], ("notes.".into(), "notes".into()));
    assert_eq!(rows[0].why, "reserved device name");
    assert_eq!(rows[1].why, "contains : and ?");
    assert_eq!(rows[2].why, "ends with a dot or space");
    assert!(rows[3].long, "the long path is shown too");
    assert!(rows[3].old.starts_with("…/dddd"), "{}", rows[3].old);
    assert!(rows[3].old.ends_with(" characters)"), "{}", rows[3].old);
    h.shot("130-names-windows");

    // Edit one name, then rename and extract.
    g(&ui).invoke_nf_answer(1);
    assert!(g(&ui).get_nf_editing());
    h.shot("131-names-editing");
    g(&ui).invoke_nf_edited(0, "console.txt".into());
    g(&ui).invoke_nf_answer(2);
    assert!(!g(&ui).get_nf_show());

    // Then the case clash.
    h.until("case clash", || g(&ui).get_cc_show());
    assert_eq!(g(&ui).get_cc_second(), "readme.md");
    assert_eq!(g(&ui).get_cc_both(), "readme (2).md");
    assert!(g(&ui).get_cc_first().starts_with("README.md · 10 bytes"));
    h.shot("132-case-clash");
    g(&ui).invoke_cc_answer(2, false);
    h.until("done", || g(&ui).get_show_done());

    let out = dir.path().join("awkward");
    let read = |p: &str| fs::read_to_string(out.join(p)).unwrap();
    assert_eq!(read("console.txt"), "con");
    assert_eq!(read("report_ v2_.pdf"), "pdf");
    assert_eq!(read("notes"), "notes");
    assert_eq!(read("README.md"), "old readme");
    assert_eq!(read("readme (2).md"), "new readme");
    g(&ui).invoke_back_to_archive();

    // Skip them, keep the newer readme only: into a fresh folder.
    fs::remove_dir_all(&out).unwrap();
    g(&ui).invoke_extract();
    h.until("names", || g(&ui).get_nf_show());
    g(&ui).invoke_nf_answer(0);
    h.until("case clash", || g(&ui).get_cc_show());
    g(&ui).invoke_cc_answer(0, true);
    h.until("done", || g(&ui).get_show_done());
    assert!(!out.join("CON_.txt").exists() && !out.join("notes").exists());
    let readmes: Vec<String> = fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.to_lowercase().starts_with("readme"))
        .collect();
    assert_eq!(readmes.len(), 1, "one file, the newer one's contents");
    assert_eq!(read(&readmes[0]), "new readme");
    g(&ui).invoke_back_to_archive();

    // Esc-style cancel writes nothing.
    fs::remove_dir_all(&out).unwrap();
    g(&ui).invoke_extract();
    h.until("names", || g(&ui).get_nf_show());
    g(&ui).invoke_nf_answer(3);
    assert!(!g(&ui).get_nf_show());
    h.render();
    assert!(!out.exists());

    // In Arabic: the rows and buttons mirror.
    crate_app::i18n::set(crate_app::i18n::Language::Arabic);
    g(&ui).invoke_extract();
    h.until("names", || g(&ui).get_nf_show());
    assert!(
        g(&ui).get_nf_title().contains("أسماء"),
        "{}",
        g(&ui).get_nf_title()
    );
    h.shot("133-names-arabic");
    g(&ui).invoke_nf_answer(2);
    h.until("case clash", || g(&ui).get_cc_show());
    h.shot("134-case-clash-arabic");
    g(&ui).invoke_cc_answer(3, false);
    crate_app::i18n::set(crate_app::i18n::Language::English);
}
