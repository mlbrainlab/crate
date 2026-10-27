//! The README's and AppStream's screenshots, with tidy content: archives in
//! `~/Downloads` of a stand-in home folder, so paths read `~/Downloads/…`.
//! Written to target/ui-snapshots/readme/; `scripts/readme-screenshots.sh`
//! copies them to docs/screenshots/.

mod common;

use std::fs;
use std::io::Write;
use std::path::Path;

use common::{Harness, g};
use slint::ComponentHandle;

/// Bytes that don't compress, like real PDFs and images.
fn noise(n: usize, seed: u32) -> Vec<u8> {
    let mut x = seed.wrapping_mul(2_654_435_761).max(1);
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        })
        .collect()
}

fn design_system_zip(path: &Path) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let opts = zip::write::SimpleFileOptions::default();
    let text = |n: usize| "Lorem ipsum dolor sit amet, consectetur adipiscing elit.\n".repeat(n);
    let files: Vec<(&str, Vec<u8>)> = vec![
        ("design-system/README.md", text(40).into_bytes()),
        ("design-system/tokens.json", text(120).into_bytes()),
        ("design-system/docs/colors.pdf", noise(180_000, 1)),
        ("design-system/docs/typography.pdf", noise(240_000, 2)),
        ("design-system/docs/spacing.md", text(60).into_bytes()),
        ("design-system/icons/archive.svg", text(8).into_bytes()),
        ("design-system/icons/folder.svg", text(8).into_bytes()),
        ("design-system/images/hero@2x.png", noise(420_000, 3)),
        ("design-system/images/cover.jpg", noise(310_000, 4)),
        ("design-system/src/button.tsx", text(90).into_bytes()),
        ("design-system/src/theme.ts", text(70).into_bytes()),
        ("design-system/CHANGELOG.md", text(30).into_bytes()),
    ];
    for (name, data) in files {
        zip.start_file(name, opts).unwrap();
        zip.write_all(&data).unwrap();
    }
    zip.finish().unwrap();
}

#[test]
fn readme_screenshots() {
    let h = Harness::new();
    let home = tempfile::tempdir().unwrap();
    let downloads = home.path().join("Downloads");
    fs::create_dir_all(&downloads).unwrap();
    // SAFETY: before any window or worker thread uses HOME.
    unsafe { std::env::set_var("HOME", home.path()) };
    let shots = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/ui-snapshots/readme");
    fs::create_dir_all(&shots).unwrap();
    let shot = |name: &str| {
        h.shot(&format!("readme/{name}"));
    };

    let archive = downloads.join("design-system.zip");
    design_system_zip(&archive);

    // Mini view.
    let ui = h.open(&archive);
    h.until("listing", || g(&ui).get_status() == "ready");
    g(&ui).invoke_row_activated(0);
    shot("mini");

    // Full view, a file selected.
    g(&ui).invoke_toggle_full();
    let docs = common::files(&ui).iter().position(|f| f == "docs").unwrap();
    g(&ui).invoke_row_activated(docs as i32);
    g(&ui).invoke_row_pressed(1, false, false);
    shot("full");

    // Dark.
    ui.global::<crate_app::Palette>()
        .set_color_scheme(slint::language::ColorScheme::Dark);
    shot("dark");

    // New archive.
    let project = home.path().join("Projects/brand-refresh");
    fs::create_dir_all(project.join("exports")).unwrap();
    fs::write(project.join("brief.md"), "Brand refresh\n".repeat(400)).unwrap();
    fs::write(project.join("exports/logo.svg"), "<svg/>\n".repeat(900)).unwrap();
    fs::write(project.join("exports/hero.png"), noise(600_000, 5)).unwrap();
    let main = h.home(vec![project.clone()]);
    h.until("estimated", || {
        let level = main
            .global::<crate_app::CreateUi>()
            .get_level_text()
            .to_string();
        level.contains('≈')
    });
    shot("create");

    // Arabic.
    let mut s = crate_app::settings::Settings::load();
    s.set("pref.language", "ar");
    s.save();
    let ui = h.open(&archive);
    h.until("listing", || g(&ui).get_status() == "ready");
    g(&ui).invoke_row_activated(0);
    shot("arabic");
}
