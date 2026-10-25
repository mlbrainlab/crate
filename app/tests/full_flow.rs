//! Drives the full view: switching density, folders, tree, breadcrumbs,
//! sorting, details, search, clipboard, test, and a big tar streaming in.

mod common;
#[path = "../../crates/crate-core/tests/support/mod.rs"]
mod support;

use std::path::Path;

use common::{Harness, files, g};
use slint::Model;
use slint::platform::Key;

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
    ui.set_custom_frame(true); // the Linux header with window controls
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

    // The Extract dialog: a pattern, into a chosen folder.
    let downloads = dir.path().join("dl");
    std::fs::create_dir_all(&downloads).unwrap();
    std::fs::write(
        config.path().join("user-dirs.dirs"),
        format!("XDG_DOWNLOAD_DIR=\"{}\"\n", downloads.display()),
    )
    .unwrap();
    let ui = h.open(&path("project-assets.zip"));
    h.until("listing", || g(&ui).get_status() == "ready");
    g(&ui).invoke_extract_dialog();
    assert!(g(&ui).get_ex_show());
    assert_eq!(g(&ui).get_ex_what(), 0);
    assert_eq!(g(&ui).get_ex_button(), "Extract everything");
    g(&ui).invoke_ex_set_what(2);
    g(&ui).set_ex_pattern("*.txt".into());
    g(&ui).invoke_ex_pattern_edited("*.txt".into());
    assert_eq!(g(&ui).get_ex_pattern_note(), "2 files match");
    assert_eq!(g(&ui).get_ex_button(), "Extract 2 files");
    let target = dir.path().join("picked");
    g(&ui).set_ex_dest(target.display().to_string().into());
    g(&ui).invoke_ex_dest_edited(target.display().to_string().into());
    assert_eq!(g(&ui).get_ex_folder_name(), "project-assets");
    g(&ui).invoke_ex_toggle(2); // don't open the folder
    h.shot("24-extract-dialog");
    g(&ui).invoke_ex_confirm("".into());
    h.until("done", || g(&ui).get_show_done());
    assert!(
        target
            .join("project-assets/project/docs/guide.txt")
            .is_file()
    );
    assert!(!target.join("project-assets/project/README.md").exists());
    g(&ui).invoke_back_to_archive();

    // Into an existing folder, asking about clashes: keep both, for all.
    g(&ui).invoke_extract_dialog();
    g(&ui).invoke_ex_set_what(2);
    g(&ui).set_ex_pattern("*.txt".into());
    g(&ui).invoke_ex_pattern_edited("*.txt".into());
    let existing = target.join("project-assets");
    g(&ui).invoke_ex_dest_edited(existing.display().to_string().into());
    g(&ui).invoke_ex_toggle(0); // no new folder
    g(&ui).invoke_ex_toggle(2); // don't open
    assert_eq!(g(&ui).get_ex_if_exists(), 0, "Ask is the default");
    g(&ui).invoke_ex_confirm("".into());
    h.until("conflict", || g(&ui).get_conflict_show());
    let first = g(&ui).get_conflict_name();
    assert!(first == "guide.txt" || first == "تقرير.txt", "{first}");
    h.shot("25-conflict");
    g(&ui).invoke_conflict_answer(1, true);
    h.until("done", || g(&ui).get_show_done());
    assert!(existing.join("project/docs/guide (2).txt").is_file());
    assert!(
        existing.join("project/docs/تقرير (2).txt").is_file(),
        "for all: no second question"
    );
    g(&ui).invoke_back_to_archive();

    // A folder Crate can't write to.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let locked = dir.path().join("locked");
        std::fs::create_dir(&locked).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
        g(&ui).invoke_extract_dialog();
        g(&ui).invoke_ex_dest_edited(locked.display().to_string().into());
        g(&ui).invoke_ex_toggle(2);
        g(&ui).invoke_ex_confirm("".into());
        h.until("error", || g(&ui).get_show_error());
        assert_eq!(g(&ui).get_error_kind(), 3, "{}", g(&ui).get_error_message());
        assert!(g(&ui).get_error_title().starts_with("Crate can’t write to"));
        h.shot("26-permission");
        g(&ui).invoke_error_to_downloads();
        h.until("done", || g(&ui).get_show_done());
        assert!(downloads.join("project/README.md").is_file());
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    // Quick preview: an image, code and a binary, read straight from the archive.
    preview_zip(&path("preview.zip"));
    let ui = h.open(&path("preview.zip"));
    h.until("listing", || g(&ui).get_status() == "ready");
    g(&ui).invoke_toggle_full();
    assert_eq!(files(&ui), ["cover.png", "display.icc", "main.rs"]);
    g(&ui).invoke_row_pressed(0, false, false);
    g(&ui).invoke_preview_current();
    h.until("image", || g(&ui).get_pv_kind() == 1);
    assert_eq!(
        g(&ui).get_pv_info(),
        format!(
            "640 × 400 · {}",
            crate_app::model::size_text(png_size(&path("preview.zip")))
        )
    );
    assert!(!g(&ui).get_pv_can_prev() && g(&ui).get_pv_can_next());
    h.shot("27-preview-image");
    g(&ui).invoke_pv_step(1);
    h.until("hex", || g(&ui).get_pv_kind() == 3);
    assert_eq!(g(&ui).get_pv_name(), "display.icc");
    assert!(
        g(&ui)
            .get_pv_lines()
            .row_data(0)
            .unwrap()
            .starts_with("00000000  00 00 02 24")
    );
    h.shot("28-preview-hex");
    g(&ui).invoke_pv_step(1);
    h.until("text", || g(&ui).get_pv_kind() == 2);
    assert_eq!(g(&ui).get_pv_language(), "Rust · UTF-8");
    assert_eq!(
        g(&ui).get_pv_lines().row_data(1).unwrap(),
        "    let src = Path::new(\"photos.zip\");"
    );
    assert_eq!(g(&ui).get_cursor(), 2, "the list follows the preview");
    h.shot("29-preview-text");
    g(&ui).invoke_pv_copy();
    assert!(g(&ui).get_clip_text().starts_with("fn main() {"));
    g(&ui).invoke_pv_close();
    assert!(!g(&ui).get_pv_show());
    assert!(
        std::fs::read_dir(dir.path()).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".crate")),
        "nothing written"
    );

    // A folder of photos switches to the grid; thumbnails come in as tiles show.
    photos_zip(&path("trip.zip"));
    let ui = h.open(&path("trip.zip"));
    h.until("listing", || g(&ui).get_status() == "ready");
    g(&ui).invoke_toggle_full();
    h.render();
    h.render();
    assert!(g(&ui).get_grid(), "9 of 10 are photos");
    assert!(g(&ui).get_grid_banner().contains("9 of 10"));
    let cols = g(&ui).get_grid_columns();
    assert!(cols >= 3, "{cols}");
    h.until("thumbnails", || {
        let rows = g(&ui).get_grid_rows();
        let first = rows.row_data(0).unwrap().tiles;
        (0..first.row_count()).all(|i| {
            let t = first.row_data(i).unwrap();
            t.kind != 5 || t.has_thumb
        })
    });
    g(&ui).invoke_row_pressed(1, false, false);
    g(&ui).invoke_row_pressed(2, true, false);
    let first = g(&ui).get_grid_rows().row_data(0).unwrap().tiles;
    assert!(first.row_data(1).unwrap().selected && first.row_data(2).unwrap().selected);
    h.shot("30-grid");
    g(&ui).invoke_grid_banner_list();
    assert!(!g(&ui).get_grid(), "Use list here");
    g(&ui).invoke_set_grid(true);
    assert!(g(&ui).get_grid());
    assert_eq!(g(&ui).get_grid_banner(), "");
    g(&ui).invoke_set_grid(false);

    // Editing: new folder, rename, add, delete (with a backup).
    support::sample_zip(&path("edit.zip"));
    let ui = h.open(&path("edit.zip"));
    h.until("listing", || g(&ui).get_status() == "ready");
    g(&ui).invoke_toggle_full();
    assert!(g(&ui).get_can_edit());
    g(&ui).invoke_row_activated(0); // into project/
    g(&ui).invoke_new_folder();
    assert!(g(&ui).get_name_show());
    assert_eq!(g(&ui).get_name_value(), "New folder");
    g(&ui).invoke_name_edited("docs".into());
    assert!(
        g(&ui).get_name_error().contains("already in"),
        "{}",
        g(&ui).get_name_error()
    );
    g(&ui).invoke_name_edited("drafts".into());
    assert_eq!(g(&ui).get_name_error(), "");
    g(&ui).invoke_name_confirm("drafts".into());
    h.until("folder added", || {
        files(&ui).contains(&"drafts".to_string())
    });
    assert_eq!(
        crumbs(&ui),
        ["edit.zip", "project"],
        "still in the same folder"
    );
    let readme = files(&ui).iter().position(|f| f == "README.md").unwrap();
    g(&ui).invoke_row_pressed(readme as i32, false, false);
    g(&ui).invoke_rename_current();
    assert_eq!(g(&ui).get_name_value(), "README.md");
    g(&ui).invoke_name_confirm("INDEX.md".into());
    h.until("renamed", || files(&ui).contains(&"INDEX.md".to_string()));
    let extra = path("extra.txt");
    std::fs::write(&extra, "extra").unwrap();
    crate_app::view::post(move |v| v.confirm_add(vec![extra], "project".into()));
    h.until("add prompt", || g(&ui).get_add_show());
    assert_eq!(g(&ui).get_add_title(), "Add 1 item to project/");
    h.shot("31-add-confirm");
    g(&ui).invoke_add_confirm();
    h.until("added", || files(&ui).contains(&"extra.txt".to_string()));
    let run = files(&ui).iter().position(|f| f == "run.sh").unwrap();
    g(&ui).invoke_row_pressed(run as i32, false, false);
    g(&ui).invoke_delete_selected();
    assert_eq!(
        g(&ui).get_delete_title(),
        "Delete “run.sh” from the archive?"
    );
    h.shot("32-delete-confirm");
    g(&ui).invoke_delete_confirm(true);
    h.until("deleted", || !files(&ui).contains(&"run.sh".to_string()));
    assert!(
        g(&ui)
            .get_toast()
            .contains("previous version kept as edit.zip.bak"),
        "{}",
        g(&ui).get_toast()
    );
    assert!(path("edit.zip.bak").is_file());
    let names: Vec<String> = crate_core::Archive::open(path("edit.zip"))
        .unwrap()
        .entries()
        .iter()
        .map(|e| e.path.clone())
        .collect();
    assert!(
        names.contains(&"project/INDEX.md".to_string())
            && names.contains(&"project/drafts/".to_string()),
        "{names:?}"
    );

    // Properties: overview, integrity, checksums, verify by paste.
    g(&ui).invoke_open_properties();
    assert!(g(&ui).get_pr_show());
    h.until("breakdown", || g(&ui).get_pr_categories().row_count() > 0);
    assert!(
        g(&ui).get_pr_saved().ends_with("%"),
        "{}",
        g(&ui).get_pr_saved()
    );
    h.until("checksums", || !g(&ui).get_pr_sha256().is_empty());
    let sha = g(&ui).get_pr_sha256().to_string();
    assert_eq!(
        sha,
        crate_core::checksum::compute(
            &path("edit.zip"),
            &mut |_| {},
            &crate_core::CancelToken::new()
        )
        .unwrap()
        .sha256
    );
    h.shot("33-properties");
    g(&ui).invoke_pr_copy(0);
    assert_eq!(g(&ui).get_clip_text(), sha.as_str());
    g(&ui).invoke_pr_open_tab(2);
    g(&ui).invoke_pr_verify_edited(format!("sha256: {sha}  edit.zip").into());
    assert_eq!(g(&ui).get_pr_verify_state(), 1);
    g(&ui).invoke_pr_verify_edited("ffffffffffffffffffffffffffffffffffffffff".into());
    assert_eq!(g(&ui).get_pr_verify_state(), 2);
    h.shot("34-checksums");
    g(&ui).invoke_pr_open_tab(1);
    g(&ui).invoke_test_archive();
    h.until("tested", || g(&ui).get_test_state() == 2);
    assert!(
        g(&ui).get_pr_tested().contains("tested just now"),
        "{}",
        g(&ui).get_pr_tested()
    );
    g(&ui).invoke_pr_close();

    // Convert: edit.zip → 7z next to it.
    g(&ui).invoke_open_convert();
    assert!(g(&ui).get_cv_show());
    assert_eq!(g(&ui).get_cv_from(), "ZIP");
    assert_eq!(g(&ui).get_cv_format(), 1, "ZIP defaults to 7z");
    assert_eq!(g(&ui).get_cv_target(), "edit.7z");
    h.shot("35-convert");
    g(&ui).invoke_cv_confirm(true);
    h.until("converted", || {
        g(&ui).get_toast().starts_with("Converted to edit.7z")
    });
    assert!(path("edit.zip").is_file(), "the original is kept");
    let converted = crate_core::Archive::open(path("edit.7z")).unwrap();
    assert_eq!(converted.format(), crate_core::Format::SevenZ);
    assert!(
        converted
            .entries()
            .iter()
            .any(|e| e.path == "project/INDEX.md")
    );

    // RAR is read-only: save a copy instead.
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/rar/solid.rar"),
        path("solid.rar"),
    )
    .unwrap();
    let ui = h.open(&path("solid.rar"));
    h.until("listing", || g(&ui).get_status() == "ready");
    assert!(!g(&ui).get_can_edit());
    let (src, out) = (path("solid.rar"), path("solid-copy.zip"));
    crate_app::view::post(move |v| v.convert_copy(src, None, out));
    h.until("copied", || {
        g(&ui)
            .get_toast()
            .starts_with("Saved a copy as solid-copy.zip")
    });
    assert_eq!(
        crate_core::Archive::open(path("solid-copy.zip"))
            .unwrap()
            .entries()
            .len(),
        1
    );

    // Several levels down and back: Enter, breadcrumbs, Backspace, Alt+Up.
    common::nested_zip(&path("nested.zip"));
    let ui = h.open(&path("nested.zip"));
    h.until("listing", || g(&ui).get_status() == "ready");
    g(&ui).invoke_toggle_full();
    assert_eq!(files(&ui), ["عقود", "top.txt"]);
    g(&ui).invoke_row_activated(0);
    g(&ui).invoke_step(1, false);
    h.key(Key::Return);
    assert_eq!(files(&ui), ["q1"]);
    g(&ui).invoke_row_activated(0);
    assert_eq!(files(&ui), ["contract.pdf", "notes.txt"]);
    assert_eq!(crumbs(&ui), ["nested.zip", "عقود", "2024", "q1"]);
    h.shot("36-full-deep-folder");
    h.key(Key::Backspace);
    assert_eq!(crumbs(&ui), ["nested.zip", "عقود", "2024"]);
    h.key_with(Key::Alt, Key::UpArrow);
    assert_eq!(files(&ui), ["2024", "2025", "readme.txt"]);
    g(&ui).invoke_row_activated(1);
    g(&ui).invoke_open_crumb(0);
    assert_eq!(files(&ui), ["عقود", "top.txt"]);
    // The folder carries over to the mini view and back.
    g(&ui).invoke_open_crumb(0);
    g(&ui).invoke_row_activated(0);
    g(&ui).invoke_row_activated(0);
    g(&ui).invoke_toggle_full();
    assert_eq!(g(&ui).get_mini_folder(), "2024");
    assert_eq!(common::rows(&ui), ["q1"]);
    g(&ui).invoke_toggle_full();
    assert_eq!(crumbs(&ui), ["nested.zip", "عقود", "2024"]);
    // Extract all from deep inside still extracts the whole archive.
    g(&ui).invoke_clear_selection(); // the cursor selected a row earlier
    g(&ui).invoke_extract();
    h.until("done popup", || g(&ui).get_show_done());
    assert!(path("nested/top.txt").is_file());
    assert!(path("nested/عقود/2024/q1/notes.txt").is_file());
    g(&ui).invoke_back_to_archive();

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

