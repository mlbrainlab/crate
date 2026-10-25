//! Drives the real mini view (controller, worker threads and all) on Slint's
//! software renderer: open, select, extract, passwords, encodings, errors.
//! Screenshots of each state go to `target/ui-snapshots/` for a visual check.

#[path = "../../crates/crate-core/tests/support/mod.rs"]
mod support;

mod common;

use std::path::Path;

use common::{Harness, OPENED, g, record_opens, rows};
use slint::{ComponentHandle, Model};

/// One test, because the platform and the controller live on this thread.
#[test]
fn mini_view_flows() {
    let h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    // Opened items go to $XDG_CACHE_HOME/crate; record instead of launching apps.
    let cache = tempfile::tempdir().unwrap();
    record_opens(cache.path());
    let path = |name: &str| dir.path().join(name);

    // Open, select, extract: the smart folder keeps the archive's own folder.
    support::sample_zip(&path("project-assets.zip"));
    let ui = h.open(&path("project-assets.zip"));
    h.until("listing", || g(&ui).get_status() == "ready");
    assert_eq!(rows(&ui), ["project"]);
    assert_eq!(g(&ui).get_dest_name(), "project");
    assert_eq!(g(&ui).get_format_name(), "ZIP");
    h.shot("01-ready");
    g(&ui).invoke_search("readme".into());
    assert_eq!(rows(&ui), ["project/README.md"]);
    g(&ui).invoke_row_pressed(0, false, false);
    assert_eq!(g(&ui).get_selected_count(), 1);
    h.shot("02-search-selected");
    g(&ui).invoke_extract();
    h.until("done popup", || g(&ui).get_show_done());
    h.shot("03-done");
    assert!(path("project/README.md").is_file());
    assert!(!path("project/run.sh").exists(), "only the selection");
    assert!(
        g(&ui)
            .get_done_message()
            .starts_with("1 selected item extracted")
    );
    g(&ui).invoke_back_to_archive();
    // Now "project/" exists, so everything goes into a new folder.
    assert_eq!(g(&ui).get_dest_name(), "project-assets");
    g(&ui).invoke_search("".into());
    assert_eq!(
        g(&ui).get_selected_count(),
        1,
        "selection survives the search"
    );
    g(&ui).invoke_search("readme".into());
    g(&ui).invoke_row_pressed(0, true, false);
    assert_eq!(g(&ui).get_selected_count(), 0, "Ctrl+click unselects");
    g(&ui).invoke_extract();
    h.until("done popup", || g(&ui).get_show_done());
    assert!(path("project-assets/project/run.sh").is_file());

    // Dark mode follows the same tokens.
    ui.global::<crate_app::Palette>()
        .set_color_scheme(slint::language::ColorScheme::Dark);
    g(&ui).invoke_row_pressed(0, false, false);
    g(&ui).invoke_extract();
    h.until("done popup", || g(&ui).get_show_done());
    h.shot("11-dark-done");
    g(&ui).invoke_back_to_archive();
    h.shot("12-dark");
    ui.global::<crate_app::Palette>()
        .set_color_scheme(slint::language::ColorScheme::Light);

    // Keyboard selection and double-click to open.
    g(&ui).invoke_search("".into());
    g(&ui).invoke_search("p".into()); // every entry: a longer list
    let n = g(&ui).get_rows().row_count() as i32;
    g(&ui).invoke_select_all();
    assert_eq!(g(&ui).get_selected_count(), n);
    g(&ui).invoke_clear_selection();
    g(&ui).invoke_step(1, false);
    g(&ui).invoke_step(1, true);
    g(&ui).invoke_step(1, true);
    assert_eq!((g(&ui).get_selected_count(), g(&ui).get_cursor()), (3, 2));
    g(&ui).invoke_row_pressed(0, false, false);
    g(&ui).invoke_row_pressed(3, false, true);
    assert_eq!(
        g(&ui).get_selected_count(),
        4,
        "Shift+click selects a range"
    );
    h.shot("13-keyboard");
    let readme = rows(&ui)
        .iter()
        .position(|r| r == "project/README.md")
        .unwrap();
    g(&ui).invoke_row_activated(readme as i32);
    h.until("opened", || !OPENED.lock().unwrap().is_empty());
    let opened = OPENED.lock().unwrap().pop().unwrap();
    assert!(opened.ends_with("project/README.md"), "{opened:?}");
    assert!(opened.starts_with(cache.path()), "{opened:?}");
    assert_eq!(std::fs::read_to_string(&opened).unwrap(), "# Sample\n");

    // Open → edit → update: a save to the opened copy offers to update.
    assert_eq!(g(&ui).get_edits_open(), 1);
    std::fs::write(&opened, "# Edited in a text editor\n").unwrap();
    h.until("update prompt", || g(&ui).get_upd_show());
    assert!(
        g(&ui).get_upd_text().contains("README.md was changed"),
        "{}",
        g(&ui).get_upd_text()
    );
    h.shot("14-update-prompt");
    g(&ui).invoke_upd_update();
    h.until("updated", || {
        crate_core::Archive::open(path("project-assets.zip"))
            .and_then(|mut a| {
                let i = a
                    .entries()
                    .iter()
                    .position(|e| e.path == "project/README.md")
                    .unwrap();
                a.read_entry(i, 1000, &crate_core::CancelToken::new())
            })
            .is_ok_and(|d| d == b"# Edited in a text editor\n")
    });
    h.until("tray clear", || {
        g(&ui).get_edits_changed() == 0 && !g(&ui).get_busy()
    });
    // Changed again, then closing: asked first.
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&opened, "# Edited twice\n").unwrap();
    h.until("changed again", || {
        g(&ui).get_edits_changed() == 1 || g(&ui).get_upd_show()
    });
    g(&ui).invoke_upd_discard();
    std::fs::write(&opened, "# Edited three times\n").unwrap();
    h.until("changed", || g(&ui).get_edits_changed() == 1);
    g(&ui).invoke_close_window();
    assert!(g(&ui).get_close_show());
    assert!(
        g(&ui)
            .get_close_title()
            .contains("isn’t in the archive yet")
    );
    g(&ui).invoke_close_cancel();

    // Encrypted entries: ask for the password, show a wrong one, then extract.
    support::encrypted_zip(&path("secret.zip"));
    let ui = h.open(&path("secret.zip"));
    h.until("listing", || g(&ui).get_status() == "ready");
    assert_eq!(g(&ui).get_encrypted_count(), 1);
    g(&ui).invoke_extract();
    assert!(g(&ui).get_show_password());
    g(&ui).invoke_unlock("wrong".into());
    h.until("password check", || !g(&ui).get_password_checking());
    assert_eq!(g(&ui).get_password_error(), "Incorrect password");
    h.shot("04-password-wrong");
    g(&ui).invoke_unlock("secret".into());
    h.until("done popup", || g(&ui).get_show_done());
    assert_eq!(
        std::fs::read_to_string(path("secret/secret.txt")).unwrap(),
        "secret"
    );

    // Opening an encrypted item asks for the password first.
    let ui = h.open(&path("secret.zip"));
    h.until("listing", || g(&ui).get_status() == "ready");
    let secret = rows(&ui).iter().position(|r| r == "secret.txt").unwrap();
    g(&ui).invoke_row_activated(secret as i32);
    assert!(g(&ui).get_show_password());
    g(&ui).invoke_unlock("secret".into());
    h.until("opened", || !OPENED.lock().unwrap().is_empty());
    let opened = OPENED.lock().unwrap().pop().unwrap();
    assert_eq!(std::fs::read_to_string(&opened).unwrap(), "secret");

    // An encrypted file list: locked until the password is given.
    support::encrypted_7z(&path("names.7z"), true);
    let ui = h.open(&path("names.7z"));
    h.until("password prompt", || g(&ui).get_show_password());
    assert_eq!(g(&ui).get_status(), "locked");
    g(&ui).invoke_unlock("nope".into());
    h.until("second prompt", || g(&ui).get_password_error() != "");
    g(&ui).invoke_cancel_password();
    h.shot("05-locked");
    g(&ui).invoke_ask_password();
    g(&ui).invoke_unlock("secret".into());
    h.until("listing", || g(&ui).get_status() == "ready");
    assert_eq!(rows(&ui), ["secret.txt"]);

    // Old Arabic names: banner, picker, fixed names.
    support::arabic_cp1256_zip(&path("arabic.zip"));
    let ui = h.open(&path("arabic.zip"));
    h.until("listing", || g(&ui).get_status() == "ready");
    assert!(g(&ui).get_show_banner());
    assert!(g(&ui).get_banner_text().contains("Windows-1256"));
    h.shot("06-banner");
    g(&ui).invoke_open_picker();
    assert!(g(&ui).get_show_picker());
    let labels: Vec<String> = g(&ui)
        .get_codec_labels()
        .iter()
        .map(|l| l.to_string())
        .collect();
    assert_eq!(
        labels[g(&ui).get_codec_index() as usize],
        "Arabic (Windows-1256)"
    );
    assert_eq!(g(&ui).get_codec_suggested(), g(&ui).get_codec_index());
    h.shot("07-picker");
    let cp437 = labels.iter().position(|l| l.contains("CP437")).unwrap();
    g(&ui).invoke_pick_codec(cp437 as i32);
    g(&ui).invoke_cancel_picker();
    assert!(
        rows(&ui).iter().any(|r| r == "الصور"),
        "cancel keeps the names"
    );
    g(&ui).invoke_open_picker();
    let arabic = g(&ui).get_codec_index();
    g(&ui).invoke_pick_codec(arabic);
    g(&ui).invoke_apply_codec();
    assert!(!g(&ui).get_show_banner());
    assert_eq!(g(&ui).get_encoding_chip(), "Arabic (Windows-1256)");

    // Damaged archive.
    support::truncated_tar_gz(&path("cut.tar.gz"));
    let ui = h.open(&path("cut.tar.gz"));
    h.until("error state", || g(&ui).get_status() == "failed");
    h.shot("08-failed");

    // Likely bomb: refused, with "Extract anyway".
    support::bomb_zip(&path("bomb.zip"));
    let ui = h.open(&path("bomb.zip"));
    h.until("listing", || g(&ui).get_status() == "ready");
    g(&ui).invoke_extract();
    h.until("error popup", || g(&ui).get_show_error());
    assert!(g(&ui).get_error_can_force());
    h.shot("09-bomb");
    g(&ui).invoke_dismiss_error();

    // Cancel a large extraction: nothing stays behind.
    big_zip(&path("big.zip"));
    let ui = h.open(&path("big.zip"));
    h.until("listing", || g(&ui).get_status() == "ready");
    // Cancelled right after starting, so a fast disk can't finish first.
    g(&ui).invoke_extract();
    assert!(g(&ui).get_busy());
    g(&ui).invoke_cancel_extract();
    h.until("cancelled", || !g(&ui).get_busy());
    assert!(
        g(&ui).get_toast().contains("cancelled"),
        "toast {:?} done {} error {:?}",
        g(&ui).get_toast(),
        g(&ui).get_show_done(),
        g(&ui).get_error_message()
    );
    assert!(!path("big").exists());
    // The progress fill, from a run that may finish.
    g(&ui).invoke_extract();
    h.until("progress", || {
        g(&ui).get_progress() > 0.05 || g(&ui).get_show_done()
    });
    h.shot_now("10-progress");
    h.until("done popup", || g(&ui).get_show_done());
    std::fs::remove_dir_all(path("big")).unwrap();
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(".crate-extract-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

/// 48 × 8 MiB of zeros, stored: big enough to watch the progress.
fn big_zip(path: &Path) {
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let chunk = vec![0u8; 1 << 20];
    for i in 0..48 {
        zip.start_file(format!("part{i:02}.bin"), stored).unwrap();
        for _ in 0..8 {
            zip.write_all(&chunk).unwrap();
        }
    }
    zip.finish().unwrap();
}
