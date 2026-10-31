//! Nested archives and missing volumes (EdgeCases.dc.html): a split set
//! opens from any part; a missing part shows which, and the window reopens
//! by itself when it appears; Batch extract can locate it; an archive inside
//! the archive opens in Crate; "Also unpack archives found inside".
//! Screenshots 11x-*.png.

mod common;
#[path = "../../crates/crate-core/tests/support/mod.rs"]
mod support;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use common::{Harness, g};
use crate_app::{BatchState, BatchUi};
use slint::{ComponentHandle, Model};

static OPENED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

fn record(path: &Path) -> std::io::Result<()> {
    OPENED.lock().unwrap().push(path.to_owned());
    Ok(())
}

/// `archive` cut into `name.001`, `.002`… of `size` bytes (7-Zip style).
fn split(archive: &Path, size: usize) -> Vec<PathBuf> {
    let data = fs::read(archive).unwrap();
    let parts: Vec<PathBuf> = data
        .chunks(size)
        .enumerate()
        .map(|(i, chunk)| {
            let part = PathBuf::from(format!("{}.{:03}", archive.display(), i + 1));
            fs::write(&part, chunk).unwrap();
            part
        })
        .collect();
    fs::remove_file(archive).unwrap();
    parts
}

#[test]
fn edge_cases_flow() {
    let h = Harness::new();
    crate_app::view::set_archive_opener(record);
    let dir = tempfile::tempdir().unwrap();
    let path = |name: &str| dir.path().join(name);

    // A split set opens from part 1, as one archive.
    support::sample_zip(&path("set.zip"));
    let parts = split(&path("set.zip"), 300);
    assert!(parts.len() >= 3);
    let ui = h.open(&parts[0]);
    h.until("listing", || g(&ui).get_status() != "loading");
    assert_eq!(g(&ui).get_status(), "ready", "{}", g(&ui).get_status_text());
    assert_eq!(common::rows(&ui), ["project"]);

    // Part 2 missing: which one, and the window reopens once it's back.
    let away = path("away");
    fs::create_dir(&away).unwrap();
    fs::rename(&parts[1], away.join("set.zip.002")).unwrap();
    let ui = h.open(&parts[0]);
    h.until("missing", || g(&ui).get_status() != "loading");
    assert_eq!(g(&ui).get_status(), "missing");
    assert_eq!(
        g(&ui).get_vol_title(),
        format!("Part 2 of {} is missing", parts.len())
    );
    let found: Vec<bool> = g(&ui).get_vol_parts().iter().map(|p| p.found).collect();
    assert_eq!(found[..2], [true, false]);
    assert_eq!(g(&ui).get_vol_locate(), "Locate part 2…");
    h.shot("110-missing-volume");
    fs::rename(away.join("set.zip.002"), &parts[1]).unwrap();
    h.until("reopened", || {
        g(&ui).get_status() == "ready" && !g(&ui).get_streaming()
    });

    // Batch extract: the same, located through the row.
    fs::rename(&parts[1], away.join("set.zip.002")).unwrap();
    let home = h.home(vec![]);
    home.global::<crate_app::FirstRunUi>().invoke_skip();
    let b = || home.global::<BatchUi>();
    crate_app::batch::start(&home, vec![parts[0].clone()], true);
    h.until("failed", || {
        b().get_items().row_data(0).unwrap().state == BatchState::Failed
    });
    let row = b().get_items().row_data(0).unwrap();
    assert_eq!(row.status, "Missing set.zip.002");
    assert!(row.locate);
    h.shot("111-batch-missing-volume");
    crate_app::batch::located(0, away.join("set.zip.002"));
    h.until("done", || b().get_finished());
    assert!(path("project/README.md").is_file());
    b().invoke_close();

    // An archive inside: opens in Crate (from a temp copy).
    support::sample_zip(&path("inner.zip"));
    let mut zip = zip::ZipWriter::new(fs::File::create(path("outer.zip")).unwrap());
    zip.start_file("vendor/inner.zip", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(&fs::read(path("inner.zip")).unwrap())
        .unwrap();
    zip.start_file("note.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"hi").unwrap();
    zip.finish().unwrap();
    let ui = h.open(&path("outer.zip"));
    h.until("listing", || {
        g(&ui).get_status() == "ready" && !g(&ui).get_streaming()
    });
    g(&ui).invoke_toggle_full();
    let vendor = common::files(&ui)
        .iter()
        .position(|r| r == "vendor")
        .unwrap();
    g(&ui).invoke_row_activated(vendor as i32);
    let inner = common::files(&ui)
        .iter()
        .position(|r| r == "inner.zip")
        .expect("inner.zip listed");
    g(&ui).invoke_row_activated(inner as i32);
    h.until("opened in Crate", || !OPENED.lock().unwrap().is_empty());
    assert!(OPENED.lock().unwrap()[0].ends_with("vendor/inner.zip"));

    // Extract with "Also unpack archives found inside".
    g(&ui).invoke_go_up();
    g(&ui).invoke_extract_dialog();
    g(&ui).invoke_ex_toggle(4);
    assert!(g(&ui).get_ex_nested());
    h.shot("112-extract-nested");
    let into = path("nested-out");
    fs::create_dir(&into).unwrap();
    g(&ui).invoke_ex_dest_edited(into.to_string_lossy().to_string().into());
    g(&ui).invoke_ex_toggle(2); // don't open the folder: show the message
    g(&ui).invoke_ex_confirm("".into());
    h.until("done", || g(&ui).get_show_done());
    assert!(
        g(&ui)
            .get_done_message()
            .contains("1 archive inside unpacked"),
        "{}",
        g(&ui).get_done_message()
    );
    assert!(into.join("outer/vendor/project/README.md").is_file());
    assert!(!into.join("outer/vendor/inner.zip").exists());
}