/// cover.png (640 × 400), main.rs and a binary ICC profile.
fn preview_zip(path: &Path) {
    use std::io::Write;
    let mut png = Vec::new();
    image::RgbaImage::from_fn(640, 400, |x, y| {
        image::Rgba([(x % 256) as u8, (y % 256) as u8, 160, 255])
    })
    .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
    .unwrap();
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let opts = zip::write::SimpleFileOptions::default();
    zip.start_file("cover.png", opts).unwrap();
    zip.write_all(&png).unwrap();
    zip.start_file("main.rs", opts).unwrap();
    zip.write_all(b"fn main() {\n\tlet src = Path::new(\"photos.zip\");\n}\n")
        .unwrap();
    zip.start_file("display.icc", opts).unwrap();
    let mut icc = b"\x00\x00\x02\x24applmntrRGB XYZ ".to_vec();
    icc.extend((0..2000u32).map(|i| (i * 7 % 251) as u8));
    zip.write_all(&icc).unwrap();
    zip.finish().unwrap();
}

/// Nine photos and a text file.
fn photos_zip(path: &Path) {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let opts = zip::write::SimpleFileOptions::default();
    for i in 0..9u8 {
        let mut png = Vec::new();
        image::RgbImage::from_fn(320, 200, |x, y| {
            image::Rgb([i * 28, (x / 2) as u8, (y + x) as u8])
        })
        .write_to(
            &mut std::io::Cursor::new(&mut png),
            image::ImageFormat::Jpeg,
        )
        .unwrap();
        zip.start_file(format!("IMG_{i:04}.jpg"), opts).unwrap();
        zip.write_all(&png).unwrap();
    }
    zip.start_file("notes.txt", opts).unwrap();
    zip.write_all(b"shot list").unwrap();
    zip.finish().unwrap();
}

fn png_size(zip: &Path) -> u64 {
    let archive = crate_core::Archive::open(zip).unwrap();
    archive
        .entries()
        .iter()
        .find(|e| e.path == "cover.png")
        .unwrap()
        .size
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
