//! Drives the full view: switching density, folders, tree, breadcrumbs,
//! sorting, details, search, clipboard, test, and a big tar streaming in.

mod common;
#[path = "../../crates/crate-core/tests/support/mod.rs"]
mod support;

use std::path::Path;

use common::{Harness, files, g};
use slint::Model;

#[test]
fn full_view_flows() {
    let h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let path = |name: &str| dir.path().join(name);
    // The full view's size is remembered under XDG_CONFIG_HOME.
    let config = tempfile::tempdir().unwrap();
    // SAFETY: set before any other thread starts; this file is its own process.
    unsafe { std::env::set_var("XDG_CONFIG_HOME", config.path()) };

    support::sample_zip(&path("project-assets.zip"));
    let ui = h.open(&path("project-assets.zip"));
    h.until("listing", || g(&ui).get_status() == "ready");
    g(&ui).invoke_toggle_full();
    assert!(g(&ui).get_full());
    assert_eq!(files(&ui), ["project"]);
    let crumbs = |ui| -> Vec<String> { g(ui).get_crumbs().iter().map(|c| c.to_string()).collect() };
    assert_eq!(crumbs(&ui), ["project-assets.zip"]);

    // Into a folder: folders first, then files by name.
    g(&ui).invoke_row_activated(0);
    assert_eq!(crumbs(&ui), ["project-assets.zip", "project"]);
    assert_eq!(files(&ui), ["docs", "README.md", "run.sh"]);
    let tree: Vec<String> = g(&ui)
        .get_tree()
        .iter()
        .map(|t| t.name.to_string())
        .collect();
    assert_eq!(
        tree,
        ["project-assets.zip", "project", "docs"],
        "ancestors expand"
    );

    // Details of the row under the cursor.
    g(&ui).invoke_row_pressed(1, false, false);
    assert!(g(&ui).get_has_details());
    let d = g(&ui).get_details();
    assert_eq!(
        (d.name.as_str(), d.path.as_str()),
        ("README.md", "project/README.md")
    );
    assert_eq!(d.method, "Deflate");
    assert_eq!(d.crc.len(), 8);
    g(&ui).invoke_copy_path();
    assert_eq!(g(&ui).get_clip_text(), "project/README.md");
    let serial = g(&ui).get_clip_serial();
    g(&ui).invoke_copy_checksum();
    assert_eq!(g(&ui).get_clip_text(), d.crc);
    assert_eq!(g(&ui).get_clip_serial(), serial + 1);
    h.shot("20-full-folder");

    // Sort by size: biggest first, folders still on top.
    g(&ui).invoke_sort_by(1);
    assert_eq!(files(&ui), ["docs", "run.sh", "README.md"]);
    g(&ui).invoke_sort_by(1);
    assert_eq!(
        files(&ui),
        ["docs", "README.md", "run.sh"],
        "second click flips"
    );
    g(&ui).invoke_sort_by(0);

    // Search from the folder down; results show their folder.
    g(&ui).set_query("guide".into());
    g(&ui).invoke_search("guide".into());
    assert!(g(&ui).get_searching());
    assert_eq!(files(&ui), ["guide.txt"]);
    assert_eq!(
        g(&ui).get_files().row_data(0).unwrap().folder,
        "project/docs"
    );
    h.shot("21-full-search");
    g(&ui).invoke_open_crumb(0);
    assert!(!g(&ui).get_searching(), "navigating clears the search");
    assert_eq!(files(&ui), ["project"]);

    // Selecting a folder and extracting it.
    g(&ui).invoke_row_activated(0);
    g(&ui).invoke_row_pressed(0, false, false); // docs
    assert_eq!(g(&ui).get_selected_count(), 1);
    g(&ui).invoke_extract();
    h.until("done", || g(&ui).get_show_done());
    assert!(path("project/docs/guide.txt").is_file());
    assert!(!path("project/README.md").exists());
    g(&ui).invoke_back_to_archive();
    g(&ui).invoke_go_up();
    assert_eq!(crumbs(&ui), ["project-assets.zip"]);

    // Test the archive while browsing.
    g(&ui).invoke_test_archive();
    h.until("tested", || g(&ui).get_test_state() >= 2);
    assert_eq!(g(&ui).get_test_state(), 2);
    assert_eq!(g(&ui).get_test_status(), "Archive OK");

    // Back to the mini view: same size as before; the full size is kept.
    g(&ui).invoke_toggle_full();
    assert!(!g(&ui).get_full());
    assert_eq!(common::rows(&ui), ["project"]);
    let state = std::fs::read_to_string(config.path().join("crate/state.ini")).unwrap();
    assert!(state.contains("full_width=1180"), "{state}");

    // A big tar streams in; the full view fills while it loads.
    many_files_tar(&path("big.tar.zst"), 120_000);
    let ui = h.open(&path("big.tar.zst"));
    g(&ui).invoke_toggle_full();
    let saw_streaming = std::cell::Cell::new(false);
    h.until("listing", || {
        if g(&ui).get_streaming() && !saw_streaming.get() {
            saw_streaming.set(true);
            h.shot_now("22-full-streaming");
        }
        g(&ui).get_status() == "ready" && !g(&ui).get_streaming()
    });
    assert!(
        saw_streaming.get(),
        "a 120k-entry tar shows rows while it loads"
    );
    assert_eq!(g(&ui).get_item_count(), 120_000);
    assert_eq!(files(&ui).len(), 12, "twelve folders at the top");
    g(&ui).invoke_row_activated(0);
    assert_eq!(files(&ui).len(), 10_000);
    h.shot("23-full-big-folder");
}

/// `n` small files spread over 12 folders, in a tar.zst.
fn many_files_tar(path: &Path, n: usize) {
    let mut tar = support::RawTar::new();
    for i in 0..n {
        tar.file(format!("folder{:02}/file{i:06}.txt", i % 12), b"x");
    }
    let data = tar.finish();
    std::fs::write(path, zstd::encode_all(&data[..], 3).unwrap()).unwrap();
}
