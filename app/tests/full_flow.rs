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
